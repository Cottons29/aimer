//! The framed, width-filling block both report panes end with.
//!
//! A failed build ends with its errors ([`ErrorReport`]) and a running app that
//! recovers a widget panic ends with that panic ([`PanicReport`]), and both are
//! drawn the same way: a centred header rule, one padded panel per entry, a
//! footer rule. The layout is shared here because the delicate part of it —
//! keeping every row exactly as wide as the pane, background included — is the
//! same whatever the block reports.
//!
//! A row wider than the pane would be wrapped by the console at a column the
//! panel knows nothing about, which leaves the panel background torn across two
//! rows; that is why the block is laid out for a known width and wraps its own
//! rows.
//!
//! [`ErrorReport`]: super::cargo_message::ErrorReport
//! [`PanicReport`]: super::panic_report::PanicReport

use colored::Colorize;

use crate::console::state::strip_ansi;

/// Width a block falls back to when the terminal size is unknown — output
/// redirected to a file or a pipe has no width to lay a block out for.
pub const MIN_WIDTH: usize = 58;

/// Columns a log pane spends on its own border, and which therefore aren't
/// available to the block inside it.
const PANE_BORDER: u16 = 2;

/// Background painted behind an entry inside a block — a muted light red, dark
/// enough for cargo's own red and blue to stay readable on top of it.
pub const PANEL_BACKGROUND: (u8, u8, u8) = (74, 40, 40);

/// Color of the wave rule between two entries. Darker than the framing headers
/// so the rule reads as a seam between panels rather than as another entry line.
pub const DIVIDER_COLOR: (u8, u8, u8) = (112, 52, 52);

/// How wide a block should be drawn: the whole terminal, minus the columns the
/// pane spends on its border, so the block fills the pane it lands in.
///
/// Zero when the terminal size is unavailable — output redirected to a file or a
/// pipe has no width to speak of; a block then falls back to [`MIN_WIDTH`].
pub fn block_width() -> usize {
    crossterm::terminal::size()
        .map(|(columns, _)| columns.saturating_sub(PANE_BORDER) as usize)
        .unwrap_or(0)
}

/// The width a block is laid out for: `width` as given, or [`MIN_WIDTH`] when it
/// isn't known yet.
///
/// Every non-zero width is honoured, however narrow the pane is: a block wider
/// than its pane wraps and tears, which is worse than a cramped one.
#[inline]
pub fn resolve_width(width: usize) -> usize {
    if width == 0 { MIN_WIDTH } else { width }
}

/// A centred title framed by `=` rules, `width` cells wide.
pub fn header(title: &str, width: usize) -> String {
    let text = format!(" {} ", title);
    let fill = width.saturating_sub(text.chars().count());
    let left = fill / 2;
    format!("{}{}{}", "=".repeat(left), text, "=".repeat(fill - left))
        .red()
        .bold()
        .to_string()
}

/// The wave rule between two panels of a block, `width` cells wide.
///
/// The color is written out by hand, like the panel background, so the rule
/// keeps its darker tint no matter what the `colored` crate decides about the
/// current output stream.
pub fn divider(width: usize) -> String {
    let (r, g, b) = DIVIDER_COLOR;
    format!("\x1b[38;2;{r};{g};{b}m{}\x1b[0m", "~".repeat(width))
}

/// Lay `rendered` out as one panel: every line wrapped and padded to `width`
/// cells and painted on [`PANEL_BACKGROUND`], with a blank row above and below so
/// the text doesn't touch the edges.
pub fn panel(rendered: &[String], width: usize) -> Vec<String> {
    let mut lines = Vec::with_capacity(rendered.len() + 2);
    lines.push(panel_line("", width));
    for line in rendered {
        lines.extend(wrap_ansi(line, width).iter().map(|row| panel_line(row, width)));
    }
    lines.push(panel_line("", width));
    lines
}

/// Split `line` into rows of at most `width` visible cells.
///
/// Cargo renders its diagnostics for a terminal that wraps them for it, so a
/// rendered line can be wider than the pane the panel lands in. Leaving that to
/// the console breaks the panel: it wraps the padded row at a column the panel
/// knows nothing about, so the padding — and with it the background — ends up in
/// the middle of the row and the remainder is left unpainted. Wrapping here
/// instead keeps every row exactly one panel row wide.
///
/// Escape sequences occupy no cell and never count towards `width`. Those still
/// in effect at a break are repeated at the start of the next row, so a color
/// cargo opened before the break survives it.
fn wrap_ansi(line: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    if visible_width(line) <= width {
        return vec![line.to_string()];
    }

    let mut rows = Vec::new();
    let mut row = String::new();
    // The sequences that are still in effect, re-armed after every break.
    let mut active = String::new();
    let mut escape = String::new();
    let mut in_escape = false;
    let mut cells = 0;

    for c in line.chars() {
        if in_escape {
            escape.push(c);
            if c.is_ascii_alphabetic() || c == '@' || c == '~' {
                in_escape = false;
                if escape == "\x1b[0m" || escape == "\x1b[m" {
                    active.clear();
                } else {
                    active.push_str(&escape);
                }
                row.push_str(&escape);
                escape.clear();
            }
            continue;
        }
        if c == '\x1b' {
            in_escape = true;
            escape.push(c);
            continue;
        }
        if cells == width {
            rows.push(std::mem::take(&mut row));
            row.push_str(&active);
            cells = 0;
        }
        row.push(c);
        cells += 1;
    }

    // An unterminated sequence is kept verbatim rather than swallowed.
    row.push_str(&escape);
    rows.push(row);
    rows
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
pub fn visible_width(line: &str) -> usize {
    strip_ansi(line).chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_is_exactly_as_wide_as_the_block() {
        assert_eq!(visible_width(&header("Compile Error", 80)), 80);
    }

    #[test]
    fn a_header_centres_its_title() {
        let rendered = strip_ansi(&header("Oops", 20));

        assert_eq!(rendered, "======= Oops =======");
    }

    #[test]
    fn a_title_wider_than_the_block_is_never_truncated() {
        let rendered = strip_ansi(&header("a very long title indeed", 4));

        assert!(rendered.contains("a very long title indeed"), "{rendered}");
    }

    #[test]
    fn a_divider_fills_the_block() {
        assert_eq!(visible_width(&divider(30)), 30);
    }

    #[test]
    fn a_panel_pads_every_row_to_the_block_width() {
        let rows = panel(&["short".to_string(), "a bit longer".to_string()], 40);

        assert!(rows.iter().all(|row| visible_width(row) == 40), "{rows:?}");
    }

    #[test]
    fn a_panel_frames_its_content_with_blank_rows() {
        let rows = panel(&["only".to_string()], 20);

        assert_eq!(rows.len(), 3);
        assert!(strip_ansi(&rows[0]).trim().is_empty());
        assert_eq!(strip_ansi(&rows[1]).trim_end(), "only");
        assert!(strip_ansi(&rows[2]).trim().is_empty());
    }

    #[test]
    fn a_row_wider_than_the_block_is_wrapped_rather_than_cut() {
        let rows = panel(&["x".repeat(25)], 10);

        assert_eq!(rows.len(), 5);
        assert_eq!(
            strip_ansi(&rows[1..4].join("")).matches('x').count(),
            25,
            "{rows:?}"
        );
    }

    #[test]
    fn an_unknown_width_falls_back_to_the_minimum() {
        assert_eq!(resolve_width(0), MIN_WIDTH);
        assert_eq!(resolve_width(120), 120);
    }
}
