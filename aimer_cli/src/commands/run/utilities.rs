use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

use colored::Colorize;
use crossterm::style::Stylize;
use serde::Deserialize;

use crate::commands::run::panic_report::PanicReport;

pub trait LogStyling {
    fn process_log(self) -> StyledLog;

    fn process_app_output(self) -> AppOutput;
}

/// What one line of application output turned out to be.
///
/// Almost everything is a log line, ready to be drawn. The exception is a widget
/// panic the framework recovered from: it is reported as a whole block — the
/// headline, where it happened and the source line that panicked — and is kept
/// as a [`PanicReport`] so the pane can lay that block out for its own width.
#[derive(Clone, Debug)]
pub enum AppOutput {
    /// An ordinary line of application output.
    Log(StyledLog),
    /// A widget panic the framework recovered from.
    Panic(PanicReport),
}

/// One line of application output, ready to be drawn, with the source location
/// of the log call kept apart from the message.
///
/// Both parts are already colored by the reader thread that produced them, so
/// the console only concatenates them. Keeping the location separate is what
/// lets the console show or hide it at any time — the `e` key toggles it —
/// without re-parsing the record it came from.
///
/// Output that is not a log record — a plain `println!`, build noise — has no
/// location and is therefore unaffected by the toggle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyledLog {
    text: String,
    location: Option<String>,
}

impl StyledLog {
    /// A line without a source location, shown as-is whatever the toggle says.
    #[inline]
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            location: None,
        }
    }

    /// A line whose `location` suffix — including its leading separator — is
    /// only appended while the source location is shown.
    #[inline]
    pub fn with_location(text: impl Into<String>, location: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            location: Some(location.into()),
        }
    }

    /// Whether this line carries a source location that can be toggled.
    #[inline]
    pub fn has_location(&self) -> bool {
        self.location.is_some()
    }

    /// The line as it must appear on screen.
    ///
    /// The source location is appended only when `show_location` is `true` and
    /// the producer supplied one.
    pub fn render(&self, show_location: bool) -> String {
        match &self.location {
            Some(location) if show_location => {
                let mut out = String::with_capacity(self.text.len() + location.len());
                out.push_str(&self.text);
                out.push_str(location);
                out
            }
            _ => self.text.clone(),
        }
    }

    /// Drop the carriage returns a terminal application may use to redraw a
    /// line in place, which would otherwise confuse the pane layout.
    #[inline]
    pub fn without_carriage_returns(mut self) -> Self {
        self.text = self.text.replace('\r', "");
        self.location = self.location.map(|l| l.replace('\r', ""));
        self
    }
}

impl From<String> for StyledLog {
    #[inline]
    fn from(text: String) -> Self {
        Self::plain(text)
    }
}

impl From<&str> for StyledLog {
    #[inline]
    fn from(text: &str) -> Self {
        Self::plain(text)
    }
}

/// Severity of a structured log record emitted by the running application.
///
/// The names match the `level` field written by `aimer_utils::log` when the app
/// runs with [`JSON_OUTPUT_FLAG`](aimer_utils::log::JSON_OUTPUT_FLAG); any other
/// value makes the record fail to parse, so the line is passed through as plain
/// application output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl LogLevel {
    /// Label shown in front of the message, without the surrounding brackets.
    #[inline]
    pub const fn label(self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Debug => "DEBUG",
        }
    }

    /// Apply this level's color to an already assembled console line.
    #[inline]
    fn colorize(self, line: String) -> String {
        match self {
            LogLevel::Info => line.bright_cyan().to_string(),
            LogLevel::Warn => line.yellow().to_string(),
            LogLevel::Error => line.red().to_string(),
            LogLevel::Debug => line.green().to_string(),
        }
    }
}

/// One log event produced by `aimer_utils::log` in JSON mode.
///
/// The application writes one compact JSON object per line, which lets the
/// console style the event from its real severity instead of guessing it from
/// the text. `file` and `line` are optional so a record stays usable even when
/// the producer cannot supply a caller location.
///
/// # Examples
///
/// ```ignore
/// let record = LogRecord::parse(
///     r#"{"__aimer":1,"level":"warn","message":"low memory","file":"src/main.rs","line":7}"#,
/// )
/// .expect("valid record");
/// assert_eq!(record.level, LogLevel::Warn);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LogRecord {
    pub level: LogLevel,
    pub message: String,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub line: Option<u32>,
}

impl LogRecord {
    /// Try to read one line of application output as a JSON log record.
    ///
    /// Returns `None` for anything that is not a JSON object with a known
    /// `level` and a `message` — plain `println!` output, build noise or JSON
    /// the application prints for its own purposes — so callers can forward the
    /// line unchanged.
    pub fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            return None;
        }
        serde_json::from_str(trimmed).ok()
    }

    /// The `" (file:line)"` suffix naming the source of the log call, or `None`
    /// when the producer could not supply a location.
    pub fn location(&self) -> Option<String> {
        let file = self.file.as_deref()?;
        Some(match self.line {
            Some(number) => format!(" ({}:{})", file, number),
            None => format!(" ({})", file),
        })
    }

    /// Render the record as a colored console line whose source location can
    /// still be hidden later.
    pub fn style(&self) -> StyledLog {
        let text = self
            .level
            .colorize(format!("[{}] {}", self.level.label(), self.message));
        match self.location() {
            Some(location) => StyledLog::with_location(text, self.level.colorize(location)),
            None => StyledLog::plain(text),
        }
    }
}

pub fn get_project_root(allow_workspace: bool) -> Result<PathBuf, Box<dyn Error>> {
    let mut command = Command::new("cargo");
    command.args(["locate-project", "--message-format=plain"]);

    if allow_workspace {
        command.arg("--workspace");
    }

    let output = command.output()?;
    if !output.status.success() {
        return Err("cargo locate-project failed".into());
    }
    let cargo_toml = String::from_utf8(output.stdout)?;
    let root = PathBuf::from(cargo_toml.trim())
        .parent()
        .ok_or("Failed to get workspace root")?
        .to_path_buf();

    Ok(root)
}

impl LogStyling for String {
    /// Style one line of application output.
    ///
    /// A JSON log record is styled from its declared level and keeps its source
    /// location toggleable; everything else falls back to detecting the level in
    /// the text, and is returned unchanged when no level can be recognised — so
    /// raw `println!` output reaches the console verbatim.
    fn process_log(self) -> StyledLog {
        if let Some(record) = LogRecord::parse(&self) {
            return record.style();
        }
        style_plain(self)
    }

    /// Style one line of application output, telling a recovered widget panic
    /// apart from an ordinary log line.
    ///
    /// The record is parsed once for both answers, so recognising a panic costs
    /// the reader thread nothing on the lines that aren't one.
    fn process_app_output(self) -> AppOutput {
        match LogRecord::parse(&self) {
            Some(record) => match PanicReport::of(&record) {
                Some(report) => AppOutput::Panic(report),
                None => AppOutput::Log(record.style()),
            },
            None => AppOutput::Log(style_plain(self)),
        }
    }
}

/// Style a line that is not a log record from the level it mentions, and leave it
/// alone when it mentions none — raw `println!` output reaches the console
/// verbatim.
fn style_plain(line: String) -> StyledLog {
    StyledLog::plain(if line.contains("[ERROR]") {
        line.red().to_string()
    } else if line.contains("[WARN]") {
        line.yellow().to_string()
    } else if line.contains("[DEBUG]") || line.contains("hot-reload") {
        line.green().to_string()
    } else if line.contains("[INFO]") {
        line.bright_cyan().to_string()
    } else {
        line
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Style `line` the way a reader thread does and render it with the source
    /// location shown, which is the console default.
    fn styled(line: &str) -> String {
        line.to_string().process_log().render(true)
    }

    #[test]
    fn process_log_error_contains_original_text() {
        let result = styled("[ERROR] something broke");
        assert!(result.contains("[ERROR] something broke"));
    }

    #[test]
    fn process_log_warn_contains_original_text() {
        let result = styled("[WARN] be careful");
        assert!(result.contains("[WARN] be careful"));
    }
    #[test]
    fn process_log_debug_contains_original_text() {
        let result = styled("[DEBUG] trace info");
        assert!(result.contains("[DEBUG] trace info"));
    }

    #[test]
    fn process_log_hot_reload_contains_original_text() {
        let result = styled("hot-reload triggered");
        assert!(result.contains("hot-reload triggered"));
    }

    #[test]
    fn process_log_info_contains_original_text() {
        let result = styled("[INFO] all good");
        assert!(result.contains("[INFO] all good"));
    }

    #[test]
    fn process_log_plain_text_unchanged() {
        let input = "just a normal message";
        assert_eq!(styled(input), input);
    }

    #[test]
    fn process_log_empty_string() {
        assert_eq!(styled(""), "");
    }

    #[test]
    fn process_log_of_plain_output_has_no_toggleable_location() {
        // A raw `println!` must look the same whether or not source locations
        // are shown.
        let styled = String::from("just a normal message").process_log();
        assert!(!styled.has_location());
        assert_eq!(styled.render(true), styled.render(false));
    }

    #[test]
    fn process_app_output_reports_a_recovered_widget_panic() {
        let line = concat!(
            r#"{"__aimer":1,"level":"error","message":"Widget `Button` panicked during build: "#,
            r#"boom\n\nat app/src/main.rs:7:9","file":"recovery.rs","line":66}"#,
        )
        .to_string();

        let AppOutput::Panic(report) = line.process_app_output() else {
            panic!("a recovered panic must not be shown as an ordinary log line");
        };
        assert!(report.lines_with_width(80).iter().any(|l| l.contains("boom")));
    }

    #[test]
    fn process_app_output_keeps_an_ordinary_error_a_log_line() {
        let line =
            r#"{"__aimer":1,"level":"error","message":"request failed","line":7}"#.to_string();

        let AppOutput::Log(styled) = line.process_app_output() else {
            panic!("an error the app logged itself is not a panic");
        };
        assert!(styled.render(true).contains("[ERROR] request failed"));
    }

    #[test]
    fn process_app_output_forwards_plain_output_as_a_log_line() {
        let AppOutput::Log(styled) = "just a normal message".to_string().process_app_output() else {
            panic!("plain output is not a panic");
        };
        assert_eq!(styled.render(true), "just a normal message");
    }

    #[test]
    fn process_log_formats_json_info_record() {
        let result = styled(
            r#"{"level":"info","message":"application started","file":"src/main.rs","line":12}"#,
        );

        assert!(result.contains("[INFO] application started"));
        assert!(!result.contains(r#""level""#));
    }

    #[test]
    fn process_log_formats_every_json_level_differently() {
        let record = |level: &str| {
            styled(&format!(
                r#"{{"level":"{level}","message":"same","file":"src/main.rs","line":1}}"#
            ))
        };

        let info = record("info");
        let warn = record("warn");
        let error = record("error");
        let debug = record("debug");

        assert!(info.contains("[INFO] same"));
        assert!(warn.contains("[WARN] same"));
        assert!(error.contains("[ERROR] same"));
        assert!(debug.contains("[DEBUG] same"));
        assert_ne!(info, warn);
        assert_ne!(warn, error);
        assert_ne!(error, debug);
    }

    #[test]
    fn process_log_preserves_message_escaping_from_json() {
        let result = styled(
            r#"{"level":"error","message":"quoted: \"value\"","file":"src/main.rs","line":3}"#,
        );

        assert!(result.contains("[ERROR] quoted: \"value\""));
    }

    #[test]
    fn process_log_keeps_the_json_location_toggleable() {
        let styled =
            String::from(r#"{"level":"info","message":"ready","file":"src/main.rs","line":12}"#)
                .process_log();

        assert!(styled.has_location());
        assert!(styled.render(true).contains("(src/main.rs:12)"));
        assert!(!styled.render(false).contains("src/main.rs"));
        assert!(styled.render(false).contains("[INFO] ready"));
    }

    // ── StyledLog ────────────────────────────────────────────────────

    #[test]
    fn styled_log_plain_ignores_the_toggle() {
        let line = StyledLog::plain("raw output");
        assert!(!line.has_location());
        assert_eq!(line.render(true), "raw output");
        assert_eq!(line.render(false), "raw output");
    }

    #[test]
    fn styled_log_appends_the_location_only_when_shown() {
        let line = StyledLog::with_location("[INFO] ready", " (src/main.rs:1)");
        assert!(line.has_location());
        assert_eq!(line.render(true), "[INFO] ready (src/main.rs:1)");
        assert_eq!(line.render(false), "[INFO] ready");
    }

    #[test]
    fn styled_log_from_string_is_plain() {
        assert_eq!(StyledLog::from("x".to_string()), StyledLog::plain("x"));
        assert_eq!(StyledLog::from("x"), StyledLog::plain("x"));
    }

    #[test]
    fn styled_log_strips_carriage_returns_from_both_parts() {
        let line = StyledLog::with_location("a\rb", "\r (f:1)").without_carriage_returns();
        assert_eq!(line.render(true), "ab (f:1)");
    }

    #[test]
    fn log_record_location_is_a_suffix_with_its_separator() {
        let record = LogRecord {
            level: LogLevel::Info,
            message: "m".to_string(),
            file: Some("src/lib.rs".to_string()),
            line: Some(9),
        };
        assert_eq!(record.location().as_deref(), Some(" (src/lib.rs:9)"));
    }

    #[test]
    fn log_record_location_without_a_line_number() {
        let record = LogRecord {
            level: LogLevel::Info,
            message: "m".to_string(),
            file: Some("src/lib.rs".to_string()),
            line: None,
        };
        assert_eq!(record.location().as_deref(), Some(" (src/lib.rs)"));
    }

    #[test]
    fn log_record_location_is_none_without_a_file() {
        let record = LogRecord {
            level: LogLevel::Info,
            message: "m".to_string(),
            file: None,
            line: Some(9),
        };
        assert!(record.location().is_none());
    }

    #[test]
    fn log_record_parse_accepts_the_logger_record() {
        let record = LogRecord::parse(
            r#"{"__aimer":1,"level":"warn","message":"low memory","file":"src/main.rs","line":7}"#,
        )
        .expect("record should parse");

        assert_eq!(record.level, LogLevel::Warn);
        assert_eq!(record.message, "low memory");
        assert_eq!(record.file.as_deref(), Some("src/main.rs"));
        assert_eq!(record.line, Some(7));
    }

    #[test]
    fn log_record_parse_tolerates_a_missing_location() {
        let record =
            LogRecord::parse(r#"{"level":"debug","message":"tick"}"#).expect("record should parse");

        assert_eq!(record.level, LogLevel::Debug);
        assert!(record.file.is_none());
        assert!(record.line.is_none());
    }

    #[test]
    fn log_record_parse_ignores_surrounding_whitespace() {
        assert!(LogRecord::parse("  {\"level\":\"info\",\"message\":\"m\"}  ").is_some());
    }

    #[test]
    fn log_record_parse_rejects_non_records() {
        assert!(LogRecord::parse("just a println!").is_none());
        assert!(LogRecord::parse("").is_none());
        assert!(LogRecord::parse(r#"{"level":"info"}"#).is_none());
        assert!(LogRecord::parse(r#"["level","info"]"#).is_none());
        assert!(LogRecord::parse(r#"{"level":"verbose","message":"m"}"#).is_none());
    }

    #[test]
    fn log_record_style_keeps_the_caller_location() {
        let record = LogRecord {
            level: LogLevel::Error,
            message: "boom".to_string(),
            file: Some("src/main.rs".to_string()),
            line: Some(42),
        };

        let styled = record.style();
        assert!(styled.render(true).contains("(src/main.rs:42)"));
        assert!(styled.render(false).contains("[ERROR] boom"));
        assert!(!styled.render(false).contains("src/main.rs"));
    }

    #[test]
    fn log_record_style_omits_the_location_when_absent() {
        let record = LogRecord {
            level: LogLevel::Info,
            message: "ready".to_string(),
            file: None,
            line: None,
        };

        let styled = record.style();
        assert!(!styled.has_location());
        assert!(styled.render(true).contains("[INFO] ready"));
        assert!(!styled.render(true).contains('('));
    }

    #[test]
    fn process_log_leaves_invalid_json_unchanged() {
        let input = r#"{"level":"info","message":broken}"#;
        assert_eq!(styled(input), input);
    }

    #[test]
    fn process_log_leaves_unknown_json_level_unchanged() {
        let input = r#"{"level":"trace","message":"details","file":"src/main.rs","line":4}"#;
        assert_eq!(styled(input), input);
    }

    #[test]
    fn process_log_different_levels_produce_different_output() {
        let error = styled("[ERROR] bad");
        let warn = styled("[WARN] bad");
        let info = styled("[INFO] bad");
        let debug = styled("[DEBUG] bad");
        let plain = styled("bad");

        // Each branch should produce a distinct styled output
        assert_ne!(error, warn);
        assert_ne!(error, info);
        assert_ne!(error, debug);
        assert_ne!(error, plain);
        assert_ne!(warn, info);
        assert_ne!(warn, debug);
        assert_ne!(info, debug);
        assert_ne!(info, plain);
    }

    #[test]
    fn process_log_error_takes_priority_over_warn() {
        // "[ERROR] [WARN]" should hit the ERROR branch first (not WARN)
        let result = styled("[ERROR] [WARN] conflict");
        // Verify it goes through the error branch (red) by checking it's styled
        // and NOT plain text
        assert_ne!(result, "[ERROR] [WARN] conflict");
        // Also confirm a pure WARN is styled differently
        let warn_result = styled("[WARN] conflict");
        assert_ne!(result, warn_result);
    }

    #[test]
    fn process_log_hot_reload_in_middle() {
        let result = styled("something hot-reload happened");
        // hot-reload is in the DEBUG branch
        let debug = styled("[DEBUG] something");
        // Both go through the green branch — verify they're both styled
        assert_ne!(result, "something hot-reload happened");
        assert_ne!(debug, "[DEBUG] something");
    }
}
