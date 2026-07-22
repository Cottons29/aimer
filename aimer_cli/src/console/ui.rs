use aimer_inspector::{InspectorState, render_tree_lines_with_ids};
use ansi_to_tui::IntoText;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::console::state::{VisualRow, strip_ansi};
use crate::console::{AppState, ConsoleType, PaneView, Selection, Status};

/// Render the full console UI as a function of the current [`AppState`] and the
/// latest inspector snapshot. Pane scroll positions are clamped here as a side
/// effect of layout, which is why `state` is taken mutably.
pub fn render(
    f: &mut Frame,
    state: &mut AppState,
    inspector_state: &InspectorState,
    inspector_address: &str,
    frames: &[&str],
    running_frame: &[&str],
    frame_index: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
        .split(f.area());

    let build_text = state
        .build_logs
        .iter()
        .flat_map(|l| {
            l.into_text()
                .map(|t| t.lines)
                .unwrap_or_else(|_| vec![Line::from(strip_ansi(l))])
        })
        .collect::<Vec<_>>();
    let app_text = state
        .app_logs
        .iter()
        .flat_map(|l| {
            l.into_text()
                .map(|t| t.lines)
                .unwrap_or_else(|_| vec![Line::from(strip_ansi(l))])
        })
        .collect::<Vec<_>>();

    let inspector_status = if !inspector_state.connected {
        " [disconnected]"
    } else if inspector_state.enabled {
        " [ON]"
    } else {
        " [OFF]"
    };
    let inspector_title = format!("Inspector{}", inspector_status);

    let build_block = Block::default()
        .borders(Borders::ALL)
        .title("Build Logs")
        .border_style(Style::default().fg(if state.pane == ConsoleType::Build {
            Color::Yellow
        } else {
            Color::White
        }));

    let app_block = Block::default()
        .borders(Borders::ALL)
        .title("App Logs")
        .border_style(Style::default().fg(if state.pane == ConsoleType::App {
            Color::Yellow
        } else {
            Color::White
        }));

    let inspector_block = Block::default()
        .borders(Borders::ALL)
        .title(inspector_title)
        .border_style(
            Style::default().fg(if state.pane == ConsoleType::Inspector {
                Color::Cyan
            } else {
                Color::White
            }),
        );

    let area = chunks[0];
    let height = area.height.saturating_sub(2) as usize;
    let width = area
        .width
        .saturating_sub(2)
        .max(1) as usize;

    // Background painted under selected text in Vim-style selection mode.
    let selection_highlight = Style::default().bg(Color::Blue);
    // Reset the hit-test snapshot every frame; only the focused log pane in
    // selection mode publishes one (see `build_selection_view`).
    state.last_view = None;

    let calc_scroll = |logs: &[Line],
                       height: usize,
                       width: usize,
                       requested_scroll: usize|
     -> (usize, u16, u16) {
        if logs.is_empty() {
            return (0, 0, 0);
        }
        let mut total_wrapped = 0;
        for line in logs.iter() {
            let line_width = line.width();
            let w = line_width.div_ceil(width);
            total_wrapped += w.max(1);
        }

        let max_scroll = total_wrapped.saturating_sub(height);
        let actual_scroll = requested_scroll.min(max_scroll);

        let target_lines = height + actual_scroll;
        let mut start = 0;
        let mut wrapped_lines = 0;

        for (i, line) in logs.iter().enumerate().rev() {
            let line_width = line.width();
            let w = line_width.div_ceil(width);
            wrapped_lines += w.max(1);
            if wrapped_lines >= target_lines {
                start = i;
                break;
            }
        }

        let skip_top = wrapped_lines.saturating_sub(target_lines);

        (start, skip_top as u16, actual_scroll as u16)
    };

    if state.pane == ConsoleType::Build {
        if state.selection_mode {
            let (rendered, view, new_scroll) = build_selection_view(
                &build_text,
                area.x + 1,
                area.y + 1,
                area.width.saturating_sub(2),
                area.height.saturating_sub(2),
                state.build_pane.scroll,
                state.selection,
                selection_highlight,
            );
            state.build_pane.scroll = new_scroll;
            state.last_view = Some(view);
            f.render_widget(Paragraph::new(rendered).block(build_block), area);
        } else {
            let (start, skip_top, new_scroll) =
                calc_scroll(&build_text, height, width, state.build_pane.scroll as usize);
            state.build_pane.scroll = new_scroll;
            let p = Paragraph::new(build_text[start..].to_vec())
                .block(build_block)
                .wrap(Wrap { trim: false })
                .scroll((skip_top, 0));
            f.render_widget(p, area);
        }
    } else if state.pane == ConsoleType::App {
        if state.selection_mode {
            let (rendered, view, new_scroll) = build_selection_view(
                &app_text,
                area.x + 1,
                area.y + 1,
                area.width.saturating_sub(2),
                area.height.saturating_sub(2),
                state.app_pane.scroll,
                state.selection,
                selection_highlight,
            );
            state.app_pane.scroll = new_scroll;
            state.last_view = Some(view);
            f.render_widget(Paragraph::new(rendered).block(app_block), area);
        } else {
            let (start, skip_top, new_scroll) =
                calc_scroll(&app_text, height, width, state.app_pane.scroll as usize);
            state.app_pane.scroll = new_scroll;
            let p = Paragraph::new(app_text[start..].to_vec())
                .block(app_block)
                .wrap(Wrap { trim: false })
                .scroll((skip_top, 0));
            f.render_widget(p, area);
        }
    } else {
        // Inspector pane
        let mut tree_lines: Vec<String> = Vec::new();
        if !inspector_state.connected {
            tree_lines.push("Waiting for app to start...".to_string());
            tree_lines.push(format!("Connecting to ws://{}", inspector_address));
        } else if !inspector_state.enabled {
            tree_lines.push("Inspector is OFF.".to_string());
            tree_lines.push("Press F12 to enable.".to_string());
        } else {
            let mut tree_ids: Vec<u64> = Vec::new();
            match &inspector_state.tree {
                Some(root) => render_tree_lines_with_ids(
                    root,
                    &mut tree_lines,
                    &mut tree_ids,
                    state.inspector_full_tree,
                ),
                None => tree_lines.push("No widget tree received yet.".to_string()),
            }
            // Auto-move cursor to hovered widget
            let Some(hid) = inspector_state.hovered_widget_id else {
                return;
            };
            if let Some(idx) = tree_ids
                .iter()
                .position(|&id| id == hid)
            {
                state.inspector_cursor = idx;
            }
        }
        // Clamp cursor to valid range
        if !tree_lines.is_empty() {
            state.inspector_cursor = state
                .inspector_cursor
                .min(tree_lines.len() - 1);
        } else {
            state.inspector_cursor = 0;
        }
        // Auto-scroll to keep cursor visible
        if (state.inspector_cursor as u16) < state.inspector_pane.scroll {
            state.inspector_pane.scroll = state.inspector_cursor as u16;
        } else if state.inspector_cursor as u16 >= state.inspector_pane.scroll + height as u16 {
            state.inspector_pane.scroll =
                (state.inspector_cursor as u16).saturating_sub(height as u16 - 1);
        }
        let highlight_style = Style::default()
            .bg(Color::DarkGray)
            .fg(Color::White);
        let inspector_text: Vec<Line> = tree_lines
            .iter()
            .enumerate()
            .map(|(i, l)| {
                if i == state.inspector_cursor {
                    Line::from(Span::styled(l.as_str(), highlight_style))
                } else {
                    Line::from(l.as_str())
                }
            })
            .collect();
        let max_scroll = (inspector_text.len() as u16).saturating_sub(height as u16);
        state.inspector_pane.scroll = state
            .inspector_pane
            .scroll
            .min(max_scroll);
        let p = Paragraph::new(inspector_text)
            .block(inspector_block)
            .wrap(Wrap { trim: false })
            .scroll((state.inspector_pane.scroll, 0));
        f.render_widget(p, area);
    }

    let (status_icon, status_text) = match state.status {
        Status::Locking => (frames[frame_index], "Locking dependencies...".to_string()),
        Status::Fetching(p) => (frames[frame_index], format!("Fetching {}%", p)),
        Status::Compiling(p) => (frames[frame_index], format!("Compiling {}%", p)),
        Status::Building(p) => (frames[frame_index], format!("Building {}%", p)),
        Status::Launching => (frames[frame_index], "Launching...".to_string()),
        Status::Running => (running_frame[frame_index], "Running".to_string()),
        Status::Error => ("✗", "Error".to_string()),
        Status::Idling => ("✓", "Idling".to_string()),
    };

    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(30), Constraint::Min(0)].as_ref())
        .split(chunks[1]);

    let status_color = match state.status {
        Status::Locking => Color::LightBlue,
        Status::Fetching(_) => Color::Blue,
        Status::Compiling(_) => Color::Yellow,
        Status::Building(_) => Color::Cyan,
        Status::Launching => Color::Magenta,
        Status::Running => Color::Green,
        Status::Idling => Color::DarkGray,
        Status::Error => Color::Red,
    };

    let status_line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!("{} {}", status_icon, status_text),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let controls_line = Line::from(vec![
        Span::styled(
            "[r] ",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("reload | "),
        Span::styled(
            "[Shift+Q] ",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("exit | "),
        // Span::styled("[c] ",
        // Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
        // Span::raw("copy | "),
        Span::styled(
            "[s] ",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(if state.selection_mode {
            "select "
        } else {
            "scroll "
        }),
        Span::raw("| "),
        // Span::styled("[y] ",
        // Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
        // Span::raw("yank | "),
        Span::styled(
            "[Tab] ",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("switch pane | "),
        Span::styled(
            "[F12] ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("inspector"),
        // Span::styled("[t] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        // Span::raw(if state.inspector_full_tree { "full tree " } else { "widgets " }),
        // Span::raw("| "),
        // Span::styled("●", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        // Span::raw(" hot-reload "),
    ]);

    let status_bar = Paragraph::new(status_line).style(Style::default());
    let controls_bar = Paragraph::new(controls_line)
        .style(Style::default())
        .alignment(ratatui::layout::Alignment::Right);

    f.render_widget(status_bar, bottom_chunks[0]);
    f.render_widget(controls_bar, bottom_chunks[1]);
}

/// Render the focused log pane for Vim-style selection mode.
///
/// Unlike the normal path (which lets ratatui word-wrap the text), this does
/// its own *character* wrapping so every screen cell maps 1:1 to a source
/// character. That exact mapping is what lets the mouse handler hit-test a
/// click/drag back to a `(line, column)` text position. It flattens each styled
/// logical line into `(char, Style)` cells, wraps them at `width`, renders only
/// the rows visible for the bottom-anchored `scroll`, and paints `highlight`
/// under the active selection.
///
/// Returns the rendered lines, the hit-test [`PaneView`], and the clamped
/// scroll value (so callers can write it back).
#[allow(clippy::too_many_arguments)]
fn build_selection_view(
    lines: &[Line],
    inner_x: u16,
    inner_y: u16,
    width: u16,
    height: u16,
    scroll: u16,
    selection: Option<Selection>,
    highlight: Style,
) -> (Vec<Line<'static>>, PaneView, u16) {
    let w = (width as usize).max(1);
    let h = height as usize;

    // Flatten each logical line into styled cells + its plain text.
    let mut cells: Vec<Vec<(char, Style)>> = Vec::with_capacity(lines.len());
    let mut logical: Vec<String> = Vec::with_capacity(lines.len());
    for line in lines {
        let mut row: Vec<(char, Style)> = Vec::new();
        for span in &line.spans {
            for ch in span.content.chars() {
                row.push((ch, span.style));
            }
        }
        logical.push(
            row.iter()
                .map(|(c, _)| *c)
                .collect(),
        );
        cells.push(row);
    }

    // Character-wrap into a flat list of visual rows (empty lines keep a row).
    let mut rows: Vec<VisualRow> = Vec::new();
    for (l, row) in cells.iter().enumerate() {
        if row.is_empty() {
            rows.push(VisualRow {
                line: l,
                start: 0,
                len: 0,
            });
        } else {
            let mut start = 0;
            while start < row.len() {
                let len = w.min(row.len() - start);
                rows.push(VisualRow {
                    line: l,
                    start,
                    len,
                });
                start += len;
            }
        }
    }

    // Bottom-anchored window: scroll counts visual rows up from the bottom.
    let total = rows.len();
    let max_scroll = total.saturating_sub(h);
    let scroll = (scroll as usize).min(max_scroll);
    let start_idx = total.saturating_sub(h + scroll);
    let end_idx = (start_idx + h).min(total);
    let visible = &rows[start_idx..end_idx];

    let ordered = selection.map(|s| s.ordered());
    let is_selected = |line: usize, col: usize| -> bool {
        match ordered {
            Some((lo, hi)) => (line, col) >= lo && (line, col) <= hi,
            None => false,
        }
    };

    let mut rendered: Vec<Line<'static>> = Vec::with_capacity(visible.len());
    for vr in visible {
        if vr.len == 0 {
            // Blank source line: a highlighted space marks it as part of a
            // multi-line selection; otherwise it stays empty.
            if is_selected(vr.line, 0) {
                rendered.push(Line::from(Span::styled(" ".to_string(), highlight)));
            } else {
                rendered.push(Line::default());
            }
            continue;
        }
        let src = &cells[vr.line];
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut buf = String::new();
        let mut buf_style: Option<Style> = None;
        for i in 0..vr.len {
            let (ch, base) = src[vr.start + i];
            let style = if is_selected(vr.line, vr.start + i) {
                base.patch(highlight)
            } else {
                base
            };
            match buf_style {
                Some(s) if s == style => buf.push(ch),
                _ => {
                    if let Some(s) = buf_style.take() {
                        spans.push(Span::styled(std::mem::take(&mut buf), s));
                    }
                    buf.push(ch);
                    buf_style = Some(style);
                }
            }
        }
        if let Some(s) = buf_style.take() {
            spans.push(Span::styled(buf, s));
        }
        rendered.push(Line::from(spans));
    }

    let view = PaneView {
        x: inner_x,
        y: inner_y,
        height: h as u16,
        visible_rows: visible.to_vec(),
        logical,
    };

    (rendered, view, scroll as u16)
}
