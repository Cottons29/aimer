use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use aimer_reload_protocol::{
    DevelopmentHostConfig, ModuleMetadata, ProtocolLimits, ReloadResult,
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use sha2::Digest;

use crate::config::{AimerManifest, ApplicationRuntime, BuildProfile, ExecutionPolicy, ReloadPolicy};

use super::build::{BuildCommand, HotReloadBuildPlan, load_guest_artifact};
use super::client::{ClientError, ReloadClient};
use super::generation::{
    GuestGenerationMode, GuestPackageSpec, generate_guest_package, prepare_automatic_guest,
};
use super::launch::{LaunchConfiguration, LaunchStep};
use super::pipeline::PipelineOperations;
use super::readiness::await_listener_readiness;
use super::route::{
    AndroidRouteAdapter, DesktopRouteAdapter, IosDeviceRouteAdapter, OwnedCommandGuard,
    RouteReservation, SimulatorRouteAdapter, SystemCommandExecutor,
};
use super::session::DevelopmentSession;
use super::status::ReloadStatus;
use super::targets::TargetFamily;
use super::watch::{ChangeImpact, WatchSet};

const PROTOCOL_VERSION: (u16, u16) = (1, 0);
const READINESS_TIMEOUT: Duration = Duration::from_secs(30);

/// Device information required by the reusable hot-reload pipeline.
///
/// The command binary's `Device` type is intentionally not part of the reusable
/// `aimer_cli` library. Integration converts the selected device into this
/// value, preserving the exact target and identifier chosen by the user.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemDevice {
    name: String,
    family: TargetFamily,
    id: String,
}

impl SystemDevice {
    /// Creates the device selection consumed by [`SystemPipelineOperations`].
    #[inline]
    pub fn new(
        name: impl Into<String>,
        family: TargetFamily,
        id: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            family,
            id: id.into(),
        }
    }

    /// Returns the selected target family.
    #[inline]
    pub const fn family(&self) -> TargetFamily {
        self.family
    }

    /// Returns the exact platform-tool device identifier.
    #[inline]
    pub fn id(&self) -> &str {
        &self.id
    }
}

/// Result of the target-specific native assembly stage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssembledHost {
    /// A local desktop executable that can be started directly.
    DesktopExecutable(PathBuf),
    /// An installed mobile application addressed by its bundle/application id.
    MobileApplication { bundle_id: String },
}

/// Narrow side-effect boundary used by the production pipeline.
///
/// Implementations must retain every process and watcher they create until
/// [`Self::cleanup`] is called. Private launch environment and standard-input
/// values are passed only to `launch`; implementations must never include them
/// in diagnostics.
pub trait SystemRuntime: Send + Sync + 'static {
    fn build_host(
        &self,
        project_root: &Path,
        workspace_root: &Path,
        package: &str,
        device: &SystemDevice,
    ) -> Result<(), String>;

    fn build_guest(
        &self,
        project_root: &Path,
        workspace_root: &Path,
        package: &str,
        build_number: u64,
    ) -> Result<Vec<u8>, String>;

    /// Cancels compiler processes owned by the current startup branch.
    ///
    /// Production implementations must terminate pending native and guest
    /// compiler children and make them observable as cancelled to the caller.
    /// Test and embedded runtimes may keep the default no-op when their build
    /// operations are already bounded.
    fn cancel_startup(&self) {}

    fn assemble(
        &self,
        project_root: &Path,
        workspace_root: &Path,
        package: &str,
        device: &SystemDevice,
    ) -> Result<AssembledHost, String>;

    fn prepare_route(&self, device: &SystemDevice, listener_port: u16) -> Result<(), String>;

    fn launch(
        &self,
        device: &SystemDevice,
        package: &str,
        host: &AssembledHost,
        session: &DevelopmentSession,
    ) -> Result<(), String>;

    fn connect(&self, device: &SystemDevice, session: &DevelopmentSession) -> Result<(), String>;

    fn push_reload(
        &self,
        request_id: u64,
        metadata: ModuleMetadata,
        module: &[u8],
    ) -> Result<ReloadResult, String>;

    fn recover_result(&self, request_id: u64) -> Result<Option<ReloadResult>, String>;

    fn start_watcher(&self, project_root: &Path, ignored: &[PathBuf]) -> Result<(), String>;

    fn next_change(&self, timeout: Duration) -> Result<Option<Vec<PathBuf>>, String>;

    fn cleanup(&self);
}

/// Concrete owner of one native hot-reload run.
///
/// Construction is side-effect free. The existing pipeline driver invokes the
/// methods in `PipelineOperations` order; this value retains the initial guest
/// build handle, target route, launched app, watcher, and request recovery
/// state until the driver's mandatory cleanup call.
pub struct SystemPipelineOperations<R: SystemRuntime = SystemProcessRuntime> {
    project_root: PathBuf,
    workspace_root: PathBuf,
    policy: ExecutionPolicy,
    device: SystemDevice,
    package: String,
    runtime: Arc<R>,
    shutdown: Arc<AtomicBool>,
    statuses: Arc<Mutex<Vec<ReloadStatus>>>,
    status_output: Arc<dyn Fn(&ReloadStatus) + Send + Sync>,
    watch_set: WatchSet,
    listener_port: u16,
    widget_ir_diagnostics: bool,
    session: Option<DevelopmentSession>,
    assembled_host: Option<AssembledHost>,
    initial_guest_build: Option<JoinHandle<Result<Vec<u8>, String>>>,
    request_id: u64,
    build_number: u64,
    cleaned: bool,
}

impl SystemPipelineOperations<SystemProcessRuntime> {
    /// Creates the production operations owner from the run command's resolved inputs.
    ///
    /// The constructor starts no process and creates no credential. Side effects
    /// begin only when the pipeline driver calls `resolve` and subsequent stages.
    #[inline]
    pub fn new(
        project_root: impl Into<PathBuf>,
        workspace_root: impl Into<PathBuf>,
        policy: ExecutionPolicy,
        device: SystemDevice,
        package: impl Into<String>,
    ) -> Self {
        Self::with_runtime(
            project_root,
            workspace_root,
            policy,
            device,
            package,
            Arc::new(SystemProcessRuntime::new()),
        )
        .status_output(|status| eprintln!("{status}"))
    }
}

impl<R> SystemPipelineOperations<R>
where
    R: SystemRuntime,
{
    /// Creates a pipeline from resolved run-command inputs and a runtime.
    #[inline]
    pub fn with_runtime(
        project_root: impl Into<PathBuf>,
        workspace_root: impl Into<PathBuf>,
        policy: ExecutionPolicy,
        device: SystemDevice,
        package: impl Into<String>,
        runtime: Arc<R>,
    ) -> Self {
        let project_root = project_root.into();
        let workspace_root = workspace_root.into();
        Self {
            watch_set: WatchSet::automatic(
                project_root.clone(),
                portable_source_root(&project_root),
                local_path_dependencies(&project_root),
                vec![
                    workspace_root.join("target"),
                    project_root.join("target"),
                    project_root.join("builds"),
                ],
            ),
            project_root,
            workspace_root,
            policy,
            device,
            package: package.into(),
            runtime,
            shutdown: Arc::new(AtomicBool::new(false)),
            statuses: Arc::new(Mutex::new(Vec::new())),
            status_output: Arc::new(|_| {}),
            listener_port: 0,
            widget_ir_diagnostics: false,
            session: None,
            assembled_host: None,
            initial_guest_build: None,
            request_id: 0,
            build_number: 0,
            cleaned: false,
        }
    }

    /// Supplies the shared cancellation flag observed by the watch loop.
    #[inline]
    pub fn shutdown_signal(mut self, shutdown: Arc<AtomicBool>) -> Self {
        self.shutdown = shutdown;
        self
    }

    /// Enables verbose Widget IR stage diagnostics in the launched native host.
    #[inline]
    pub fn widget_ir_diagnostics(mut self, enabled: bool) -> Self {
        self.widget_ir_diagnostics = enabled;
        self
    }

    /// Replaces the source classification resolved from Cargo metadata.
    #[inline]
    pub fn watch_set(mut self, watch_set: WatchSet) -> Self {
        self.watch_set = watch_set;
        self
    }

    /// Supplies the secret-free destination for progress and terminal statuses.
    #[inline]
    pub fn status_output(
        mut self,
        output: impl Fn(&ReloadStatus) + Send + Sync + 'static,
    ) -> Self {
        self.status_output = Arc::new(output);
        self
    }

    /// Returns the secret-free status history emitted by this run.
    pub fn statuses(&self) -> Vec<ReloadStatus> {
        self.statuses.lock().expect("status lock poisoned").clone()
    }

    fn status(&self, status: ReloadStatus) {
        (self.status_output)(&status);
        self.statuses.lock().expect("status lock poisoned").push(status);
    }

    fn rebuild_and_push(&mut self) -> Result<(), String> {
        self.build_number = self.build_number.checked_add(1).ok_or_else(|| {
            "hot-reload build counter exhausted; restart the development session".to_owned()
        })?;
        self.status(ReloadStatus::Compiling {
            build: self.build_number,
        });
        let module = self.runtime.build_guest(
            &self.project_root,
            &self.workspace_root,
            &self.package,
            self.build_number,
        )?;
        self.push(module)
    }

    fn push(&mut self, module: Vec<u8>) -> Result<(), String> {
        self.request_id = self.request_id.checked_add(1).ok_or_else(|| {
            "hot-reload request counter exhausted; restart the development session".to_owned()
        })?;
        let request_id = self.request_id;
        self.status(ReloadStatus::Uploading {
            sent: module.len(),
            total: module.len(),
        });
        self.status(ReloadStatus::WaitingForCommit {
            request: request_id,
        });
        let metadata = ModuleMetadata::new(
            stable_identity(&self.package),
            build_identity(&module),
            1,
            0,
            empty_capability_digest(),
        );
        let result = match self.runtime.push_reload(request_id, metadata, &module) {
            Ok(result) => result,
            Err(disconnected) => self
                .runtime
                .recover_result(request_id)?
                .ok_or(disconnected)?,
        };
        let committed = matches!(result, ReloadResult::Committed { .. });
        self.status(ReloadStatus::Terminal(result));
        if !committed && self.build_number == 1 {
            return Err("the initial guest generation was not committed".to_owned());
        }
        Ok(())
    }

    fn finish_initial_guest_build(&mut self) -> Result<Vec<u8>, String> {
        let handle = self
            .initial_guest_build
            .take()
            .ok_or_else(|| "initial guest build was not started".to_owned())?;
        match handle.join() {
            Ok(result) => result,
            Err(_) => {
                self.runtime.cancel_startup();
                Err("initial guest build thread panicked".to_owned())
            }
        }
    }

    fn cancel_initial_guest_build(&mut self) {
        if let Some(handle) = self.initial_guest_build.take() {
            self.runtime.cancel_startup();
            let _ = handle.join();
        }
    }
}

impl<R> Drop for SystemPipelineOperations<R>
where
    R: SystemRuntime,
{
    fn drop(&mut self) {
        if !self.cleaned {
            self.cleaned = true;
            self.cancel_initial_guest_build();
            self.runtime.cleanup();
            self.assembled_host = None;
            self.session = None;
        }
    }
}

impl<R> PipelineOperations for SystemPipelineOperations<R>
where
    R: SystemRuntime,
{
    fn resolve(&mut self) -> Result<(), String> {
        if self.policy.profile() != BuildProfile::Debug
            || self.policy.runtime() != ApplicationRuntime::Wasmi
            || self.policy.reload() != ReloadPolicy::HotReload
        {
            return Err("the system hot-reload pipeline requires debug/wasmi/hot-reload".to_owned());
        }
        if !self.device.family.supports_hot_reload() {
            return Err(format!("{} does not support native hot reload", self.device.family));
        }
        if self.package.trim().is_empty() {
            return Err("the hot-reload host package name is empty".to_owned());
        }
        if !self.project_root.join("Cargo.toml").is_file() {
            return Err(format!(
                "project manifest is missing at {}",
                self.project_root.join("Cargo.toml").display()
            ));
        }
        if !self.workspace_root.join("Cargo.toml").is_file() {
            return Err(format!(
                "workspace manifest is missing at {}",
                self.workspace_root.join("Cargo.toml").display()
            ));
        }
        Ok(())
    }

    fn create_session(&mut self) -> Result<(), String> {
        let reservation = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("failed to allocate a reload listener port: {error}"))?;
        self.listener_port = reservation
            .local_addr()
            .map_err(|error| format!("failed to inspect the reload listener port: {error}"))?
            .port();
        drop(reservation);
        self.session = DevelopmentSession::for_policy(self.policy, self.listener_port)
            .map_err(|error| format!("failed to create the development session: {error}"))?
            .map(|session| session.widget_ir_diagnostics(self.widget_ir_diagnostics));
        if self.session.is_none() {
            return Err("the hot-reload policy did not create a development session".to_owned());
        }
        Ok(())
    }

    fn build_host(&mut self) -> Result<(), String> {
        self.runtime.build_host(
            &self.project_root,
            &self.workspace_root,
            &self.package,
            &self.device,
        )
    }

    fn build_initial_guest(&mut self) -> Result<(), String> {
        if self.initial_guest_build.is_some() {
            return Err("the initial guest build was already started".to_owned());
        }
        self.build_number = 1;
        self.status(ReloadStatus::Compiling { build: 1 });
        let runtime = Arc::clone(&self.runtime);
        let project_root = self.project_root.clone();
        let workspace_root = self.workspace_root.clone();
        let package = self.package.clone();
        self.initial_guest_build = Some(
            thread::Builder::new()
                .name("aimer-hot-reload-guest-build".to_owned())
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        runtime.build_guest(&project_root, &workspace_root, &package, 1)
                    }));
                    match result {
                        Ok(Ok(module)) => Ok(module),
                        Ok(Err(error)) => {
                            runtime.cancel_startup();
                            Err(error)
                        }
                        Err(_) => {
                            runtime.cancel_startup();
                            Err("initial guest build panicked".to_owned())
                        }
                    }
                })
                .map_err(|error| format!("failed to start initial guest build: {error}"))?,
        );
        Ok(())
    }

    fn assemble(&mut self) -> Result<(), String> {
        self.assembled_host = Some(self.runtime.assemble(
            &self.project_root,
            &self.workspace_root,
            &self.package,
            &self.device,
        )?);
        Ok(())
    }

    fn prepare_route(&mut self) -> Result<(), String> {
        self.runtime.prepare_route(&self.device, self.listener_port)
    }

    fn launch_app(&mut self) -> Result<(), String> {
        let host = self
            .assembled_host
            .as_ref()
            .ok_or_else(|| "native host was not assembled before launch".to_owned())?;
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| "development session was not created before launch".to_owned())?;
        self.runtime
            .launch(&self.device, &self.package, host, session)
    }

    fn discover_and_authenticate(&mut self) -> Result<(), String> {
        let session = self
            .session
            .as_ref()
            .ok_or_else(|| "development session was not created before discovery".to_owned())?;
        self.runtime.connect(&self.device, session)
    }

    fn push_initial_module(&mut self) -> Result<(), String> {
        let module = self.finish_initial_guest_build()?;
        self.push(module)
    }

    fn watch(&mut self) -> Result<(), String> {
        self.runtime
            .start_watcher(&self.project_root, self.watch_set.watch_roots())?;
        while !self.shutdown.load(Ordering::Acquire) {
            let Some(paths) = self.runtime.next_change(Duration::from_millis(50))? else {
                continue;
            };
            match self.watch_set.classify(paths.iter().map(PathBuf::as_path)) {
                ChangeImpact::Ignored => {}
                ChangeImpact::RebuildGuest => {
                    if let Err(diagnostic) = self.rebuild_and_push() {
                        self.status(ReloadStatus::RecoverableFailure { diagnostic });
                    }
                }
                ChangeImpact::RestartNativeHost => {
                    self.status(ReloadStatus::NativeRestartRequired {
                        reason: "native host source changed".to_owned(),
                    });
                }
            }
        }
        Ok(())
    }

    fn cleanup(&mut self) {
        if !self.cleaned {
            self.cleaned = true;
            self.cancel_initial_guest_build();
            self.runtime.cleanup();
            self.assembled_host = None;
            self.session = None;
        }
    }
}

struct RuntimeResources {
    children: Vec<Child>,
    host_build: Option<Child>,
    guest_build: Option<Child>,
    startup_cancelled: bool,
    announcements: Option<Receiver<String>>,
    watcher: Option<RecommendedWatcher>,
    changes: Option<Receiver<notify::Result<notify::Event>>>,
    client: Option<ReloadClient>,
    android_route: Option<RouteReservation<SystemCommandExecutor>>,
    android_session: Option<OwnedCommandGuard<SystemCommandExecutor>>,
    exact_watch_roots: Option<Vec<PathBuf>>,
}

impl RuntimeResources {
    const fn new() -> Self {
        Self {
            children: Vec::new(),
            host_build: None,
            guest_build: None,
            startup_cancelled: false,
            announcements: None,
            watcher: None,
            changes: None,
            client: None,
            android_route: None,
            android_session: None,
            exact_watch_roots: None,
        }
    }
}

/// Production runtime backed by `std::process`, TCP, and `notify`.
///
/// Child handles, Android route guards, the authenticated client, and the
/// filesystem watcher live in one mutex so cleanup can atomically take and
/// release everything created by this run.
pub struct SystemProcessRuntime {
    resources: Mutex<RuntimeResources>,
    executor: Arc<SystemCommandExecutor>,
}

impl SystemProcessRuntime {
    /// Creates an empty production runtime.
    #[inline]
    pub fn new() -> Self {
        Self {
            resources: Mutex::new(RuntimeResources::new()),
            executor: Arc::new(SystemCommandExecutor),
        }
    }

    fn run_build(
        &self,
        root: &Path,
        build: &BuildCommand,
        guest: bool,
    ) -> Result<(), String> {
        let child = Command::new(build.program())
            .args(build.arguments())
            .current_dir(root)
            .spawn()
            .map_err(|error| format!("failed to start {}: {error}", build.program()))?;

        {
            let mut resources = self.resources.lock().map_err(lock_error)?;
            if resources.startup_cancelled {
                let mut child = child;
                let _ = child.kill();
                let _ = child.wait();
                return Err("hot-reload startup was cancelled".to_owned());
            }
            let slot = if guest {
                &mut resources.guest_build
            } else {
                &mut resources.host_build
            };
            if slot.is_some() {
                let mut child = child;
                let _ = child.kill();
                let _ = child.wait();
                return Err(if guest {
                    "an initial guest compiler is already running".to_owned()
                } else {
                    "a native host compiler is already running".to_owned()
                });
            }
            *slot = Some(child);
        }

        loop {
            let poll = {
                let mut resources = self.resources.lock().map_err(lock_error)?;
                if resources.startup_cancelled {
                    let slot = if guest {
                        &mut resources.guest_build
                    } else {
                        &mut resources.host_build
                    };
                    terminate_child(slot);
                    return Err("hot-reload startup was cancelled".to_owned());
                }
                let slot = if guest {
                    &mut resources.guest_build
                } else {
                    &mut resources.host_build
                };
                slot.as_mut()
                    .ok_or_else(|| "hot-reload compiler process disappeared".to_owned())?
                    .try_wait()
            };
            match poll {
                Ok(Some(status)) => {
                    let mut resources = self.resources.lock().map_err(lock_error)?;
                    let slot = if guest {
                        &mut resources.guest_build
                    } else {
                        &mut resources.host_build
                    };
                    slot.take();
                    return if status.success() {
                        Ok(())
                    } else {
                        Err(format!("{} exited with {status}", build.program()))
                    };
                }
                Ok(None) => thread::sleep(Duration::from_millis(20)),
                Err(error) => {
                    let mut resources = self.resources.lock().map_err(lock_error)?;
                    let slot = if guest {
                        &mut resources.guest_build
                    } else {
                        &mut resources.host_build
                    };
                    slot.take();
                    return Err(format!(
                        "failed to wait for {}: {error}",
                        build.program()
                    ));
                }
            }
        }
    }

    fn execute_step(root: &Path, step: &LaunchStep) -> Result<(), String> {
        let mut command = Self::step_command(root, step);
        command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
        if step.stdin().is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("failed to start {}: {error}", step.command().program()))?;
        if let Some(payload) = step.stdin() {
            child
                .stdin
                .take()
                .ok_or_else(|| "private launch input pipe was unavailable".to_owned())?
                .write_all(payload.as_bytes())
                .map_err(|error| format!("failed to provide private launch input: {error}"))?;
        }
        let status = child
            .wait()
            .map_err(|error| format!("failed to wait for {}: {error}", step.command().program()))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("{} exited with {status}", step.command().program()))
        }
    }

    fn spawn_step(root: &Path, step: &LaunchStep) -> Result<(Child, Receiver<String>), String> {
        let mut command = Self::step_command(root, step);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if step.stdin().is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("failed to launch {}: {error}", step.command().program()))?;
        if let Some(payload) = step.stdin() {
            child
                .stdin
                .take()
                .ok_or_else(|| "private launch input pipe was unavailable".to_owned())?
                .write_all(payload.as_bytes())
                .map_err(|error| format!("failed to provide private launch input: {error}"))?;
        }
        let (sender, receiver) = mpsc::channel();
        if let Some(stdout) = child.stdout.take() {
            stream_lines(stdout, sender.clone());
        }
        if let Some(stderr) = child.stderr.take() {
            stream_lines(stderr, sender);
        }
        Ok((child, receiver))
    }

    fn step_command(root: &Path, step: &LaunchStep) -> Command {
        let mut command = Command::new(step.command().program());
        command.args(step.command().arguments()).current_dir(root);
        for (name, value) in step.environment() {
            command.env(name, value.as_str());
        }
        command
    }

    fn launch_configuration(
        &self,
        root: &Path,
        device: &SystemDevice,
        launch: LaunchConfiguration,
    ) -> Result<(), String> {
        let app_index = launch.steps().len() - 1;
        for step in &launch.steps()[..app_index] {
            Self::execute_step(root, step)?;
        }
        if device.family == TargetFamily::Android {
            Self::execute_step(root, launch.app())?;
            let logcat = LaunchStep::new(super::route::CommandSpec::new(
                "adb",
                vec!["-s".into(), device.id.clone(), "logcat".into()],
            ));
            let (child, announcements) = Self::spawn_step(root, &logcat)?;
            let mut resources = self.resources.lock().map_err(lock_error)?;
            resources.children.push(child);
            resources.announcements = Some(announcements);
        } else {
            let (child, announcements) = Self::spawn_step(root, launch.app())?;
            let mut resources = self.resources.lock().map_err(lock_error)?;
            resources.children.push(child);
            resources.announcements = Some(announcements);
        }
        Ok(())
    }
}

impl Default for SystemProcessRuntime {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl SystemRuntime for SystemProcessRuntime {
    fn build_host(
        &self,
        project_root: &Path,
        _: &Path,
        package: &str,
        _: &SystemDevice,
    ) -> Result<(), String> {
        let plan = HotReloadBuildPlan::new(
            project_root.join("Cargo.toml"),
            "aimer_generated_guest",
            package,
            project_root.join("target/aimer-hot-reload/guest"),
        );
        self.run_build(project_root, &plan.host_command(), false)
    }

    fn build_guest(
        &self,
        project_root: &Path,
        workspace_root: &Path,
        package: &str,
        _: u64,
    ) -> Result<Vec<u8>, String> {
        let manifest = AimerManifest::load_from(project_root)
            .map_err(|error| format!("failed to load hot-reload metadata: {error}"))?;
        let session_root = project_root.join("target/aimer-hot-reload");
        let target_root = session_root.join("guest");
        let (generated, host_package) = match GuestGenerationMode::select(manifest.as_ref()) {
            GuestGenerationMode::Automatic => (
                prepare_automatic_guest(project_root, workspace_root, &session_root)
                    .map_err(|error| error.to_string())?,
                package,
            ),
            GuestGenerationMode::Manual => {
                let guest = manifest
                    .as_ref()
                    .and_then(AimerManifest::hot_reload_guest)
                    .expect("manual mode has explicit metadata");
                (
                    generate_guest_package(
                        project_root,
                        &workspace_root.join("crates/aimer_wasm_guest"),
                        &session_root.join("generated"),
                        &GuestPackageSpec::new(guest.package(), guest.program(), guest.limits()),
                    )
                    .map_err(|error| error.to_string())?,
                    guest.package(),
                )
            }
        };
        let plan = HotReloadBuildPlan::new(
            project_root.join("Cargo.toml"),
            generated.package(),
            host_package,
            target_root.clone(),
        )
        .guest_manifest(generated.manifest().to_owned())
        .guest_source_remap(generated.application_root().to_owned(), project_root.to_owned());
        self.run_build(project_root, &plan.guest_command(), true)?;
        let artifact = target_root
            .join("wasm32-unknown-unknown/debug")
            .join(format!("{}.wasm", generated.package().replace('-', "_")));
        let module = load_guest_artifact(
            &artifact,
            &target_root,
            DevelopmentHostConfig::cli_safe_profile().module_bytes_limit(),
        )
        .map_err(|error| error.to_string())?;
        if let Some(portable_source_root) = generated.portable_source_root() {
            let watch_set = WatchSet::automatic(
                project_root.to_owned(),
                portable_source_root.to_owned(),
                local_path_dependencies(project_root),
                vec![
                    workspace_root.join("target"),
                    project_root.join("target"),
                    project_root.join("builds"),
                ],
            );
            self.resources
                .lock()
                .map_err(lock_error)?
                .exact_watch_roots = Some(watch_set.watch_roots().to_vec());
        }
        Ok(module)
    }

    fn cancel_startup(&self) {
        let Ok(mut resources) = self.resources.lock() else {
            return;
        };
        resources.startup_cancelled = true;
        terminate_child(&mut resources.host_build);
        terminate_child(&mut resources.guest_build);
    }

    fn assemble(
        &self,
        project_root: &Path,
        _: &Path,
        package: &str,
        device: &SystemDevice,
    ) -> Result<AssembledHost, String> {
        match device.family {
            TargetFamily::Windows | TargetFamily::Linux => Ok(AssembledHost::DesktopExecutable(
                project_root.join("target/debug").join(format!("{package}{}", std::env::consts::EXE_SUFFIX)),
            )),
            TargetFamily::Macos => Ok(AssembledHost::DesktopExecutable(
                project_root.join("builds/macos/build/Debug").join(format!("{package}.app/Contents/MacOS/{package}")),
            )),
            TargetFamily::IosSimulator | TargetFamily::IosDevice => Ok(AssembledHost::MobileApplication {
                bundle_id: mobile_application_id(project_root, package),
            }),
            TargetFamily::Android => Ok(AssembledHost::MobileApplication {
                bundle_id: android_application_id(project_root, package),
            }),
            TargetFamily::Web => Err("web cannot assemble a native hot-reload host".to_owned()),
        }
    }

    fn prepare_route(&self, device: &SystemDevice, listener_port: u16) -> Result<(), String> {
        if device.family == TargetFamily::Android {
            let adapter = AndroidRouteAdapter::new(Arc::clone(&self.executor));
            let reservation = adapter
                .prepare(&device.id, listener_port)
                .map_err(|error| error.to_string())?;
            self.resources.lock().map_err(lock_error)?.android_route = Some(reservation);
        }
        Ok(())
    }

    fn launch(
        &self,
        device: &SystemDevice,
        package: &str,
        host: &AssembledHost,
        session: &DevelopmentSession,
    ) -> Result<(), String> {
        let launch = match (device.family, host) {
            (
                TargetFamily::Macos | TargetFamily::Windows | TargetFamily::Linux,
                AssembledHost::DesktopExecutable(binary),
            ) => DesktopRouteAdapter::new(device.family, binary).launch(session),
            (TargetFamily::IosSimulator, AssembledHost::MobileApplication { bundle_id }) => {
                SimulatorRouteAdapter::new(&device.id, bundle_id).launch(session)
            }
            (TargetFamily::IosDevice, AssembledHost::MobileApplication { bundle_id }) => {
                IosDeviceRouteAdapter::new(&device.id, bundle_id).launch(session)
            }
            (TargetFamily::Android, AssembledHost::MobileApplication { bundle_id }) => {
                let adapter = AndroidRouteAdapter::new(Arc::clone(&self.executor));
                self.resources.lock().map_err(lock_error)?.android_session =
                    Some(adapter.session_file_guard(&device.id, bundle_id));
                adapter.launch(&device.id, bundle_id, session)
            }
            _ => return Err(format!("assembled host does not match {}", device.family)),
        };
        self.launch_configuration(Path::new("."), device, launch).map_err(|error| {
            format!("failed to launch {package} on {}: {error}", device.name)
        })
    }

    fn connect(&self, device: &SystemDevice, session: &DevelopmentSession) -> Result<(), String> {
        let announcements = self
            .resources
            .lock()
            .map_err(lock_error)?
            .announcements
            .take()
            .ok_or_else(|| "launch output is unavailable for listener discovery".to_owned())?;
        let readiness = await_listener_readiness(
            &announcements,
            *session.credentials().session_id(),
            PROTOCOL_VERSION,
            device.family.name(),
            READINESS_TIMEOUT,
        )
        .map_err(|error| error.to_string())?;
        let address = match device.family {
            TargetFamily::Macos | TargetFamily::Windows | TargetFamily::Linux => {
                DesktopRouteAdapter::new(device.family, "unused")
                    .endpoint(&readiness)
                    .map_err(|error| error.to_string())?
                    .address()
            }
            TargetFamily::IosSimulator => SimulatorRouteAdapter::new(&device.id, "unused")
                .endpoint(&readiness)
                .map_err(|error| error.to_string())?
                .address(),
            TargetFamily::Android => {
                let resources = self.resources.lock().map_err(lock_error)?;
                let route = resources
                    .android_route
                    .as_ref()
                    .ok_or_else(|| "Android route was not prepared".to_owned())?;
                AndroidRouteAdapter::new(Arc::clone(&self.executor))
                    .endpoint(route)
                    .map_err(|error| error.to_string())?
                    .address()
            }
            TargetFamily::IosDevice => {
                let service = IosDeviceRouteAdapter::service_name(*session.credentials().session_id());
                let discovery = LaunchStep::new(IosDeviceRouteAdapter::resolution_command(&service));
                let (child, resolutions) = Self::spawn_step(Path::new("."), &discovery)?;
                self.resources.lock().map_err(lock_error)?.children.push(child);
                IosDeviceRouteAdapter::resolve(
                    &resolutions,
                    &readiness,
                    &service,
                    READINESS_TIMEOUT,
                )
                .map_err(|error| error.to_string())?
                .address()
            }
            TargetFamily::Web => return Err("web has no native reload listener".to_owned()),
        };
        let limits = client_protocol_limits(session.host_config());
        self.resources.lock().map_err(lock_error)?.client = Some(ReloadClient::new(
            address,
            session.credentials().clone(),
            limits,
        ));
        Ok(())
    }

    fn push_reload(
        &self,
        request_id: u64,
        metadata: ModuleMetadata,
        module: &[u8],
    ) -> Result<ReloadResult, String> {
        let resources = self.resources.lock().map_err(lock_error)?;
        resources
            .client
            .as_ref()
            .ok_or_else(|| "reload client is not connected".to_owned())?
            .push_reload(request_id, metadata, module)
            .map_err(client_error)
    }

    fn recover_result(&self, request_id: u64) -> Result<Option<ReloadResult>, String> {
        let resources = self.resources.lock().map_err(lock_error)?;
        resources
            .client
            .as_ref()
            .ok_or_else(|| "reload client is not connected".to_owned())?
            .query_result(request_id)
            .map_err(client_error)
    }

    fn start_watcher(&self, _: &Path, watch_roots: &[PathBuf]) -> Result<(), String> {
        let (sender, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .map_err(|error| format!("failed to create source watcher: {error}"))?;
        let exact_watch_roots = self
            .resources
            .lock()
            .map_err(lock_error)?
            .exact_watch_roots
            .clone();
        let watch_roots = exact_watch_roots.as_deref().unwrap_or(watch_roots);
        for root in watch_roots.iter().filter(|root| root.exists()) {
            watcher
                .watch(root, RecursiveMode::Recursive)
                .map_err(|error| format!("failed to watch {}: {error}", root.display()))?;
        }
        let mut resources = self.resources.lock().map_err(lock_error)?;
        resources.watcher = Some(watcher);
        resources.changes = Some(receiver);
        Ok(())
    }

    fn next_change(&self, timeout: Duration) -> Result<Option<Vec<PathBuf>>, String> {
        let resources = self.resources.lock().map_err(lock_error)?;
        let receiver = resources
            .changes
            .as_ref()
            .ok_or_else(|| "source watcher was not started".to_owned())?;
        match receiver.recv_timeout(timeout) {
            Ok(Ok(event)) => {
                let mut paths = event.paths;
                while let Ok(event) = receiver.recv_timeout(Duration::from_millis(75)) {
                    paths.extend(
                        event
                            .map_err(|error| format!("source watcher failed: {error}"))?
                            .paths,
                    );
                }
                Ok(Some(paths))
            }
            Ok(Err(error)) => Err(format!("source watcher failed: {error}")),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => {
                Err("source watcher stopped unexpectedly".to_owned())
            }
        }
    }

    fn cleanup(&self) {
        let Ok(mut resources) = self.resources.lock() else {
            return;
        };
        resources.startup_cancelled = true;
        terminate_child(&mut resources.host_build);
        terminate_child(&mut resources.guest_build);
        resources.watcher = None;
        resources.changes = None;
        resources.client = None;
        resources.android_session = None;
        resources.android_route = None;
        for child in &mut resources.children {
            let _ = child.kill();
            let _ = child.wait();
        }
        resources.children.clear();
        resources.announcements = None;
    }
}

fn terminate_child(child: &mut Option<Child>) {
    if let Some(mut child) = child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn stream_lines<R>(reader: R, sender: mpsc::Sender<String>)
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            eprintln!("{line}");
            let _ = sender.send(line);
        }
    });
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> String {
    "hot-reload runtime state lock was poisoned".to_owned()
}

fn client_error(error: ClientError) -> String {
    error.to_string()
}

fn client_protocol_limits(config: DevelopmentHostConfig) -> ProtocolLimits {
    ProtocolLimits::new(
        config.module_bytes_limit(),
        Duration::from_millis(config.protocol_io_timeout_ms_limit()),
    )
    .max_chunk_bytes(config.protocol_chunk_bytes_limit())
    .max_diagnostic_bytes(config.protocol_diagnostic_bytes_limit())
    .max_terminal_results(config.protocol_terminal_result_limit())
}

fn mobile_application_id(project_root: &Path, fallback: &str) -> String {
    AimerManifest::load_from(project_root)
        .ok()
        .flatten()
        .map(|manifest| {
            let group = manifest.package.group.trim_end_matches('.');
            if group.is_empty() || group.ends_with(fallback) {
                if group.is_empty() { fallback.to_owned() } else { group.to_owned() }
            } else {
                format!("{group}.{fallback}")
            }
        })
        .unwrap_or_else(|| fallback.to_owned())
}

fn android_application_id(project_root: &Path, fallback: &str) -> String {
    fs::read_to_string(project_root.join("builds/android/app/build.gradle.kts"))
        .ok()
        .and_then(|script| {
            script
                .lines()
                .find(|line| line.contains("applicationId"))
                .and_then(|line| line.split('"').nth(1))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| mobile_application_id(project_root, fallback))
}

fn portable_source_root(project_root: &Path) -> PathBuf {
    let manifest = fs::read_to_string(project_root.join("Cargo.toml"))
        .ok()
        .and_then(|source| toml::from_str::<toml::Value>(&source).ok());
    let crate_source = manifest
        .as_ref()
        .and_then(|manifest| manifest.get("lib"))
        .and_then(|library| library.get("path"))
        .and_then(toml::Value::as_str)
        .map(|path| project_root.join(path))
        .or_else(|| {
            let library = project_root.join("src/lib.rs");
            library.is_file().then_some(library)
        })
        .unwrap_or_else(|| project_root.join("src/main.rs"));
    crate_source
        .parent()
        .unwrap_or(project_root)
        .to_owned()
}

fn local_path_dependencies(project_root: &Path) -> Vec<PathBuf> {
    let Some(manifest) = fs::read_to_string(project_root.join("Cargo.toml"))
        .ok()
        .and_then(|source| toml::from_str::<toml::Value>(&source).ok())
    else {
        return Vec::new();
    };
    let mut roots = Vec::new();
    collect_dependency_paths(&manifest, project_root, &mut roots);
    roots.sort();
    roots.dedup();
    roots
}

fn collect_dependency_paths(value: &toml::Value, project_root: &Path, roots: &mut Vec<PathBuf>) {
    let Some(table) = value.as_table() else {
        return;
    };
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dependencies) = table.get(section).and_then(toml::Value::as_table) {
            for dependency in dependencies.values() {
                if let Some(path) = dependency
                    .as_table()
                    .and_then(|dependency| dependency.get("path"))
                    .and_then(toml::Value::as_str)
                {
                    let path = project_root.join(path);
                    roots.push(fs::canonicalize(&path).unwrap_or(path));
                }
            }
        }
    }
    if let Some(targets) = table.get("target").and_then(toml::Value::as_table) {
        for target in targets.values() {
            collect_dependency_paths(target, project_root, roots);
        }
    }
}

fn stable_identity(package: &str) -> [u8; 16] {
    let digest = sha2::Sha256::digest(format!("AIMER-APPLICATION-V1\0{package}"));
    digest[..16].try_into().expect("SHA-256 prefix has 16 bytes")
}

fn build_identity(module: &[u8]) -> [u8; 16] {
    let digest = sha2::Sha256::digest(module);
    digest[..16].try_into().expect("SHA-256 prefix has 16 bytes")
}

fn empty_capability_digest() -> [u8; 32] {
    use sha2::Digest;

    let mut digest = sha2::Sha256::new();
    digest.update(b"AIMER-CAPABILITY-MANIFEST-V1");
    digest.update(0_u64.to_le_bytes());
    digest.finalize().into()
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;

    use aimer_reload_protocol::{ReloadResult, ReloadStage};
    use tempfile::tempdir;

    use crate::config::{ApplicationRuntime, BuildProfile, ReloadPolicy};

    use super::*;

    #[test]
    fn reload_client_uses_the_exact_session_protocol_policy() {
        let config = DevelopmentHostConfig::cli_safe_profile()
            .max_module_bytes(12_345_678)
            .protocol_max_chunk_bytes(12_345)
            .protocol_max_diagnostic_bytes(2_345)
            .protocol_max_terminal_results(3)
            .protocol_io_timeout_ms(4_567);

        let limits = client_protocol_limits(config);

        assert_eq!(limits.max_module_bytes(), 12_345_678);
        assert_eq!(limits.chunk_bytes_limit(), 12_345);
        assert_eq!(limits.diagnostic_bytes_limit(), 2_345);
        assert_eq!(limits.terminal_result_limit(), 3);
        assert_eq!(limits.io_timeout(), Duration::from_millis(4_567));
    }

    #[derive(Default)]
    struct FakeRuntime {
        calls: Mutex<Vec<String>>,
        changes: Mutex<VecDeque<Option<Vec<PathBuf>>>>,
        results: Mutex<VecDeque<Result<ReloadResult, String>>>,
        recovered: Mutex<Option<ReloadResult>>,
        shutdown: Mutex<Option<Arc<AtomicBool>>>,
    }

    impl FakeRuntime {
        fn record(&self, call: impl Into<String>) {
            self.calls.lock().unwrap().push(call.into());
        }
    }

    impl SystemRuntime for FakeRuntime {
        fn build_host(&self, _: &Path, _: &Path, _: &str, device: &SystemDevice) -> Result<(), String> {
            self.record(format!("build host {}", device.family.name()));
            Ok(())
        }

        fn build_guest(&self, _: &Path, _: &Path, _: &str, build: u64) -> Result<Vec<u8>, String> {
            self.record(format!("build guest {build}"));
            Ok(vec![0, 97, 115, 109, build as u8])
        }

        fn assemble(
            &self,
            root: &Path,
            _: &Path,
            package: &str,
            device: &SystemDevice,
        ) -> Result<AssembledHost, String> {
            self.record("assemble");
            Ok(match device.family {
                TargetFamily::Macos | TargetFamily::Windows | TargetFamily::Linux => {
                    AssembledHost::DesktopExecutable(root.join(package))
                }
                _ => AssembledHost::MobileApplication { bundle_id: package.to_owned() },
            })
        }

        fn prepare_route(&self, _: &SystemDevice, port: u16) -> Result<(), String> {
            self.record(format!("route {port}"));
            Ok(())
        }

        fn launch(
            &self,
            _: &SystemDevice,
            _: &str,
            _: &AssembledHost,
            session: &DevelopmentSession,
        ) -> Result<(), String> {
            self.record(format!("launch port={}", session.listener_port()));
            Ok(())
        }

        fn connect(&self, _: &SystemDevice, session: &DevelopmentSession) -> Result<(), String> {
            self.record(format!("connect {}", session.listener_port()));
            Ok(())
        }

        fn push_reload(&self, request: u64, _: ModuleMetadata, _: &[u8]) -> Result<ReloadResult, String> {
            self.record(format!("push {request}"));
            self.results.lock().unwrap().pop_front().unwrap_or_else(|| Ok(committed(request)))
        }

        fn recover_result(&self, request: u64) -> Result<Option<ReloadResult>, String> {
            self.record(format!("recover {request}"));
            Ok(self.recovered.lock().unwrap().clone())
        }

        fn start_watcher(&self, _: &Path, _: &[PathBuf]) -> Result<(), String> {
            self.record("watch");
            Ok(())
        }

        fn next_change(&self, _: Duration) -> Result<Option<Vec<PathBuf>>, String> {
            let mut changes = self.changes.lock().unwrap();
            let change = changes.pop_front().flatten();
            if change.is_some()
                && changes.is_empty()
                && let Some(shutdown) = self.shutdown.lock().unwrap().as_ref()
            {
                shutdown.store(true, Ordering::Release);
            }
            Ok(change)
        }

        fn cleanup(&self) {
            self.record("cleanup");
        }
    }

    fn committed(request: u64) -> ReloadResult {
        ReloadResult::Committed {
            active_generation: request,
            reset_state_entries: 0,
            cleanup_warnings: 0,
        }
    }

    fn policy() -> ExecutionPolicy {
        ExecutionPolicy::new(BuildProfile::Debug, ApplicationRuntime::Wasmi, ReloadPolicy::HotReload).unwrap()
    }

    fn fixture(runtime: Arc<FakeRuntime>) -> (tempfile::TempDir, SystemPipelineOperations<FakeRuntime>) {
        let root = tempdir().unwrap();
        fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let operations = SystemPipelineOperations::with_runtime(
            root.path(),
            root.path(),
            policy(),
            SystemDevice::new("local", TargetFamily::Linux, "linux"),
            "counter",
            runtime,
        );
        (root, operations)
    }

    #[cfg(feature = "hot-reload")]
    #[test]
    fn startup_owns_initial_commit_and_cleanup_without_exposing_session_values() {
        let runtime = Arc::new(FakeRuntime::default());
        let (_root, mut operations) = fixture(Arc::clone(&runtime));

        operations.resolve().unwrap();
        operations.create_session().unwrap();
        operations.build_host().unwrap();
        operations.build_initial_guest().unwrap();
        operations.assemble().unwrap();
        operations.prepare_route().unwrap();
        operations.launch_app().unwrap();
        operations.discover_and_authenticate().unwrap();
        operations.push_initial_module().unwrap();
        operations.cleanup();
        operations.cleanup();

        assert_eq!(runtime.calls.lock().unwrap().last().unwrap(), "cleanup");
        assert_eq!(runtime.calls.lock().unwrap().iter().filter(|call| *call == "cleanup").count(), 1);
        assert!(matches!(operations.statuses().last(), Some(ReloadStatus::Terminal(ReloadResult::Committed { .. }))));
    }

    #[test]
    fn failed_startup_cleanup_releases_a_session_before_any_generation_commits() {
        let runtime = Arc::new(FakeRuntime::default());
        let (_root, mut operations) = fixture(Arc::clone(&runtime));

        operations.resolve().unwrap();
        operations.create_session().unwrap();
        operations.cleanup();
        drop(operations);

        let calls = runtime.calls.lock().unwrap();
        assert_eq!(calls.iter().filter(|call| *call == "cleanup").count(), 1);
    }

    #[test]
    fn a_disconnect_recovers_the_terminal_result_before_rebuilding_again() {
        let runtime = Arc::new(FakeRuntime::default());
        runtime.results.lock().unwrap().push_back(Err("connection reset".to_owned()));
        *runtime.recovered.lock().unwrap() = Some(committed(1));
        let (_root, mut operations) = fixture(Arc::clone(&runtime));
        operations.build_initial_guest().unwrap();

        operations.push_initial_module().unwrap();

        assert_eq!(runtime.calls.lock().unwrap().as_slice(), ["build guest 1", "push 1", "recover 1"]);
    }

    #[test]
    fn watch_rebuilds_once_for_a_change_and_stops_on_the_shared_signal() {
        let runtime = Arc::new(FakeRuntime::default());
        let root = tempdir().unwrap();
        fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        runtime
            .changes
            .lock()
            .unwrap()
            .push_back(Some(vec![root.path().join("src/page.rs")]));
        let shutdown = Arc::new(AtomicBool::new(false));
        *runtime.shutdown.lock().unwrap() = Some(Arc::clone(&shutdown));
        let mut operations = SystemPipelineOperations::with_runtime(
            root.path(),
            root.path(),
            policy(),
            SystemDevice::new("local", TargetFamily::Linux, "linux"),
            "counter",
            Arc::clone(&runtime),
        )
        .shutdown_signal(shutdown);

        operations.watch().unwrap();

        assert_eq!(runtime.calls.lock().unwrap().as_slice(), ["watch", "build guest 1", "push 1"]);
    }

    #[test]
    fn watch_retains_the_app_after_failure_and_accepts_a_later_edit() {
        let runtime = Arc::new(FakeRuntime::default());
        runtime.results.lock().unwrap().extend([
            Ok(committed(1)),
            Err("guest compilation failed".to_owned()),
            Ok(committed(3)),
        ]);
        let (root, mut operations) = fixture(Arc::clone(&runtime));
        let changed = root.path().join("src/page.rs");
        runtime
            .changes
            .lock()
            .unwrap()
            .extend([Some(vec![changed.clone()]), Some(vec![changed])]);
        let shutdown = Arc::new(AtomicBool::new(false));
        *runtime.shutdown.lock().unwrap() = Some(Arc::clone(&shutdown));
        operations.shutdown = shutdown;
        operations.build_initial_guest().unwrap();
        operations.push_initial_module().unwrap();

        operations.watch().unwrap();

        let statuses = operations.statuses();
        assert!(statuses.iter().any(|status| matches!(
            status,
            ReloadStatus::RecoverableFailure { diagnostic }
                if diagnostic.contains("guest compilation failed")
        )));
        assert!(matches!(
            statuses.last(),
            Some(ReloadStatus::Terminal(ReloadResult::Committed {
                active_generation: 3,
                ..
            }))
        ));
    }

    #[test]
    fn watch_rejection_retains_the_last_generation_and_recovers_on_the_next_edit() {
        let runtime = Arc::new(FakeRuntime::default());
        runtime.results.lock().unwrap().extend([
            Ok(committed(1)),
            Ok(ReloadResult::Rejected {
                stage: ReloadStage::Validation,
                error_code: 17,
                active_generation: 1,
                diagnostic: "unknown required widget".to_owned(),
            }),
            Ok(committed(3)),
        ]);
        let (root, mut operations) = fixture(Arc::clone(&runtime));
        let first_change = root.path().join("src/page.rs");
        let second_change = root.path().join("src/other.rs");
        runtime
            .changes
            .lock()
            .unwrap()
            .extend([Some(vec![first_change]), Some(vec![second_change])]);
        let shutdown = Arc::new(AtomicBool::new(false));
        *runtime.shutdown.lock().unwrap() = Some(Arc::clone(&shutdown));
        operations.shutdown = shutdown;
        operations.build_initial_guest().unwrap();
        operations.push_initial_module().unwrap();

        operations.watch().unwrap();

        let terminal = operations
            .statuses()
            .into_iter()
            .filter_map(|status| match status {
                ReloadStatus::Terminal(result) => Some(result),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(terminal.len(), 3);
        assert!(matches!(
            terminal[1],
            ReloadResult::Rejected {
                active_generation: 1,
                ..
            }
        ));
        assert!(matches!(
            terminal[2],
            ReloadResult::Committed {
                active_generation: 3,
                ..
            }
        ));
    }

    struct StartupOverlapRuntime {
        guest_started: AtomicBool,
        guest_finished: AtomicBool,
        host_saw_guest_running: AtomicBool,
        release_guest: AtomicBool,
        cancelled: AtomicBool,
        cancel_called: AtomicBool,
        fail_guest: AtomicBool,
    }

    impl StartupOverlapRuntime {
        fn new() -> Self {
            Self {
                guest_started: AtomicBool::new(false),
                guest_finished: AtomicBool::new(false),
                host_saw_guest_running: AtomicBool::new(false),
                release_guest: AtomicBool::new(false),
                cancelled: AtomicBool::new(false),
                cancel_called: AtomicBool::new(false),
                fail_guest: AtomicBool::new(false),
            }
        }

        fn wait_for_guest_start(&self) {
            while !self.guest_started.load(Ordering::Acquire)
                && !self.release_guest.load(Ordering::Acquire)
            {
                thread::yield_now();
            }
        }
    }

    impl SystemRuntime for StartupOverlapRuntime {
        fn build_host(&self, _: &Path, _: &Path, _: &str, _: &SystemDevice) -> Result<(), String> {
            self.wait_for_guest_start();
            if self.guest_started.load(Ordering::Acquire)
                && !self.guest_finished.load(Ordering::Acquire)
            {
                self.host_saw_guest_running.store(true, Ordering::Release);
            }
            if self.cancelled.load(Ordering::Acquire) {
                Err("native build cancelled".to_owned())
            } else {
                Ok(())
            }
        }

        fn build_guest(&self, _: &Path, _: &Path, _: &str, _: u64) -> Result<Vec<u8>, String> {
            self.guest_started.store(true, Ordering::Release);
            if self.fail_guest.load(Ordering::Acquire) {
                return Err("guest compilation failed".to_owned());
            }
            while !self.release_guest.load(Ordering::Acquire)
                && !self.cancelled.load(Ordering::Acquire)
            {
                thread::yield_now();
            }
            if self.cancelled.load(Ordering::Acquire) {
                return Err("guest build cancelled".to_owned());
            }
            self.guest_finished.store(true, Ordering::Release);
            Ok(vec![0, 97, 115, 109, 1])
        }

        fn assemble(
            &self,
            _: &Path,
            _: &Path,
            package: &str,
            _: &SystemDevice,
        ) -> Result<AssembledHost, String> {
            Ok(AssembledHost::DesktopExecutable(PathBuf::from(package)))
        }

        fn prepare_route(&self, _: &SystemDevice, _: u16) -> Result<(), String> {
            Ok(())
        }

        fn launch(
            &self,
            _: &SystemDevice,
            _: &str,
            _: &AssembledHost,
            _: &DevelopmentSession,
        ) -> Result<(), String> {
            Ok(())
        }

        fn connect(&self, _: &SystemDevice, _: &DevelopmentSession) -> Result<(), String> {
            Ok(())
        }

        fn push_reload(
            &self,
            request: u64,
            _: ModuleMetadata,
            _: &[u8],
        ) -> Result<ReloadResult, String> {
            Ok(committed(request))
        }

        fn recover_result(&self, _: u64) -> Result<Option<ReloadResult>, String> {
            Ok(None)
        }

        fn start_watcher(&self, _: &Path, _: &[PathBuf]) -> Result<(), String> {
            Ok(())
        }

        fn next_change(&self, _: Duration) -> Result<Option<Vec<PathBuf>>, String> {
            Ok(None)
        }

        fn cancel_startup(&self) {
            self.cancel_called.store(true, Ordering::Release);
            self.cancelled.store(true, Ordering::Release);
            self.release_guest.store(true, Ordering::Release);
        }

        fn cleanup(&self) {}
    }

    #[cfg(feature = "hot-reload")]
    #[test]

    fn initial_guest_build_overlaps_native_build_and_waits_until_initial_push() {
        let runtime = Arc::new(StartupOverlapRuntime::new());
        let root = tempdir().unwrap();
        fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let mut operations = SystemPipelineOperations::with_runtime(
            root.path(),
            root.path(),
            policy(),
            SystemDevice::new("local", TargetFamily::Linux, "linux"),
            "counter",
            Arc::clone(&runtime),
        );
        operations.create_session().unwrap();

        let release = Arc::clone(&runtime);
        let delayed_release = thread::spawn(move || {
            while !release.guest_started.load(Ordering::Acquire) {
                thread::yield_now();
            }
            thread::sleep(Duration::from_millis(100));
            release.release_guest.store(true, Ordering::Release);
        });

        operations.build_initial_guest().unwrap();
        operations.build_host().unwrap();
        assert!(runtime.host_saw_guest_running.load(Ordering::Acquire));
        assert!(!runtime.guest_finished.load(Ordering::Acquire));

        operations.assemble().unwrap();
        operations.prepare_route().unwrap();
        operations.launch_app().unwrap();
        operations.discover_and_authenticate().unwrap();
        assert!(!runtime.guest_finished.load(Ordering::Acquire));

        runtime.release_guest.store(true, Ordering::Release);
        operations.push_initial_module().unwrap();
        delayed_release.join().unwrap();
        assert!(runtime.guest_finished.load(Ordering::Acquire));
    }

    #[cfg(feature = "hot-reload")]
    #[test]
    fn guest_failure_cancels_the_other_initial_startup_branch() {
        let runtime = Arc::new(StartupOverlapRuntime::new());
        runtime.fail_guest.store(true, Ordering::Release);
        let root = tempdir().unwrap();
        fs::write(root.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let mut operations = SystemPipelineOperations::with_runtime(
            root.path(),
            root.path(),
            policy(),
            SystemDevice::new("local", TargetFamily::Linux, "linux"),
            "counter",
            Arc::clone(&runtime),
        );

        operations.build_initial_guest().unwrap();
        while !runtime.cancel_called.load(Ordering::Acquire) {
            thread::yield_now();
        }
        let error = operations.build_host().unwrap_err();

        assert_eq!(error, "native build cancelled");
        operations.cleanup();
    }

    #[cfg(feature = "hot-reload")]
    #[test]
    fn invalid_policy_and_web_target_fail_before_any_side_effect() {
        let runtime = Arc::new(FakeRuntime::default());
        let (_root, mut native) = fixture(Arc::clone(&runtime));
        native.policy = ExecutionPolicy::new(
            BuildProfile::Debug,
            ApplicationRuntime::NativeAot,
            ReloadPolicy::Disabled,
        )
        .unwrap();
        assert!(native.resolve().unwrap_err().contains("debug/wasmi/hot-reload"));

        let (_root, mut web) = fixture(Arc::clone(&runtime));
        web.device.family = TargetFamily::Web;
        assert!(web.resolve().unwrap_err().contains("does not support"));
        assert!(runtime.calls.lock().unwrap().is_empty());
    }
}
