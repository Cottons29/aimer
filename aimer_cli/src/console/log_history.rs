//! Capped log storage that keeps its rendered rows ready for the renderer.
//!
//! The console redraws several times a second and a running app can produce
//! thousands of log lines a second — a real iOS device streamed through
//! `devicectl --console` is the worst case. Parsing the ANSI escapes of the
//! whole backlog on every frame therefore costs more than the frame budget, and
//! a frame that overruns its budget is a frame during which the terminal input
//! queue keeps growing.
//!
//! [`LogHistory`] parses each entry exactly once, when it is pushed, and hands
//! the renderer a ready slice of [`Line`]s.

use std::borrow::Cow;

use ansi_to_tui::IntoText;
use ratatui::text::Line;

use crate::console::state::strip_ansi;

/// Parse `text` into the rows the renderer draws.
///
/// Text that is not valid ANSI falls back to its escape-stripped form rather
/// than being dropped, and an entry always occupies at least one row so blank
/// lines keep their place in the pane.
fn parse_rows(text: &str) -> Vec<Line<'static>> {
    let mut rows = text
        .into_text()
        .map(|parsed| parsed.lines)
        .unwrap_or_else(|_| vec![Line::from(strip_ansi(text))]);
    if rows.is_empty() {
        rows.push(Line::default());
    }
    rows
}

/// A bounded log history holding both the raw entries and their parsed rows.
///
/// Entries are dropped oldest-first once the history exceeds its capacity. The
/// drop happens in batches ([`TRIM_TO`] of the capacity at a time) so pushing
/// into a full history stays amortised O(1) instead of shifting the whole
/// backlog down by one on every line.
pub struct LogHistory<T> {
    entries: Vec<T>,
    /// Parsed rows of every entry, in order and flattened.
    rows: Vec<Line<'static>>,
    /// Rows contributed by each entry, parallel to `entries`.
    row_counts: Vec<usize>,
    cap: usize,
}

/// Fraction of the capacity kept when a full history is trimmed.
const TRIM_TO: f32 = 0.75;

impl<T> LogHistory<T> {
    /// An empty history holding at most `cap` entries.
    #[inline]
    pub fn new(cap: usize) -> Self {
        Self {
            entries: Vec::with_capacity(128),
            rows: Vec::with_capacity(128),
            row_counts: Vec::with_capacity(128),
            cap: cap.max(1),
        }
    }

    /// Append `entry`, parsing the text `render` produces for it.
    pub fn push(&mut self, entry: T, render: impl for<'a> Fn(&'a T) -> Cow<'a, str>) {
        let rows = parse_rows(render(&entry).as_ref());
        self.row_counts.push(rows.len());
        self.rows.extend(rows);
        self.entries.push(entry);
        self.trim();
    }

    /// Re-render every entry, e.g. after a display toggle changed what the
    /// lines should say. O(n) — only worth calling on an explicit user action.
    pub fn rebuild(&mut self, render: impl for<'a> Fn(&'a T) -> Cow<'a, str>) {
        self.rows.clear();
        self.row_counts.clear();
        for entry in &self.entries {
            let rows = parse_rows(render(entry).as_ref());
            self.row_counts.push(rows.len());
            self.rows.extend(rows);
        }
    }

    /// Drop the oldest entries once the capacity is exceeded.
    fn trim(&mut self) {
        if self.entries.len() <= self.cap {
            return;
        }
        let keep = ((self.cap as f32 * TRIM_TO) as usize).max(1);
        let drop = self.entries.len() - keep;
        let dropped_rows: usize = self.row_counts[..drop].iter().sum();
        self.entries.drain(..drop);
        self.row_counts.drain(..drop);
        self.rows.drain(..dropped_rows);
    }

    /// Forget every entry.
    #[inline]
    pub fn clear(&mut self) {
        self.entries.clear();
        self.rows.clear();
        self.row_counts.clear();
    }

    /// The raw entries, oldest first.
    #[inline]
    pub fn entries(&self) -> &[T] {
        &self.entries
    }

    /// The parsed rows of every entry, ready to render.
    #[inline]
    pub fn rows(&self) -> &[Line<'static>] {
        &self.rows
    }

    /// Number of entries currently held.
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the history holds no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(history: &mut LogHistory<String>, text: &str) {
        history.push(text.to_string(), |s| Cow::Borrowed(s.as_str()));
    }

    fn row_texts(history: &LogHistory<String>) -> Vec<String> {
        history
            .rows()
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn a_new_history_is_empty() {
        let history: LogHistory<String> = LogHistory::new(8);
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
        assert!(history.rows().is_empty());
    }

    #[test]
    fn pushed_entries_are_kept_in_order() {
        let mut history = LogHistory::new(8);
        plain(&mut history, "first");
        plain(&mut history, "second");

        assert_eq!(
            history.entries(),
            ["first".to_string(), "second".to_string()]
        );
        assert_eq!(row_texts(&history), vec!["first", "second"]);
    }

    #[test]
    fn rows_are_parsed_once_at_push_time() {
        let mut history = LogHistory::new(8);
        plain(&mut history, "\x1b[31mred\x1b[0m");

        // The escapes are gone from the text and became a style instead.
        assert_eq!(row_texts(&history), vec!["red"]);
        assert!(history.rows()[0].spans.iter().any(|s| s.style.fg.is_some()));
    }

    #[test]
    fn a_blank_entry_still_occupies_a_row() {
        let mut history = LogHistory::new(8);
        plain(&mut history, "");
        assert_eq!(history.rows().len(), 1);
    }

    #[test]
    fn a_multi_line_entry_contributes_every_row() {
        let mut history = LogHistory::new(8);
        plain(&mut history, "a\nb\nc");

        assert_eq!(history.len(), 1);
        assert_eq!(row_texts(&history), vec!["a", "b", "c"]);
    }

    #[test]
    fn the_capacity_is_never_exceeded() {
        let mut history = LogHistory::new(16);
        for i in 0..200 {
            plain(&mut history, &format!("line {i}"));
        }

        assert!(history.len() <= 16, "held {} entries", history.len());
        assert_eq!(history.entries().last().unwrap(), "line 199");
    }

    #[test]
    fn trimming_drops_the_rows_of_the_dropped_entries() {
        let mut history = LogHistory::new(4);
        for i in 0..40 {
            // Two rows per entry, so a mismatched trim would show up here.
            plain(&mut history, &format!("line {i}\ncont {i}"));
        }

        assert_eq!(history.rows().len(), history.len() * 2);
        assert_eq!(row_texts(&history).last().unwrap(), "cont 39");
    }

    #[test]
    fn trimming_keeps_the_oldest_surviving_entry_intact() {
        let mut history = LogHistory::new(4);
        for i in 0..5 {
            plain(&mut history, &format!("line {i}"));
        }

        let first = history.entries().first().unwrap().clone();
        assert_eq!(row_texts(&history).first().unwrap(), &first);
    }

    #[test]
    fn a_zero_capacity_still_keeps_the_latest_entry() {
        let mut history = LogHistory::new(0);
        plain(&mut history, "only");

        assert_eq!(history.len(), 1);
        assert_eq!(row_texts(&history), vec!["only"]);
    }

    #[test]
    fn rebuild_re_renders_every_entry() {
        let mut history = LogHistory::new(8);
        plain(&mut history, "a");
        plain(&mut history, "b");

        history.rebuild(|s| Cow::Owned(format!("{s}!")));

        assert_eq!(row_texts(&history), vec!["a!", "b!"]);
        assert_eq!(history.entries(), ["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn clear_drops_entries_and_rows() {
        let mut history = LogHistory::new(8);
        plain(&mut history, "a");
        history.clear();

        assert!(history.is_empty());
        assert!(history.rows().is_empty());
    }
}
