//! The framed block a recovered widget panic is shown as.
//!
//! `aimer_widget` catches a panic raised while a widget builds, turns it into an
//! error element and logs it, so the app keeps running instead of dying. The log
//! record it writes carries more than one line: the headline, the `file:line:col`
//! the panic came from, and the source line with its carets — the same shape
//! rustc uses. Left as plain app log lines that block would scroll past
//! unnoticed among the app's own output.
//!
//! [`PanicReport`] gives it the same treatment a failed build gets: the framed,
//! width-filling block of [`framed_block`](super::framed_block), so the panic is
//! impossible to miss and stays readable when the pane is resized.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use colored::Colorize;

use crate::commands::run::framed_block::{block_width, header, panel, resolve_width};
use crate::commands::run::utilities::{LogLevel, LogRecord};

/// Titles the block header is drawn with, picked at random for every panic —
/// crashing is grim enough without the console being grim about it too.
pub const TITLES: [&str; 5] = [
    "Oop! Panicked",
    "I have Panicked",
    "Panic is Appear",
    "Oh no, Panicked!",
    "Panicked Moew ^_^",
];

/// The headline of a recovered build panic always names the widget it came from.
const HEADLINE_PREFIX: &str = "Widget `";

/// … and the phase it panicked in.
const HEADLINE_MARKER: &str = "` panicked during ";

/// One recovered widget panic, ready to be laid out for the App Logs pane.
///
/// The report keeps the panic as text rather than as rendered rows: the pane it
/// lands in can be resized, and a block laid out for another width would be
/// wrapped by the renderer and lose its panel background. The title is drawn once
/// here, when the panic arrives, so re-laying the block out doesn't shuffle it.
///
/// # Examples
///
/// ```ignore
/// let record = LogRecord::parse(
///     r#"{"__aimer":1,"level":"error","message":"Widget `Button` panicked during build: boom"}"#,
/// )
/// .expect("a log record");
/// let report = PanicReport::of(&record).expect("a panic");
///
/// assert!(report.lines().iter().any(|line| line.contains("boom")));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanicReport {
    /// Header title of this block, drawn when the panic arrived.
    title: &'static str,
    /// The `Widget `X` panicked during build: …` line.
    headline: String,
    /// Everything below it — the location, the source line, its carets, and a
    /// backtrace when the app was run with one.
    body: Vec<String>,
}

impl PanicReport {
    /// Reads `record` as a recovered widget panic, or `None` when it is an
    /// ordinary log event.
    ///
    /// Only an error whose headline names the widget and the build phase it
    /// panicked in is a panic; anything else — including an error the app logged
    /// itself — stays an ordinary app log line.
    pub fn of(record: &LogRecord) -> Option<Self> {
        if record.level != LogLevel::Error {
            return None;
        }
        let mut lines = record.message.lines();
        let headline = lines.next()?;
        if !headline.starts_with(HEADLINE_PREFIX) || !headline.contains(HEADLINE_MARKER) {
            return None;
        }

        let mut body: Vec<String> = lines.map(str::to_owned).collect();
        while body.first().is_some_and(|line| line.trim().is_empty()) {
            body.remove(0);
        }
        while body.last().is_some_and(|line| line.trim().is_empty()) {
            body.pop();
        }

        Some(Self {
            title: random_title(),
            headline: headline.to_owned(),
            body,
        })
    }

    /// The block as console lines, laid out for the current terminal — see
    /// [`lines_with_width`](Self::lines_with_width).
    #[inline]
    pub fn lines(&self) -> Vec<String> {
        self.lines_with_width(block_width())
    }

    /// The block as console lines `width` cells wide: a framed header, the panic
    /// on one light-red panel, and a footer rule.
    ///
    /// A line wider than `width` is wrapped rather than truncated — cutting the
    /// carets off would hide the very expression that panicked — and a `width` of
    /// zero falls back to the minimum, as for a compile error block.
    pub fn lines_with_width(&self, width: usize) -> Vec<String> {
        let width = resolve_width(width);
        let mut lines = vec![String::new(), header(self.title, width)];
        lines.extend(panel(&self.rendered(), width));
        lines.push(header("1 panic", width));
        lines
    }

    /// The panic as colored lines, before they are laid out on a panel.
    ///
    /// The text is the app's own: the framework already formats the location and
    /// the caret run, so only the colors are added here.
    fn rendered(&self) -> Vec<String> {
        let mut lines = Vec::with_capacity(self.body.len() + 1);
        lines.push(format!(
            "{}{}",
            "Panicked".red().bold(),
            format!(": {}", self.headline).bold()
        ));
        lines.push(String::new());
        lines.extend(self.body.iter().map(|line| colorize_body_line(line)));
        lines
    }
}

/// Color one line below the headline the way rustc colors its own: the location
/// blue, the carets red, everything else as it came.
fn colorize_body_line(line: &str) -> String {
    if line.starts_with("at ") {
        return line.bright_blue().to_string();
    }
    let trimmed = line.trim();
    if !trimmed.is_empty() && trimmed.chars().all(|c| c == '^') {
        return line.red().bold().to_string();
    }
    line.to_owned()
}

/// One of the [`TITLES`], at random.
fn random_title() -> &'static str {
    TITLES[(next_random() % TITLES.len() as u64) as usize]
}

/// A xorshift64 draw, seeded from the clock on first use.
///
/// A panic banner is not worth a random number generator dependency, and the
/// only property asked of it is that consecutive panics don't all pick the same
/// title.
fn next_random() -> u64 {
    static STATE: AtomicU64 = AtomicU64::new(0);

    let mut state = STATE.load(Ordering::Relaxed);
    if state == 0 {
        state = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(0x2545_F491_4F6C_DD1D)
            | 1;
    }
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    STATE.store(state, Ordering::Relaxed);
    state
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::commands::run::framed_block::{MIN_WIDTH, visible_width};
    use crate::console::state::strip_ansi;

    /// The record `aimer_widget` writes for a recovered build panic.
    fn panic_record() -> LogRecord {
        LogRecord {
            level: LogLevel::Error,
            message: concat!(
                "Widget `HttpRequestButton` panicked during build: called `Option::unwrap()` ",
                "on a `None` value\n",
                "\n",
                "at jaime/src/http_request_button.rs:117:67\n",
                "\n",
                "        let panic: Option<i32> = Option::None.unwrap();\n",
                "                                 ^^^^^^^^^^^^^^^^^^^^^\n",
            )
            .to_string(),
            file: Some("crates/aimer_widget/src/widget/recovery.rs".to_string()),
            line: Some(66),
        }
    }

    fn plain_lines(report: &PanicReport, width: usize) -> Vec<String> {
        report
            .lines_with_width(width)
            .iter()
            .map(|line| strip_ansi(line))
            .collect()
    }

    // ── Recognising a panic ──────────────────────────────────────────

    #[test]
    fn a_recovered_widget_panic_is_recognised() {
        let report = PanicReport::of(&panic_record()).expect("a panic report");

        assert!(report.headline.contains("HttpRequestButton"));
        assert_eq!(
            report.body,
            vec![
                "at jaime/src/http_request_button.rs:117:67".to_string(),
                String::new(),
                "        let panic: Option<i32> = Option::None.unwrap();".to_string(),
                "                                 ^^^^^^^^^^^^^^^^^^^^^".to_string(),
            ]
        );
    }

    #[test]
    fn an_error_the_app_logged_itself_is_not_a_panic() {
        let record = LogRecord {
            level: LogLevel::Error,
            message: "request failed: connection refused".to_string(),
            file: None,
            line: None,
        };

        assert!(PanicReport::of(&record).is_none());
    }

    #[test]
    fn a_panic_shaped_message_below_error_level_is_not_a_panic() {
        let mut record = panic_record();
        record.level = LogLevel::Warn;

        assert!(PanicReport::of(&record).is_none());
    }

    #[test]
    fn a_headline_only_panic_still_reports() {
        let record = LogRecord {
            level: LogLevel::Error,
            message: "Widget `Button` panicked during build: boom".to_string(),
            file: None,
            line: None,
        };
        let report = PanicReport::of(&record).expect("a panic report");

        assert!(report.body.is_empty());
        assert!(plain_lines(&report, 80).iter().any(|l| l.contains("boom")));
    }

    // ── The block ────────────────────────────────────────────────────

    #[test]
    fn the_block_is_framed_by_a_random_title_and_a_footer() {
        let report = PanicReport::of(&panic_record()).expect("a panic report");
        let lines = plain_lines(&report, 80);

        assert!(
            TITLES.iter().any(|title| lines[1].contains(title)),
            "{:?}",
            lines[1]
        );
        assert!(lines[1].starts_with('='));
        assert!(lines.last().unwrap().contains("1 panic"));
    }

    #[test]
    fn the_block_keeps_the_location_and_the_carets() {
        let report = PanicReport::of(&panic_record()).expect("a panic report");
        let lines = plain_lines(&report, 100);

        assert!(
            lines
                .iter()
                .any(|l| l.contains("at jaime/src/http_request_button.rs:117:67"))
        );
        assert!(lines.iter().any(|l| l.contains("^^^^^^^^^^^^^^^^^^^^^")));
    }

    #[test]
    fn the_block_states_the_panic_before_the_message() {
        let report = PanicReport::of(&panic_record()).expect("a panic report");
        let lines = plain_lines(&report, 120);

        assert!(
            lines
                .iter()
                .any(|l| l.trim_start().starts_with("Panicked: Widget `HttpRequestButton`")),
            "{lines:?}"
        );
    }

    #[test]
    fn every_row_fills_the_width_it_is_given() {
        let report = PanicReport::of(&panic_record()).expect("a panic report");

        for width in [MIN_WIDTH, 80, 120] {
            for line in report
                .lines_with_width(width)
                .iter()
                .filter(|l| !l.is_empty())
            {
                assert_eq!(visible_width(line), width, "{width}: {line:?}");
            }
        }
    }

    #[test]
    fn an_unknown_width_falls_back_to_the_minimum() {
        let report = PanicReport::of(&panic_record()).expect("a panic report");

        for line in report.lines_with_width(0).iter().filter(|l| !l.is_empty()) {
            assert_eq!(visible_width(line), MIN_WIDTH, "{line:?}");
        }
    }

    #[test]
    fn a_line_wider_than_the_block_is_wrapped_rather_than_cut() {
        let mut record = panic_record();
        record.message = format!("Widget `W` panicked during build: {}", "x".repeat(200));
        let report = PanicReport::of(&record).expect("a panic report");

        let lines = report.lines_with_width(40);
        assert!(lines.iter().all(|l| visible_width(l) <= 40), "{lines:?}");
        assert_eq!(
            plain_lines(&report, 40)
                .iter()
                .map(|l| l.matches('x').count())
                .sum::<usize>(),
            200
        );
    }

    #[test]
    fn relaying_the_block_out_keeps_its_title() {
        // The pane re-lays the block out on every resize; a title drawn per
        // layout would flicker through the list instead of naming this panic.
        let report = PanicReport::of(&panic_record()).expect("a panic report");

        let narrow = plain_lines(&report, 60).remove(1);
        let wide = plain_lines(&report, 100).remove(1);

        let title = TITLES
            .iter()
            .find(|title| narrow.contains(*title))
            .expect("a known title");
        assert!(wide.contains(title), "{wide:?}");
    }

    // ── The random title ─────────────────────────────────────────────

    #[test]
    fn every_title_can_be_drawn() {
        let drawn: HashSet<&str> = (0..2_000).map(|_| random_title()).collect();

        assert_eq!(drawn.len(), TITLES.len(), "{drawn:?}");
    }

    #[test]
    fn consecutive_panics_do_not_all_get_the_same_title() {
        let titles: HashSet<&str> = (0..50)
            .map(|_| PanicReport::of(&panic_record()).expect("a panic report").title)
            .collect();

        assert!(titles.len() > 1, "{titles:?}");
    }
}
