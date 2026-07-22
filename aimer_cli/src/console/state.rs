use crate::commands::run::utilities::LogStyling;

/// Maximum number of log lines retained per pane before old lines are dropped.
pub const MAX_LINES: usize = 32768;

/// The runner lifecycle status shown in the status bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Locking,
    Fetching(u8),
    Compiling(u8),
    Building(u8),
    Launching,
    Running,
    Idling,
    Error,
}

/// Events sent from runner/build threads to the console event loop.
pub enum RunnerEvent {
    BuildLog(String),
    AppLog(String),
    StatusChange(Status),
    HotReload,
}

/// Which pane currently has focus in the console.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ConsoleType {
    App,
    Build,
    Inspector,
}

impl ConsoleType {
    /// Cycle to the next pane (App → Build → Inspector → App).
    pub fn next(&self) -> ConsoleType {
        match self {
            ConsoleType::App => ConsoleType::Build,
            ConsoleType::Build => ConsoleType::Inspector,
            ConsoleType::Inspector => ConsoleType::App,
        }
    }
}

/// Vertical scroll position for a single pane.
pub struct ScrollablePane {
    pub scroll: u16,
}

impl ScrollablePane {
    pub fn new() -> Self {
        Self { scroll: 0 }
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll = self
            .scroll
            .saturating_add(amount);
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll = self
            .scroll
            .saturating_sub(amount);
    }

    pub fn reset(&mut self) {
        self.scroll = 0;
    }
}

impl Default for ScrollablePane {
    fn default() -> Self {
        Self::new()
    }
}

/// Strip ANSI escape sequences and stray control characters from `s`,
/// preserving newlines and tabs.
pub fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_escape = false;
    for c in s.chars() {
        if in_escape {
            if c.is_ascii_alphabetic() || c == '@' || c == '~' {
                in_escape = false;
            }
        } else if c == '\x1B' {
            in_escape = true;
        } else if !c.is_control() || c == '\n' || c == '\t' {
            result.push(c);
        }
    }
    result
}

/// A single on-screen (already wrapped) row of a log pane, mapped back to the
/// source logical line it came from. Produced by the renderer so the mouse
/// handler can translate screen cells into text positions.
#[derive(Clone, Copy, Debug)]
pub struct VisualRow {
    /// Index of the source logical line in the pane's text.
    pub line: usize,
    /// Char offset within the source line where this visual row starts.
    pub start: usize,
    /// Number of chars displayed on this visual row.
    pub len: usize,
}

/// A character-range selection expressed as `(line, column)` anchor/cursor
/// positions into a pane's logical lines. Columns are char offsets. Modelled
/// after Vim's character-wise visual mode: both endpoints are inclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub anchor: (usize, usize),
    pub cursor: (usize, usize),
}

impl Selection {
    /// Return the `(lower, upper)` endpoints ordered so `lower <= upper`.
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.cursor {
            (self.anchor, self.cursor)
        } else {
            (self.cursor, self.anchor)
        }
    }
}

/// Hit-test layout snapshot of the focused log pane, produced by the renderer
/// and consumed by the mouse event handler to map screen cells to text.
#[derive(Clone, Debug)]
pub struct PaneView {
    /// Inner (inside-border) origin and height of the pane, in screen cells.
    pub x: u16,
    pub y: u16,
    pub height: u16,
    /// The visual rows currently displayed, top to bottom.
    pub visible_rows: Vec<VisualRow>,
    /// Plain text of every logical line in the pane (selection-index space).
    pub logical: Vec<String>,
}

/// Pure, testable view/event state for the console TUI. The I/O event loop in
/// `mod.rs` owns one of these and the renderer in `ui.rs` reads from it.
pub struct AppState {
    pub build_logs: Vec<String>,
    pub app_logs: Vec<String>,
    pub status: Status,
    pub pane: ConsoleType,
    pub build_pane: ScrollablePane,
    pub app_pane: ScrollablePane,
    pub inspector_pane: ScrollablePane,
    pub inspector_full_tree: bool,
    pub inspector_cursor: usize,
    /// Whether the console is in Vim-style selection ("visual") mode. While
    /// `true`, dragging the mouse over the focused log pane highlights a
    /// character range that can be yanked to the clipboard. The scroll wheel
    /// keeps working and the mouse stays captured by the app in both modes.
    pub selection_mode: bool,
    /// The active character-range selection in the focused pane, if any.
    pub selection: Option<Selection>,
    /// True while the left mouse button is held down and dragging a selection.
    pub selecting: bool,
    /// Hit-test layout of the focused log pane from the most recent frame, used
    /// to translate mouse coordinates into `(line, column)` text positions.
    pub last_view: Option<PaneView>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            build_logs: Vec::with_capacity(MAX_LINES),
            app_logs: Vec::with_capacity(MAX_LINES),
            status: Status::Compiling(0),
            pane: ConsoleType::App,
            build_pane: ScrollablePane::new(),
            app_pane: ScrollablePane::new(),
            inspector_pane: ScrollablePane::new(),
            inspector_full_tree: false,
            inspector_cursor: 0,
            selection_mode: false,
            selection: None,
            selecting: false,
            last_view: None,
        }
    }

    /// Append a build log line (carriage returns stripped), capping history.
    pub fn push_build_log(&mut self, msg: String) {
        self.build_logs
            .push(msg.replace('\r', ""));
        if self.build_logs.len() > MAX_LINES {
            self.build_logs.remove(0);
        }
    }

    /// Append an app log line (carriage returns stripped, log styling applied),
    /// capping history.
    pub fn push_app_log(&mut self, msg: String) {
        self.app_logs.push(
            msg.replace('\r', "")
                .process_log(),
        );
        if self.app_logs.len() > MAX_LINES {
            self.app_logs.remove(0);
        }
    }

    /// Apply a status change, focusing the most relevant pane.
    pub fn apply_status(&mut self, status: Status) {
        match status {
            Status::Error => self.pane = ConsoleType::Build,
            Status::Running => self.pane = ConsoleType::App,
            _ => {}
        }
        self.status = status;
    }

    /// Cycle focus to the next pane.
    pub fn next_pane(&mut self) {
        self.pane = self.pane.next();
    }

    /// Clear and reset the build pane.
    pub fn clear_build(&mut self) {
        self.build_logs.clear();
        self.build_pane.reset();
    }

    /// Clear and reset the app pane.
    pub fn clear_app(&mut self) {
        self.app_logs.clear();
        self.app_pane.reset();
    }

    /// Drop any active selection (and stop an in-progress drag).
    pub fn clear_selection(&mut self) {
        self.selection = None;
        self.selecting = false;
    }

    /// Extract the plain text covered by the current selection, if any, using
    /// the logical lines captured in the most recent [`PaneView`]. Endpoints
    /// are inclusive, matching Vim's character-wise visual mode.
    pub fn selected_text(&self) -> Option<String> {
        let sel = self.selection?;
        let logical = &self
            .last_view
            .as_ref()?
            .logical;
        if logical.is_empty() {
            return None;
        }
        let ((la, ca), (lb, cb)) = sel.ordered();
        let last_line = logical.len() - 1;
        let la = la.min(last_line);
        let lb = lb.min(last_line);

        let line_chars = |idx: usize| -> Vec<char> { logical[idx].chars().collect() };

        if la == lb {
            let chars = line_chars(la);
            if chars.is_empty() {
                return Some(String::new());
            }
            let start = ca.min(chars.len() - 1);
            let end = cb.min(chars.len() - 1);
            Some(
                chars[start..=end]
                    .iter()
                    .collect(),
            )
        } else {
            let mut out = String::new();
            // First line: from `ca` to its end.
            let first = line_chars(la);
            if !first.is_empty() {
                let start = ca.min(first.len() - 1);
                out.extend(&first[start..]);
            }
            out.push('\n');
            // Middle lines: in full.
            for line in logical
                .iter()
                .take(lb)
                .skip(la + 1)
            {
                out.push_str(line);
                out.push('\n');
            }
            // Last line: up to and including `cb`.
            let last = line_chars(lb);
            if !last.is_empty() {
                let end = cb.min(last.len() - 1);
                out.extend(&last[..=end]);
            }
            Some(out)
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── strip_ansi ───────────────────────────────────────────────────

    #[test]
    fn strip_ansi_plain_text() {
        assert_eq!(strip_ansi("hello world"), "hello world");
    }

    #[test]
    fn strip_ansi_empty_string() {
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn strip_ansi_sgr_color_codes() {
        assert_eq!(strip_ansi("\x1b[31mError\x1b[0m"), "Error");
    }

    #[test]
    fn strip_ansi_bold_and_combined() {
        assert_eq!(strip_ansi("\x1b[1;32mOK\x1b[0m"), "OK");
    }

    #[test]
    fn strip_ansi_preserves_newlines_and_tabs() {
        assert_eq!(strip_ansi("line1\nline2\ttab"), "line1\nline2\ttab");
    }

    #[test]
    fn strip_ansi_strips_control_chars() {
        assert_eq!(strip_ansi("a\x07b"), "ab");
    }

    #[test]
    fn strip_ansi_cursor_movement_sequences() {
        assert_eq!(strip_ansi("\x1b[2J\x1b[HReady"), "Ready");
    }

    #[test]
    fn strip_ansi_multiple_sequences() {
        assert_eq!(
            strip_ansi("\x1b[31mred\x1b[0m normal \x1b[34mblue\x1b[0m"),
            "red normal blue"
        );
    }

    // ── ConsoleType::next ────────────────────────────────────────────

    #[test]
    fn console_type_next_cycles() {
        assert_eq!(ConsoleType::App.next(), ConsoleType::Build);
        assert_eq!(ConsoleType::Build.next(), ConsoleType::Inspector);
        assert_eq!(ConsoleType::Inspector.next(), ConsoleType::App);
    }

    #[test]
    fn console_type_next_full_cycle() {
        let mut pane = ConsoleType::App;
        for _ in 0..3 {
            pane = pane.next();
        }
        assert_eq!(pane, ConsoleType::App);
    }

    // ── ScrollablePane ───────────────────────────────────────────────

    #[test]
    fn scrollable_pane_starts_at_zero() {
        let pane = ScrollablePane::new();
        assert_eq!(pane.scroll, 0);
    }

    #[test]
    fn scrollable_pane_scroll_up() {
        let mut pane = ScrollablePane::new();
        pane.scroll_up(5);
        assert_eq!(pane.scroll, 5);
        pane.scroll_up(3);
        assert_eq!(pane.scroll, 8);
    }

    #[test]
    fn scrollable_pane_scroll_down_saturates() {
        let mut pane = ScrollablePane::new();
        pane.scroll_up(5);
        pane.scroll_down(3);
        assert_eq!(pane.scroll, 2);
    }

    #[test]
    fn scrollable_pane_scroll_down_does_not_underflow() {
        let mut pane = ScrollablePane::new();
        pane.scroll_down(10);
        assert_eq!(pane.scroll, 0);
    }

    #[test]
    fn scrollable_pane_reset() {
        let mut pane = ScrollablePane::new();
        pane.scroll_up(100);
        pane.reset();
        assert_eq!(pane.scroll, 0);
    }

    #[test]
    fn scrollable_pane_scroll_up_large_values() {
        let mut pane = ScrollablePane::new();
        pane.scroll_up(u16::MAX);
        assert_eq!(pane.scroll, u16::MAX);
    }

    // ── Status equality ──────────────────────────────────────────────

    #[test]
    fn status_eq_variants() {
        assert_eq!(Status::Running, Status::Running);
        assert_eq!(Status::Launching, Status::Launching);
        assert_eq!(Status::Error, Status::Error);
        assert_eq!(Status::Idling, Status::Idling);
    }

    #[test]
    fn status_eq_with_payload() {
        assert_eq!(Status::Compiling(50), Status::Compiling(50));
        assert_eq!(Status::Fetching(10), Status::Fetching(10));
        assert_eq!(Status::Building(75), Status::Building(75));
    }

    #[test]
    fn status_ne_different_variants() {
        assert_ne!(Status::Running, Status::Error);
        assert_ne!(Status::Compiling(0), Status::Fetching(0));
    }

    #[test]
    fn status_ne_different_payload() {
        assert_ne!(Status::Compiling(10), Status::Compiling(90));
    }

    #[test]
    fn status_clone() {
        let s = Status::Compiling(42);
        let s2 = s.clone();
        assert_eq!(s, s2);
    }

    // ── AppState transitions ─────────────────────────────────────────

    #[test]
    fn app_state_defaults() {
        let state = AppState::new();
        assert_eq!(state.pane, ConsoleType::App);
        assert_eq!(state.status, Status::Compiling(0));
        assert!(state.build_logs.is_empty());
        assert!(state.app_logs.is_empty());
        assert!(!state.inspector_full_tree);
        assert_eq!(state.inspector_cursor, 0);
    }

    #[test]
    fn app_state_apply_status_error_focuses_build() {
        let mut state = AppState::new();
        state.apply_status(Status::Error);
        assert_eq!(state.pane, ConsoleType::Build);
        assert_eq!(state.status, Status::Error);
    }

    #[test]
    fn app_state_apply_status_running_focuses_app() {
        let mut state = AppState::new();
        state.pane = ConsoleType::Build;
        state.apply_status(Status::Running);
        assert_eq!(state.pane, ConsoleType::App);
        assert_eq!(state.status, Status::Running);
    }

    #[test]
    fn app_state_apply_status_other_keeps_pane() {
        let mut state = AppState::new();
        state.pane = ConsoleType::Inspector;
        state.apply_status(Status::Compiling(50));
        assert_eq!(state.pane, ConsoleType::Inspector);
        assert_eq!(state.status, Status::Compiling(50));
    }

    #[test]
    fn app_state_next_pane_cycles() {
        let mut state = AppState::new();
        state.next_pane();
        assert_eq!(state.pane, ConsoleType::Build);
        state.next_pane();
        assert_eq!(state.pane, ConsoleType::Inspector);
        state.next_pane();
        assert_eq!(state.pane, ConsoleType::App);
    }

    #[test]
    fn app_state_push_build_log_strips_cr() {
        let mut state = AppState::new();
        state.push_build_log("hello\rworld".to_string());
        assert_eq!(state.build_logs, vec!["helloworld".to_string()]);
    }

    #[test]
    fn app_state_push_logs_cap_at_max() {
        let mut state = AppState::new();
        for i in 0..(MAX_LINES + 10) {
            state.push_build_log(format!("line {i}"));
        }
        assert_eq!(state.build_logs.len(), MAX_LINES);
        // Oldest lines dropped; last line preserved.
        assert_eq!(
            state
                .build_logs
                .last()
                .unwrap(),
            &format!("line {}", MAX_LINES + 9)
        );
    }

    #[test]
    fn app_state_clear_build_and_app() {
        let mut state = AppState::new();
        state.push_build_log("b".to_string());
        state.push_app_log("a".to_string());
        state.build_pane.scroll_up(5);
        state.app_pane.scroll_up(5);

        state.clear_build();
        assert!(state.build_logs.is_empty());
        assert_eq!(state.build_pane.scroll, 0);

        state.clear_app();
        assert!(state.app_logs.is_empty());
        assert_eq!(state.app_pane.scroll, 0);
    }

    // ── Selection / yank ─────────────────────────────────────────────

    /// Build an `AppState` whose most-recent pane view exposes `logical` lines,
    /// so `selected_text` has something to slice.
    fn state_with(logical: &[&str]) -> AppState {
        let mut state = AppState::new();
        state.last_view = Some(PaneView {
            x: 0,
            y: 0,
            height: 0,
            visible_rows: Vec::new(),
            logical: logical
                .iter()
                .map(|s| s.to_string())
                .collect(),
        });
        state
    }

    #[test]
    fn selection_defaults_off() {
        let state = AppState::new();
        assert!(!state.selection_mode);
        assert!(state.selection.is_none());
        assert!(!state.selecting);
        assert!(state.last_view.is_none());
    }

    #[test]
    fn selection_ordered_normalizes_endpoints() {
        let backwards = Selection {
            anchor: (2, 5),
            cursor: (1, 3),
        };
        assert_eq!(backwards.ordered(), ((1, 3), (2, 5)));
        let forwards = Selection {
            anchor: (0, 0),
            cursor: (0, 4),
        };
        assert_eq!(forwards.ordered(), ((0, 0), (0, 4)));
    }

    #[test]
    fn selected_text_single_line_inclusive() {
        let mut state = state_with(&["hello world"]);
        state.selection = Some(Selection {
            anchor: (0, 0),
            cursor: (0, 4),
        });
        assert_eq!(
            state
                .selected_text()
                .as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn selected_text_is_order_independent() {
        let mut state = state_with(&["hello world"]);
        state.selection = Some(Selection {
            anchor: (0, 4),
            cursor: (0, 0),
        });
        assert_eq!(
            state
                .selected_text()
                .as_deref(),
            Some("hello")
        );
    }

    #[test]
    fn selected_text_multi_line() {
        let mut state = state_with(&["foo", "bar", "baz"]);
        // From col 1 of "foo" through col 1 of "baz", whole middle line.
        state.selection = Some(Selection {
            anchor: (0, 1),
            cursor: (2, 1),
        });
        assert_eq!(
            state
                .selected_text()
                .as_deref(),
            Some("oo\nbar\nba")
        );
    }

    #[test]
    fn selected_text_clamps_column_past_eol() {
        let mut state = state_with(&["ab"]);
        state.selection = Some(Selection {
            anchor: (0, 0),
            cursor: (0, 99),
        });
        assert_eq!(
            state
                .selected_text()
                .as_deref(),
            Some("ab")
        );
    }

    #[test]
    fn selected_text_none_without_selection() {
        let state = state_with(&["x"]);
        assert_eq!(state.selected_text(), None);
    }

    #[test]
    fn selected_text_none_without_view() {
        let mut state = AppState::new();
        state.selection = Some(Selection {
            anchor: (0, 0),
            cursor: (0, 0),
        });
        assert_eq!(state.selected_text(), None);
    }

    #[test]
    fn clear_selection_resets_state() {
        let mut state = state_with(&["x"]);
        state.selection = Some(Selection {
            anchor: (0, 0),
            cursor: (0, 0),
        });
        state.selecting = true;
        state.clear_selection();
        assert!(state.selection.is_none());
        assert!(!state.selecting);
    }
}
