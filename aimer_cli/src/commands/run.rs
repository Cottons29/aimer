pub mod android;
pub(crate) mod cargo_build;
pub(crate) mod cargo_message;
pub(crate) mod capability_sources;
pub mod desktop;
pub(crate) mod framed_block;
pub(crate) mod helpers;
pub(crate) mod hot_reload_runtime;
pub mod ios;
pub mod macos;
pub(crate) mod panic_report;
pub mod pipeline;
pub mod utilities;
pub mod web;

use anyhow::{Context, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::{Color, ResetColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{cursor, execute};
use std::io::{Write, stdout};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{fmt, process, thread};

use crate::commands::run::utilities::get_project_root;
use crate::config::{ApplicationRuntime, BuildProfile, ExecutionPolicy, ReloadPolicy};
use crate::console;
use crate::errors::AimerError;
use crate::targets::Targets;
use crate::targets::Targets::Terminated;
use crate::tui::RawModeGuard;

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
        #[cfg(target_os = "macos")]
        Device {
            name: "macOS Desktop".to_string(),
            target: Targets::Macos,
            id: "local".to_string(),
        },
        #[cfg(target_os = "windows")]
        Device {
            name: "Windows Desktop".to_string(),
            target: Targets::Windows,
            id: "windows".to_string(),
        },
        #[cfg(target_os = "linux")]
        Device {
            name: "Linux Desktop".to_string(),
            target: Targets::Linux,
            id: "linux".to_string(),
        },
        Device {
            name: "Web Browser".to_string(),
            target: Targets::Web,
            id: "web".to_string(),
        },
    ];

    // Android Devices
    #[allow(clippy::collapsible_if)]
    if let Ok(output) = Command::new("adb").args(["devices", "-l"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.contains(" device ") || line.contains(" emulator ") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if let Some(id) = parts.first() {
                        let connection_type = if id.contains('.') && id.contains(':') {
                            "Wireless"
                        } else {
                            "Wired"
                        };

                        let mut device_name = "Android Device".to_string();
                        // Try to find a better name from 'adb devices -l' output
                        for part in &parts {
                            if let Some(model) = part.strip_prefix("model:") {
                                device_name = model.replace('_', " ");
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
    #[allow(clippy::collapsible_if)]
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
                        devices.push(Device {
                            name,
                            target: Targets::IosSimulator,
                            id,
                        });
                    }
                }
            }
        }
    }

    // iOS Physical devices via devicectl (iOS 17+)
    #[allow(clippy::collapsible_if)]
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
                    let parts: Vec<&str> =
                        line.split("   ").filter(|s| !s.trim().is_empty()).collect();
                    if parts.len() >= 4 {
                        let name = parts[0].trim().to_string();
                        let identifier = parts[2].trim().to_string();
                        let state = parts[3].trim();
                        if state.to_lowercase() == "available" {
                            let connection_type = if parts[1].trim().ends_with(".coredevice.local")
                            {
                                "Wireless/Wired"
                            } else {
                                "Wired"
                            };
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
    #[allow(clippy::collapsible_if)]
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
                                devices.push(Device {
                                    name: format!("{} (Wired)", name),
                                    target: Targets::Ios,
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
        target: Terminated,
        id: "q".to_string(),
    });

    unique_devices
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PipelineSelection {
    Native { release: bool },
    HotReload,
}

fn native_pipeline_release(policy: ExecutionPolicy) -> PipelineSelection {
    match (policy.runtime(), policy.reload()) {
        (ApplicationRuntime::NativeAot, ReloadPolicy::Disabled) => {
            PipelineSelection::Native {
                release: policy.profile() == BuildProfile::Release,
            }
        }
        (ApplicationRuntime::Wasmi, ReloadPolicy::HotReload) => PipelineSelection::HotReload,
        _ => unreachable!("ExecutionPolicy guarantees a supported runtime and reload combination"),
    }
}

#[inline]
fn manifest_is_optional(policy: ExecutionPolicy) -> bool {
    native_pipeline_release(policy) == PipelineSelection::HotReload
}

pub fn execute(
    target: Option<String>,
    device: Option<String>,
    no_tui: bool,
    policy: ExecutionPolicy,
    verbose_widget_ir: bool,
    inline_render: bool,
) -> anyhow::Result<()> {
    let selection = native_pipeline_release(policy);

    let project_root = get_project_root(false)
        .map_err(|_| anyhow!("aimer.toml not found in project root"))?;
    if crate::config::AimerManifest::load_from(&project_root)?.is_none()
        && !manifest_is_optional(policy)
    {
        return Err(anyhow!("aimer.toml not found in project root"));
    }
    let selected_device = if target.is_some() || device.is_some() || no_tui {
        // Non-interactive (scriptable) mode.
        resolve_device(target, device)?
    } else {
        // Interactive picker mode.
        println!("Finding available devices...\n");

        let devices_arc = Arc::new(Mutex::new(fetch_devices()));
        let devices_arc_clone = Arc::clone(&devices_arc);
        let has_selected = Arc::new(AtomicBool::new(false));
        let selected_clone = has_selected.clone();
        thread::spawn(move || {
            loop {
                if selected_clone.load(Ordering::Relaxed) {
                    break;
                }
                thread::sleep(Duration::from_millis(200));
                let new_devices = fetch_devices();
                if let Ok(mut devs) = devices_arc_clone.lock() {
                    *devs = new_devices;
                }


            }
        });

        match pick_device(&devices_arc)? {
            Some(d) => {
                has_selected.store(true, Ordering::Relaxed);
                d
            }
            None => {
                println!("Exiting.");
                return Ok(());
            }
        }
    };

    let pkg_name = crate::config::resolve_package_name(std::path::Path::new("."));

    if selection == PipelineSelection::HotReload {
        return run_hot_reload(
            project_root,
            selected_device,
            pkg_name,
            policy,
            verbose_widget_ir,
        );
    }
    let PipelineSelection::Native { release } = selection else {
        unreachable!("hot reload returned before native console selection")
    };

    if no_tui {
        console::start_no_tui(selected_device, pkg_name, release)
            .context("console exited with an error")?;
    } else if inline_render {
        console::start_inline(
            selected_device,
            pkg_name,
            release,
            policy.reload() == ReloadPolicy::HotReload,
        )
            .context("inline console exited with an error")?;
    } else {
        console::start(selected_device, pkg_name, release)
            .context("interactive console exited with an error")?;
    }
    Ok(())
}

fn run_hot_reload(
    project_root: std::path::PathBuf,
    device: Device,
    package: String,
    _: ExecutionPolicy,
    verbose_widget_ir: bool,
) -> anyhow::Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use aimer_cli::hot_reload::pipeline::{ProductionPipelineDriver, run_pipeline};
    use aimer_cli::hot_reload::system::{SystemDevice, SystemPipelineOperations};
    use aimer_cli::hot_reload::targets::TargetFamily;
    use aimer_cli::config::{
        ApplicationRuntime as LibraryRuntime, BuildProfile as LibraryProfile,
        ExecutionPolicy as LibraryPolicy, ReloadPolicy as LibraryReload,
    };

    use self::hot_reload_runtime::CliHotReloadRuntime;

    let family = match device.target {
        Targets::Macos => TargetFamily::Macos,
        Targets::Windows => TargetFamily::Windows,
        Targets::Linux => TargetFamily::Linux,
        Targets::Android | Targets::AndroidSimulator => TargetFamily::Android,
        Targets::Ios => TargetFamily::IosDevice,
        Targets::IosSimulator => TargetFamily::IosSimulator,
        Targets::Web => TargetFamily::Web,
        Targets::Terminated => anyhow::bail!("cannot launch the terminated device selection"),
    };
    let framework_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| anyhow!("failed to locate the Aimer framework workspace"))?
        .to_owned();
    let policy = LibraryPolicy::new(
        LibraryProfile::Debug,
        LibraryRuntime::Wasmi,
        LibraryReload::HotReload,
    )?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let interrupt_shutdown = Arc::clone(&shutdown);
    ctrlc::set_handler(move || interrupt_shutdown.store(true, Ordering::Release))
        .context("failed to install hot-reload shutdown handler")?;
    let system_device = SystemDevice::new(device.name, family, device.id);
    let operations = SystemPipelineOperations::with_runtime(
        project_root,
        framework_root,
        policy,
        system_device,
        package,
        Arc::new(CliHotReloadRuntime::new()),
    )
    .shutdown_signal(shutdown)
    .widget_ir_diagnostics(verbose_widget_ir)
    .status_output(|status| eprintln!("{status}"));
    let mut driver = ProductionPipelineDriver::new(operations);
    run_pipeline(&mut driver).map_err(anyhow::Error::msg)
}

/// Resolve a device non-interactively from `--device <id>` and/or `--target
/// <t>`.
fn resolve_device(target: Option<String>, device: Option<String>) -> anyhow::Result<Device> {
    let devices = fetch_devices();
    resolve_device_from(devices, target, device)
}

fn resolve_device_from(
    devices: Vec<Device>,
    target: Option<String>,
    device: Option<String>,
) -> anyhow::Result<Device> {
    if let Some(id) = device {
        #[cfg(target_os = "macos")]
        {
            if id == "macos" {
                return Ok(Device {
                    name: "macOS Desktop".to_string(),
                    id: "local".to_string(),
                    target: Targets::Macos,
                });
            }
        }
        // The host desktop is always present, so `--device windows|linux` is
        // accepted by its platform name as well as by its device id.
        if let Some(host) = Targets::host_desktop()
            && id == host.to_string()
            && let Some(device) = devices.iter().find(|d| d.target == host)
        {
            return Ok(device.clone());
        }
        return devices
            .into_iter()
            .find(|d| d.id == id && d.target != Terminated)
            .ok_or_else(|| AimerError::DeviceNotFound(id).into());
    }

    // Safe: this branch is only reached when `target` is `Some` (the caller
    // ensures at least one of target/device is set).
    let target = target.expect("target must be set when device is not");
    let parsed = Targets::try_from(target.as_str())
        .map_err(|_| AimerError::UnknownTarget(target.clone()))?;

    let mut matching = devices
        .into_iter()
        .filter(|device| device.target == parsed && device.target != Terminated);
    let Some(selected) = matching.next() else {
        return Err(
            AimerError::DeviceNotFound(format!("no connected device for target '{target}'"))
                .into(),
        );
    };
    let Some(second) = matching.next() else {
        return Ok(selected);
    };
    let mut matching_ids = vec![selected.id, second.id];
    matching_ids.extend(matching.map(|device| device.id));
    let device_ids = matching_ids.join(", ");

    Err(AimerError::AmbiguousDevices { target, device_ids }.into())
}

/// Interactive device picker rendered inline with `crossterm` (the original
/// hand-rolled menu). Returns `Ok(None)` when the user chooses to quit (via the
/// "Quit" entry or `q`).
///
/// The device list is read fresh on every frame from the shared, live-updating
/// `devices_arc`, so devices appearing/disappearing are reflected immediately.
fn pick_device(devices_arc: &Arc<Mutex<Vec<Device>>>) -> anyhow::Result<Option<Device>> {
    // Raw mode + hidden cursor are restored automatically when `_guard` is
    // dropped (even on panic or early return via `?`).
    let _guard = RawModeGuard::new()?;
    let mut stdout = stdout();

    let mut selected_index = 0usize;
    let mut last_devices_len = 0usize;

    let selected_device = loop {
        let devices = devices_arc
            .lock()
            .map_err(|_| anyhow::anyhow!("device list mutex was poisoned"))?
            .clone();
        if selected_index >= devices.len() {
            selected_index = devices.len().saturating_sub(1);
        }

        // Render menu
        execute!(stdout, cursor::MoveToColumn(0))?;
        // Clear previous lines (+1 for the prompt text)
        for _ in 0..last_devices_len + 1 {
            execute!(stdout, Clear(ClearType::CurrentLine), cursor::MoveUp(1))?;
        }
        execute!(stdout, Clear(ClearType::CurrentLine))?;

        writeln!(
            stdout,
            "\x1b[36m◆\x1b[0m  \x1b[1mSelect a device to launch (Press 'q' to quit):\x1b[0m\r"
        )?;
        for (i, device) in devices.iter().enumerate() {
            execute!(stdout, cursor::MoveToColumn(0))?;
            if i == selected_index {
                execute!(stdout, SetForegroundColor(Color::DarkGrey))?;
                write!(stdout, "│  ")?;
                execute!(stdout, SetForegroundColor(Color::Green))?;
                writeln!(stdout, "● {}\r", device)?;
                execute!(stdout, ResetColor)?;
            } else {
                execute!(stdout, SetForegroundColor(Color::DarkGrey))?;
                write!(stdout, "│  ")?;
                execute!(stdout, SetForegroundColor(Color::DarkGrey))?;
                writeln!(stdout, "○ {}\r", device)?;
                execute!(stdout, ResetColor)?;
            }
        }
        execute!(stdout, SetForegroundColor(Color::DarkGrey))?;
        writeln!(stdout, "└\r")?;
        execute!(stdout, ResetColor)?;

        last_devices_len = devices.len() + 1;

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key_event) = event::read()?
        {
            if key_event.kind != KeyEventKind::Press {
                continue;
            }
            match key_event.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    selected_index = if selected_index > 0 {
                        selected_index - 1
                    } else {
                        devices.len().saturating_sub(1)
                    };
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    selected_index = if selected_index < devices.len().saturating_sub(1) {
                        selected_index + 1
                    } else {
                        0
                    };
                }
                KeyCode::Enter => {
                    break devices[selected_index].clone();
                }
                KeyCode::Char('q') => {
                    break Device {
                        name: "Quit".to_string(),
                        target: Terminated,
                        id: "q".to_string(),
                    };
                }
                KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Restore the terminal before aborting; `process::exit`
                    // does not run destructors, so drop the guard manually.
                    drop(_guard);
                    process::exit(1);
                }
                _ => {}
            }
        }
    };

    // Clear the menu for a clean exit.
    for _ in 0..last_devices_len + 1 {
        execute!(stdout, cursor::MoveUp(1), Clear(ClearType::CurrentLine))?;
    }

    if selected_device.target == Terminated || selected_device.id == "q" {
        return Ok(None);
    }
    Ok(Some(selected_device))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_pipeline_uses_the_policy_profile() {
        let debug = ExecutionPolicy::new(
            BuildProfile::Debug,
            ApplicationRuntime::NativeAot,
            ReloadPolicy::Disabled,
        )
        .unwrap();
        let release = ExecutionPolicy::new(
            BuildProfile::Release,
            ApplicationRuntime::NativeAot,
            ReloadPolicy::Disabled,
        )
        .unwrap();

        assert_eq!(
            native_pipeline_release(debug),
            PipelineSelection::Native { release: false }
        );
        assert_eq!(
            native_pipeline_release(release),
            PipelineSelection::Native { release: true }
        );
    }

    #[test]
    fn wasm_policy_never_falls_through_to_the_native_pipeline() {
        let policy = ExecutionPolicy::new(
            BuildProfile::Debug,
            ApplicationRuntime::Wasmi,
            ReloadPolicy::HotReload,
        )
        .unwrap();

        assert_eq!(native_pipeline_release(policy), PipelineSelection::HotReload);
    }

    #[test]
    fn missing_aimer_manifest_is_allowed_only_for_automatic_hot_reload() {
        let hot_reload = ExecutionPolicy::new(
            BuildProfile::Debug,
            ApplicationRuntime::Wasmi,
            ReloadPolicy::HotReload,
        )
        .unwrap();
        let native = ExecutionPolicy::new(
            BuildProfile::Debug,
            ApplicationRuntime::NativeAot,
            ReloadPolicy::Disabled,
        )
        .unwrap();

        assert!(manifest_is_optional(hot_reload));
        assert!(!manifest_is_optional(native));
    }

    // ── Device struct ────────────────────────────────────────────────

    #[test]
    fn device_creation() {
        let d = Device {
            name: "Test Phone".to_string(),
            target: Targets::Android,
            id: "abc123".to_string(),
        };
        assert_eq!(d.name, "Test Phone");
        assert_eq!(d.target, Targets::Android);
        assert_eq!(d.id, "abc123");
    }

    #[test]
    fn device_clone() {
        let d = Device {
            name: "iPhone".to_string(),
            target: Targets::Ios,
            id: "udid-123".to_string(),
        };
        let d2 = d.clone();
        assert_eq!(d.name, d2.name);
        assert_eq!(d.target, d2.target);
        assert_eq!(d.id, d2.id);
    }

    #[test]
    fn target_selection_rejects_ambiguous_connected_devices() {
        let devices = vec![
            Device {
                name: "iPhone".into(),
                target: Targets::Ios,
                id: "phone-1".into(),
            },
            Device {
                name: "iPad".into(),
                target: Targets::Ios,
                id: "tablet-2".into(),
            },
        ];

        let error = resolve_device_from(devices, Some("ios".into()), None)
            .unwrap_err()
            .to_string();

        assert!(error.contains("multiple devices match target 'ios'"));
        assert!(error.contains("--device"));
        assert!(error.contains("phone-1"));
        assert!(error.contains("tablet-2"));
    }

    #[test]
    fn explicit_device_selection_resolves_one_of_multiple_matching_targets() {
        let devices = vec![
            Device {
                name: "iPhone".into(),
                target: Targets::Ios,
                id: "phone-1".into(),
            },
            Device {
                name: "iPad".into(),
                target: Targets::Ios,
                id: "tablet-2".into(),
            },
        ];

        let selected = resolve_device_from(devices, None, Some("tablet-2".into())).unwrap();

        assert_eq!(selected.id, "tablet-2");
    }

    #[test]
    fn device_display_format() {
        let d = Device {
            name: "Pixel 7".to_string(),
            target: Targets::Android,
            id: "serial".to_string(),
        };
        assert_eq!(format!("{}", d), "Pixel 7 (android)");
    }

    #[test]
    fn device_display_all_targets() {
        let cases = [
            (Targets::Macos, "macos"),
            (Targets::Web, "web"),
            (Targets::Ios, "ios"),
            (Targets::IosSimulator, "ios-simulator"),
            (Targets::Android, "android"),
            (Targets::AndroidSimulator, "android-simulator"),
            (Targets::Windows, "windows"),
            (Targets::Linux, "linux"),
            (Terminated, "terminated"),
        ];
        for (target, expected) in cases {
            let d = Device {
                name: "Dev".to_string(),
                target,
                id: "x".to_string(),
            };
            assert_eq!(format!("{}", d), format!("Dev ({expected})"));
        }
    }

    #[test]
    fn device_debug_format() {
        let d = Device {
            name: "Test".to_string(),
            target: Targets::Web,
            id: "web".to_string(),
        };
        let debug = format!("{:?}", d);
        assert!(debug.contains("Test"));
        assert!(debug.contains("Web"));
        assert!(debug.contains("web"));
    }

    // ── fetch_devices base guarantees ────────────────────────────────

    #[test]
    fn fetch_devices_always_has_base_devices() {
        let devices = fetch_devices();
        assert!(
            devices.len() >= 3,
            "Expected at least 3 devices (macOS, Web, Quit)"
        );

        let has_macos = devices
            .iter()
            .any(|d| d.target == Targets::Macos && d.id == "local");
        let has_web = devices
            .iter()
            .any(|d| d.target == Targets::Web && d.id == "web");
        let has_quit = devices
            .iter()
            .any(|d| d.target == Terminated && d.id == "q");

        assert!(has_macos, "Missing macOS Desktop device");
        assert!(has_web, "Missing Web Browser device");
        assert!(has_quit, "Missing Quit device");
    }

    #[test]
    fn fetch_devices_exposes_the_host_desktop() {
        let Some(host) = Targets::host_desktop() else {
            return;
        };
        let devices = fetch_devices();
        let device = devices
            .iter()
            .find(|d| d.target == host)
            .expect("a device for the host desktop");
        assert_eq!(device.id, host.to_string());
    }

    #[test]
    fn the_host_desktop_resolves_without_the_picker() {
        let Some(host) = Targets::host_desktop() else {
            return;
        };
        let by_device = resolve_device(None, Some(host.to_string())).expect("--device <host>");
        assert_eq!(by_device.target, host);

        let by_target = resolve_device(Some(host.to_string()), None).expect("--target <host>");
        assert_eq!(by_target.target, host);
    }

    #[test]
    fn a_desktop_target_of_another_os_has_no_device() {
        for target in [Targets::Windows, Targets::Linux] {
            if Targets::host_desktop() == Some(target) {
                continue;
            }
            assert!(resolve_device(Some(target.to_string()), None).is_err());
        }
    }

    #[test]
    fn fetch_devices_quit_is_last() {
        let devices = fetch_devices();
        let last = devices.last().expect("devices should not be empty");
        assert_eq!(last.target, Terminated);
        assert_eq!(last.id, "q");
        assert_eq!(last.name, "Quit");
    }

    #[test]
    fn fetch_devices_no_duplicate_ids() {
        let devices = fetch_devices();
        let mut seen = std::collections::HashSet::new();
        for d in &devices {
            assert!(seen.insert(&d.id), "Duplicate device id: {}", d.id);
        }
    }

    #[test]
    fn fetch_devices_unique_by_id() {
        let devices = fetch_devices();
        let ids: Vec<_> = devices.iter().map(|d| &d.id).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "Device list contains duplicates");
    }

    // ── Package name parsing (inline logic from execute) ─────────────

    #[test]
    fn parse_pkg_name_from_cargo_toml() {
        let cargo_toml = r#"
[package]
name = "my_awesome_app"
version = "0.1.0"
edition = "2021"
"#;
        let pkg_name = cargo_toml
            .lines()
            .find(|l| l.starts_with("name ="))
            .map(|l| l.split('"').nth(1).unwrap_or("").to_string())
            .unwrap_or_else(|| "aimer_template".to_string());

        assert_eq!(pkg_name, "my_awesome_app");
    }

    #[test]
    fn parse_pkg_name_missing_name_field() {
        let cargo_toml = r#"
[package]
version = "0.1.0"
"#;
        let pkg_name = cargo_toml
            .lines()
            .find(|l| l.starts_with("name ="))
            .map(|l| l.split('"').nth(1).unwrap_or("").to_string())
            .unwrap_or_else(|| "aimer_template".to_string());

        assert_eq!(pkg_name, "aimer_template");
    }

    #[test]
    fn parse_pkg_name_with_quotes() {
        let cargo_toml = r#"name = "hello-world""#;
        let pkg_name = cargo_toml
            .lines()
            .find(|l| l.starts_with("name ="))
            .map(|l| l.split('"').nth(1).unwrap_or("").to_string())
            .unwrap_or_else(|| "aimer_template".to_string());

        assert_eq!(pkg_name, "hello-world");
    }

    #[test]
    fn parse_pkg_name_empty_file() {
        let cargo_toml = "";
        let pkg_name = cargo_toml
            .lines()
            .find(|l| l.starts_with("name ="))
            .map(|l| l.split('"').nth(1).unwrap_or("").to_string())
            .unwrap_or_else(|| "aimer_template".to_string());

        assert_eq!(pkg_name, "aimer_template");
    }
}
