use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::io::{stdout, Write, BufReader, BufRead};
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use std::thread;
use std::process::{Command, Stdio, Child};

use super::Device;

#[derive(Clone, PartialEq, Eq)]
pub enum Status {
    Fetching(u8),
    Compiling(u8),
    Building(u8),
    Launching,
    Running,
    Idling,
}

pub enum RunnerEvent {
    BuildLog(String),
    AppLog(String),
    StatusChange(Status),
}

fn spawn_runner(device: Device, pkg_name: String, tx: std::sync::mpsc::Sender<RunnerEvent>) -> Arc<Mutex<Option<Child>>> {
    let current_child = Arc::new(Mutex::new(None));
    let current_child_clone = Arc::clone(&current_child);

    thread::spawn(move || {
        if device.target != "macos" {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Target {} is not yet supported for on-the-fly run.", device.target)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
            return;
        }

        let _ = tx.send(RunnerEvent::StatusChange(Status::Compiling(0)));
        let _ = tx.send(RunnerEvent::BuildLog("Compiling static library...".to_string()));

        let mut cargo_build = Command::new("cargo")
            .arg("build")
            .arg("--lib")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start cargo build");

        let stdout = cargo_build.stdout.take().unwrap();
        let stderr = cargo_build.stderr.take().unwrap();

        *current_child_clone.lock().unwrap() = Some(cargo_build);
        
        let tx_clone1 = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(l) = line {
                    let _ = tx_clone1.send(RunnerEvent::BuildLog(l));
                }
            }
        });
        
        
        let tx_clone2 = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            let mut fetch_count = 0;
            let mut compile_count = 0;
            for line in reader.lines() {
                if let Ok(l) = line {
                    if l.contains("Fetching") || l.contains("Updating") {
                        fetch_count = (fetch_count + 5).min(99);
                        let _ = tx_clone2.send(RunnerEvent::StatusChange(Status::Fetching(fetch_count)));
                    } else if l.contains("Compiling") {
                        compile_count = (compile_count + 2).min(99);
                        let _ = tx_clone2.send(RunnerEvent::StatusChange(Status::Compiling(compile_count)));
                    } else if l.contains("Finished") {
                        let _ = tx_clone2.send(RunnerEvent::StatusChange(Status::Compiling(100)));
                    }
                    let _ = tx_clone2.send(RunnerEvent::BuildLog(l));
                }
            }
        });


        let status = loop {
            let mut guard = current_child_clone.lock().unwrap();
            if let Some(child) = guard.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    break status;
                }
            } else {
                return; // Child was removed (killed)
            }
            drop(guard);
            thread::sleep(Duration::from_millis(100));
        };

        if !status.success() {
            let _ = tx.send(RunnerEvent::BuildLog("Cargo build failed.".to_string()));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
            return;
        }

        let lib_name = pkg_name.replace("-", "_");
        let src_lib = format!("target/debug/lib{}.a", lib_name);
        let dest_dir = "builds/staticlib/macos";
        let dest_lib = format!("{}/lib{}.a", dest_dir, lib_name);

        std::fs::create_dir_all(dest_dir).unwrap();
        if let Err(e) = std::fs::copy(&src_lib, &dest_lib) {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Failed to copy static library: {}", e)));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
            return;
        } else {
            let _ = tx.send(RunnerEvent::BuildLog(format!("Copied static library to {}", dest_lib)));
        }

        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x86_64",
            _ => "arm64",
        };

        let _ = tx.send(RunnerEvent::BuildLog(format!("Building Xcode project for {}...", arch)));

        let mut xcode_build = Command::new("xcodebuild")
            .arg("-project")
            .arg(format!("{}.xcodeproj", pkg_name))
            .arg("-target")
            .arg(&pkg_name)
            .arg("-configuration")
            .arg("Debug")
            .arg("SYMROOT=build")
            .arg("-arch")
            .arg(arch)
            .current_dir("builds/macos")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start xcodebuild");

        let stdout = xcode_build.stdout.take().unwrap();
        let stderr = xcode_build.stderr.take().unwrap();

        *current_child_clone.lock().unwrap() = Some(xcode_build);
        
        
        let tx_clone3 = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            let mut build_count = 0;
            for line in reader.lines() {
                if let Ok(l) = line {
                    if l.contains("Compile") || l.contains("Process") || l.contains("Link") {
                        build_count = (build_count + 2).min(99);
                        let _ = tx_clone3.send(RunnerEvent::StatusChange(Status::Building(build_count)));
                    } else if l.contains("** BUILD SUCCEEDED **") {
                        let _ = tx_clone3.send(RunnerEvent::StatusChange(Status::Building(100)));
                    }
                    let _ = tx_clone3.send(RunnerEvent::BuildLog(l));
                }
            }
        });

        
        let tx_clone4 = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(l) = line {
                    let _ = tx_clone4.send(RunnerEvent::BuildLog(l));
                }
            }
        });

        let status = loop {
            let mut guard = current_child_clone.lock().unwrap();
            if let Some(child) = guard.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    break status;
                }
            } else {
                return;
            }
            drop(guard);
            thread::sleep(Duration::from_millis(100));
        };

        if !status.success() {
            let _ = tx.send(RunnerEvent::BuildLog("Xcodebuild failed.".to_string()));
            let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
            return;
        }

        let _ = tx.send(RunnerEvent::StatusChange(Status::Launching));
        let _ = tx.send(RunnerEvent::AppLog("Launching macOS app...".to_string()));

        let app_exec_path = format!("builds/macos/build/Debug/{}.app/Contents/MacOS/{}", pkg_name, pkg_name);
        
        let mut app_run = Command::new(&app_exec_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to launch macOS app");

        let stdout = app_run.stdout.take().unwrap();
        let stderr = app_run.stderr.take().unwrap();

        *current_child_clone.lock().unwrap() = Some(app_run);
        
        let tx_clone5 = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                if let Ok(l) = line {
                    let _ = tx_clone5.send(RunnerEvent::AppLog(l));
                }
            }
        });
        
        let tx_clone6 = tx.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                if let Ok(l) = line {
                    let _ = tx_clone6.send(RunnerEvent::AppLog(l));
                }
            }
        });

        let _ = tx.send(RunnerEvent::StatusChange(Status::Running));
        
        loop {
            let mut guard = current_child_clone.lock().unwrap();
            if let Some(child) = guard.as_mut() {
                if let Ok(Some(_)) = child.try_wait() {
                    break;
                }
            } else {
                return;
            }
            drop(guard);
            thread::sleep(Duration::from_millis(100));
        }
        
        let _ = tx.send(RunnerEvent::StatusChange(Status::Idling));
    });

    current_child
}

pub fn start(device: Device, pkg_name: String) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, rx) = std::sync::mpsc::channel();
    
    let mut build_logs: Vec<String> = Vec::new();
    let mut app_logs: Vec<String> = Vec::new();
    let mut current_status = Status::Compiling(0);
    let mut current_pane = 1; // 0 for Build, 1 for App
    
    // let frames = ["▁","▂","▃","▄","▅","▆","▇","█","▇","▆","▅","▄","▃","▂"];
    // let frames = ["⠁","⠂","⠄","⡀","⢀","⠠","⠐","⠈"];
    let frames = ["⠋","⠙","⠚","⠞","⠖","⠦","⠴","⠲","⠳","⠓"];
    let running_frame = ["▣","▤","▥","▦","▧","▨","▣","▤","▥","▦"];
    let mut frame_index = 0;
    let tick_rate = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    let mut current_child = spawn_runner(device.clone(), pkg_name.clone(), tx.clone());

    loop {
        // Process all pending events
        while let Ok(event) = rx.try_recv() {
            match event {
                RunnerEvent::BuildLog(msg) => {
                    build_logs.push(msg);
                    if build_logs.len() > 1000 {
                        build_logs.remove(0);
                    }
                }
                RunnerEvent::AppLog(msg) => {
                    app_logs.push(msg);
                    if app_logs.len() > 1000 {
                        app_logs.remove(0);
                    }
                }
                RunnerEvent::StatusChange(s) => {
                    current_status = s;
                }
            }
        }

        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(1), Constraint::Length(1)].as_ref())
                .split(f.area());

            let build_text = build_logs.iter().cloned().map(Line::from).collect::<Vec<_>>();
            let app_text = app_logs.iter().cloned().map(Line::from).collect::<Vec<_>>();

            let build_block = Block::default()
                .borders(Borders::ALL)
                .title("Build Logs")
                .border_style(Style::default().fg(if current_pane == 0 { Color::Yellow } else { Color::White }));
                
            let app_block = Block::default()
                .borders(Borders::ALL)
                .title("App Logs")
                .border_style(Style::default().fg(if current_pane == 1 { Color::Yellow } else { Color::White }));

            let area = chunks[0];
            let height = area.height.saturating_sub(2) as usize;

            if current_pane == 0 {
                let start = if build_logs.len() > height { build_logs.len() - height } else { 0 };
                let p = Paragraph::new(build_text[start..].to_vec()).block(build_block);
                f.render_widget(p, area);
            } else {
                let start = if app_logs.len() > height { app_logs.len() - height } else { 0 };
                let p = Paragraph::new(app_text[start..].to_vec()).block(app_block);
                f.render_widget(p, area);
            }

            let (status_icon, status_text) = match current_status {
                Status::Fetching(p) => (frames[frame_index], format!("Fetching {}%", p)),
                Status::Compiling(p) => (frames[frame_index], format!("Compiling {}%", p)),
                Status::Building(p) => (frames[frame_index], format!("Building {}%", p)),
                Status::Launching => (frames[frame_index], "Launching...".to_string()),
                Status::Running => (running_frame[frame_index], "Running".to_string()),
                Status::Idling => ("✓", "Idling".to_string()),
            };

            let bottom_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Length(50)].as_ref())
                .split(chunks[1]);

            let status_color = match current_status {
                Status::Fetching(_) => Color::Blue,
                Status::Compiling(_) => Color::Yellow,
                Status::Building(_) => Color::Cyan,
                Status::Launching => Color::Magenta,
                Status::Running => Color::Green,
                Status::Idling => Color::DarkGray,
            };

            let status_line = Line::from(vec![
                Span::raw(" Status: "),
                Span::styled(format!("{} {}", status_icon, status_text), Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
            ]);

            let controls_line = Line::from(vec![
                Span::styled("[r] ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
                Span::raw("reload | "),
                Span::styled("[Shift+Q] ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
                Span::raw("exit | "),
                Span::styled("[Tab] ", Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)),
                Span::raw("switch pane "),
            ]);

            let status_bar = Paragraph::new(status_line).style(Style::default());
            let controls_bar = Paragraph::new(controls_line).style(Style::default()).alignment(ratatui::layout::Alignment::Right);

            f.render_widget(status_bar, bottom_chunks[0]);
            f.render_widget(controls_bar, bottom_chunks[1]);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('Q'), _) | (KeyCode::Char('q'), KeyModifiers::SHIFT) => {
                        // Kill child process if any
                        if let Some(mut child) = current_child.lock().unwrap().take() {
                            let _ = child.kill();
                        }
                        break;
                    }
                    (KeyCode::Char('r'), _) => {
                        // Kill child
                        if let Some(mut child) = current_child.lock().unwrap().take() {
                            let _ = child.kill();
                        }
                        build_logs.clear();
                        app_logs.clear();
                        current_status = Status::Compiling(0);
                        current_child = spawn_runner(device.clone(), pkg_name.clone(), tx.clone());
                    }
                    (KeyCode::Tab, _) => {
                        current_pane = (current_pane + 1) % 2;
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            frame_index = (frame_index + 1) % frames.len();
            last_tick = Instant::now();
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
