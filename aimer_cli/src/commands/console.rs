use crate::commands::run::Device;
use crate::commands::run::android::spawn_android_runner;
use crate::commands::run::ios::spawn_ios_runner;
use crate::commands::run::ios_sim::spawn_ios_simulator_runner;
use crate::commands::run::macos::spawn_macos_runner;
use crate::commands::run::utilities::LogStyling;
use crate::commands::run::web::spawn_web_runner;
use crate::targets::Targets;
use arboard::Clipboard;
use crossbeam::channel::Sender;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use inspector::{ InspectorServer, render_tree_lines_with_ids};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::cmp::PartialEq;
use std::io::{BufRead, BufReader, stdout};
use std::net::{IpAddr, Ipv4Addr};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::usize::MAX;
use tokio::runtime::Runtime;

const MAX_LINES: usize = 32768;

#[derive(Clone, PartialEq, Eq)]
pub enum Status {
    Fetching(u8),
    Compiling(u8),
    Building(u8),
    Launching,
    Running,
    Idling,
    Error,
}

pub enum RunnerEvent {
    BuildLog(String),
    AppLog(String),
    StatusChange(Status),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ConsoleType {
    App,
    Build,
    Inspector,
}

impl ConsoleType {
    pub fn next(&self) -> ConsoleType {
        match self {
            ConsoleType::App => ConsoleType::Build,
            ConsoleType::Build => ConsoleType::Inspector,
            ConsoleType::Inspector => ConsoleType::App,
        }
    }
}

struct ScrollablePane {
    scroll: u16,
}

impl ScrollablePane {
    fn new() -> Self {
        Self { scroll: 0 }
    }

    fn scroll_up(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_add(amount);
    }

    fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_sub(amount);
    }

    fn reset(&mut self) {
        self.scroll = 0;
    }
}

fn spawn_runner(
    device: Device,
    pkg_name: String,
    tx: Sender<RunnerEvent>,
    inspector_address: IpAddr,
    inspector_port: u16,
) -> Arc<Mutex<Option<Child>>> {
    let current_child = Arc::new(Mutex::new(None));
    let current_child_clone = Arc::clone(&current_child);

    thread::spawn(move || match device.target {
        Targets::Macos => spawn_macos_runner(device, pkg_name, tx, current_child_clone, inspector_address, inspector_port),
        Targets::IosSimulator => spawn_ios_simulator_runner(device, pkg_name, tx, current_child_clone, inspector_address, inspector_port),
        Targets::Web => spawn_web_runner(device, pkg_name, tx, current_child_clone, inspector_address, inspector_port),
        Targets::Ios => spawn_ios_runner(device, pkg_name, tx, current_child_clone, inspector_address, inspector_port),
        Targets::Android | Targets::AndroidSimulator => {
            spawn_android_runner(device, pkg_name, tx, current_child_clone, inspector_address, inspector_port)
        }
        _ => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Target {} is not yet supported for on-the-fly run.", device.target)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
            return;
        }
    });

    current_child
}

fn strip_ansi(s: &str) -> String {
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

pub fn start(device: Device, pkg_name: String) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = crossbeam::channel::unbounded();

    usize::MAX.checked_sub(MAX_LINES).unwrap_or(0);

    let mut build_logs: Vec<String> = Vec::with_capacity(MAX_LINES);
    let mut app_logs: Vec<String> = Vec::with_capacity(MAX_LINES);
    let mut current_status = Status::Compiling(0);
    let mut current_pane = ConsoleType::App;
    let mut build_pane = ScrollablePane::new();
    let mut app_pane = ScrollablePane::new();
    let mut inspector_pane = ScrollablePane::new();
    let mut inspector_full_tree: bool = false;
    let mut inspector_cursor: usize = 0;

    // Starting inspector server
    let Ok(inspector_runtime) = Runtime::new() else {
        return Err("Failed to start inspector server".into());
    };
    let inspector_address = match device.target {
        Targets::Ios | Targets::Android => local_ip_address::local_ip()?,
        _ => IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
    };

    let inspector_server_address = match device.target {
        Targets::Ios | Targets::Android => IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        _ => IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
    };

    let inspector_handle = match InspectorServer::start(inspector_server_address, 9229, inspector_runtime.handle()) {
        Ok(handle) => handle,
        Err(e) => {
            let _ = tx.send(RunnerEvent::AppLog(format!("Failed to start inspector server: {}", e)));
            return Err(e.into());
        }
    };

    let frames = ["⠋", "⠙", "⠚", "⠞", "⠖", "⠦", "⠴", "⠲", "⠳", "⠓"];
    let running_frame = ["▣", "▤", "▥", "▦", "▧", "▨", "▣", "▤", "▥", "▦"];
    let mut frame_index = 0;
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    let mut current_child = spawn_runner(device.clone(), pkg_name.clone(), tx.clone(), inspector_handle.address, inspector_handle.port);

    loop {
        // Process all pending events
        while let Ok(event) = rx.try_recv() {
            match event {
                RunnerEvent::BuildLog(msg) => {
                    let cleaned = msg.replace('\r', "");
                    build_logs.push(cleaned);
                    if build_logs.len() > MAX_LINES {
                        build_logs.remove(0);
                    }
                }
                RunnerEvent::AppLog(msg) => {
                    let cleaned = msg.replace('\r', "");
                    app_logs.push(cleaned.process_log());
                    if app_logs.len() > MAX_LINES {
                        app_logs.remove(0);
                    }
                }
                RunnerEvent::StatusChange(s) => {
                    match s {
                        Status::Error => current_pane = ConsoleType::Build,
                        Status::Running => current_pane = ConsoleType::App,
                        _ => {}
                    }
                    current_status = s;
                }
            }
        }

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
                .split(f.area());

            use ansi_to_tui::IntoText;
            let build_text = build_logs
                .iter()
                .flat_map(|l| {
                    l.into_text()
                        .map(|t| t.lines)
                        .unwrap_or_else(|_| vec![Line::from(strip_ansi(l))])
                })
                .collect::<Vec<_>>();
            let app_text = app_logs
                .iter()
                .flat_map(|l| {
                    l.into_text()
                        .map(|t| t.lines)
                        .unwrap_or_else(|_| vec![Line::from(strip_ansi(l))])
                })
                .collect::<Vec<_>>();

            let inspector_state = inspector_handle.state.lock().unwrap().clone();
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
                .border_style(Style::default().fg(if current_pane == ConsoleType::Build { Color::Yellow } else { Color::White }));

            let app_block = Block::default()
                .borders(Borders::ALL)
                .title("App Logs")
                .border_style(Style::default().fg(if current_pane == ConsoleType::App { Color::Yellow } else { Color::White }));

            let inspector_block = Block::default()
                .borders(Borders::ALL)
                .title(inspector_title)
                .border_style(Style::default().fg(if current_pane == ConsoleType::Inspector { Color::Cyan } else { Color::White }));

            let area = chunks[0];
            let height = area.height.saturating_sub(2) as usize;
            let width = area.width.saturating_sub(2).max(1) as usize;

            let calc_scroll = |logs: &[Line], height: usize, width: usize, requested_scroll: usize| -> (usize, u16, u16) {
                if logs.is_empty() {
                    return (0, 0, 0);
                }
                let mut total_wrapped = 0;
                for line in logs.iter() {
                    let line_width = line.width();
                    let w = (line_width + width - 1) / width;
                    total_wrapped += w.max(1);
                }

                let max_scroll = total_wrapped.saturating_sub(height);
                let actual_scroll = requested_scroll.min(max_scroll);

                let target_lines = height + actual_scroll;
                let mut start = 0;
                let mut wrapped_lines = 0;

                for (i, line) in logs.iter().enumerate().rev() {
                    let line_width = line.width();
                    let w = (line_width + width - 1) / width;
                    wrapped_lines += w.max(1);
                    if wrapped_lines >= target_lines {
                        start = i;
                        break;
                    }
                }

                let skip_top = if wrapped_lines > target_lines { wrapped_lines - target_lines } else { 0 };

                (start, skip_top as u16, actual_scroll as u16)
            };

            if current_pane == ConsoleType::Build {
                let (start, skip_top, new_scroll) = calc_scroll(&build_text, height, width, build_pane.scroll as usize);
                build_pane.scroll = new_scroll;
                let p = Paragraph::new(build_text[start..].to_vec())
                    .block(build_block)
                    .wrap(Wrap { trim: false })
                    .scroll((skip_top, 0));
                f.render_widget(p, area);
            } else if current_pane == ConsoleType::App {
                let (start, skip_top, new_scroll) = calc_scroll(&app_text, height, width, app_pane.scroll as usize);
                app_pane.scroll = new_scroll;
                let p = Paragraph::new(app_text[start..].to_vec())
                    .block(app_block)
                    .wrap(Wrap { trim: false })
                    .scroll((skip_top, 0));
                f.render_widget(p, area);
            } else {
                {
                    // Inspector pane
                    let mut tree_lines: Vec<String> = Vec::new();
                    if !inspector_state.connected {
                        tree_lines.push("Waiting for app to start...".to_string());
                        tree_lines.push(format!("Connecting to ws://{}", inspector_handle.get_address()));
                    } else if !inspector_state.enabled {
                        tree_lines.push("Inspector is OFF.".to_string());
                        tree_lines.push("Press F12 to enable.".to_string());
                    } else {
                        let mut tree_ids: Vec<u64> = Vec::new();
                        match &inspector_state.tree {
                            Some(root) => render_tree_lines_with_ids(root, &mut tree_lines, &mut tree_ids, inspector_full_tree),
                            None => tree_lines.push("No widget tree received yet.".to_string()),
                        }
                        // Auto-move cursor to hovered widget
                        if let Some(hid) = inspector_state.hovered_widget_id {
                            if let Some(idx) = tree_ids.iter().position(|&id| id == hid) {
                                inspector_cursor = idx;
                            }
                        }
                    }
                    // Clamp cursor to valid range
                    if !tree_lines.is_empty() {
                        inspector_cursor = inspector_cursor.min(tree_lines.len() - 1);
                    } else {
                        inspector_cursor = 0;
                    }
                    // Auto-scroll to keep cursor visible
                    if (inspector_cursor as u16) < inspector_pane.scroll {
                        inspector_pane.scroll = inspector_cursor as u16;
                    } else if inspector_cursor as u16 >= inspector_pane.scroll + height as u16 {
                        inspector_pane.scroll = (inspector_cursor as u16).saturating_sub(height as u16 - 1);
                    }
                    let highlight_style = Style::default().bg(Color::DarkGray).fg(Color::White);
                    let inspector_text: Vec<Line> = tree_lines
                        .iter()
                        .enumerate()
                        .map(|(i, l)| {
                            if i == inspector_cursor {
                                Line::from(Span::styled(l.as_str(), highlight_style))
                            } else {
                                Line::from(l.as_str())
                            }
                        })
                        .collect();
                    let max_scroll = (inspector_text.len() as u16).saturating_sub(height as u16);
                    inspector_pane.scroll = inspector_pane.scroll.min(max_scroll);
                    let p = Paragraph::new(inspector_text)
                        .block(inspector_block)
                        .wrap(Wrap { trim: false })
                        .scroll((inspector_pane.scroll, 0));
                    f.render_widget(p, area);
                }
            }

            let (status_icon, status_text) = match current_status {
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
                .constraints([Constraint::Min(0), Constraint::Length(60)].as_ref())
                .split(chunks[1]);

            let status_color = match current_status {
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
                Span::styled(
                    "[c] ",
                    Style::default()
                        .fg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("copy | "),
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
                Span::raw("inspector | "),
                Span::styled(
                    "[t] ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(if inspector_full_tree { "full tree " } else { "widgets " }),
            ]);

            let status_bar = Paragraph::new(status_line).style(Style::default());
            let controls_bar = Paragraph::new(controls_line)
                .style(Style::default())
                .alignment(ratatui::layout::Alignment::Right);

            f.render_widget(status_bar, bottom_chunks[0]);
            f.render_widget(controls_bar, bottom_chunks[1]);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('1'), _) => current_pane = ConsoleType::App,
                        (KeyCode::Char('2'), _) => current_pane = ConsoleType::Build,
                        (KeyCode::Char('3'), _) => current_pane = ConsoleType::Inspector,
                        (KeyCode::Char('Q'), _) | (KeyCode::Char('q'), KeyModifiers::SHIFT) => {
                            // Kill child process if any
                            if let Some(mut child) = current_child.lock().unwrap().take() {
                                let _ = child.kill();
                            }
                            break;
                        }
                        (KeyCode::Char('r'), _) => {
                            if device.target == Targets::Web {
                                build_logs.clear();
                                build_pane.reset();
                                current_status = Status::Compiling(0);
                                let tx_clone = tx.clone();
                                thread::spawn(move || {
                                    let _ = tx_clone.send(RunnerEvent::StatusChange(Status::Compiling(0)));
                                    let _ = tx_clone.send(RunnerEvent::BuildLog("Running wasm-pack build...".to_string()));
                                    let mut wasm_build = Command::new("wasm-pack")
                                        .arg("build")
                                        .arg("--debug")
                                        .arg("--target")
                                        .arg("web")
                                        .arg("--out-dir")
                                        .arg("builds/web/pkg")
                                        .stdout(Stdio::piped())
                                        .stderr(Stdio::piped())
                                        .spawn()
                                        .expect("Failed to start wasm-pack");

                                    let stdout = wasm_build.stdout.take().unwrap();
                                    let stderr = wasm_build.stderr.take().unwrap();

                                    let tx_c1 = tx_clone.clone();
                                    thread::spawn(move || {
                                        let reader = BufReader::new(stdout);
                                        for line in reader.lines() {
                                            if let Ok(l) = line {
                                                let _ = tx_c1.send(RunnerEvent::BuildLog(l));
                                            }
                                        }
                                    });

                                    let tx_c2 = tx_clone.clone();
                                    thread::spawn(move || {
                                        let reader = BufReader::new(stderr);
                                        let mut compile_count = 0;
                                        // let mut all_compile = 0;
                                        for line in reader.lines() {
                                            if let Ok(l) = line {
                                                if l.contains("Compiling") {
                                                    compile_count = (compile_count + 5).min(99);
                                                    let _ = tx_c2.send(RunnerEvent::StatusChange(Status::Compiling(compile_count)));
                                                } else if l.contains("Finished") {
                                                    let _ = tx_c2.send(RunnerEvent::StatusChange(Status::Compiling(100)));
                                                }
                                                let _ = tx_c2.send(RunnerEvent::BuildLog(l));
                                            }
                                        }
                                    });

                                    let status = wasm_build.wait().unwrap();
                                    if !status.success() {
                                        let _ = tx_clone.send(RunnerEvent::BuildLog("wasm-pack build failed.".to_string()));
                                    } else {
                                        let _ = tx_clone
                                            .send(RunnerEvent::BuildLog("wasm-pack build successful. Vite will auto-reload.".to_string()));
                                    }
                                    let _ = tx_clone.send(RunnerEvent::StatusChange(Status::Running));
                                });
                            } else {
                                // Kill child process if running
                                if let Some(mut child) = current_child.lock().unwrap().take() {
                                    let _ = child.kill();
                                }
                                build_logs.clear();
                                app_logs.clear();
                                build_pane.reset();
                                app_pane.reset();
                                current_status = Status::Compiling(0);
                                current_child =
                                    spawn_runner(device.clone(), pkg_name.clone(), tx.clone(), inspector_handle.address, inspector_handle.port);
                            }
                        }

                        (KeyCode::Char('c'), KeyModifiers::SHIFT) | (KeyCode::Char('C'), KeyModifiers::SHIFT) => match current_pane {
                            ConsoleType::App => app_logs.clear(),
                            ConsoleType::Build => build_logs.clear(),
                            _ => {}
                        },

                        (KeyCode::Char('c'), _) | (KeyCode::Char('C'), _) => {
                            if let Ok(mut clipboard) = Clipboard::new() {
                                let logs = if current_pane == ConsoleType::Build { build_logs.join("\n") } else { app_logs.join("\n") };
                                let _ = clipboard.set_text(logs);
                            }
                        }

                        (KeyCode::Tab, _) => current_pane = current_pane.next(),
                        (KeyCode::F(12), _) => {
                            inspector_handle.send_toggle();
                            current_pane = ConsoleType::Inspector;
                        }
                        (KeyCode::Char('t'), _) => {
                            if current_pane == ConsoleType::Inspector {
                                inspector_full_tree = !inspector_full_tree;
                            }
                        }
                        (KeyCode::Up, _) => match current_pane {
                            ConsoleType::Build => build_pane.scroll_up(1),
                            ConsoleType::App => app_pane.scroll_up(1),
                            ConsoleType::Inspector => {
                                inspector_cursor = inspector_cursor.saturating_sub(1);
                                if (inspector_cursor as u16) < inspector_pane.scroll {
                                    inspector_pane.scroll = inspector_cursor as u16;
                                }
                            }
                        },
                        (KeyCode::Down, _) => match current_pane {
                            ConsoleType::Build => build_pane.scroll_down(1),
                            ConsoleType::App => app_pane.scroll_down(1),
                            ConsoleType::Inspector => {
                                inspector_cursor = inspector_cursor.saturating_add(1);
                            }
                        },
                        (KeyCode::PageUp, _) => match current_pane {
                            ConsoleType::Build => build_pane.scroll_up(10),
                            ConsoleType::App => app_pane.scroll_up(10),
                            ConsoleType::Inspector => {
                                inspector_cursor = inspector_cursor.saturating_sub(10);
                                if (inspector_cursor as u16) < inspector_pane.scroll {
                                    inspector_pane.scroll = inspector_cursor as u16;
                                }
                            }
                        },
                        (KeyCode::PageDown, _) => match current_pane {
                            ConsoleType::Build => build_pane.scroll_down(10),
                            ConsoleType::App => app_pane.scroll_down(10),
                            ConsoleType::Inspector => {
                                inspector_cursor = inspector_cursor.saturating_add(10);
                            }
                        },
                        _ => {}
                    }
                }
                Event::Mouse(mouse_event) => match mouse_event.kind {
                    crossterm::event::MouseEventKind::ScrollUp => match current_pane {
                        ConsoleType::Build => build_pane.scroll_up(2),
                        ConsoleType::App => app_pane.scroll_up(2),
                        ConsoleType::Inspector => inspector_pane.scroll_up(2),
                    },
                    crossterm::event::MouseEventKind::ScrollDown => match current_pane {
                        ConsoleType::Build => build_pane.scroll_down(2),
                        ConsoleType::App => app_pane.scroll_down(2),
                        ConsoleType::Inspector => inspector_pane.scroll_down(2),
                    },
                    _ => {}
                },
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            frame_index = (frame_index + 1) % frames.len();
            last_tick = Instant::now();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
