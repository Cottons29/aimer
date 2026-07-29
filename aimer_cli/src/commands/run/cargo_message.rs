//! Cargo's machine-readable build output.
//!
//! `aimer run` asks cargo for JSON messages ([`MESSAGE_FORMAT`]) instead of
//! plain text so the console gets structured access to every diagnostic. Each
//! line cargo writes on stdout is one JSON object; [`CargoMessage::parse`] turns
//! it back into something printable.
//!
//! Two things fall out of that:
//!
//! * **The build pane still looks like a real `cargo build`.** Cargo renders
//!   every diagnostic for a terminal itself and ships the result in the
//!   `rendered` field, colors and carets included, so replaying it reproduces
//!   cargo's own output byte for byte. The `Compiling` / `Finished` status lines
//!   are unaffected: cargo keeps writing those to stderr even in JSON mode.
//! * **Errors can be repeated at the end of the build.** [`ErrorReport`]
//!   collects the failing diagnostics while they stream past and formats them as
//!   one `Compile Error` block, so a failed build ends with everything that went
//!   wrong instead of burying it in the scrollback.

use colored::Colorize;
use serde::Deserialize;

use crate::console::state::strip_ansi;

/// The `--message-format` cargo needs for [`CargoMessage::parse`] to see
/// anything.
///
/// The `-diagnostic-rendered-ansi` suffix is what makes cargo put terminal
/// colors into the `rendered` field of a diagnostic; without it the replayed
/// output would be plain grey text.
pub const MESSAGE_FORMAT: &str = "--message-format=json-diagnostic-rendered-ansi";

/// Narrowest the [`ErrorReport`] block ever gets: the width used when the
/// terminal size is unknown, and the floor [`report_width`] never goes below.
const MIN_WIDTH: usize = 58;

/// Columns the Build Logs pane spends on its own border, and which therefore
/// aren't available to the report inside it.
const PANE_BORDER: u16 = 2;

/// Background painted behind an error inside the report — a muted light red,
/// dark enough for cargo's own red and blue to stay readable on top of it.
const PANEL_BACKGROUND: (u8, u8, u8) = (74, 40, 40);

/// Color of the wave rule between two errors. Darker than the framing headers so
/// the rule reads as a seam between panels rather than as another error line.
const DIVIDER_COLOR: (u8, u8, u8) = (112, 52, 52);

/// Severity of a rustc diagnostic, as spelled in cargo's `level` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
    /// The trailing hints of a failed build, e.g. *"Some errors have detailed
    /// explanations"*. They are printed like any other diagnostic but are not
    /// errors of their own, so they never appear in an [`ErrorReport`].
    FailureNote,
}

impl DiagnosticLevel {
    /// Map cargo's `level` string, or `None` for a level we don't know — such a
    /// message is passed through verbatim rather than guessed at.
    fn from_cargo(level: &str) -> Option<Self> {
        match level {
            "error" | "error: internal compiler error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "note" => Some(Self::Note),
            "help" => Some(Self::Help),
            "failure-note" => Some(Self::FailureNote),
            _ => None,
        }
    }

    /// Whether a diagnostic of this level makes the build fail.
    #[inline]
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error)
    }
}

/// One rustc diagnostic pulled out of a cargo `compiler-message`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// How severe rustc considered it.
    pub level: DiagnosticLevel,
    /// The headline message, without the `error[E0308]: ` prefix.
    pub message: String,
    /// The error index code, e.g. `E0308`, when rustc assigned one.
    pub code: Option<String>,
    /// `file:line:column` of the primary span, when the diagnostic has one.
    pub location: Option<String>,
    /// Cargo's own terminal rendering of the diagnostic.
    rendered: Option<String>,
}

impl Diagnostic {
    /// Whether this diagnostic is one of the errors that failed the build.
    #[inline]
    pub fn is_error(&self) -> bool {
        self.level.is_error()
    }

    /// The diagnostic exactly as `cargo build` would have printed it.
    ///
    /// Cargo's own rendering is used when present — that is what keeps the build
    /// pane indistinguishable from a plain `cargo build`. Otherwise a single
    /// `level[code]: message` line is synthesised, so a diagnostic is never
    /// silently dropped.
    pub fn render(&self) -> String {
        match self.rendered.as_deref() {
            Some(rendered) if !rendered.trim().is_empty() => {
                rendered.trim_end_matches('\n').to_string()
            }
            _ => self.headline(),
        }
    }

    /// [`render`](Self::render) split into console lines, since the panes store
    /// one line per entry.
    pub fn render_lines(&self) -> Vec<String> {
        self.render().split('\n').map(str::to_string).collect()
    }

    /// The `level[code]: message` first line of the diagnostic.
    pub fn headline(&self) -> String {
        match &self.code {
            Some(code) => format!("{}[{}]: {}", self.label(), code, self.message),
            None => format!("{}: {}", self.label(), self.message),
        }
    }

    /// How rustc names this level in its output.
    fn label(&self) -> &'static str {
        match self.level {
            DiagnosticLevel::Error => "error",
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Note | DiagnosticLevel::FailureNote => "note",
            DiagnosticLevel::Help => "help",
        }
    }
}

/// A line of cargo's JSON output, reduced to what the console does something
/// with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CargoMessage {
    /// A rustc diagnostic (`compiler-message`).
    Diagnostic(Diagnostic),
    /// A compilation unit finished (`compiler-artifact`), named by its target.
    Artifact { target: String },
    /// The build ended (`build-finished`).
    Finished { success: bool },
    /// A message the console has nothing to add to, e.g.
    /// `build-script-executed`.
    Other,
}

impl CargoMessage {
    /// Parse one line of cargo's JSON output.
    ///
    /// Returns `None` for anything that is not a cargo message — an empty line,
    /// text a build script printed on stdout, or a JSON object without a
    /// `reason`. Callers forward those lines unchanged, so nothing a build emits
    /// is ever swallowed.
    pub fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            return None;
        }
        let raw: RawMessage = serde_json::from_str(trimmed).ok()?;
        Some(match raw.reason.as_str() {
            "compiler-message" => {
                let message = raw.message?;
                let level = DiagnosticLevel::from_cargo(&message.level)?;
                let location = message
                    .spans
                    .iter()
                    .find(|span| span.is_primary)
                    .or_else(|| message.spans.first())
                    .map(RawSpan::location);
                Self::Diagnostic(Diagnostic {
                    level,
                    message: message.message,
                    code: message.code.map(|code| code.code),
                    location,
                    rendered: message.rendered,
                })
            }
            "compiler-artifact" => Self::Artifact {
                target: raw.target.map(|t| t.name).unwrap_or_default(),
            },
            "build-finished" => Self::Finished {
                success: raw.success.unwrap_or(false),
            },
            _ => Self::Other,
        })
    }
}

/// The errors of one build, kept so they can be shown again together once the
/// build has failed.
///
/// A long build scrolls its errors far out of view; replaying them as a single
/// block at the very end means the last thing on screen is always the reason the
/// build failed.
#[derive(Clone, Debug, Default)]
pub struct ErrorReport {
    errors: Vec<Diagnostic>,
}

impl ErrorReport {
    /// An empty report.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember `diagnostic` if it is an error; anything else is ignored, so
    /// warnings and notes don't pad the summary.
    pub fn record(&mut self, diagnostic: &Diagnostic) {
        if diagnostic.is_error() {
            self.errors.push(diagnostic.clone());
        }
    }

    /// Whether no error was recorded, in which case [`lines`](Self::lines)
    /// produces nothing.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// The report as console lines, laid out for the current terminal — see
    /// [`lines_with_width`](Self::lines_with_width).
    #[inline]
    pub fn lines(&self) -> Vec<String> {
        self.lines_with_width(report_width())
    }

    /// The report as console lines `width` cells wide: a framed `Compile Error`
    /// block in which every error sits on its own light-red panel, panels
    /// separated by wave rules.
    ///
    /// Cargo's rendering already states the level, the code and the location, so
    /// nothing is written above a panel — the panel itself is what tells the
    /// errors apart.
    ///
    /// A panel wider than `width` is never truncated: an error whose rendering
    /// doesn't fit keeps its own width, since cutting cargo's carets off would
    /// make the diagnostic unreadable. A `width` narrower than [`MIN_WIDTH`] is
    /// raised to it, so a tiny terminal still gets a readable block.
    ///
    /// Empty when no error was recorded, so a successful build appends nothing.
    pub fn lines_with_width(&self, width: usize) -> Vec<String> {
        if self.errors.is_empty() {
            return Vec::new();
        }

        let width = width.max(MIN_WIDTH);
        let mut lines = vec![String::new(), header("Compile Error", width), String::new()];

        for (index, error) in self.errors.iter().enumerate() {
            if index > 0 {
                lines.push(divider(width));
            }
            lines.extend(panel(&error.render_lines(), width));
        }

        lines.push(String::new());
        lines.push(header(
            &match self.errors.len() {
                1 => "1 error".to_string(),
                n => format!("{} errors", n),
            },
            width,
        ));
        lines
    }
}

/// How wide the report should be drawn: the whole terminal, minus the columns the
/// Build Logs pane spends on its border, so the block fills the pane it lands in.
///
/// Zero when the terminal size is unavailable — output redirected to a file or a
/// pipe has no width to speak of; [`ErrorReport::lines_with_width`] then falls
/// back to [`MIN_WIDTH`].
fn report_width() -> usize {
    crossterm::terminal::size()
        .map(|(columns, _)| columns.saturating_sub(PANE_BORDER) as usize)
        .unwrap_or(0)
}

/// A centred title framed by `=` rules, `width` cells wide.
fn header(title: &str, width: usize) -> String {
    let text = format!(" {} ", title);
    let fill = width.saturating_sub(text.chars().count());
    let left = fill / 2;
    format!("{}{}{}", "=".repeat(left), text, "=".repeat(fill - left))
        .red()
        .bold()
        .to_string()
}

/// The wave rule between two error panels of a report, `width` cells wide.
///
/// The color is written out by hand, like the panel background, so the rule
/// keeps its darker tint no matter what the `colored` crate decides about the
/// current output stream.
fn divider(width: usize) -> String {
    let (r, g, b) = DIVIDER_COLOR;
    format!("\x1b[38;2;{r};{g};{b}m{}\x1b[0m", "~".repeat(width))
}

/// Lay `rendered` out as one error panel: every line padded to `width` cells and
/// painted on [`PANEL_BACKGROUND`], with a blank row above and below so the text
/// doesn't touch the edges.
fn panel(rendered: &[String], width: usize) -> Vec<String> {
    let width = rendered
        .iter()
        .map(|line| visible_width(line))
        .max()
        .unwrap_or(0)
        .max(width);

    let mut lines = Vec::with_capacity(rendered.len() + 2);
    lines.push(panel_line("", width));
    lines.extend(rendered.iter().map(|line| panel_line(line, width)));
    lines.push(panel_line("", width));
    lines
}

/// One row of a [`panel`], padded to `width` cells.
///
/// Cargo's rendering ends every colored span with a reset, which would also drop
/// the background, so the background is re-armed after each reset instead of
/// only at the start of the line.
fn panel_line(content: &str, width: usize) -> String {
    let (r, g, b) = PANEL_BACKGROUND;
    let background = format!("\x1b[48;2;{r};{g};{b}m");
    let padding = " ".repeat(width.saturating_sub(visible_width(content)));
    format!(
        "{background}{}{padding}\x1b[0m",
        content.replace("\x1b[0m", &format!("\x1b[0m{background}"))
    )
}

/// How many cells `line` occupies once its escape sequences are gone.
fn visible_width(line: &str) -> usize {
    strip_ansi(line).chars().count()
}

// ── Cargo's JSON shape ───────────────────────────────────────────────

#[derive(Deserialize)]
struct RawMessage {
    reason: String,
    #[serde(default)]
    success: Option<bool>,
    #[serde(default)]
    target: Option<RawTarget>,
    #[serde(default)]
    message: Option<RawDiagnostic>,
}

#[derive(Deserialize)]
struct RawTarget {
    name: String,
}

#[derive(Deserialize)]
struct RawDiagnostic {
    level: String,
    message: String,
    #[serde(default)]
    rendered: Option<String>,
    #[serde(default)]
    code: Option<RawCode>,
    #[serde(default)]
    spans: Vec<RawSpan>,
}

#[derive(Deserialize)]
struct RawCode {
    code: String,
}

#[derive(Deserialize)]
struct RawSpan {
    file_name: String,
    line_start: u32,
    column_start: u32,
    #[serde(default)]
    is_primary: bool,
}

impl RawSpan {
    fn location(&self) -> String {
        format!(
            "{}:{}:{}",
            self.file_name, self.line_start, self.column_start
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `compiler-message` line as cargo writes it.
    fn compiler_message(level: &str, message: &str, code: Option<&str>, rendered: &str) -> String {
        let code = match code {
            Some(code) => format!(r#""code":{{"code":"{code}","explanation":null}}"#),
            None => r#""code":null"#.to_string(),
        };
        format!(
            r#"{{"reason":"compiler-message","package_id":"path+file:///app#app@0.1.0","target":{{"name":"app","kind":["bin"]}},"message":{{"level":"{level}","message":"{message}",{code},"spans":[{{"file_name":"src/main.rs","line_start":2,"column_start":18,"is_primary":true}}],"rendered":"{rendered}"}}}}"#
        )
    }

    /// Parse `line` and unwrap it as a diagnostic.
    fn diagnostic(line: &str) -> Diagnostic {
        match CargoMessage::parse(line) {
            Some(CargoMessage::Diagnostic(diagnostic)) => diagnostic,
            other => panic!("expected a diagnostic, got {other:?}"),
        }
    }

    /// An error carrying `message`, as if cargo had rendered it.
    fn error_with(message: &str, code: &str) -> Diagnostic {
        Diagnostic {
            level: DiagnosticLevel::Error,
            message: message.to_string(),
            code: Some(code.to_string()),
            location: Some("src/main.rs:2:18".to_string()),
            rendered: Some(format!("error[{code}]: {message}\n --> src/main.rs:2:18\n")),
        }
    }

    // ── Parsing ──────────────────────────────────────────────────────

    #[test]
    fn parse_reads_an_error_diagnostic() {
        let error = diagnostic(&compiler_message(
            "error",
            "mismatched types",
            Some("E0308"),
            "rendered",
        ));

        assert_eq!(error.level, DiagnosticLevel::Error);
        assert!(error.is_error());
        assert_eq!(error.message, "mismatched types");
        assert_eq!(error.code.as_deref(), Some("E0308"));
        assert_eq!(error.location.as_deref(), Some("src/main.rs:2:18"));
    }

    #[test]
    fn parse_reads_a_warning_diagnostic() {
        let warning = diagnostic(&compiler_message("warning", "unused variable", None, "w"));

        assert_eq!(warning.level, DiagnosticLevel::Warning);
        assert!(!warning.is_error());
        assert!(warning.code.is_none());
    }

    #[test]
    fn parse_reads_a_failure_note_without_making_it_an_error() {
        let note = diagnostic(&compiler_message(
            "failure-note",
            "Some errors have detailed explanations.",
            None,
            "note",
        ));

        assert_eq!(note.level, DiagnosticLevel::FailureNote);
        assert!(!note.is_error());
    }

    #[test]
    fn parse_reads_an_artifact() {
        let line = r#"{"reason":"compiler-artifact","target":{"name":"aimer_app","kind":["lib"]},"filenames":[]}"#;

        assert_eq!(
            CargoMessage::parse(line),
            Some(CargoMessage::Artifact {
                target: "aimer_app".to_string()
            })
        );
    }

    #[test]
    fn parse_reads_the_build_outcome() {
        assert_eq!(
            CargoMessage::parse(r#"{"reason":"build-finished","success":false}"#),
            Some(CargoMessage::Finished { success: false })
        );
        assert_eq!(
            CargoMessage::parse(r#"{"reason":"build-finished","success":true}"#),
            Some(CargoMessage::Finished { success: true })
        );
    }

    #[test]
    fn parse_maps_unhandled_reasons_to_other() {
        let line = r#"{"reason":"build-script-executed","package_id":"x","linked_libs":[]}"#;
        assert_eq!(CargoMessage::parse(line), Some(CargoMessage::Other));
    }

    #[test]
    fn parse_rejects_output_that_is_not_a_cargo_message() {
        assert!(CargoMessage::parse("").is_none());
        assert!(CargoMessage::parse("   ").is_none());
        assert!(CargoMessage::parse("Compiling app v0.1.0").is_none());
        assert!(CargoMessage::parse("[1, 2, 3]").is_none());
        assert!(CargoMessage::parse(r#"{"level":"info","message":"app log"}"#).is_none());
        assert!(CargoMessage::parse(r#"{"reason":"compiler-message"}"#).is_none());
        assert!(CargoMessage::parse(r#"{"reason":"broken"#).is_none());
    }

    #[test]
    fn parse_rejects_an_unknown_diagnostic_level() {
        let line = compiler_message("gossip", "who knows", None, "x");
        assert!(CargoMessage::parse(&line).is_none());
    }

    #[test]
    fn parse_prefers_the_primary_span_for_the_location() {
        let line = r#"{"reason":"compiler-message","message":{"level":"error","message":"m","spans":[{"file_name":"src/a.rs","line_start":1,"column_start":1,"is_primary":false},{"file_name":"src/b.rs","line_start":7,"column_start":3,"is_primary":true}],"rendered":"r"}}"#;

        assert_eq!(diagnostic(line).location.as_deref(), Some("src/b.rs:7:3"));
    }

    #[test]
    fn parse_leaves_the_location_out_when_there_is_no_span() {
        let line = r#"{"reason":"compiler-message","message":{"level":"error","message":"m","spans":[],"rendered":"r"}}"#;
        assert!(diagnostic(line).location.is_none());
    }

    // ── Rendering ────────────────────────────────────────────────────

    #[test]
    fn render_replays_cargos_own_output() {
        let line = compiler_message(
            "error",
            "mismatched types",
            Some("E0308"),
            "error[E0308]: mismatched types\\n --> src/main.rs:2:18\\n",
        );

        assert_eq!(
            diagnostic(&line).render(),
            "error[E0308]: mismatched types\n --> src/main.rs:2:18"
        );
    }

    #[test]
    fn render_keeps_the_colors_cargo_produced() {
        let line = compiler_message("error", "boom", None, "\\u001b[91merror\\u001b[0m: boom");
        let rendered = diagnostic(&line).render();

        assert!(rendered.contains('\u{1b}'));
        assert_eq!(strip_ansi(&rendered), "error: boom");
    }

    #[test]
    fn render_falls_back_to_the_headline_without_a_rendering() {
        let line = r#"{"reason":"compiler-message","message":{"level":"error","message":"mismatched types","code":{"code":"E0308"},"spans":[]}}"#;
        assert_eq!(diagnostic(line).render(), "error[E0308]: mismatched types");
    }

    #[test]
    fn render_falls_back_when_the_rendering_is_blank() {
        let line = compiler_message("warning", "unused", None, "   ");
        assert_eq!(diagnostic(&line).render(), "warning: unused");
    }

    #[test]
    fn render_lines_splits_the_rendering_into_console_lines() {
        let line = compiler_message("error", "m", None, "first\\nsecond\\nthird");
        assert_eq!(
            diagnostic(&line).render_lines(),
            vec!["first", "second", "third"]
        );
    }

    // ── ErrorReport ──────────────────────────────────────────────────

    /// The report block as plain text lines, at a fixed width so the assertions
    /// don't depend on the terminal the tests happen to run in.
    fn report_lines(report: &ErrorReport) -> Vec<String> {
        report
            .lines_with_width(MIN_WIDTH)
            .iter()
            .map(|l| strip_ansi(l))
            .collect()
    }

    /// The report block at `width`, as plain text lines.
    fn report_lines_at(report: &ErrorReport, width: usize) -> Vec<String> {
        report
            .lines_with_width(width)
            .iter()
            .map(|l| strip_ansi(l))
            .collect()
    }

    #[test]
    fn error_report_starts_empty() {
        let report = ErrorReport::new();
        assert!(report.is_empty());
        assert!(report.lines().is_empty());
    }

    #[test]
    fn error_report_ignores_everything_that_is_not_an_error() {
        let mut report = ErrorReport::new();
        for level in [
            DiagnosticLevel::Warning,
            DiagnosticLevel::Note,
            DiagnosticLevel::Help,
            DiagnosticLevel::FailureNote,
        ] {
            let mut diagnostic = error_with("m", "E0001");
            diagnostic.level = level;
            report.record(&diagnostic);
        }

        assert!(report.is_empty());
        assert!(report.lines().is_empty());
    }

    #[test]
    fn error_report_of_one_error_is_framed_and_unseparated() {
        let mut report = ErrorReport::new();
        report.record(&error_with("mismatched types", "E0308"));

        let lines = report_lines(&report);
        assert!(!report.is_empty());
        assert!(lines[1].contains("Compile Error"));
        assert!(lines[1].starts_with('='));
        // The rendering cargo produced is replayed as is, padded to the panel.
        assert!(
            lines
                .iter()
                .any(|l| l.trim_end() == "error[E0308]: mismatched types")
        );
        assert!(lines.last().unwrap().contains("1 error"));
        // A single error needs no rule.
        assert!(!lines.iter().any(|l| l.starts_with('~')));
    }

    #[test]
    fn error_report_does_not_label_its_errors() {
        // Cargo's own rendering states level, code and location already.
        let mut report = ErrorReport::new();
        report.record(&error_with("mismatched types", "E0308"));
        report.record(&error_with("cannot find function", "E0425"));

        assert!(!report_lines(&report).iter().any(|l| l.contains("Error 1")));
    }

    #[test]
    fn error_report_separates_several_errors_with_a_wave_rule() {
        let mut report = ErrorReport::new();
        report.record(&error_with("mismatched types", "E0308"));
        report.record(&error_with("cannot find function", "E0425"));

        let lines = report_lines(&report);
        assert_eq!(
            lines.iter().filter(|l| l.starts_with("~~~")).count(),
            1,
            "one rule between two errors"
        );
        assert!(lines.last().unwrap().contains("2 errors"));
    }

    #[test]
    fn error_report_rule_is_dimmer_than_the_headers() {
        let mut report = ErrorReport::new();
        report.record(&error_with("a", "E0001"));
        report.record(&error_with("b", "E0002"));

        let (r, g, b) = DIVIDER_COLOR;
        let rule = report
            .lines()
            .into_iter()
            .find(|l| strip_ansi(l).starts_with('~'))
            .expect("a wave rule");
        assert!(rule.contains(&format!("38;2;{r};{g};{b}")), "{rule:?}");
    }

    #[test]
    fn error_report_paints_every_error_line_on_the_panel() {
        let mut report = ErrorReport::new();
        report.record(&error_with("mismatched types", "E0308"));

        let (r, g, b) = PANEL_BACKGROUND;
        let background = format!("48;2;{r};{g};{b}");
        let panelled = report
            .lines()
            .into_iter()
            .filter(|l| l.contains(&background))
            .count();
        // Both rendered lines plus the blank row above and below.
        assert_eq!(panelled, 4);
    }

    #[test]
    fn error_report_rearms_the_panel_after_a_reset() {
        // Cargo resets colors mid-line; without re-arming, the background would
        // stop at the first reset.
        let mut error = error_with("m", "E0001");
        error.rendered = Some("\x1b[31merror\x1b[0m: m".to_string());
        let mut report = ErrorReport::new();
        report.record(&error);

        let (r, g, b) = PANEL_BACKGROUND;
        let background = format!("\x1b[48;2;{r};{g};{b}m");
        let line = report
            .lines()
            .into_iter()
            .find(|l| strip_ansi(l).starts_with("error"))
            .expect("the rendered line");
        assert_eq!(line.matches(&background).count(), 2, "{line:?}");
    }

    #[test]
    fn error_report_pads_every_panel_line_to_one_width() {
        let mut error = error_with("m", "E0001");
        error.rendered = Some("short\na much longer rendered line".to_string());
        let mut report = ErrorReport::new();
        report.record(&error);

        let widths: Vec<usize> = report_lines(&report)
            .iter()
            .filter(|l| !l.is_empty() && !l.starts_with('='))
            .map(|l| l.chars().count())
            .collect();
        assert!(widths.iter().all(|w| *w == MIN_WIDTH), "{widths:?}");
    }

    #[test]
    fn error_report_fills_the_width_it_is_given() {
        // The block spans the whole Build Logs pane rather than a fixed 58
        // columns, so every rule and panel row grows with the width.
        let mut report = ErrorReport::new();
        report.record(&error_with("a", "E0001"));
        report.record(&error_with("b", "E0002"));

        let width = 120;
        for line in report_lines_at(&report, width)
            .iter()
            .filter(|l| !l.is_empty())
        {
            assert_eq!(line.chars().count(), width, "{line:?}");
        }
    }

    #[test]
    fn error_report_never_goes_below_the_minimum_width() {
        let mut report = ErrorReport::new();
        report.record(&error_with("m", "E0001"));

        for line in report_lines_at(&report, 0).iter().filter(|l| !l.is_empty()) {
            assert_eq!(line.chars().count(), MIN_WIDTH, "{line:?}");
        }
    }

    #[test]
    fn error_report_keeps_a_rendering_wider_than_the_panel() {
        // Truncating cargo's carets would make the diagnostic unreadable.
        let long = "x".repeat(MIN_WIDTH + 20);
        let mut error = error_with("m", "E0001");
        error.rendered = Some(long.clone());
        let mut report = ErrorReport::new();
        report.record(&error);

        assert!(report_lines(&report).iter().any(|l| l.trim_end() == long));
    }

    #[test]
    fn error_report_keeps_the_order_the_errors_arrived_in() {
        let mut report = ErrorReport::new();
        report.record(&error_with("first", "E0001"));
        report.record(&error_with("second", "E0002"));
        report.record(&error_with("third", "E0003"));

        let lines = report_lines(&report);
        let position = |needle: &str| {
            lines
                .iter()
                .position(|l| l.contains(needle))
                .expect("error listed")
        };
        assert!(position("first") < position("second"));
        assert!(position("second") < position("third"));
        assert!(lines.last().unwrap().contains("3 errors"));
    }

    #[test]
    fn error_report_opens_with_a_blank_line() {
        // The block must not stick to the build output above it.
        let mut report = ErrorReport::new();
        report.record(&error_with("m", "E0001"));
        assert_eq!(report.lines()[0], "");
    }

    #[test]
    fn error_report_rules_are_all_the_same_width() {
        let mut report = ErrorReport::new();
        report.record(&error_with("a", "E0001"));
        report.record(&error_with("b", "E0002"));

        for line in report_lines(&report)
            .iter()
            .filter(|l| l.starts_with('=') || l.starts_with('~'))
        {
            assert_eq!(line.chars().count(), MIN_WIDTH, "{line:?}");
        }
    }
}
