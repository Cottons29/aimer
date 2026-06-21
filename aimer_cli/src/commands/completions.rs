use crate::Cli;
use anyhow::{Context, anyhow};
use clap::CommandFactory;
use clap_complete::{Shell, generate};
use std::io;
use std::path::PathBuf;

/// Generate a shell completion script for the requested shell.
///
/// When `install` is false the script is written to stdout (so it can be
/// redirected or `eval`'d). When `install` is true the script is written to
/// the shell's conventional per-user completion directory and an activation
/// hint is printed.
pub fn execute(shell: Shell, install: bool) -> anyhow::Result<()> {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();

    if !install {
        generate(shell, &mut cmd, bin_name, &mut io::stdout());
        return Ok(());
    }

    let target = install_target(shell, &bin_name)?;
    std::fs::create_dir_all(&target.dir)
        .with_context(|| format!("creating completion directory {}", target.dir.display()))?;

    let path = target.dir.join(&target.file_name);
    let mut file = std::fs::File::create(&path)
        .with_context(|| format!("writing completion script to {}", path.display()))?;
    generate(shell, &mut cmd, bin_name, &mut file);

    println!("Installed {shell} completions to {}", path.display());
    if let Some(hint) = target.hint {
        println!("{hint}");
    }
    Ok(())
}

/// Where a completion script for `shell` should be installed.
struct InstallTarget {
    dir: PathBuf,
    file_name: String,
    /// Optional one-time activation instructions to print after installing.
    hint: Option<String>,
}

/// `$HOME` as a `PathBuf`, or a contextual error if it is not set.
fn home_dir() -> anyhow::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("cannot determine completion directory: $HOME is not set"))
}

/// Resolve `$XDG_CONFIG_HOME`, falling back to `$HOME/.config`.
fn config_dir() -> anyhow::Result<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let p = PathBuf::from(xdg);
        if p.is_absolute() {
            return Ok(p);
        }
    }
    Ok(home_dir()?.join(".config"))
}

/// Resolve `$XDG_DATA_HOME`, falling back to `$HOME/.local/share`.
fn data_dir() -> anyhow::Result<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        let p = PathBuf::from(xdg);
        if p.is_absolute() {
            return Ok(p);
        }
    }
    Ok(home_dir()?.join(".local").join("share"))
}

/// Compute the install location and activation hint for the given shell.
fn install_target(shell: Shell, bin: &str) -> anyhow::Result<InstallTarget> {
    match shell {
        Shell::Fish => Ok(InstallTarget {
            dir: config_dir()?.join("fish").join("completions"),
            file_name: format!("{bin}.fish"),
            // fish autoloads from this directory; just start a new shell.
            hint: Some("Restart your shell (or run `exec fish`) to load completions.".into()),
        }),
        Shell::Zsh => Ok(InstallTarget {
            dir: home_dir()?.join(".zsh").join("completions"),
            file_name: format!("_{bin}"),
            hint: Some(
                "Add this to ~/.zshrc (once), then restart your shell:\n  \
                 fpath=(~/.zsh/completions $fpath)\n  \
                 autoload -Uz compinit && compinit"
                    .into(),
            ),
        }),
        Shell::Bash => Ok(InstallTarget {
            dir: data_dir()?.join("bash-completion").join("completions"),
            file_name: bin.to_string(),
            hint: Some(
                "Requires the `bash-completion` package. Restart your shell to load completions."
                    .into(),
            ),
        }),
        other => Err(anyhow!(
            "--install is not supported for {other}; pipe the output to the right location, e.g. \
             `aimer completions {other} > <path>`"
        )),
    }
}
