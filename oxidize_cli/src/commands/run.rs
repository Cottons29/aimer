pub mod console;

use std::process::Command;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};
use std::io::{stdout, Write};

#[derive(Clone, Debug)]
pub struct Device {
    pub name: String,
    pub target: String,
    pub id: String,
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.target)
    }
}

fn fetch_devices() -> Vec<Device> {
    let mut devices = vec![
        Device {
            name: "macOS Desktop".to_string(),
            target: "macos".to_string(),
            id: "local".to_string(),
        },
        Device {
            name: "Web Browser".to_string(),
            target: "web".to_string(),
            id: "web".to_string(),
        },
    ];

    // Android Devices
    if let Ok(output) = Command::new("adb").args(["devices", "-l"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains(" device ") || line.contains(" emulator ") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(id) = parts.first() {
                        let mut name = id.to_string();
                        for part in parts.iter().skip(1) {
                            if part.starts_with("model:") {
                                name = part.trim_start_matches("model:").replace('_', " ");
                            }
                        }
                        let connection_type = if id.contains('.') && id.contains(':') {
                            "Wireless"
                        } else {
                            "Wired"
                        };
                        devices.push(Device {
                            name: format!("{} ({})", name, connection_type),
                            target: "android".to_string(),
                            id: id.to_string(),
                        });
                    }
                }
            }
        }
    }

    // iOS Simulators (Booted) and physical devices via xcrun xctrace
    if let Ok(output) = Command::new("xcrun").args(["simctl", "list", "devices"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("(Booted)") {
                    if let Some(start) = line.find('(') {
                        let name = line[..start].trim().to_string();
                        let rest = &line[start..];
                        let mut id = "".to_string();
                        if let Some(udid_start) = rest.find('(') {
                            if let Some(udid_end) = rest[udid_start+1..].find(')') {
                                id = rest[udid_start+1..udid_start+1+udid_end].to_string();
                            }
                        }
                        devices.push(Device {
                            name,
                            target: "ios".to_string(),
                            id,
                        });
                    }
                }
            }
        }
    }

    // iOS Physical devices via devicectl (iOS 17+)
    if let Ok(output) = Command::new("xcrun").args(["devicectl", "list", "devices"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout.lines().collect();
            let mut in_devices = false;
            for line in lines {
                if line.starts_with("---------") {
                    in_devices = true;
                    continue;
                }
                if in_devices && !line.is_empty() {
                    let parts: Vec<&str> = line.split("   ").filter(|s| !s.trim().is_empty()).collect();
                    if parts.len() >= 4 {
                        let name = parts[0].trim().to_string();
                        let identifier = parts[2].trim().to_string();
                        let state = parts[3].trim();
                        if state.to_lowercase() == "available" {
                            let connection_type = if parts[1].trim().ends_with(".coredevice.local") {
                                "Wireless/Wired"
                            } else {
                                "Wired"
                            };
                            devices.push(Device {
                                name: format!("{} ({})", name, connection_type),
                                target: "ios".to_string(),
                                id: identifier,
                            });
                        }
                    }
                }
            }
        }
    }

    // iOS Physical devices via xctrace fallback
    if let Ok(output) = Command::new("xcrun").args(["xctrace", "list", "devices"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut is_offline = false;
            let mut is_simulator = false;
            for line in stdout.lines() {
                if line.starts_with("== Devices Offline ==") {
                    is_offline = true;
                } else if line.starts_with("== Simulators ==") {
                    is_simulator = true;
                } else if line.starts_with("== Devices ==") {
                    is_offline = false;
                    is_simulator = false;
                } else if !line.is_empty() && !is_offline && !is_simulator {
                    if let Some(start) = line.rfind('(') {
                        if let Some(end) = line.rfind(')') {
                            let name = line[..start].trim().to_string();
                            let id = line[start+1..end].to_string();
                            if !id.contains("-") || id.len() == 40 || id.len() == 25 {
                                devices.push(Device {
                                    name: format!("{} (Wired)", name),
                                    target: "ios".to_string(),
                                    id,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let mut unique_devices = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for dev in devices {
        if !seen_ids.contains(&dev.id) {
            seen_ids.insert(dev.id.clone());
            unique_devices.push(dev);
        }
    }

    unique_devices.push(Device {
        name: "Quit".to_string(),
        target: "exit".to_string(),
        id: "q".to_string(),
    });

    unique_devices
}

pub fn execute() {
    println!("Finding available devices...\n");

    let devices_arc = Arc::new(Mutex::new(fetch_devices()));
    let devices_arc_clone = Arc::clone(&devices_arc);

    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(1));
            let new_devices = fetch_devices();
            let mut devs = devices_arc_clone.lock().unwrap();
            *devs = new_devices;
        }
    });

    let mut selected_index = 0;
    let mut stdout = stdout();

    enable_raw_mode().unwrap();
    execute!(stdout, cursor::Hide).unwrap();

    let mut last_devices_len = 0;

    let selected_device = loop {
        let devices = { devices_arc.lock().unwrap().clone() };
        if selected_index >= devices.len() {
            selected_index = devices.len().saturating_sub(1);
        }

        // Render menu
        execute!(stdout, cursor::MoveToColumn(0)).unwrap();
        // Clear previous lines
        for _ in 0..last_devices_len + 1 { // +1 for the prompt text
            execute!(stdout, Clear(ClearType::CurrentLine), cursor::MoveUp(1)).unwrap();
        }
        execute!(stdout, Clear(ClearType::CurrentLine)).unwrap();

        writeln!(stdout, "Select a device to launch (or 'q' to quit): ").unwrap();
        for (i, device) in devices.iter().enumerate() {
            execute!(stdout, cursor::MoveToColumn(0)).unwrap();
            if i == selected_index {
                execute!(stdout, SetForegroundColor(Color::Green)).unwrap();
                writeln!(stdout, "> {}", device).unwrap();
                execute!(stdout, ResetColor).unwrap();
            } else {
                writeln!(stdout, "  {}", device).unwrap();
            }
        }
        last_devices_len = devices.len();

        if event::poll(Duration::from_millis(100)).unwrap() {
            if let Event::Key(key_event) = event::read().unwrap() {
                match key_event.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if selected_index > 0 {
                            selected_index -= 1;
                        } else {
                            selected_index = devices.len().saturating_sub(1);
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if selected_index < devices.len().saturating_sub(1) {
                            selected_index += 1;
                        } else {
                            selected_index = 0;
                        }
                    }
                    KeyCode::Enter => {
                        break devices[selected_index].clone();
                    }
                    KeyCode::Char('q') => {
                        break Device {
                            name: "Quit".to_string(),
                            target: "exit".to_string(),
                            id: "q".to_string(),
                        };
                    }
                    KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                        disable_raw_mode().unwrap();
                        execute!(stdout, cursor::Show).unwrap();
                        std::process::exit(1);
                    }
                    _ => {}
                }
            }
        }
    };

    disable_raw_mode().unwrap();
    execute!(stdout, cursor::Show).unwrap();
    
    // Clear the menu for clean exit
    for _ in 0..last_devices_len + 1 {
        execute!(stdout, cursor::MoveUp(1), Clear(ClearType::CurrentLine)).unwrap();
    }


    if selected_device.id == "q" {
        println!("Exiting.");
        std::process::exit(0);
    }

    println!("Launching on: {}", selected_device);

    let pkg_name = std::fs::read_to_string("Cargo.toml")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("name ="))
                .map(|l| l.split('"').nth(1).unwrap_or("").to_string())
        })
        .unwrap_or_else(|| "oxidize_template".to_string());

    console::start(selected_device, pkg_name).unwrap();
}

