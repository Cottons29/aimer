use std::path::Path;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use aimer_cli::hot_reload::session::DevelopmentSession;
use aimer_cli::hot_reload::system::{
    AssembledHost, SystemDevice, SystemProcessRuntime, SystemRuntime,
};
use aimer_cli::hot_reload::targets::TargetFamily;
use aimer_reload_protocol::{ModuleMetadata, ReloadResult};

use crate::commands::assemble::{self, AndroidPlan, IosPlan, StdioReporter};
use crate::commands::run::capability_sources::configure_command;
use crate::targets::Targets;

/// Connects the reusable reload lifecycle to native platform packagers.
pub(crate) struct CliHotReloadRuntime {
    inner: SystemProcessRuntime,
    host_build: Mutex<Option<Child>>,
    startup_cancelled: AtomicBool,
}

impl CliHotReloadRuntime {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            inner: SystemProcessRuntime::new(),
            host_build: Mutex::new(None),
            startup_cancelled: AtomicBool::new(false),
        }
    }

    fn run_host_build(&self, command: &mut Command, action: &str) -> Result<(), String> {
        if self.startup_cancelled.load(Ordering::Acquire) {
            return Err("hot-reload startup was cancelled".to_owned());
        }
        let child = command
            .spawn()
            .map_err(|error| format!("failed to {action}: {error}"))?;
        {
            let mut slot = self
                .host_build
                .lock()
                .map_err(|_| "hot-reload host build lock was poisoned".to_owned())?;
            if self.startup_cancelled.load(Ordering::Acquire) {
                let mut child = child;
                let _ = child.kill();
                let _ = child.wait();
                return Err("hot-reload startup was cancelled".to_owned());
            }
            if slot.is_some() {
                let mut child = child;
                let _ = child.kill();
                let _ = child.wait();
                return Err("a native host compiler is already running".to_owned());
            }
            *slot = Some(child);
        }

        loop {
            if self.startup_cancelled.load(Ordering::Acquire) {
                let mut slot = self
                    .host_build
                    .lock()
                    .map_err(|_| "hot-reload host build lock was poisoned".to_owned())?;
                terminate_child(&mut slot);
                return Err("hot-reload startup was cancelled".to_owned());
            }
            let status = {
                let mut slot = self
                    .host_build
                    .lock()
                    .map_err(|_| "hot-reload host build lock was poisoned".to_owned())?;
                slot.as_mut()
                    .ok_or_else(|| "native host compiler process disappeared".to_owned())?
                    .try_wait()
            };
            match status {
                Ok(Some(status)) => {
                    let mut slot = self
                        .host_build
                        .lock()
                        .map_err(|_| "hot-reload host build lock was poisoned".to_owned())?;
                    slot.take();
                    return status
                        .success()
                        .then_some(())
                        .ok_or_else(|| format!("failed to {action}: process exited with {status}"));
                }
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(error) => {
                    let mut slot = self
                        .host_build
                        .lock()
                        .map_err(|_| "hot-reload host build lock was poisoned".to_owned())?;
                    slot.take();
                    return Err(format!("failed to wait for {action}: {error}"));
                }
            }
        }
    }

    fn cancel_host_build(&self) {
        if let Ok(mut slot) = self.host_build.lock() {
            terminate_child(&mut slot);
        }
    }
}

impl SystemRuntime for CliHotReloadRuntime {
    fn build_host(&self, root: &Path, _: &Path, package: &str, device: &SystemDevice) -> Result<(), String> {
        let mut command = Command::new("cargo");
        match device.family() {
            TargetFamily::Macos => {
                let target = if std::env::consts::ARCH == "x86_64" { "x86_64-apple-darwin" } else { "aarch64-apple-darwin" };
                command.args(["build", "--target", target, "--lib"]);
            }
            TargetFamily::Windows | TargetFamily::Linux => {
                command.args(["build", "--bin", package]);
            }
            TargetFamily::IosSimulator | TargetFamily::IosDevice => {
                let plan = IosPlan::resolve(device.family() == TargetFamily::IosSimulator);
                let print_arg = assemble::link_flags::print_arg(plan.rust_target, false)
                    .map_err(display_error)?;
                command.args([
                    "rustc",
                    "--lib",
                    "--target",
                    plan.rust_target,
                    "--features",
                    aimer_cli::hot_reload::session::HOST_RELOAD_FEATURE,
                    "--",
                    "--print",
                ]).arg(print_arg);
            }
            TargetFamily::Android => {
                let plan = android_plan(device.id())?;
                command.args(["ndk", "-t", plan.rust_target, "build", "--lib"]);
            }
            TargetFamily::Web => return Err("web cannot build a native hot-reload host".to_owned()),
        }
        if !matches!(device.family(), TargetFamily::IosSimulator | TargetFamily::IosDevice) {
            command.args(["--features", aimer_cli::hot_reload::session::HOST_RELOAD_FEATURE]);
        }
        command.current_dir(root);
        configure_command(&mut command)?;
        self.run_host_build(&mut command, "build native hot-reload host")
    }

    fn build_guest(&self, project: &Path, workspace: &Path, package: &str, build: u64) -> Result<Vec<u8>, String> {
        self.inner.build_guest(project, workspace, package, build)
    }

    fn cancel_startup(&self) {
        self.startup_cancelled.store(true, Ordering::Release);
        self.cancel_host_build();
        self.inner.cancel_startup();
    }

    fn assemble(&self, root: &Path, _: &Path, package: &str, device: &SystemDevice) -> Result<AssembledHost, String> {
        let reporter = StdioReporter;
        match device.family() {
            TargetFamily::Windows | TargetFamily::Linux => {
                let target = if device.family() == TargetFamily::Windows { Targets::Windows } else { Targets::Linux };
                let bundle = assemble::package_desktop(target, package, false, &reporter).map_err(display_error)?;
                Ok(AssembledHost::DesktopExecutable(root.join(bundle).join(assemble::desktop_exe_name(package))))
            }
            TargetFamily::Macos => {
                let app = assemble::package_macos_for_hot_reload(package, false, &reporter).map_err(display_error)?;
                Ok(AssembledHost::DesktopExecutable(root.join(app).join(format!("Contents/MacOS/{package}"))))
            }
            TargetFamily::IosSimulator | TargetFamily::IosDevice => {
                let plan = IosPlan::resolve(device.family() == TargetFamily::IosSimulator);
                let app = assemble::package_ios(package, &plan, false, &reporter).map_err(display_error)?;
                install_ios(device, &app)?;
                Ok(AssembledHost::MobileApplication { bundle_id: ios_bundle_id(&app).unwrap_or_else(|| package.to_owned()) })
            }
            TargetFamily::Android => {
                let plan = android_plan(device.id())?;
                let apk = assemble::package_android(package, &plan, false, &reporter).map_err(display_error)?;
                let mut install = Command::new("adb");
                install.args(["-s", device.id(), "install", "-r", &apk]);
                run_checked(&mut install, "install Android host")?;
                Ok(AssembledHost::MobileApplication {
                    bundle_id: android_application_id(root, package),
                })
            }
            TargetFamily::Web => Err("web cannot assemble a native hot-reload host".to_owned()),
        }
    }

    fn prepare_route(&self, device: &SystemDevice, port: u16) -> Result<(), String> { self.inner.prepare_route(device, port) }
    fn launch(&self, device: &SystemDevice, package: &str, host: &AssembledHost, session: &DevelopmentSession) -> Result<(), String> { self.inner.launch(device, package, host, session) }
    fn connect(&self, device: &SystemDevice, session: &DevelopmentSession) -> Result<(), String> { self.inner.connect(device, session) }
    fn push_reload(&self, id: u64, metadata: ModuleMetadata, module: &[u8]) -> Result<ReloadResult, String> { self.inner.push_reload(id, metadata, module) }
    fn recover_result(&self, id: u64) -> Result<Option<ReloadResult>, String> { self.inner.recover_result(id) }
    fn start_watcher(&self, root: &Path, ignored: &[std::path::PathBuf]) -> Result<(), String> { self.inner.start_watcher(root, ignored) }
    fn next_change(&self, timeout: std::time::Duration) -> Result<Option<Vec<std::path::PathBuf>>, String> { self.inner.next_change(timeout) }
    fn cleanup(&self) {
        self.cancel_startup();
        self.inner.cleanup();
    }
}

fn terminate_child(child: &mut Option<Child>) {
    if let Some(mut child) = child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn display_error(error: anyhow::Error) -> String { format!("{error:#}") }

fn run_checked(command: &mut Command, action: &str) -> Result<(), String> {
    let status = command.status().map_err(|error| format!("failed to {action}: {error}"))?;
    status.success().then_some(()).ok_or_else(|| format!("failed to {action}: process exited with {status}"))
}

fn android_plan(device: &str) -> Result<AndroidPlan, String> {
    let output = Command::new("adb").args(["-s", device, "shell", "getprop", "ro.product.cpu.abi"]).output().map_err(|error| format!("failed to query Android ABI: {error}"))?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned()); }
    Ok(AndroidPlan::for_abi(String::from_utf8_lossy(&output.stdout).trim()))
}

fn install_ios(device: &SystemDevice, app: &str) -> Result<(), String> {
    let mut command = Command::new("xcrun");
    if device.family() == TargetFamily::IosSimulator { command.args(["simctl", "install", device.id(), app]); }
    else { command.args(["devicectl", "device", "install", "app", "--device", device.id(), app]); }
    run_checked(&mut command, "install iOS host")
}

fn ios_bundle_id(app: &str) -> Option<String> {
    let output = Command::new("plutil").args(["-extract", "CFBundleIdentifier", "raw", &format!("{app}/Info.plist")]).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn android_application_id(root: &Path, fallback: &str) -> String {
    std::fs::read_to_string(root.join("builds/android/app/build.gradle.kts"))
        .ok()
        .and_then(|source| {
            source.lines().find_map(|line| {
                let value = line.trim().strip_prefix("applicationId")?.trim();
                value
                    .strip_prefix('=')?
                    .trim()
                    .strip_prefix('"')?
                    .strip_suffix('"')
                    .map(ToOwned::to_owned)
            })
        })
        .unwrap_or_else(|| fallback.to_owned())
}
