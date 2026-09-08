//! Run-session identity, discovery, and the private control protocol used by
//! the aimer mcp adapter.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, TryLockError, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

const SESSION_DIRECTORY: &str = ".aimer/sessions";
const LOG_CAPACITY: usize = 32_768;
const CONTROL_RETRY_COUNT: usize = 3;
const CONTROL_RETRY_DELAY: Duration = Duration::from_millis(40);
const LIVENESS_RETRY_COUNT: usize = 3;
const LIVENESS_RETRY_DELAY: Duration = Duration::from_millis(10);
const LIVENESS_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60 * 10);

/// Whether a process belongs to the host or to a connected device.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessScope {
    /// A process addressable by host operating-system tooling.
    Host,
    /// A process addressable through a device bridge or platform adapter.
    Device,
}

/// The role played by a process in a run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRole {
    /// The application being debugged.
    App,
    /// A development server or log/runner process.
    Runner,
    /// A short-lived launcher process.
    Launcher,
}

/// How an agent may use the process identity for visual inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualTarget {
    /// A host PID suitable for host computer-use tooling.
    HostProcess,
    /// An app PID that belongs to a connected device.
    DeviceApp,
    /// A web application that must be opened through its URL.
    WebUrl,
    /// No visual target is currently available.
    Unavailable,
}

/// Structured identity for the process associated with the current run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProcessIdentity {
    /// The process ID, when the platform can resolve one.
    pub pid: Option<u32>,
    /// Where the process runs.
    pub scope: ProcessScope,
    /// The process's role in the run.
    pub role: ProcessRole,
    /// Executable path when known.
    pub executable: Option<PathBuf>,
    /// Bundle/application identifier when the platform exposes one.
    pub bundle_id: Option<String>,
    /// Whether this identity can be passed to a visual tool.
    pub visual_target: VisualTarget,
}

impl ProcessIdentity {
    /// Creates a host application identity.
    pub fn host_app(pid: u32, executable: Option<PathBuf>, bundle_id: Option<String>) -> Self {
        Self {
            pid: Some(pid),
            scope: ProcessScope::Host,
            role: ProcessRole::App,
            executable,
            bundle_id,
            visual_target: VisualTarget::HostProcess,
        }
    }

    /// Creates a device application identity.
    pub fn device_app(pid: Option<u32>, bundle_id: Option<String>) -> Self {
        Self {
            pid,
            scope: ProcessScope::Device,
            role: ProcessRole::App,
            executable: None,
            bundle_id,
            visual_target: VisualTarget::DeviceApp,
        }
    }

    /// Creates a web development-server identity. The URL is carried by the
    /// session snapshot alongside this identity.
    pub fn web_runner(pid: Option<u32>, executable: Option<PathBuf>) -> Self {
        Self {
            pid,
            scope: ProcessScope::Host,
            role: ProcessRole::Runner,
            executable,
            bundle_id: None,
            visual_target: VisualTarget::WebUrl,
        }
    }

    /// Creates an identity for a non-host app without a resolvable PID.
    pub fn unavailable(scope: ProcessScope, bundle_id: Option<String>) -> Self {
        Self {
            pid: None,
            scope,
            role: ProcessRole::App,
            executable: None,
            bundle_id,
            visual_target: VisualTarget::Unavailable,
        }
    }

    /// Returns whether this identity is suitable for host visual tooling.
    #[inline]
    pub fn is_host_visual_process(&self) -> bool {
        self.scope == ProcessScope::Host
            && self.role == ProcessRole::App
            && self.pid.is_some()
            && self.visual_target == VisualTarget::HostProcess
    }
}

/// The lifecycle state reported by a run session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Starting,
    Locking,
    Fetching,
    Compiling,
    Building,
    Launching,
    Running,
    Idling,
    Restarting,
    Stopping,
    Stopped,
    Error,
}

impl Default for SessionStatus {
    fn default() -> Self {
        Self::Starting
    }
}

/// The configuration that can be replayed by a restart request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunConfiguration {
    pub project_root: PathBuf,
    pub target: String,
    pub device: Option<String>,
    pub execution_policy: String,
    pub parent_pid: u32,
}

/// A single bounded log record retained by a run session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: u64,
    pub run_id: u64,
    pub stream: LogStream,
    pub level: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

/// Logical source stream for a log query.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogStream {
    All,
    Build,
    App,
    System,
}

impl LogStream {
    fn matches(self, other: Self) -> bool {
        self == Self::All || self == other
    }
}

/// Query parameters accepted by the log control operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GetLogsRequest {
    #[serde(default)]
    pub stream: Option<LogStream>,
    /// Return entries strictly after this sequence number.
    #[serde(default)]
    pub cursor: Option<u64>,
    #[serde(default)]
    pub run_id: Option<u64>,
    #[serde(default = "default_log_limit")]
    pub limit: usize,
    #[serde(default = "default_true")]
    pub include_source_locations: bool,
}

impl Default for GetLogsRequest {
    fn default() -> Self {
        Self {
            stream: None,
            cursor: None,
            run_id: None,
            limit: default_log_limit(),
            include_source_locations: true,
        }
    }
}

fn default_log_limit() -> usize {
    200
}

fn default_true() -> bool {
    true
}

/// A page of logs and the cursor to use for the next page.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogsPage {
    pub entries: Vec<LogEntry>,
    pub next_cursor: Option<u64>,
    pub oldest_seq: Option<u64>,
    pub newest_seq: Option<u64>,
}

/// Result returned after clearing logs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClearLogsResult {
    pub cleared_through: u64,
}

/// Current externally visible session state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub session_id: Uuid,
    pub run_id: u64,
    pub project_root: PathBuf,
    pub target: String,
    pub device: Option<String>,
    pub execution_policy: String,
    pub parent_pid: u32,
    pub status: SessionStatus,
    pub last_error: Option<String>,
    pub app_process: Option<ProcessIdentity>,
    /// Web targets expose the dev-server URL independently of the runner PID.
    pub app_url: Option<String>,
}

/// A persisted session descriptor used for discovery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SessionDescriptor {
    pub session_id: Uuid,
    pub project_root: PathBuf,
    pub endpoint: String,
    pub target: String,
    pub device: Option<String>,
    pub parent_pid: u32,
    pub run_id: u64,
    pub status: SessionStatus,
    pub app_process: Option<ProcessIdentity>,
    pub app_url: Option<String>,
    /// The token is intentionally omitted from sessions output.
    #[serde(skip_serializing)]
    token: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredSessionDescriptor {
    session_id: Uuid,
    project_root: PathBuf,
    endpoint: String,
    token: String,
    target: String,
    device: Option<String>,
    parent_pid: u32,
    run_id: u64,
    status: SessionStatus,
    app_process: Option<ProcessIdentity>,
    app_url: Option<String>,
}

impl StoredSessionDescriptor {
    fn into_descriptor(self) -> SessionDescriptor {
        SessionDescriptor {
            session_id: self.session_id,
            project_root: self.project_root,
            endpoint: self.endpoint,
            target: self.target,
            device: self.device,
            parent_pid: self.parent_pid,
            run_id: self.run_id,
            status: self.status,
            app_process: self.app_process,
            app_url: self.app_url,
            token: self.token,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct WireResponse {
    request_id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct WireRequest {
    request_id: String,
    token: String,
    operation: String,
    #[serde(default)]
    payload: Value,
}

/// A command delivered from the control endpoint to the run loop.
pub enum SessionCommand {
    /// Stop the current run and leave the session stopped.
    Stop {
        reply: mpsc::Sender<Result<SessionSnapshot, String>>,
    },
    /// Restart the run with the original configuration.
    Restart {
        reply: mpsc::Sender<Result<SessionSnapshot, String>>,
    },
    /// Clear retained logs.
    ClearLogs {
        request: GetLogsRequest,
        reply: mpsc::Sender<Result<ClearLogsResult, String>>,
    },
}

enum PendingMutation {
    Restart(mpsc::Sender<Result<SessionSnapshot, String>>),
}

struct SessionState {
    run_id: u64,
    status: SessionStatus,
    last_error: Option<String>,
    app_process: Option<ProcessIdentity>,
    app_url: Option<String>,
    next_seq: u64,
    logs: VecDeque<LogEntry>,
    commands: VecDeque<SessionCommand>,
    pending_mutation: Option<PendingMutation>,
}

struct SharedSession {
    session_id: Uuid,
    configuration: RunConfiguration,
    endpoint: Mutex<String>,
    token: String,
    descriptor_path: PathBuf,
    descriptor_write: Mutex<()>,
    state: Mutex<SessionState>,
    wake: Condvar,
    mutation: Mutex<()>,
}

/// A cloneable handle used by the run loop to publish state and consume
/// control commands.
#[derive(Clone)]
pub struct SessionHandle {
    shared: Arc<SharedSession>,
    run_id: Option<u64>,
}

impl SessionHandle {
    /// Returns the stable session identifier.
    #[inline]
    pub fn session_id(&self) -> Uuid {
        self.shared.session_id
    }

    /// Returns a snapshot suitable for status output.
    pub fn snapshot(&self) -> SessionSnapshot {
        let state = self.shared.state.lock().expect("session mutex poisoned");
        snapshot_locked(&self.shared, &state)
    }

    /// Creates a handle scoped to the current run. Lifecycle events from a
    /// previous runner then cannot clear the identity of a newly restarted
    /// app.
    pub fn scoped(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            run_id: Some(self.snapshot().run_id),
        }
    }

    /// Publish a lifecycle status. Running, Idling, and Error also complete a
    /// pending restart request.
    pub fn set_status(&self, status: SessionStatus, last_error: Option<String>) {
        let mut pending = None;
        {
            let mut state = self.shared.state.lock().expect("session mutex poisoned");
            if self.run_id.is_some_and(|run_id| run_id != state.run_id) {
                return;
            }
            state.status = status;
            if last_error.is_some() {
                state.last_error = last_error;
            } else if status != SessionStatus::Error {
                state.last_error = None;
            }
            if matches!(status, SessionStatus::Idling | SessionStatus::Stopped) {
                state.app_process = None;
                state.app_url = None;
            }
            if matches!(status, SessionStatus::Running | SessionStatus::Error)
                && let Some(PendingMutation::Restart(reply)) = state.pending_mutation.take()
            {
                pending = Some((reply, status, snapshot_locked(&self.shared, &state)));
            }
        }
        self.persist_descriptor();
        if let Some((reply, status, snapshot)) = pending {
            let result = if status == SessionStatus::Running {
                Ok(snapshot)
            } else {
                Err(snapshot
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "project restart failed".to_string()))
            };
            let _ = reply.send(result);
        }
        self.shared.wake.notify_all();
    }

    /// Set or replace the current application or runner process identity.
    pub fn set_process(&self, process: Option<ProcessIdentity>, app_url: Option<String>) {
        {
            let mut state = self.shared.state.lock().expect("session mutex poisoned");
            if self.run_id.is_some_and(|run_id| run_id != state.run_id) {
                return;
            }
            state.app_process = process;
            state.app_url = app_url;
        }
        self.persist_descriptor();
        self.shared.wake.notify_all();
    }

    /// Clear the current process as soon as a restart or app exit begins.
    pub fn clear_process(&self) {
        self.set_process(None, None);
    }

    /// Append a bounded log record to the current run.
    pub fn push_log(
        &self,
        stream: LogStream,
        level: impl Into<String>,
        message: impl Into<String>,
        location: Option<String>,
    ) {
        let mut state = self.shared.state.lock().expect("session mutex poisoned");
        let entry = LogEntry {
            seq: state.next_seq,
            run_id: state.run_id,
            stream,
            level: level.into(),
            message: message.into(),
            location,
        };
        state.next_seq = state.next_seq.saturating_add(1);
        state.logs.push_back(entry);
        while state.logs.len() > LOG_CAPACITY {
            state.logs.pop_front();
        }
    }

    /// Read a cursor-paginated page of logs.
    pub fn get_logs(&self, request: GetLogsRequest) -> LogsPage {
        let state = self.shared.state.lock().expect("session mutex poisoned");
        logs_locked(&state, request)
    }

    /// Take the next mutation command, if one is waiting for the run loop.
    pub fn take_command(&self) -> Option<SessionCommand> {
        self.shared
            .state
            .lock()
            .expect("session mutex poisoned")
            .commands
            .pop_front()
    }

    /// Mark a restart command as accepted. The PID is cleared and run_id
    /// increments before the old child is killed.
    pub fn accept_restart(
        &self,
        reply: mpsc::Sender<Result<SessionSnapshot, String>>,
    ) -> anyhow::Result<()> {
        {
            let mut state = self.shared.state.lock().expect("session mutex poisoned");
            if state.pending_mutation.is_some() {
                anyhow::bail!("busy")
            }
            state.run_id = state.run_id.saturating_add(1);
            state.status = SessionStatus::Restarting;
            state.last_error = None;
            state.app_process = None;
            state.app_url = None;
            state.pending_mutation = Some(PendingMutation::Restart(reply));
        }
        self.persist_descriptor();
        self.shared.wake.notify_all();
        Ok(())
    }

    /// Complete a restart immediately when the runner cannot be started.
    pub fn fail_restart(&self, error: impl Into<String>) {
        let reply = {
            let mut state = self.shared.state.lock().expect("session mutex poisoned");
            state.status = SessionStatus::Error;
            state.last_error = Some(error.into());
            match state.pending_mutation.take() {
                Some(PendingMutation::Restart(reply)) => Some(reply),
                None => None,
            }
        };
        self.persist_descriptor();
        if let Some(reply) = reply {
            let _ = reply.send(Err(self
                .snapshot()
                .last_error
                .unwrap_or_else(|| "project restart failed".to_string())));
        }
    }

    /// Mark the session stopped and answer a stop request.
    pub fn complete_stop(&self, reply: mpsc::Sender<Result<SessionSnapshot, String>>) {
        self.clear_process();
        self.set_status(SessionStatus::Stopped, None);
        let _ = reply.send(Ok(self.snapshot()));
    }

    /// Clear logs for the requested stream and reply with the sequence
    /// boundary. This is called by the run loop so it shares mutation
    /// serialization with restart and stop.
    pub fn complete_clear_logs(
        &self,
        request: GetLogsRequest,
        reply: mpsc::Sender<Result<ClearLogsResult, String>>,
    ) {
        let boundary = {
            let mut state = self.shared.state.lock().expect("session mutex poisoned");
            let boundary = state.next_seq.saturating_sub(1);
            let stream = request.stream.unwrap_or(LogStream::All);
            if stream == LogStream::All && request.run_id.is_none() {
                state.logs.clear();
            } else {
                state.logs.retain(|entry| {
                    !(stream.matches(entry.stream)
                        && request
                            .run_id
                            .map_or(true, |run_id| entry.run_id == run_id))
                });
            }
            boundary
        };
        let _ = reply.send(Ok(ClearLogsResult {
            cleared_through: boundary,
        }));
    }

    fn enqueue_command(&self, command: SessionCommand) {
        let mut state = self.shared.state.lock().expect("session mutex poisoned");
        state.commands.push_back(command);
        self.shared.wake.notify_all();
    }

    fn request_mutation<T>(
        &self,
        command: impl FnOnce(mpsc::Sender<Result<T, String>>) -> SessionCommand,
    ) -> Result<T, String> {
        let _guard = match self.shared.mutation.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => return Err("busy".to_string()),
            Err(TryLockError::Poisoned(_)) => {
                return Err("session mutation lock poisoned".to_string());
            }
        };
        let (reply, receiver) = mpsc::channel();
        self.enqueue_command(command(reply));
        receiver
            .recv_timeout(REQUEST_TIMEOUT)
            .map_err(|_| "timed out waiting for the run loop".to_string())?
    }

    fn request_status(&self, operation: &str, payload: Value) -> Result<Value, String> {
        match operation {
            "ping" | "status" => {
                serde_json::to_value(self.snapshot()).map_err(|e| e.to_string())
            }
            "logs" => {
                let request: GetLogsRequest =
                    serde_json::from_value(payload).map_err(|e| e.to_string())?;
                serde_json::to_value(self.get_logs(request)).map_err(|e| e.to_string())
            }
            "restart" => {
                let snapshot = self.request_mutation(|reply| SessionCommand::Restart { reply })?;
                serde_json::to_value(snapshot).map_err(|e| e.to_string())
            }
            "stop" => {
                let snapshot = self.request_mutation(|reply| SessionCommand::Stop { reply })?;
                serde_json::to_value(snapshot).map_err(|e| e.to_string())
            }
            "clear_logs" => {
                let request: GetLogsRequest = if payload.is_null() {
                    GetLogsRequest::default()
                } else {
                    serde_json::from_value(payload).map_err(|e| e.to_string())?
                };
                let result =
                    self.request_mutation(|reply| SessionCommand::ClearLogs { request, reply })?;
                serde_json::to_value(result).map_err(|e| e.to_string())
            }
            other => Err(format!("unknown control operation '{other}'")),
        }
    }

    fn persist_descriptor(&self) {
        if let Err(error) = self.persist_descriptor_checked() {
            tracing::debug!(error = %error, "failed to update aimer session descriptor");
        }
    }

    fn persist_descriptor_checked(&self) -> anyhow::Result<()> {
        let _write_guard = self
            .shared
            .descriptor_write
            .lock()
            .expect("descriptor mutex poisoned");
        let state = self.shared.state.lock().expect("session mutex poisoned");
        let descriptor = StoredSessionDescriptor {
            session_id: self.shared.session_id,
            project_root: self.shared.configuration.project_root.clone(),
            endpoint: self
                .shared
                .endpoint
                .lock()
                .expect("endpoint mutex poisoned")
                .clone(),
            token: self.shared.token.clone(),
            target: self.shared.configuration.target.clone(),
            device: self.shared.configuration.device.clone(),
            parent_pid: self.shared.configuration.parent_pid,
            run_id: state.run_id,
            status: state.status,
            app_process: state.app_process.clone(),
            app_url: state.app_url.clone(),
        };
        drop(state);
        write_descriptor(&self.shared.descriptor_path, &descriptor)
    }
}

/// The server registration kept alive for the duration of an aimer run.
pub struct SessionRuntime {
    handle: SessionHandle,
    shutdown: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

/// Stable name for the run-scoped control lifetime exposed to embedders.
pub type RunSession = SessionRuntime;

impl SessionRuntime {
    /// Start a private loopback endpoint and persist its discovery descriptor.
    pub fn start(configuration: RunConfiguration) -> anyhow::Result<Self> {
        Self::start_in(sessions_directory()?, configuration)
    }

    /// Start a session in an explicit registry directory. This is useful for
    /// embedding and tests; the CLI uses the per-user default location.
    pub fn start_in(
        directory: PathBuf,
        configuration: RunConfiguration,
    ) -> anyhow::Result<Self> {
        let session_id = Uuid::new_v4();
        let token = Uuid::new_v4().to_string();
        fs::create_dir_all(&directory)
            .with_context(|| format!("creating {}", directory.display()))?;
        let descriptor_path = directory.join(format!("{session_id}.json"));
        let (listener, endpoint) = bind_control_listener()?;
        let state = SessionState {
            run_id: 1,
            status: SessionStatus::Starting,
            last_error: None,
            app_process: None,
            app_url: None,
            next_seq: 1,
            logs: VecDeque::with_capacity(LOG_CAPACITY.min(1024)),
            commands: VecDeque::new(),
            pending_mutation: None,
        };
        let shared = Arc::new(SharedSession {
            session_id,
            configuration,
            endpoint: Mutex::new(endpoint),
            token,
            descriptor_path,
            descriptor_write: Mutex::new(()),
            state: Mutex::new(state),
            wake: Condvar::new(),
            mutation: Mutex::new(()),
        });
        let handle = SessionHandle {
            shared: Arc::clone(&shared),
            run_id: None,
        };
        handle
            .persist_descriptor_checked()
            .context("registering aimer run session")?;

        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let thread_shared = Arc::clone(&shared);
        let join = match thread::Builder::new()
            .name("aimer-mcp-control".to_string())
            .spawn(move || serve(listener, thread_shared, thread_shutdown))
        {
            Ok(join) => join,
            Err(error) => {
                let _ = fs::remove_file(&shared.descriptor_path);
                return Err(error).context("starting aimer MCP control endpoint");
            }
        };

        Ok(Self {
            handle,
            shutdown,
            join: Some(join),
        })
    }

    /// Clone the handle used by the run lifecycle.
    #[inline]
    pub fn handle(&self) -> SessionHandle {
        self.handle.clone()
    }
}

impl Drop for SessionRuntime {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        let endpoint = self
            .handle
            .shared
            .endpoint
            .lock()
            .expect("endpoint mutex poisoned")
            .clone();
        let _ = TcpStream::connect(endpoint);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        let _ = fs::remove_file(&self.handle.shared.descriptor_path);
    }
}

/// Registry access for session discovery. The default path is the user's
/// aimer sessions directory; tests may supply an isolated directory.
#[derive(Clone, Debug)]
pub struct SessionRegistry {
    directory: PathBuf,
}

impl SessionRegistry {
    /// Creates a registry rooted at directory.
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    /// Returns the default per-user session registry.
    pub fn default_location() -> anyhow::Result<Self> {
        Ok(Self::new(sessions_directory()?))
    }

    /// Lists live sessions, removing a descriptor only after its endpoint
    /// fails token-authenticated liveness validation.
    pub fn live(&self, project: Option<&Path>) -> anyhow::Result<Vec<SessionDescriptor>> {
        let mut sessions = Vec::new();
        let entries = match fs::read_dir(&self.directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(sessions),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("reading {}", self.directory.display()));
            }
        };
        let expected_project = project.map(normalize_project_path);
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let path = entry.path();
            let descriptor = match read_descriptor(&path) {
                Ok(descriptor) => descriptor,
                Err(_) => continue,
            };
            let Some(file_session_id) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| Uuid::parse_str(stem).ok())
            else {
                continue;
            };
            if descriptor.session_id != file_session_id {
                continue;
            }
            if expected_project.as_ref().is_some_and(|expected| {
                normalize_project_path(&descriptor.project_root) != *expected
            }) {
                continue;
            }
            if ControlClient::new(descriptor.clone())
                .ping_with_retries()
                .is_ok()
            {
                sessions.push(descriptor);
            } else {
                let _ = fs::remove_file(path);
            }
        }
        sessions.sort_by_key(|session| session.session_id);
        Ok(sessions)
    }

    /// Load a descriptor by exact session ID and validate its endpoint/token.
    pub fn attach(&self, session_id: Uuid) -> anyhow::Result<ControlClient> {
        let path = self.directory.join(format!("{session_id}.json"));
        let descriptor = read_descriptor(&path)
            .with_context(|| format!("session '{session_id}' was not found"))?;
        if descriptor.session_id != session_id {
            anyhow::bail!("session descriptor identity mismatch for '{session_id}'");
        }
        let client = ControlClient::new(descriptor);
        if let Err(error) = client.ping_with_retries() {
            let _ = fs::remove_file(path);
            return Err(anyhow!("session '{session_id}' is not reachable: {error}"));
        }
        Ok(client)
    }
}

/// A validated client for the private session control endpoint.
#[derive(Clone, Debug)]
pub struct ControlClient {
    descriptor: SessionDescriptor,
}

impl ControlClient {
    fn new(descriptor: SessionDescriptor) -> Self {
        Self { descriptor }
    }

    /// Returns the descriptor used for this attachment.
    #[inline]
    pub fn descriptor(&self) -> &SessionDescriptor {
        &self.descriptor
    }

    /// Send a control request and return its JSON result.
    pub fn request(&self, operation: &str, payload: Value) -> anyhow::Result<Value> {
        self.request_with_timeout(operation, payload, REQUEST_TIMEOUT)
    }

    fn request_with_timeout(
        &self,
        operation: &str,
        payload: Value,
        read_timeout: Duration,
    ) -> anyhow::Result<Value> {
        let request_id = Uuid::new_v4().to_string();
        let request = json!({
            "request_id": request_id,
            "token": self.descriptor.token,
            "operation": operation,
            "payload": payload,
        });
        let address = self
            .descriptor
            .endpoint
            .parse()
            .context("invalid aimer MCP endpoint")?;
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
        stream.set_read_timeout(Some(read_timeout))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        serde_json::to_writer(&mut stream, &request)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        let mut response_line = String::new();
        BufReader::new(stream).read_line(&mut response_line)?;
        let response: WireResponse = serde_json::from_str(&response_line)
            .context("invalid response from aimer MCP control endpoint")?;
        if response.request_id != request_id {
            return Err(anyhow!("mismatched control response ID"));
        }
        if response.ok {
            Ok(response.result.unwrap_or(Value::Null))
        } else {
            Err(anyhow!(
                "{}",
                response
                    .error
                    .unwrap_or_else(|| "control request failed".to_string())
            ))
        }
    }

    /// Validate endpoint liveness and the descriptor token.
    pub fn ping(&self) -> anyhow::Result<SessionSnapshot> {
        let value = self.request_with_timeout("ping", Value::Null, LIVENESS_TIMEOUT)?;
        Ok(serde_json::from_value(value).context("invalid session ping response")?)
    }

    fn ping_with_retries(&self) -> anyhow::Result<SessionSnapshot> {
        let mut last_error = None;
        for attempt in 0..LIVENESS_RETRY_COUNT {
            match self.ping() {
                Ok(snapshot) => return Ok(snapshot),
                Err(error) => {
                    last_error = Some(error);
                    if attempt + 1 < LIVENESS_RETRY_COUNT {
                        thread::sleep(LIVENESS_RETRY_DELAY);
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| anyhow!("session liveness validation failed")))
    }
}

fn bind_control_listener() -> anyhow::Result<(TcpListener, String)> {
    let mut last_error = None;
    for _ in 0..CONTROL_RETRY_COUNT {
        match TcpListener::bind(("127.0.0.1", 0)) {
            Ok(listener) => {
                listener.set_nonblocking(true)?;
                let endpoint = listener.local_addr()?.to_string();
                return Ok((listener, endpoint));
            }
            Err(error) => {
                last_error = Some(error);
                thread::sleep(CONTROL_RETRY_DELAY);
            }
        }
    }
    Err(anyhow!(
        "failed to bind aimer MCP endpoint: {}",
        last_error.map_or_else(|| "unknown error".to_string(), |e| e.to_string())
    ))
}

fn serve(listener: TcpListener, shared: Arc<SharedSession>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                let request_shared = Arc::clone(&shared);
                thread::spawn(move || serve_connection(stream, request_shared));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                tracing::debug!(error = %error, "aimer MCP endpoint stopped accepting connections");
                break;
            }
        }
    }
}

fn serve_connection(mut stream: TcpStream, shared: Arc<SharedSession>) {
    let _ = stream.set_read_timeout(Some(REQUEST_TIMEOUT));
    let reader_stream = match stream.try_clone() {
        Ok(stream) => stream,
        Err(_) => return,
    };
    let mut reader = BufReader::new(reader_stream);
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let response = match serde_json::from_str::<WireRequest>(&line) {
            Ok(request) => handle_request(request, &shared),
            Err(error) => WireResponse {
                request_id: String::new(),
                ok: false,
                result: None,
                error: Some(format!("invalid control request: {error}")),
            },
        };
        if serde_json::to_writer(&mut stream, &response).is_err()
            || stream.write_all(b"\n").is_err()
            || stream.flush().is_err()
        {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}

fn handle_request(request: WireRequest, shared: &Arc<SharedSession>) -> WireResponse {
    if request.token != shared.token {
        return WireResponse {
            request_id: request.request_id,
            ok: false,
            result: None,
            error: Some("invalid session token".to_string()),
        };
    }
    let handle = SessionHandle {
        shared: Arc::clone(shared),
        run_id: None,
    };
    match handle.request_status(&request.operation, request.payload) {
        Ok(result) => WireResponse {
            request_id: request.request_id,
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => WireResponse {
            request_id: request.request_id,
            ok: false,
            result: None,
            error: Some(error),
        },
    }
}

fn snapshot_locked(shared: &SharedSession, state: &SessionState) -> SessionSnapshot {
    SessionSnapshot {
        session_id: shared.session_id,
        run_id: state.run_id,
        project_root: shared.configuration.project_root.clone(),
        target: shared.configuration.target.clone(),
        device: shared.configuration.device.clone(),
        execution_policy: shared.configuration.execution_policy.clone(),
        parent_pid: shared.configuration.parent_pid,
        status: state.status,
        last_error: state.last_error.clone(),
        app_process: state.app_process.clone(),
        app_url: state.app_url.clone(),
    }
}

fn logs_locked(state: &SessionState, request: GetLogsRequest) -> LogsPage {
    let limit = request.limit.clamp(1, 2_000);
    let stream = request.stream.unwrap_or(LogStream::All);
    let mut entries = Vec::with_capacity(limit);
    for entry in state.logs.iter().filter(|entry| {
        request
            .cursor
            .map_or(true, |cursor| entry.seq > cursor)
            && request
                .run_id
                .map_or(true, |run_id| entry.run_id == run_id)
            && stream.matches(entry.stream)
    }) {
        let mut entry = entry.clone();
        if !request.include_source_locations {
            entry.location = None;
        }
        entries.push(entry);
        if entries.len() == limit {
            break;
        }
    }
    let next_cursor = entries.last().map(|entry| entry.seq);
    LogsPage {
        entries,
        next_cursor,
        oldest_seq: state.logs.front().map(|entry| entry.seq),
        newest_seq: state.logs.back().map(|entry| entry.seq),
    }
}

fn sessions_directory() -> anyhow::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(SESSION_DIRECTORY))
        .ok_or_else(|| anyhow!("could not determine the user home directory"))
}

fn normalize_project_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|current| current.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    })
}

fn read_descriptor(path: &Path) -> anyhow::Result<SessionDescriptor> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("reading session descriptor {}", path.display()))?;
    Ok(serde_json::from_str::<StoredSessionDescriptor>(&contents)?.into_descriptor())
}

fn write_descriptor(path: &Path, descriptor: &StoredSessionDescriptor) -> anyhow::Result<()> {
    let temporary = path.with_extension("json.tmp");
    let contents = serde_json::to_vec_pretty(descriptor)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&contents)?;
    file.write_all(b"\n")?;
    file.sync_data().ok();
    drop(file);
    fs::rename(&temporary, path)?;
    set_descriptor_permissions(path);
    Ok(())
}

#[cfg(unix)]
fn set_descriptor_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn set_descriptor_permissions(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration(project_root: &Path) -> RunConfiguration {
        RunConfiguration {
            project_root: project_root.to_path_buf(),
            target: "macos".to_string(),
            device: Some("local".to_string()),
            execution_policy: "debug/native-aot/disabled".to_string(),
            parent_pid: 42,
        }
    }

    #[test]
    fn host_app_identity_is_visual_and_serializes_stable_names() {
        let identity = ProcessIdentity::host_app(
            19_999,
            Some(PathBuf::from("/Applications/Jaime.app/Contents/MacOS/Jaime")),
            None,
        );
        assert!(identity.is_host_visual_process());
        let value = serde_json::to_value(identity).unwrap();
        assert_eq!(value["pid"], 19_999);
        assert_eq!(value["scope"], "host");
        assert_eq!(value["role"], "app");
        assert_eq!(value["visual_target"], "host_process");
    }

    #[test]
    fn device_and_web_identities_are_not_host_visual_processes() {
        assert!(!ProcessIdentity::device_app(Some(123), Some("com.example.app".into()))
            .is_host_visual_process());
        assert!(!ProcessIdentity::web_runner(Some(123), None).is_host_visual_process());
        assert_eq!(
            ProcessIdentity::device_app(Some(123), None).visual_target,
            VisualTarget::DeviceApp
        );
    }

    #[test]
    fn logs_paginate_by_monotonic_cursor_and_filter_by_run() {
        let temp = tempfile::tempdir().unwrap();
        let runtime =
            SessionRuntime::start_in(temp.path().join("sessions"), configuration(temp.path()))
                .unwrap();
        let handle = runtime.handle();
        handle.push_log(LogStream::Build, "info", "compile one", None);
        handle.push_log(
            LogStream::App,
            "error",
            "panic",
            Some("src/main.rs:4".into()),
        );
        handle.set_status(SessionStatus::Running, None);
        let first = handle.get_logs(GetLogsRequest {
            limit: 1,
            ..GetLogsRequest::default()
        });
        assert_eq!(first.entries.len(), 1);
        let second = handle.get_logs(GetLogsRequest {
            cursor: first.next_cursor,
            stream: Some(LogStream::App),
            ..GetLogsRequest::default()
        });
        assert_eq!(second.entries[0].message, "panic");
        assert_eq!(second.entries[0].location.as_deref(), Some("src/main.rs:4"));
    }

    #[test]
    fn restart_clears_pid_increments_run_and_preserves_logs() {
        let temp = tempfile::tempdir().unwrap();
        let runtime =
            SessionRuntime::start_in(temp.path().join("sessions"), configuration(temp.path()))
                .unwrap();
        let handle = runtime.handle();
        handle.set_process(Some(ProcessIdentity::host_app(111, None, None)), None);
        handle.push_log(LogStream::App, "info", "before", None);
        let (reply, receiver) = mpsc::channel();
        handle.accept_restart(reply).unwrap();
        let snapshot = handle.snapshot();
        assert_eq!(snapshot.run_id, 2);
        assert_eq!(snapshot.status, SessionStatus::Restarting);
        assert_eq!(snapshot.app_process, None);
        assert_eq!(handle.get_logs(GetLogsRequest::default()).entries.len(), 1);
        handle.set_status(SessionStatus::Running, None);
        let result = receiver.recv().unwrap().unwrap();
        assert_eq!(result.run_id, 2);
        assert_eq!(result.status, SessionStatus::Running);
    }

    #[test]
    fn registry_discovers_multiple_live_sessions_without_guessing() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let registry_dir = temp.path().join("sessions");
        let _first = SessionRuntime::start_in(registry_dir.clone(), configuration(&project)).unwrap();
        let _second = SessionRuntime::start_in(registry_dir.clone(), configuration(&project)).unwrap();
        let live = SessionRegistry::new(registry_dir).live(Some(&project)).unwrap();
        assert_eq!(live.len(), 2);
    }

    #[test]
    fn attach_validates_the_token_and_returns_the_current_app_identity() {
        let temp = tempfile::tempdir().unwrap();
        let registry_dir = temp.path().join("sessions");
        let runtime = SessionRuntime::start_in(
            registry_dir.clone(),
            configuration(temp.path()),
        )
        .unwrap();
        let handle = runtime.handle();
        handle.set_process(
            Some(ProcessIdentity::host_app(
                19_999,
                Some(PathBuf::from("/tmp/Jaime")),
                None,
            )),
            None,
        );
        handle.set_status(SessionStatus::Running, None);

        let client = SessionRegistry::new(registry_dir)
            .attach(handle.session_id())
            .unwrap();
        let snapshot = client.ping().unwrap();
        assert_eq!(snapshot.app_process.as_ref().and_then(|process| process.pid), Some(19_999));

        let mut bad_descriptor = client.descriptor().clone();
        bad_descriptor.token = "wrong-token".to_string();
        let bad_client = ControlClient::new(bad_descriptor);
        assert!(bad_client.ping().is_err());
    }

    #[test]
    fn concurrent_readers_work_while_a_second_mutation_is_busy() {
        let temp = tempfile::tempdir().unwrap();
        let registry_dir = temp.path().join("sessions");
        let runtime = SessionRuntime::start_in(registry_dir.clone(), configuration(temp.path()))
            .unwrap();
        let handle = runtime.handle();
        let client = SessionRegistry::new(registry_dir)
            .attach(handle.session_id())
            .unwrap();

        let first_client = client.clone();
        let first = thread::spawn(move || first_client.request("restart", Value::Null));
        let mut restart_reply = None;
        for _ in 0..100 {
            if let Some(SessionCommand::Restart { reply }) = handle.take_command() {
                restart_reply = Some(reply);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let reply = restart_reply.expect("restart command should reach the run loop");
        handle.accept_restart(reply).unwrap();

        let status = client.ping().unwrap();
        assert_eq!(status.run_id, 2);
        let second = client.request("clear_logs", Value::Null).unwrap_err();
        assert_eq!(second.to_string(), "busy");

        handle.set_status(SessionStatus::Running, None);
        assert_eq!(first.join().unwrap().unwrap()["run_id"], 2);
    }
}
