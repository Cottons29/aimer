pub mod state;
pub mod ui;

pub use state::{AppState, ConsoleType, PaneView, RunnerEvent, Selection, Status};

use crate::commands::run::Device;
use crate::commands::run::pipeline::{self, RunContext};
use crate::targets::Targets;
use crate::tui::RawModeGuard;
use aimer_inspector::InspectorServer;
use anyhow::Context;
use arboard::Clipboard;
use crossbeam::channel::Sender;
use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use notify::{Event as NotifyEvent, RecursiveMode, Watcher};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io::{Write, stdout};
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

/// Spawn the per-target runner on a background thread, dispatching via the
/// pipeline [`Runner`](crate::commands::run::pipeline::Runner) trait.
fn spawn_runner(
    device: Device,
    pkg_name: String,
    tx: Sender<RunnerEvent>,
    inspector_address: IpAddr,
    inspector_port: u16,
) -> Arc<Mutex<Option<Child>>> {
    let current_child = Arc::new(Mutex::new(None));
    let current_child_clone = Arc::clone(&current_child);
    let target = device.target;

    match pipeline::runner_for(target) {
        Some(runner) => {
            let ctx = RunContext { device, pkg_name, tx, current_child: current_child_clone, inspector_address, inspector_port };
            thread::spawn(move || runner.run(ctx));
        }
        None => {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Target {} is not yet supported for on-the-fly run.", target)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
        }
    }

    current_child
}

/// Returns true if the current terminal is known to support the ConEmu
/// `OSC 9;4` progress protocol used to paint the bar at the top of the window
/// (Ghostty, WezTerm, Windows Terminal, ConEmu). Gated behind a TTY check so
/// the escape never leaks into pipes or CI logs.
fn terminal_supports_progress() -> bool {
    use std::io::IsTerminal;
    if !stdout().is_terminal() {
        return false;
    }
    if std::env::var_os("ConEmuANSI").is_some() || std::env::var_os("WT_SESSION").is_some() {
        return true;
    }
    match std::env::var("TERM_PROGRAM") {
        Ok(p) => {
            let p = p.to_ascii_lowercase();
            p.contains("ghostty") || p.contains("wezterm")
        }
        Err(_) => false,
    }
}

/// Build the `OSC 9;4` escape sequence corresponding to a runner [`Status`].
///
/// State codes: `1` determinate progress, `2` error, `3` indeterminate, `0` clear.
fn progress_sequence(status: &Status) -> String {
    match status {
        // Indeterminate spinner while resolving deps / launching (no concrete %).
        Status::Locking | Status::Launching => "\x1b]9;4;3;\x1b\\".to_string(),
        // Determinate progress for the build phases.
        Status::Fetching(p) | Status::Compiling(p) | Status::Building(p) => {
            format!("\x1b]9;4;1;{}\x1b\\", p)
        }
        // Error state paints the bar red.
        Status::Error => "\x1b]9;4;2;\x1b\\".to_string(),
        // Clear the bar once the build is done / app is running.
        Status::Running | Status::Idling => "\x1b]9;4;0;\x1b\\".to_string(),
    }
}

/// Emit the terminal progress escape for `status` directly to stdout, flushing
/// immediately so the terminal (not ratatui) paints the bar.
fn emit_terminal_progress(status: &Status) {
    let seq = progress_sequence(status);
    let mut out = stdout();
    let _ = out.write_all(seq.as_bytes());
    let _ = out.flush();
}

/// Translate a mouse cell `(col, row)` into a `(line, column)` text position
/// within the pane described by `view`. Cells outside the inner text area are
/// clamped to the nearest edge so dragging past the border still extends the
/// selection sensibly. Returns `None` when the pane has no visible rows.
fn hit_test(view: &PaneView, col: u16, row: u16) -> Option<(usize, usize)> {
    if view.visible_rows.is_empty() {
        return None;
    }
    let cy = row
        .saturating_sub(view.y)
        .min(view.visible_rows.len() as u16 - 1) as usize;
    let cx = col.saturating_sub(view.x) as usize;
    let vr = view.visible_rows[cy];
    let within = if vr.len == 0 { 0 } else { cx.min(vr.len - 1) };
    Some((vr.line, vr.start + within))
}

pub fn start(device: Device, pkg_name: String) -> anyhow::Result<()> {
    // The guard restores the terminal on drop (even on panic / early return).
    // Declared before `terminal` so it is dropped *after* it.
    let _guard = RawModeGuard::with_alternate_screen()?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = crossbeam::channel::unbounded();

    let mut state = AppState::new();

    // Starting inspector server
    let inspector_runtime = Runtime::new().context("failed to start inspector server tokio runtime")?;

    let inspector_server_address = match device.target {
        Targets::Ios | Targets::Android => IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        _ => IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
    };

    let inspector_handle = match InspectorServer::start(inspector_server_address, 9229, inspector_runtime.handle()) {
        Ok(handle) => handle,
        Err(e) => {
            let _ = tx.send(RunnerEvent::AppLog(format!("Failed to start inspector server: {}", e)));
            return Err(anyhow::anyhow!("failed to start inspector server: {e}"));
        }
    };

    let frames = ["⠋", "⠙", "⠚", "⠞", "⠖", "⠦", "⠴", "⠲", "⠳", "⠓"];
    let running_frame = ["▣", "▤", "▥", "▦", "▧", "▨", "▣", "▤", "▥", "▦"];
    let mut frame_index = 0;
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    // Drives the ConEmu `OSC 9;4` progress bar (e.g. the blue line at the top of
    // Ghostty). Computed once; the escape is only emitted when the status changes.
    let progress_supported = terminal_supports_progress();
    let mut last_progress: Option<Status> = None;

    let mut current_child = spawn_runner(device.clone(), pkg_name.clone(), tx.clone(), inspector_handle.address, inspector_handle.port);

    // Hot-reload file watcher
    let _watcher = {
        let tx_watch = tx.clone();
        let mut debounce_last = Instant::now();
        let mut watcher = notify::recommended_watcher(move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                use notify::EventKind;
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                        let dominated_by_rs = event
                            .paths
                            .iter()
                            .any(|p| p.extension().is_some_and(|ext| ext == "rs"));
                        if dominated_by_rs {
                            let now = Instant::now();
                            if now.duration_since(debounce_last) > Duration::from_millis(500) {
                                debounce_last = now;
                                let _ = tx_watch.send(RunnerEvent::HotReload);
                            }
                        }
                    }
                    _ => {}
                }
            }
        })
        .ok();
        if let Some(ref mut w) = watcher {
            let _ = w.watch(Path::new("src"), RecursiveMode::Recursive);
            // Also watch crates/ if it exists
            if Path::new("crates").exists() {
                let _ = w.watch(Path::new("crates"), RecursiveMode::Recursive);
            }
        }
        watcher
    };

    loop {
        // Process all pending events
        while let Ok(event) = rx.try_recv() {
            match event {
                RunnerEvent::BuildLog(msg) => state.push_build_log(msg),
                RunnerEvent::AppLog(msg) => state.push_app_log(msg),
                RunnerEvent::StatusChange(s) => state.apply_status(s),
                RunnerEvent::HotReload => {
                    // Only trigger reload if app is currently running or idling
                    match state.status {
                        Status::Running | Status::Idling | Status::Error => {
                            let _ = tx.send(RunnerEvent::AppLog("[hot-reload] File change detected, rebuilding...".to_string()));
                            if device.target == Targets::Web {
                                state.clear_build();
                                state.status = Status::Compiling(0);
                                pipeline::spawn_wasm_pack(tx.clone());
                            } else {
                                if let Some(mut child) = current_child.lock().unwrap().take() {
                                    let _ = child.kill();
                                }
                                state.clear_build();
                                state.clear_app();
                                state.status = Status::Compiling(0);
                                current_child = spawn_runner(
                                    device.clone(),
                                    pkg_name.clone(),
                                    tx.clone(),
                                    inspector_handle.address,
                                    inspector_handle.port,
                                );
                            }
                        }
                        _ => {} // Don't reload while already compiling/building
                    }
                }
            }
        }

        // Update the terminal progress bar only when the status actually changed,
        // so we don't spam the escape on every frame.
        if progress_supported && last_progress.as_ref() != Some(&state.status) {
            emit_terminal_progress(&state.status);
            last_progress = Some(state.status.clone());
        }

        let inspector_state = inspector_handle.state.lock().unwrap().clone();
        let inspector_address = inspector_handle.get_address();
        terminal.draw(|f| {
            ui::render(f, &mut state, &inspector_state, &inspector_address, &frames, &running_frame, frame_index);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('1'), _) => {
                            state.pane = ConsoleType::App;
                            state.clear_selection();
                        }
                        (KeyCode::Char('2'), _) => {
                            state.pane = ConsoleType::Build;
                            state.clear_selection();
                        }
                        (KeyCode::Char('3'), _) => {
                            state.pane = ConsoleType::Inspector;
                            state.clear_selection();
                        }
                        (KeyCode::Char('Q'), _) | (KeyCode::Char('q'), KeyModifiers::SHIFT) => {
                            // Kill child process if any
                            if let Some(mut child) = current_child.lock().unwrap().take() {
                                let _ = child.kill();
                            }
                            break;
                        }
                        (KeyCode::Char('r'), _) => {
                            if device.target == Targets::Web {
                                state.clear_build();
                                state.status = Status::Compiling(0);
                                pipeline::spawn_wasm_pack(tx.clone());
                            } else {
                                // Kill child process if running
                                if let Some(mut child) = current_child.lock().unwrap().take() {
                                    let _ = child.kill();
                                }
                                state.clear_build();
                                state.clear_app();
                                state.status = Status::Compiling(0);
                                current_child = spawn_runner(
                                    device.clone(),
                                    pkg_name.clone(),
                                    tx.clone(),
                                    inspector_handle.address,
                                    inspector_handle.port,
                                );
                            }
                        }

                        (KeyCode::Char('c'), KeyModifiers::SHIFT) | (KeyCode::Char('C'), KeyModifiers::SHIFT) => {
                            state.clear_selection();
                            match state.pane {
                                ConsoleType::App => state.app_logs.clear(),
                                ConsoleType::Build => state.build_logs.clear(),
                                _ => {}
                            }
                        }
                        #[cfg(target_os = "macos")]
                        (KeyCode::Char('c'), KeyModifiers::META) => {
                            if let Some(text) = state.selected_text() {
                                if let Ok(mut clipboard) = Clipboard::new() {
                                    let _ = clipboard.set_text(text);
                                }
                                state.clear_selection();
                            }
                        }

                        #[cfg(not(target_os = "macos"))]
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            if let Some(text) = state.selected_text() {
                                if let Ok(mut clipboard) = Clipboard::new() {
                                    let _ = clipboard.set_text(text);
                                }
                                state.clear_selection();
                            }
                        }

                        // Ctrl+C / Cmd+C: copy the active selection if there is
                        // one, else fall back to copying the whole pane.
                        (KeyCode::Char('c') | KeyCode::Char('C'), m)
                            if m.contains(KeyModifiers::CONTROL) || m.contains(KeyModifiers::SUPER) =>
                        {
                            let text = state.selected_text().unwrap_or_else(|| {
                                if state.pane == ConsoleType::Build { state.build_logs.join("\n") } else { state.app_logs.join("\n") }
                            });
                            if let Ok(mut clipboard) = Clipboard::new() {
                                let _ = clipboard.set_text(text);
                            }
                            state.clear_selection();
                        }

                        (KeyCode::Char('c'), _) | (KeyCode::Char('C'), _) => {
                            if let Ok(mut clipboard) = Clipboard::new() {
                                let logs =
                                    if state.pane == ConsoleType::Build { state.build_logs.join("\n") } else { state.app_logs.join("\n") };
                                let _ = clipboard.set_text(logs);
                            }
                        }

                        // Yank the active selection (Vim-style) to the clipboard.
                        (KeyCode::Char('y'), _) | (KeyCode::Char('Y'), _) => {
                            if let Some(text) = state.selected_text() {
                                if let Ok(mut clipboard) = Clipboard::new() {
                                    let _ = clipboard.set_text(text);
                                }
                                state.clear_selection();
                            }
                        }

                        (KeyCode::Char('s'), _) | (KeyCode::Char('S'), _) => {
                            // Toggle Vim-style selection ("visual") mode. The
                            // mouse stays captured by the app in both modes; this
                            // just decides whether a left-drag paints a character
                            // selection. Leaving the mode drops any highlight.
                            state.selection_mode = !state.selection_mode;
                            if !state.selection_mode {
                                state.clear_selection();
                            }
                        }

                        (KeyCode::Tab, _) => {
                            state.next_pane();
                            state.clear_selection();
                        }
                        (KeyCode::F(12), _) => {
                            inspector_handle.send_toggle();
                            state.pane = ConsoleType::Inspector;
                            state.clear_selection();
                        }
                        (KeyCode::Char('t'), _) if state.pane == ConsoleType::Inspector => {
                            state.inspector_full_tree = !state.inspector_full_tree;
                        }
                        (KeyCode::Up, _) => match state.pane {
                            ConsoleType::Build => state.build_pane.scroll_up(1),
                            ConsoleType::App => state.app_pane.scroll_up(1),
                            ConsoleType::Inspector => {
                                state.inspector_cursor = state.inspector_cursor.saturating_sub(1);
                                if (state.inspector_cursor as u16) < state.inspector_pane.scroll {
                                    state.inspector_pane.scroll = state.inspector_cursor as u16;
                                }
                            }
                        },
                        (KeyCode::Down, _) => match state.pane {
                            ConsoleType::Build => state.build_pane.scroll_down(1),
                            ConsoleType::App => state.app_pane.scroll_down(1),
                            ConsoleType::Inspector => {
                                state.inspector_cursor = state.inspector_cursor.saturating_add(1);
                            }
                        },
                        (KeyCode::PageUp, _) => match state.pane {
                            ConsoleType::Build => state.build_pane.scroll_up(10),
                            ConsoleType::App => state.app_pane.scroll_up(10),
                            ConsoleType::Inspector => {
                                state.inspector_cursor = state.inspector_cursor.saturating_sub(10);
                                if (state.inspector_cursor as u16) < state.inspector_pane.scroll {
                                    state.inspector_pane.scroll = state.inspector_cursor as u16;
                                }
                            }
                        },
                        (KeyCode::PageDown, _) => match state.pane {
                            ConsoleType::Build => state.build_pane.scroll_down(10),
                            ConsoleType::App => state.app_pane.scroll_down(10),
                            ConsoleType::Inspector => {
                                state.inspector_cursor = state.inspector_cursor.saturating_add(10);
                            }
                        },
                        _ => {}
                    }
                }
                Event::Mouse(mouse_event) => {
                    let (col, row) = (mouse_event.column, mouse_event.row);
                    match mouse_event.kind {
                        MouseEventKind::ScrollUp => match state.pane {
                            ConsoleType::Build => state.build_pane.scroll_up(2),
                            ConsoleType::App => state.app_pane.scroll_up(2),
                            ConsoleType::Inspector => state.inspector_pane.scroll_up(2),
                        },
                        MouseEventKind::ScrollDown => match state.pane {
                            ConsoleType::Build => state.build_pane.scroll_down(2),
                            ConsoleType::App => state.app_pane.scroll_down(2),
                            ConsoleType::Inspector => state.inspector_pane.scroll_down(2),
                        },
                        // Begin a selection at the clicked cell.
                        MouseEventKind::Down(MouseButton::Left) if state.selection_mode => {
                            if let Some(pos) = state.last_view.as_ref().and_then(|v| hit_test(v, col, row)) {
                                state.selection = Some(Selection { anchor: pos, cursor: pos });
                                state.selecting = true;
                            }
                        }
                        // Extend the selection while dragging, auto-scrolling at
                        // the top/bottom edges of the pane.
                        MouseEventKind::Drag(MouseButton::Left) if state.selection_mode && state.selecting => {
                            if let Some((vy, vh)) = state.last_view.as_ref().map(|v| (v.y, v.height)) {
                                if row < vy {
                                    match state.pane {
                                        ConsoleType::Build => state.build_pane.scroll_up(1),
                                        ConsoleType::App => state.app_pane.scroll_up(1),
                                        _ => {}
                                    }
                                } else if vh > 0 && row >= vy + vh {
                                    match state.pane {
                                        ConsoleType::Build => state.build_pane.scroll_down(1),
                                        ConsoleType::App => state.app_pane.scroll_down(1),
                                        _ => {}
                                    }
                                }
                            }
                            if let Some(pos) = state.last_view.as_ref().and_then(|v| hit_test(v, col, row))
                                && let Some(sel) = state.selection.as_mut()
                            {
                                sel.cursor = pos;
                            }
                        }
                        MouseEventKind::Up(MouseButton::Left) => state.selecting = false,
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick_rate {
            frame_index = (frame_index + 1) % frames.len();
            last_tick = Instant::now();
        }
    }

    // Always clear the progress bar on exit, otherwise it stays stuck after
    // the CLI quits and leaves the alternate screen.
    if progress_supported {
        emit_terminal_progress(&Status::Idling);
    }

    // Terminal restoration is handled by `_guard` on drop.
    Ok(())
}

/// Start the console in non-interactive (no-TUI) mode.
///
/// Prints build and app logs directly to stdout/stderr without creating an
/// alternate screen or using ratatui. Designed for IDE and CI integrations
/// where no terminal device is available.
pub fn start_no_tui(device: Device, pkg_name: String) -> anyhow::Result<()> {
    let (tx, rx) = crossbeam::channel::unbounded();

    // Starting inspector server
    let inspector_runtime = Runtime::new().context("failed to start inspector server tokio runtime")?;

    let inspector_server_address = match device.target {
        Targets::Ios | Targets::Android => IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
        _ => IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
    };

    let inspector_handle = match InspectorServer::start(inspector_server_address, 9229, inspector_runtime.handle()) {
        Ok(handle) => handle,
        Err(e) => {
            let _ = tx.send(RunnerEvent::AppLog(format!("Failed to start inspector server: {}", e)));
            return Err(anyhow::anyhow!("failed to start inspector server: {e}"));
        }
    };

    let mut current_child = spawn_runner(device.clone(), pkg_name.clone(), tx.clone(), inspector_handle.address, inspector_handle.port);

    // Hot-reload file watcher
    let _watcher = {
        let tx_watch = tx.clone();
        let mut debounce_last = Instant::now();
        let mut watcher = notify::recommended_watcher(move |res: Result<NotifyEvent, notify::Error>| {
            if let Ok(event) = res {
                use notify::EventKind;
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_) => {
                        let dominated_by_rs = event
                            .paths
                            .iter()
                            .any(|p| p.extension().is_some_and(|ext| ext == "rs"));
                        if dominated_by_rs {
                            let now = Instant::now();
                            if now.duration_since(debounce_last) > Duration::from_millis(500) {
                                debounce_last = now;
                                let _ = tx_watch.send(RunnerEvent::HotReload);
                            }
                        }
                    }
                    _ => {}
                }
            }
        })
        .ok();
        if let Some(ref mut w) = watcher {
            let _ = w.watch(Path::new("src"), RecursiveMode::Recursive);
            if Path::new("crates").exists() {
                let _ = w.watch(Path::new("crates"), RecursiveMode::Recursive);
            }
        }
        watcher
    };

    // Simple blocking event loop — print logs to stdout/stderr.
    while let Ok(event) = rx.recv()  {
        match event {
            RunnerEvent::BuildLog(msg) => {
                eprintln!("[build] {}", msg);
            }
            RunnerEvent::AppLog(msg) => {
                println!("{}", msg);
            }
            RunnerEvent::StatusChange(status) => {
                match &status {
                    Status::Compiling(pct) => eprintln!("[status] Compiling {}%", pct),
                    Status::Building(pct) => eprintln!("[status] Building {}%", pct),
                    Status::Fetching(pct) => eprintln!("[status] Fetching {}%", pct),
                    Status::Launching => eprintln!("[status] Launching..."),
                    Status::Running => eprintln!("[status] Running"),
                    Status::Error => eprintln!("[status] Error"),
                    Status::Locking => eprintln!("[status] Locking..."),
                    Status::Idling => {}
                }
            }
            RunnerEvent::HotReload => {
                eprintln!("[hot-reload] File change detected, rebuilding...");
                if device.target == Targets::Web {
                    pipeline::spawn_wasm_pack(tx.clone());
                } else {
                    if let Some(mut child) = current_child.lock().unwrap().take() {
                        let _ = child.kill();
                    }
                    current_child = spawn_runner(
                        device.clone(),
                        pkg_name.clone(),
                        tx.clone(),
                        inspector_handle.address,
                        inspector_handle.port,
                    );
                }
            }
        }
    }

    Ok(())
}
