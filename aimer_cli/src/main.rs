use std::env::{set_current_dir, var_os};
use std::ffi::OsStr;
use std::path::Path;

use anyhow::Context;
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

use crate::commands::version::VersionCommand;
use crate::config::{ApplicationRuntime, BuildProfile, ExecutionPolicy, ReloadPolicy};
use crate::targets::{MigrateTarget, Targets};

pub mod commands;
pub mod config;
pub mod console;
pub mod errors;
pub mod session;
pub mod targets;
pub mod tui;

#[derive(Parser)]
#[command(name = "aimer")]
#[command(about = "Aimer Framework CLI", long_about = None)]
struct Cli {
    /// Select the unstable toolchain used by development-only Aimer features.
    #[arg(value_parser = parse_toolchain_selector)]
    toolchain: Option<ToolchainSelector>,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Show the version of the CLI
    #[arg(short = 'v', long = "version")]
    version: bool,

    /// Enable verbose (debug) logging
    #[arg(long, global = true)]
    verbose: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ToolchainSelector {
    Nightly,
}

fn parse_toolchain_selector(value: &str) -> Result<ToolchainSelector, String> {
    match value {
        "+nightly" => Ok(ToolchainSelector::Nightly),
        _ => Err(format!(
            "unsupported toolchain selector '{value}'; only '+nightly' is available"
        )),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum UnstableFeature {
    #[value(name = "hot-reload")]
    WasmHotReload,
    #[value(name = "inline-render")]
    InlineRender,
}

impl UnstableFeature {
    #[inline]
    fn cli_name(self) -> &'static str {
        match self {
            Self::WasmHotReload => "hot-reload",
            Self::InlineRender => "inline-render",
        }
    }
}

impl Cli {
    fn validate_unstable_invocation(&self) -> anyhow::Result<()> {
        let unstable = match &self.command {
            Some(Commands::Run { unstable, .. }) => *unstable,
            _ => None,
        };

        match (self.toolchain, unstable) {
            (Some(ToolchainSelector::Nightly), Some(_)) | (None, None) => Ok(()),
            (None, Some(feature)) => anyhow::bail!(
                "'-Z {}' requires the nightly selector; use 'aimer +nightly run -Z {}'",
                feature.cli_name(),
                feature.cli_name(),
            ),
            (Some(ToolchainSelector::Nightly), None) => anyhow::bail!(
                "'+nightly' is reserved for unstable Aimer features; use 'aimer +nightly run -Z hot-reload'"
            ),
        }
    }

    fn execution_policy(&self) -> anyhow::Result<Option<ExecutionPolicy>> {
        self.validate_unstable_invocation()?;

        let Some(Commands::Run {
            release, unstable, ..
        }) = self.command
        else {
            return Ok(None);
        };

        let profile = if release {
            BuildProfile::Release
        } else {
            BuildProfile::Debug
        };
        let (runtime, reload) = match unstable {
            Some(UnstableFeature::WasmHotReload) => {
                (ApplicationRuntime::Wasmi, ReloadPolicy::HotReload)
            }
            Some(UnstableFeature::InlineRender) | None => {
                (ApplicationRuntime::NativeAot, ReloadPolicy::Disabled)
            }
        };

        ExecutionPolicy::new(profile, runtime, reload).map(Some)
    }
}

/// Initialise the tracing subscriber. Honours `RUST_LOG` if set, otherwise
/// defaults to `warn` (or `debug` when `--verbose` is passed).
fn init_logging(verbose: bool) {
    use tracing_subscriber::{EnvFilter, fmt};

    let default_level = if verbose { "debug" } else { "warn" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    let _ = fmt().with_env_filter(filter).with_target(false).try_init();
}

#[cfg(debug_assertions)]
fn apply_project_dir_override(project_dir: Option<&OsStr>) -> anyhow::Result<()> {
    match project_dir {
        Some(dir) => set_current_dir(Path::new(dir))
            .with_context(|| format!("failed to set current dir to '{}'", dir.to_string_lossy())),
        None => {
            tracing::debug!("MY_PROJECT_DIR is not set");
            Ok(())
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new project
    Create {
        /// Name of the project
        project_name: String,
    },

    /// Run the project (interactive picker, or scriptable with
    /// --target/--device)
    Run {
        /// Build/run for this target without showing the picker
        #[arg(short, long, value_enum)]
        target: Option<Targets>,
        /// Run on the device with this id without showing the picker
        #[arg(short, long)]
        device: Option<String>,
        /// Disable the interactive TUI and print logs to stdout/stderr instead.
        /// Useful when running from an IDE or CI where no terminal is
        /// available.
        #[arg(long)]
        no_tui: bool,
        /// Build, package and launch with the release profile
        #[arg(short, long)]
        release: bool,
        /// Enable a nightly-only unstable feature.
        #[arg(short = 'Z', value_enum, value_name = "UNSTABLE-FEATURE")]
        unstable: Option<UnstableFeature>,
        /// Print every Widget IR stage after successful native materialization.
        #[arg(long, requires = "unstable")]
        verbose_widget_ir: bool,
        /// Do not start or register the private MCP control session.
        #[arg(long)]
        no_mcp: bool,
    },

    /// Expose a running Aimer session as an MCP server over stdio.
    Mcp {
        /// Install the Aimer MCP server for one or more supported agents.
        #[arg(
            long,
            value_name = "AGENT",
            num_args = 1..,
            value_delimiter = ',',
            conflicts_with = "attach"
        )]
        install: Option<Vec<String>>,
        /// Install in the user's global agent configuration instead of binding
        /// the entry to the current Aimer project.
        #[arg(long, requires = "install", conflicts_with_all = ["project", "attach"])]
        global: bool,
        /// Project path used for automatic session discovery.
        #[arg(long, conflicts_with = "attach")]
        project: Option<std::path::PathBuf>,
        /// Attach to this exact session ID.
        #[arg(long, conflicts_with = "project")]
        attach: Option<String>,
    },

    /// List live Aimer run sessions.
    Sessions {
        /// Restrict discovery to a project path.
        #[arg(long)]
        project: Option<std::path::PathBuf>,
        /// Emit machine-readable JSON.
        #[arg(long)]
        json: bool,
    },

    /// Build the project for a target without launching it
    Build {
        /// Target to build for (defaults to aimer.toml's default_target)
        #[arg(short, long, value_enum)]
        target: Option<Targets>,
        /// Build in release mode
        #[arg(short, long)]
        release: bool,
    },

    /// Assemble the distributable platform bundle (.app, .apk, .ipa, ...)
    Assemble {
        /// Target platform to bundle for (macos, android, ios, web, ...)
        #[arg(value_enum)]
        platform: Targets,
        /// Assemble in release mode
        #[arg(short, long)]
        release: bool,
    },

    /// Check that the required toolchains are installed
    Doctor,

    /// Remove build artifacts (builds/ and target/)
    Clean,

    /// Generate a shell completion script
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, ...)
        shell: clap_complete::Shell,
        /// Install the script into the shell's completion directory instead of
        /// printing it to stdout
        #[arg(long)]
        install: bool,
    },

    /// Migrate platform build scaffolds to the latest version
    Migrate {
        /// Target to migrate (macos, windows, linux, android, ios, web, all)
        #[arg(value_enum)]
        target: MigrateTarget,
    },
}

fn main() -> anyhow::Result<()> {
    // Dynamic, self-updating shell completions. When the shell invokes the
    // binary with `COMPLETE=<shell>` set, this generates completions from the
    // *current* command tree (so newly added subcommands appear automatically)
    // and exits. Must run before anything writes to stdout.
    clap_complete::env::CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();
    let execution_policy = cli.execution_policy()?;
    init_logging(cli.verbose);

    #[cfg(debug_assertions)]
    apply_project_dir_override(var_os("MY_PROJECT_DIR").as_deref())?;

    if cli.version {
        VersionCommand::execute();
        return Ok(());
    }

    match &cli.command {
        Some(Commands::Create { project_name }) => {
            commands::create::execute(project_name)?;
        }
        Some(Commands::Run {
            target,
            device,
            no_tui,
            release: _,
            unstable,
            verbose_widget_ir,
            no_mcp,
        }) => {
            let policy = execution_policy.expect("run commands always resolve an execution policy");
            commands::run::execute(
                target.map(|t| t.to_string()),
                device.clone(),
                *no_tui,
                policy,
                *verbose_widget_ir,
                matches!(unstable, Some(UnstableFeature::InlineRender)),
                *no_mcp,
            )?;
        }
        Some(Commands::Mcp {
            project,
            attach,
            install,
            global,
        }) => {
            if let Some(agents) = install {
                let scope = if *global {
                    commands::mcp_install::InstallScope::Global
                } else {
                    commands::mcp_install::InstallScope::Project(
                        project
                            .clone()
                            .unwrap_or(std::env::current_dir()?),
                    )
                };
                let reports = commands::mcp_install::install(agents, scope)?;
                for report in reports {
                    let state = if report.changed {
                        "installed"
                    } else {
                        "already installed"
                    };
                    println!(
                        "Aimer MCP {state} for {} in {}",
                        report.agent,
                        report.path.display()
                    );
                }
            } else {
                commands::mcp::execute(project.clone(), attach.clone())?;
            }
        }
        Some(Commands::Sessions { project, json }) => {
            commands::mcp::list_sessions(project.clone(), *json)?;
        }
        Some(Commands::Build { target, release }) => {
            commands::build::execute(target.map(|t| t.to_string()), *release)?;
        }
        Some(Commands::Assemble { platform, release }) => {
            commands::assemble::execute(platform.to_string(), *release)?;
        }
        Some(Commands::Doctor) => {
            commands::doctor::execute()?;
        }
        Some(Commands::Clean) => {
            commands::clean::execute()?;
        }
        Some(Commands::Completions { shell, install }) => {
            commands::completions::execute(*shell, *install)?;
        }
        Some(Commands::Migrate { target }) => {
            commands::migrate::execute(target.as_str().to_string())?;
        }
        None => {
            Cli::parse_from(["aimer", "--help"]);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(debug_assertions)]
    use std::env::{current_dir, set_current_dir};
    use clap::Parser;

    use crate::config::{ApplicationRuntime, BuildProfile, ReloadPolicy};

    use super::{Cli, Commands, UnstableFeature};

    #[cfg(debug_assertions)]
    #[test]
    fn runtime_project_dir_override_uses_the_launch_environment() {
        let original = current_dir().unwrap();
        let project = tempfile::tempdir().unwrap();

        super::apply_project_dir_override(Some(project.path().as_os_str())).unwrap();
        assert_eq!(current_dir().unwrap(), project.path().canonicalize().unwrap());

        set_current_dir(original).unwrap();
    }

    #[test]
    fn nightly_wasm_hot_reload_invocation_is_accepted() {
        let cli = Cli::try_parse_from([
            "aimer",
            "+nightly",
            "run",
            "-Z",
            "hot-reload",
        ])
        .unwrap();

        assert!(cli.validate_unstable_invocation().is_ok());
    }

    #[test]
    fn nightly_inline_render_invocation_is_accepted() {
        let cli = Cli::try_parse_from([
            "aimer",
            "+nightly",
            "run",
            "-Z",
            "inline-render",
        ])
        .unwrap();

        assert!(cli.validate_unstable_invocation().is_ok());
        assert!(matches!(
            cli.command,
            Some(Commands::Run {
                unstable: Some(UnstableFeature::InlineRender),
                ..
            })
        ));
    }

    #[test]
    fn verbose_widget_ir_is_available_only_on_an_unstable_run() {
        let cli = Cli::try_parse_from([
            "aimer",
            "+nightly",
            "run",
            "-Z",
            "hot-reload",
            "--verbose-widget-ir",
        ])
        .unwrap();
        assert!(cli.validate_unstable_invocation().is_ok());

        assert!(Cli::try_parse_from(["aimer", "run", "--verbose-widget-ir"]).is_err());
    }

    #[test]
    fn ordinary_run_remains_accepted() {
        let cli = Cli::try_parse_from(["aimer", "run"]).unwrap();

        assert!(cli.validate_unstable_invocation().is_ok());
    }

    #[test]
    fn run_can_disable_automatic_mcp_registration() {
        let cli = Cli::try_parse_from(["aimer", "run", "--no-mcp"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Commands::Run { no_mcp: true, .. })
        ));
    }

    #[test]
    fn mcp_project_and_exact_attach_are_mutually_exclusive() {
        assert!(
            Cli::try_parse_from(["aimer", "mcp", "--project", ".", "--attach", "abc"])
                .is_err()
        );
        assert!(Cli::try_parse_from(["aimer", "mcp", "--attach", "abc"]).is_ok());
    }

    #[test]
    fn mcp_global_install_accepts_multiple_agents() {
        let cli = Cli::try_parse_from([
            "aimer",
            "mcp",
            "--install",
            "codex",
            "claude",
            "--global",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Some(Commands::Mcp {
                install: Some(agents),
                global: true,
                ..
            }) if agents == vec!["codex", "claude"]
        ));
    }

    #[test]
    fn mcp_global_flag_requires_install() {
        assert!(Cli::try_parse_from(["aimer", "mcp", "--global"]).is_err());
    }

    #[test]
    fn mcp_install_rejects_global_project_and_exact_attach_combinations() {
        assert!(
            Cli::try_parse_from(["aimer", "mcp", "--install", "codex", "--global", "--project", "."])
                .is_err()
        );
        assert!(
            Cli::try_parse_from(["aimer", "mcp", "--install", "codex", "--attach", "abc"])
                .is_err()
        );
    }

    #[test]
    fn wasm_hot_reload_requires_the_nightly_selector() {
        let cli = Cli::try_parse_from(["aimer", "run", "-Z", "hot-reload"]).unwrap();

        let error = cli.validate_unstable_invocation().unwrap_err().to_string();
        assert!(error.contains("+nightly"));
        assert!(error.contains("aimer +nightly run -Z hot-reload"));
    }

    #[test]
    fn inline_render_requires_the_nightly_selector() {
        let cli = Cli::try_parse_from(["aimer", "run", "-Z", "inline-render"]).unwrap();

        let error = cli.validate_unstable_invocation().unwrap_err().to_string();
        assert!(error.contains("+nightly"));
        assert!(error.contains("aimer +nightly run -Z inline-render"));
    }

    #[test]
    fn nightly_selector_requires_the_wasm_hot_reload_flag() {
        let cli = Cli::try_parse_from(["aimer", "+nightly", "run"]).unwrap();

        let error = cli.validate_unstable_invocation().unwrap_err().to_string();
        assert!(error.contains("-Z hot-reload"));
    }

    #[test]
    fn unsupported_toolchain_selector_is_rejected() {
        let error = Cli::try_parse_from(["aimer", "+stable", "run"])
            .err()
            .unwrap()
            .to_string();

        assert!(error.contains("+nightly"));
    }

    #[test]
    fn unknown_unstable_feature_is_rejected() {
        let error = Cli::try_parse_from(["aimer", "+nightly", "run", "-Z", "unknown"])
            .err()
            .unwrap()
            .to_string();

        assert!(error.contains("hot-reload"));
    }

    #[test]
    fn nightly_selector_after_run_is_rejected() {
        assert!(
            Cli::try_parse_from([
                "aimer",
                "run",
                "+nightly",
                "-Z",
                "hot-reload",
            ])
            .is_err()
        );
    }

    #[test]
    fn ordinary_debug_run_resolves_to_native_aot_without_reload() {
        let cli = Cli::try_parse_from(["aimer", "run"]).unwrap();
        let policy = cli.execution_policy().unwrap().unwrap();

        assert_eq!(policy.profile(), BuildProfile::Debug);
        assert_eq!(policy.runtime(), ApplicationRuntime::NativeAot);
        assert_eq!(policy.reload(), ReloadPolicy::Disabled);
    }

    #[test]
    fn release_run_resolves_to_native_aot_without_reload() {
        let cli = Cli::try_parse_from(["aimer", "run", "--release"]).unwrap();
        let policy = cli.execution_policy().unwrap().unwrap();

        assert_eq!(policy.profile(), BuildProfile::Release);
        assert_eq!(policy.runtime(), ApplicationRuntime::NativeAot);
        assert_eq!(policy.reload(), ReloadPolicy::Disabled);
    }

    #[test]
    fn nightly_flag_resolves_to_wasmi_hot_reload() {
        let cli = Cli::try_parse_from([
            "aimer",
            "+nightly",
            "run",
            "-Z",
            "hot-reload",
        ])
        .unwrap();
        let policy = cli.execution_policy().unwrap().unwrap();

        assert_eq!(policy.profile(), BuildProfile::Debug);
        assert_eq!(policy.runtime(), ApplicationRuntime::Wasmi);
        assert_eq!(policy.reload(), ReloadPolicy::HotReload);
    }

    #[test]
    fn nightly_inline_render_resolves_to_native_aot_without_reload() {
        let cli = Cli::try_parse_from([
            "aimer",
            "+nightly",
            "run",
            "-Z",
            "inline-render",
        ])
        .unwrap();
        let policy = cli.execution_policy().unwrap().unwrap();

        assert_eq!(policy.profile(), BuildProfile::Debug);
        assert_eq!(policy.runtime(), ApplicationRuntime::NativeAot);
        assert_eq!(policy.reload(), ReloadPolicy::Disabled);
    }

    #[test]
    fn nightly_hot_reload_rejects_release_profile() {
        let cli = Cli::try_parse_from([
            "aimer",
            "+nightly",
            "run",
            "--release",
            "-Z",
            "hot-reload",
        ])
        .unwrap();

        let error = cli.execution_policy().unwrap_err().to_string();
        assert!(error.contains("profile=release"));
        assert!(error.contains("runtime=wasmi"));
        assert!(error.contains("reload=hot-reload"));
    }

    #[test]
    fn former_wasm_hot_reload_spelling_is_rejected() {
        assert!(
            Cli::try_parse_from([
                "aimer",
                "+nightly",
                "run",
                "-Z",
                "wasm-hot-reload",
            ])
            .is_err()
        );
    }
}
