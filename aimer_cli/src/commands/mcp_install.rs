use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map as JsonMap, Value as JsonValue, json};
use toml::{Table as TomlTable, Value as TomlValue};
use uuid::Uuid;

/// Selects whether an agent registration is user-wide or project-specific.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallScope {
    /// Register `aimer mcp` without binding it to a project. The running MCP
    /// adapter discovers the nearest `Aimer.toml` from its working directory.
    Global,
    /// Register the adapter with an explicit project root (or a descendant of
    /// one containing `Aimer.toml`).
    Project(PathBuf),
}

/// Describes one agent configuration updated by an installation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallReport {
    /// Canonical agent name.
    pub agent: String,
    /// Configuration file updated for this agent.
    pub path: PathBuf,
    /// Whether the file content changed.
    pub changed: bool,
}

/// Install Aimer's stdio MCP entry in the requested agent configurations.
pub fn install(agents: &[String], scope: InstallScope) -> Result<Vec<InstallReport>> {
    let home = dirs::home_dir().context("cannot determine the user home directory")?;
    let executable = std::env::current_exe()
        .context("cannot determine the aimer executable")?
        .canonicalize()
        .context("cannot resolve the aimer executable")?;
    install_at(agents, scope, &home, &executable, true)
}

fn install_at(
    agents: &[String],
    scope: InstallScope,
    home: &Path,
    executable: &Path,
    honor_codex_home: bool,
) -> Result<Vec<InstallReport>> {
    if agents.is_empty() {
        bail!("at least one MCP agent is required after --install");
    }

    let agents = agents
        .iter()
        .map(|agent| Agent::parse(agent))
        .collect::<Result<Vec<_>>>()?;
    let project = match scope {
        InstallScope::Global => None,
        InstallScope::Project(path) => Some(discover_project_root(&path)?),
    };
    let args = command_args(project.as_deref());

    let mut reports = Vec::with_capacity(agents.len());
    for agent in agents {
        let path = agent.config_path(home, honor_codex_home);
        let changed = match agent {
            Agent::Codex => install_toml(&path, executable, &args)?,
            Agent::Claude | Agent::Cursor => install_json(&path, executable, &args)?,
        };
        reports.push(InstallReport {
            agent: agent.name().to_string(),
            path,
            changed,
        });
    }
    Ok(reports)
}

fn command_args(project: Option<&Path>) -> Vec<String> {
    let mut args = vec!["mcp".to_string()];
    if let Some(project) = project {
        args.push("--project".to_string());
        args.push(project.display().to_string());
    }
    args
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Agent {
    Codex,
    Claude,
    Cursor,
}

impl Agent {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            "claude" | "claude-code" => Ok(Self::Claude),
            "cursor" => Ok(Self::Cursor),
            other => Err(anyhow!(
                "unsupported MCP agent '{other}'; supported agents: codex, claude, cursor"
            )),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::Cursor => "cursor",
        }
    }

    fn config_path(self, home: &Path, honor_codex_home: bool) -> PathBuf {
        match self {
            Self::Codex => {
                let root = if honor_codex_home {
                    std::env::var_os("CODEX_HOME")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| home.join(".codex"))
                } else {
                    home.join(".codex")
                };
                root.join("config.toml")
            }
            Self::Claude => home.join(".claude.json"),
            Self::Cursor => home.join(".cursor/mcp.json"),
        }
    }
}

pub(crate) fn discover_project_root(start: &Path) -> Result<PathBuf> {
    let canonical = start
        .canonicalize()
        .with_context(|| format!("resolving project path '{}'", start.display()))?;
    let directory = if canonical.is_dir() {
        canonical.as_path()
    } else {
        canonical
            .parent()
            .ok_or_else(|| anyhow!("project path '{}' has no parent", start.display()))?
    };

    for candidate in directory.ancestors() {
        if candidate.join(crate::config::MANIFEST_FILE).is_file() {
            return Ok(candidate.to_path_buf());
        }
    }

    bail!(
        "no {} found from '{}'; run this command from an Aimer project or pass --project",
        crate::config::MANIFEST_FILE,
        start.display()
    )
}

fn install_toml(path: &Path, executable: &Path, args: &[String]) -> Result<bool> {
    let mut document = if path.exists() {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("reading MCP configuration '{}'", path.display()))?;
        toml::from_str::<TomlValue>(&contents)
            .with_context(|| format!("parsing MCP configuration '{}'", path.display()))?
    } else {
        TomlValue::Table(TomlTable::new())
    };

    let root = document
        .as_table_mut()
        .ok_or_else(|| anyhow!("MCP configuration '{}' must contain a TOML table", path.display()))?;
    let servers = root
        .entry("mcp_servers")
        .or_insert_with(|| TomlValue::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow!("'mcp_servers' in '{}' must be a TOML table", path.display()))?;
    let server = servers
        .entry("aimer")
        .or_insert_with(|| TomlValue::Table(TomlTable::new()))
        .as_table_mut()
        .ok_or_else(|| anyhow!("'mcp_servers.aimer' in '{}' must be a TOML table", path.display()))?;

    server.insert(
        "command".to_string(),
        TomlValue::String(executable.display().to_string()),
    );
    server.insert(
        "args".to_string(),
        TomlValue::Array(args.iter().cloned().map(TomlValue::String).collect()),
    );

    let contents = toml::to_string_pretty(&document)
        .with_context(|| format!("serializing MCP configuration '{}'", path.display()))?;
    write_if_changed(path, contents.as_bytes())
}

fn install_json(path: &Path, executable: &Path, args: &[String]) -> Result<bool> {
    let mut document = if path.exists() {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("reading MCP configuration '{}'", path.display()))?;
        serde_json::from_str::<JsonValue>(&contents)
            .with_context(|| format!("parsing MCP configuration '{}'", path.display()))?
    } else {
        JsonValue::Object(JsonMap::new())
    };

    let root = document
        .as_object_mut()
        .ok_or_else(|| anyhow!("MCP configuration '{}' must contain a JSON object", path.display()))?;
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| JsonValue::Object(JsonMap::new()))
        .as_object_mut()
        .ok_or_else(|| anyhow!("'mcpServers' in '{}' must be a JSON object", path.display()))?;
    let server = servers
        .entry("aimer")
        .or_insert_with(|| JsonValue::Object(JsonMap::new()))
        .as_object_mut()
        .ok_or_else(|| anyhow!("'mcpServers.aimer' in '{}' must be a JSON object", path.display()))?;

    server.insert(
        "command".to_string(),
        JsonValue::String(executable.display().to_string()),
    );
    server.insert("args".to_string(), json!(args));

    let contents = serde_json::to_vec_pretty(&document)
        .with_context(|| format!("serializing MCP configuration '{}'", path.display()))?;
    write_if_changed(path, &contents)
}

fn write_if_changed(path: &Path, contents: &[u8]) -> Result<bool> {
    match fs::read(path) {
        Ok(existing) if existing == contents => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("reading MCP configuration '{}'", path.display()));
        }
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating MCP configuration directory '{}'", parent.display()))?;
    }

    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|name| name.to_str()).unwrap_or("mcp"),
        Uuid::new_v4()
    ));
    fs::write(&temporary, contents)
        .with_context(|| format!("writing temporary MCP configuration '{}'", temporary.display()))?;

    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(&temporary, metadata.permissions()).with_context(|| {
            format!("preserving permissions for MCP configuration '{}'", path.display())
        })?;
    } else {
        #[cfg(unix)]
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).with_context(|| {
            format!("securing new MCP configuration '{}'", path.display())
        })?;
    }

    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error)
            .with_context(|| format!("installing MCP configuration '{}'", path.display()));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn global_codex_install_uses_runtime_project_discovery() {
        let home = tempfile::tempdir().unwrap();
        let executable = Path::new("/opt/aimer/bin/aimer");

        let reports = install_in(
            &["codex".to_string()],
            InstallScope::Global,
            home.path(),
            executable,
        )
        .unwrap();

        assert_eq!(reports.len(), 1);
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(home.path().join(".codex/config.toml")).unwrap())
                .unwrap();
        assert_eq!(
            config["mcp_servers"]["aimer"]["command"],
            TomlValue::String(executable.display().to_string())
        );
        assert_eq!(
            config["mcp_servers"]["aimer"]["args"],
            TomlValue::Array(vec![TomlValue::String("mcp".to_string())])
        );
    }

    #[test]
    fn project_install_persists_canonical_project_path() {
        let home = tempfile::tempdir().unwrap();
        let project = home.path().join("projects/demo");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("Aimer.toml"), "[package]\nname = \"demo\"\n").unwrap();

        install_in(
            &["codex".to_string()],
            InstallScope::Project(project.clone()),
            home.path(),
            Path::new("/opt/aimer/bin/aimer"),
        )
        .unwrap();

        let config: toml::Value =
            toml::from_str(&fs::read_to_string(home.path().join(".codex/config.toml")).unwrap())
                .unwrap();
        assert_eq!(
            config["mcp_servers"]["aimer"]["args"],
            TomlValue::Array(vec![
                TomlValue::String("mcp".to_string()),
                TomlValue::String("--project".to_string()),
                TomlValue::String(project.canonicalize().unwrap().display().to_string()),
            ])
        );
    }

    #[test]
    fn install_preserves_other_servers_and_is_idempotent() {
        let home = tempfile::tempdir().unwrap();
        let config_dir = home.path().join(".codex");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("config.toml"),
            "[mcp_servers.keep]\ncommand = \"keep\"\nargs = [\"--keep\"]\n\n[mcp_servers.aimer]\nstartup_timeout_sec = 120\n",
        )
        .unwrap();

        let first = install_in(
            &["codex".to_string()],
            InstallScope::Global,
            home.path(),
            Path::new("/opt/aimer/bin/aimer"),
        )
        .unwrap();
        let second = install_in(
            &["codex".to_string()],
            InstallScope::Global,
            home.path(),
            Path::new("/opt/aimer/bin/aimer"),
        )
        .unwrap();

        assert!(first[0].changed);
        assert!(!second[0].changed);
        let config: toml::Value =
            toml::from_str(&fs::read_to_string(config_dir.join("config.toml")).unwrap()).unwrap();
        assert_eq!(
            config["mcp_servers"]["keep"]["command"],
            TomlValue::String("keep".to_string())
        );
        assert_eq!(
            config["mcp_servers"]["aimer"]["startup_timeout_sec"],
            TomlValue::Integer(120)
        );
    }

    #[test]
    fn json_agents_receive_the_same_global_stdio_entry() {
        let home = tempfile::tempdir().unwrap();
        let reports = install_in(
            &["claude".to_string(), "cursor".to_string()],
            InstallScope::Global,
            home.path(),
            Path::new("/opt/aimer/bin/aimer"),
        )
        .unwrap();

        assert_eq!(reports.len(), 2);
        for path in [
            home.path().join(".claude.json"),
            home.path().join(".cursor/mcp.json"),
        ] {
            let config: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
            assert_eq!(config["mcpServers"]["aimer"]["command"], "/opt/aimer/bin/aimer");
            assert_eq!(config["mcpServers"]["aimer"]["args"], serde_json::json!(["mcp"]));
        }
    }

    #[test]
    fn unsupported_agent_is_rejected_before_writing() {
        let home = tempfile::tempdir().unwrap();
        let error = install_in(
            &["unknown-agent".to_string()],
            InstallScope::Global,
            home.path(),
            Path::new("/opt/aimer/bin/aimer"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("unsupported MCP agent 'unknown-agent'"));
        assert!(!home.path().join(".codex/config.toml").exists());
    }

    fn install_in(
        agents: &[String],
        scope: InstallScope,
        home: &Path,
        executable: &Path,
    ) -> Result<Vec<InstallReport>> {
        install_at(agents, scope, home, executable, false)
    }
}
