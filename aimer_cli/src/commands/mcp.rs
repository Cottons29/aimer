use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::{ServiceExt, schemars, tool, tool_router, transport::stdio};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::session::{ControlClient, SessionDescriptor, SessionRegistry};

#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
struct LogsArguments {
    /// One of all, build, app, or system.
    #[serde(default)]
    stream: Option<String>,
    /// Return entries strictly after this sequence number.
    #[serde(default)]
    cursor: Option<u64>,
    /// Restrict results to one run ID.
    #[serde(default)]
    run_id: Option<u64>,
    /// Maximum number of entries to return.
    #[serde(default)]
    limit: Option<usize>,
    /// Include source file and line information when available.
    #[serde(default)]
    include_source_locations: Option<bool>,
}

#[derive(Clone)]
struct AimerMcpServer {
    client: ControlClient,
}

#[tool_router(server_handler)]
impl AimerMcpServer {
    #[tool(
        name = "aimer_status",
        description = "Return the current Aimer run status and app process identity."
    )]
    fn status(&self) -> String {
        encode(self.client.request("status", Value::Null))
    }

    #[tool(
        name = "aimer_restart_project",
        description = "Restart the attached Aimer project and wait for the new app run to start."
    )]
    fn restart_project(&self) -> String {
        encode(self.client.request("restart", Value::Null))
    }

    #[tool(
        name = "aimer_stop_project",
        description = "Stop the attached Aimer project."
    )]
    fn stop_project(&self) -> String {
        encode(self.client.request("stop", Value::Null))
    }

    #[tool(
        name = "aimer_get_logs",
        description = "Read bounded Aimer build, app, or system logs with cursor pagination."
    )]
    fn get_logs(&self, Parameters(arguments): Parameters<LogsArguments>) -> String {
        encode(self.client.request("logs", logs_payload(arguments)))
    }

    #[tool(
        name = "aimer_clear_logs",
        description = "Clear retained Aimer logs and return the sequence boundary that was cleared."
    )]
    fn clear_logs(&self, Parameters(arguments): Parameters<LogsArguments>) -> String {
        encode(self.client.request("clear_logs", logs_payload(arguments)))
    }
}

fn encode(result: anyhow::Result<Value>) -> String {
    match result {
        Ok(value) => serde_json::to_string(&value)
            .unwrap_or_else(|error| json!({ "error": error.to_string() }).to_string()),
        Err(error) => json!({ "error": error.to_string() }).to_string(),
    }
}

fn logs_payload(arguments: LogsArguments) -> Value {
    let mut payload = Map::new();
    if let Some(stream) = arguments.stream.filter(|stream| stream != "all") {
        payload.insert("stream".to_string(), Value::String(stream));
    }
    if let Some(cursor) = arguments.cursor {
        payload.insert("cursor".to_string(), json!(cursor));
    }
    if let Some(run_id) = arguments.run_id {
        payload.insert("run_id".to_string(), json!(run_id));
    }
    if let Some(limit) = arguments.limit {
        payload.insert("limit".to_string(), json!(limit));
    }
    if let Some(include) = arguments.include_source_locations {
        payload.insert("include_source_locations".to_string(), json!(include));
    }
    Value::Object(payload)
}

/// Run the MCP stdio adapter for a selected Aimer session.
pub fn execute(project: Option<PathBuf>, attach: Option<String>) -> anyhow::Result<()> {
    if project.is_some() && attach.is_some() {
        anyhow::bail!("--project and --attach are mutually exclusive");
    }
    let client = select_client(project, attach)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("starting MCP stdio runtime")?;
    runtime.block_on(async move {
        let server = AimerMcpServer { client };
        let service = server.serve(stdio()).await?;
        service.waiting().await?;
        Ok::<(), anyhow::Error>(())
    })
}

/// List live session descriptors for the sessions command.
pub fn list_sessions(project: Option<PathBuf>, json_output: bool) -> anyhow::Result<()> {
    let project = project.map(resolve_project_path).transpose()?;
    let registry = SessionRegistry::default_location()?;
    let sessions = registry.live(project.as_deref())?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&sessions)?);
        return Ok(());
    }
    if sessions.is_empty() {
        println!("No live Aimer run sessions.");
        return Ok(());
    }
    for session in sessions {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            session.session_id,
            session.status_name(),
            session.target,
            session
                .app_process
                .as_ref()
                .and_then(|process| process.pid)
                .map_or_else(|| "-".to_string(), |pid| pid.to_string()),
            session.project_root.display(),
        );
    }
    Ok(())
}

fn select_client(project: Option<PathBuf>, attach: Option<String>) -> anyhow::Result<ControlClient> {
    let registry = SessionRegistry::default_location()?;
    if let Some(attach) = attach {
        return registry.attach(parse_session_id(&attach)?);
    }
    let project = match project {
        Some(project) => Some(resolve_project_path(project)?),
        None => Some(discover_project_root(&std::env::current_dir()?)?),
    };
    let sessions = registry.live(project.as_deref())?;
    match sessions.as_slice() {
        [] => Err(anyhow!(
            "no live Aimer session matches {}",
            project
                .as_deref()
                .map_or_else(|| "the current directory".to_string(), |path| path.display().to_string())
        )),
        [session] => registry.attach(session.session_id),
        many => {
            let ids = many
                .iter()
                .map(|session| session.session_id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            Err(anyhow!(
                "multiple live Aimer sessions match; use --attach with one of: {ids}"
            ))
        }
    }
}

fn resolve_project_path(path: PathBuf) -> anyhow::Result<PathBuf> {
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    path.canonicalize()
        .with_context(|| format!("project path '{}' does not exist", path.display()))
}

/// Discover the canonical Aimer project root by walking upward for
/// `Aimer.toml`.
pub(crate) fn discover_project_root(start: &Path) -> anyhow::Result<PathBuf> {
    crate::commands::mcp_install::discover_project_root(start)
}

impl SessionDescriptor {
    fn status_name(&self) -> &'static str {
        match self.status {
            crate::session::SessionStatus::Starting => "starting",
            crate::session::SessionStatus::Locking => "locking",
            crate::session::SessionStatus::Fetching => "fetching",
            crate::session::SessionStatus::Compiling => "compiling",
            crate::session::SessionStatus::Building => "building",
            crate::session::SessionStatus::Launching => "launching",
            crate::session::SessionStatus::Running => "running",
            crate::session::SessionStatus::Idling => "idling",
            crate::session::SessionStatus::Restarting => "restarting",
            crate::session::SessionStatus::Stopping => "stopping",
            crate::session::SessionStatus::Stopped => "stopped",
            crate::session::SessionStatus::Error => "error",
        }
    }
}

/// Parse an exact session ID for the CLI.
pub fn parse_session_id(value: &str) -> anyhow::Result<Uuid> {
    Uuid::parse_str(value).with_context(|| format!("invalid session ID '{value}'"))
}

/// Attach to an exact session. This helper is kept separate so tests and
/// future non-MCP adapters can use the same identity validation.
pub fn attach_exact(value: &str) -> anyhow::Result<ControlClient> {
    let registry = SessionRegistry::default_location()?;
    registry.attach(parse_session_id(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_payload_omits_unspecified_fields() {
        let payload = logs_payload(LogsArguments {
            stream: Some("app".to_string()),
            ..LogsArguments::default()
        });
        assert_eq!(payload["stream"], "app");
        assert!(payload.get("cursor").is_none());
    }

    #[test]
    fn log_payload_treats_all_as_the_default_stream() {
        let payload = logs_payload(LogsArguments {
            stream: Some("all".to_string()),
            limit: Some(5),
            ..LogsArguments::default()
        });

        assert!(payload.get("stream").is_none());
        assert_eq!(payload["limit"], 5);
    }

    #[test]
    fn global_mcp_discovery_uses_the_nearest_aimer_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let nested = project.join("src/deep");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(project.join("Aimer.toml"), "[package]\nname = \"demo\"\n").unwrap();

        assert_eq!(discover_project_root(&nested).unwrap(), project.canonicalize().unwrap());
    }

    #[test]
    fn global_mcp_discovery_rejects_directories_without_aimer_manifest() {
        let temp = tempfile::tempdir().unwrap();

        let error = discover_project_root(temp.path()).unwrap_err().to_string();

        assert!(error.contains("no Aimer.toml found"));
    }
}
