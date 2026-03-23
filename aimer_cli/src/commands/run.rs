pub mod console;
pub mod ios;
pub mod ios_sim;
pub mod macos;
pub mod web;
pub mod android;
pub mod android_sim;

use crate::targets::Targets;
use crate::targets::Targets::Terminated;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode},
};

use std::{fmt, process};
use std::io::{ Write, stdout};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Device {
    pub name: String,
    pub target: Targets,
    pub id: String,
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.target)
    }
}

fn fetch_devices() -> Vec<Device> {
    let mut devices = vec![
        Device { name: "macOS Desktop".to_string(), target: Targets::Macos, id: "local".to_string() },
        Device { name: "Web Browser".to_string(), target: Targets::Web, id: "web".to_string() },
    ];

    // Android Devices
    if let Ok(output) = Command::new("adb").args(["devices", "-l"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains(" device ") || line.contains(" emulator ") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(id) = parts.first() {
                        let connection_type = if id.contains('.') && id.contains(':') { "Wireless" } else { "Wired" };

                        let mut device_name = "Android Device".to_string();
                        // Try to find a better name from 'adb devices -l' output
                        for part in &parts {
                            if part.starts_with("model:") {
                                device_name = part["model:".len()..].replace('_', " ");
                                break;
                            }
                        }

                        let pretty_name_cmd = Command::new("adb")
                            .args(["-s", id, "emu", "avd", "name"])
                            .output();

                        if let Ok(output) = pretty_name_cmd {
                            if output.status.success() {
                                let output_str = String::from_utf8_lossy(&output.stdout);
                                if let Some(name) = output_str.split_whitespace().next() {
                                    device_name = name.to_string();
                                }
                            }
                        }

                        devices.push(Device {
                            name: format!("{} ({})", device_name, connection_type),
                            target: Targets::Android,
                            id: id.to_string(),
                        });
                    }
                }
            }
        }
    }

    // iOS Simulators (Booted) and physical devices via xcrun xctrace
    if let Ok(output) = Command::new("xcrun")
        .args(["simctl", "list", "devices"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains("(Booted)") {
                    if let Some(start) = line.find('(') {
                        let name = line[..start].trim().to_string();
                        let rest = &line[start..];
                        let mut id = "".to_string();
                        if let Some(udid_start) = rest.find('(') {
                            if let Some(udid_end) = rest[udid_start + 1..].find(')') {
                                id = rest[udid_start + 1..udid_start + 1 + udid_end].to_string();
                            }
                        }
                        devices.push(Device { name, target: Targets::IosSimulator, id });
                    }
                }
            }
        }
    }

    // iOS Physical devices via devicectl (iOS 17+)
    if let Ok(output) = Command::new("xcrun")
        .args(["devicectl", "list", "devices"])
        .output()
    {
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
                            let connection_type =
                                if parts[1].trim().ends_with(".coredevice.local") { "Wireless/Wired" } else { "Wired" };
                            if devices.iter().any(|d| d.id != identifier) {
                                devices.push(Device {
                                    name: format!("{} ({})", name, connection_type),
                                    target: Targets::Ios,
                                    id: identifier,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // iOS Physical devices via xctrace fallback
    if let Ok(output) = Command::new("xcrun")
        .args(["xctrace", "list", "devices"])
        .output()
    {
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
                            let id = line[start + 1..end].to_string();
                            if !id.contains("-") || id.len() == 40 || id.len() == 25 {
                                devices.push(Device { name: format!("{} (Wired)", name), target: Targets::Ios, id });
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

    unique_devices.push(Device { name: "Quit".to_string(), target: Terminated, id: "q".to_string() });

    unique_devices
}


// fn check_project() -> Result<(), Box<dyn std::error::Error>> {
//
//     let current_dir = std::env::current_dir()?;
//
//
//     Ok(())
// }

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
        for _ in 0..last_devices_len + 1 {
            // +1 for the prompt text
            execute!(stdout, Clear(ClearType::CurrentLine), cursor::MoveUp(1)).unwrap();
        }
        execute!(stdout, Clear(ClearType::CurrentLine)).unwrap();

        writeln!(stdout, "\x1b[36m◆\x1b[0m  \x1b[1mSelect a device to launch (Press 'q' to quit):\x1b[0m\r").unwrap();
        for (i, device) in devices.iter().enumerate() {
            execute!(stdout, cursor::MoveToColumn(0)).unwrap();
            if i == selected_index {
                execute!(stdout, SetForegroundColor(Color::DarkGrey)).unwrap();
                write!(stdout, "│  ").unwrap();
                execute!(stdout, SetForegroundColor(Color::Green)).unwrap();
                writeln!(stdout, "● {}\r", device).unwrap();
                execute!(stdout, ResetColor).unwrap();
            } else {
                execute!(stdout, SetForegroundColor(Color::DarkGrey)).unwrap();
                write!(stdout, "│  ").unwrap();
                execute!(stdout, SetForegroundColor(Color::DarkGrey)).unwrap();
                writeln!(stdout, "○ {}\r", device).unwrap();
                execute!(stdout, ResetColor).unwrap();
            }
        }
        execute!(stdout, SetForegroundColor(Color::DarkGrey)).unwrap();
        writeln!(stdout, "└\r").unwrap();
        execute!(stdout, ResetColor).unwrap();
        
        last_devices_len = devices.len() + 1;

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
                        break Device { name: "Quit".to_string(), target: Terminated, id: "q".to_string() };
                    }
                    KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                        disable_raw_mode().unwrap();
                        execute!(stdout, cursor::Show).unwrap();
                        process::exit(1);
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
        process::exit(0);
    }

    // println!("Launching on: {}", selected_device);

    let pkg_name = std::fs::read_to_string("Cargo.toml")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("name ="))
                .map(|l| l.split('"').nth(1).unwrap_or("").to_string())
        })
        .unwrap_or_else(|| "aimer_template".to_string());

    console::start(selected_device, pkg_name).unwrap();
}
