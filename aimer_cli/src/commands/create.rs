pub mod android;
pub mod ios;
pub mod linux;
pub mod macos;
pub mod web;
pub mod window;

use crate::config::AimerManifest;
use crate::errors::AimerError;
use anyhow::Context;
use inquire::{ui::{Color, RenderConfig, Styled}, Confirm, MultiSelect, Text};
use std::fs;
use std::path::{Path, PathBuf};

macro_rules! prompt_abortable {
    ($prompt:expr) => {
        loop {
            match $prompt.prompt() {
                Ok(val) => break val,
                Err(inquire::error::InquireError::OperationInterrupted) => {
                    println!("press 'ctrl + c' again to exit");
                    crossterm::terminal::enable_raw_mode().unwrap();
                    if let Ok(crossterm::event::Event::Key(event)) = crossterm::event::read() {
                        if event.code == crossterm::event::KeyCode::Char('c') && event.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) {
                            crossterm::terminal::disable_raw_mode().unwrap();
                            std::process::exit(1);
                        }
                    }
                    crossterm::terminal::disable_raw_mode().unwrap();
                }
                Err(inquire::error::InquireError::OperationCanceled) => {
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };
}

pub fn execute(project_name: &str) -> anyhow::Result<()> {
    // Reject invalid names up-front, before any prompts or filesystem writes.
    validate_project_name(project_name)?;

    let config = RenderConfig {
        prompt_prefix: Styled::new("◆").with_fg(Color::LightCyan),
        highlighted_option_prefix: Styled::new("│  ❯").with_fg(Color::LightCyan),
        unhighlighted_option_prefix: Styled::new("│   ").with_fg(Color::DarkGrey),
        selected_checkbox: Styled::new("●").with_fg(Color::LightGreen),
        unselected_checkbox: Styled::new("○").with_fg(Color::DarkGrey),
        ..RenderConfig::default()
    };
    inquire::set_global_render_config(config);

    let current_dir = std::env::current_dir().context("failed to read the current directory")?;
    tracing::debug!("current_dir : {}", current_dir.display());
    println!("Creating project '{}'", project_name);

    let description = prompt_abortable!(Text::new("Description:"));
    let version = prompt_abortable!(Text::new("Version:").with_default("0.1.0"));
    let author = prompt_abortable!(Text::new("Author:"));
    let targets = prompt_abortable!(MultiSelect::new("Targets:", vec!["macos", "windows", "linux", "android", "ios", "web"]));

    println!(
        "\nProject config:\n- Name: {}\n- Version: {}\n- Description: {}\n- Author: {}\n- Targets: {:?}",
        project_name, version, description, author, targets
    );

    if !prompt_abortable!(Confirm::new("Is this okay?").with_default(true)) {
        println!("Aborted.");
        return Ok(());
    }

    let dir = PathBuf::from(project_name);
    if dir.exists() {
        println!("Directory '{}' already exists!", project_name);
        return Ok(());
    }

    // Run the scaffold inside a helper so that any failure cleans up the
    // partially created directory instead of leaving it behind.
    if let Err(err) = scaffold(&dir, project_name, &version, &description, &author, &targets) {
        let _ = fs::remove_dir_all(&dir);
        return Err(err).with_context(|| format!("failed to create project '{project_name}'"));
    }

    println!("Project '{}' created successfully.", project_name);
    Ok(())
}

/// Write the full project tree into `dir`. Returns an error (with context) on
/// the first failing filesystem operation so the caller can clean up.
fn scaffold(
    dir: &Path,
    project_name: &str,
    version: &str,
    description: &str,
    author: &str,
    targets: &[&str],
) -> anyhow::Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("creating directory {}", dir.display()))?;
    fs::create_dir_all(dir.join("src")).context("creating src directory")?;
    fs::create_dir_all(dir.join("builds")).context("creating builds directory")?;
    fs::create_dir_all(dir.join("builds/web")).context("creating builds/web directory")?;
    // fs::create_dir_all(dir.join("builds/build_src/src")).context("creating builds/build_src/src directory")?;

    if targets.contains(&"android") {
        android::create(dir);
    }
    if targets.contains(&"ios") {
        ios::create(dir);
    }
    if targets.contains(&"macos") {
        macos::create(dir);
    }
    if targets.contains(&"windows") {
        window::create(dir);
    }
    if targets.contains(&"linux") {
        linux::create(dir);
    }
    if targets.contains(&"web") {
        web::create(dir);
    }

    fs::write(dir.join(".gitignore"), include_str!("../../templates/.gitignore.template"))
        .context("writing .gitignore")?;

    // Cargo.toml
    fs::write(
        dir.join("Cargo.toml"),
        include_str!("../../templates/Cargo.toml.template")
            .replace("${project_name}", project_name)
            .replace("${version}", version),
    )
    .context("writing Cargo.toml")?;

    // src/lib.rs
    fs::write(dir.join("src/lib.rs"), include_str!("../../templates/lib.rs.template"))
        .context("writing src/lib.rs")?;

    fs::write(dir.join("README.md"), format!("# {}\n\n{}", project_name, description))
        .context("writing README.md")?;

    // Persist project metadata so run/build can read it back instead of
    // re-parsing Cargo.toml.
    AimerManifest::new(project_name, version, description, author)
        .write_to(dir)
        .context("writing aimer.toml")?;

    Ok(())
}

/// Validate a project name before any prompts or filesystem writes.
///
/// Rejects empty names, path separators, whitespace, the `.`/`..` specials and
/// characters that are reserved on common filesystems.
pub fn validate_project_name(name: &str) -> Result<(), AimerError> {
    const RESERVED: &[char] = &[':', '*', '?', '"', '<', '>', '|'];
    let reject = |reason: &str| {
        Err(AimerError::InvalidProjectName(name.to_string(), reason.to_string()))
    };

    if name.trim().is_empty() {
        return reject("name must not be empty");
    }
    if name == "." || name == ".." {
        return reject("name must not be '.' or '..'");
    }
    if name.contains(['/', '\\']) {
        return reject("name must not contain path separators");
    }
    if name.chars().any(char::is_whitespace) {
        return reject("name must not contain whitespace");
    }
    if name.contains(RESERVED) {
        return reject("name must not contain reserved characters (: * ? \" < > |)");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_names() {
        for name in ["myapp", "my_app", "my-app", "App123"] {
            assert!(validate_project_name(name).is_ok(), "should accept '{name}'");
        }
    }

    #[test]
    fn rejects_empty_name() {
        assert!(validate_project_name("").is_err());
        assert!(validate_project_name("   ").is_err());
    }

    #[test]
    fn rejects_dot_specials() {
        assert!(validate_project_name(".").is_err());
        assert!(validate_project_name("..").is_err());
    }

    #[test]
    fn rejects_path_separators() {
        assert!(validate_project_name("foo/bar").is_err());
        assert!(validate_project_name("foo\\bar").is_err());
    }

    #[test]
    fn rejects_whitespace() {
        assert!(validate_project_name("my app").is_err());
        assert!(validate_project_name("my\tapp").is_err());
    }

    #[test]
    fn rejects_reserved_characters() {
        for name in ["a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b"] {
            assert!(validate_project_name(name).is_err(), "should reject '{name}'");
        }
    }

    #[test]
    fn scaffold_creates_expected_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("myapp");

        scaffold(&dir, "myapp", "0.2.0", "a test app", "tester", &["web"]).unwrap();

        // Core files and directories are present.
        assert!(dir.join("src/lib.rs").exists(), "missing src/lib.rs");
        assert!(dir.join("Cargo.toml").exists(), "missing Cargo.toml");
        assert!(dir.join("aimer.toml").exists(), "missing aimer.toml");
        assert!(dir.join(".gitignore").exists(), "missing .gitignore");
        assert!(dir.join("README.md").exists(), "missing README.md");
        assert!(dir.join("builds/web").is_dir(), "missing builds/web");

        // Generated Cargo.toml parses and carries the right package name.
        assert_eq!(
            crate::config::parse_cargo_package_name(&dir),
            Some("myapp".to_string())
        );

        // aimer.toml round-trips with the collected metadata.
        let manifest = AimerManifest::load_from(&dir).unwrap().unwrap();
        assert_eq!(manifest.package.name, "myapp");
        assert_eq!(manifest.package.version, "0.2.0");
        assert_eq!(manifest.package.description, "a test app");
        assert_eq!(manifest.package.author, "tester");
    }

    #[test]
    fn scaffold_cleanup_on_caller_failure_is_possible() {
        // Mirrors `execute`: a failed scaffold leaves a directory the caller can
        // remove. Here we just verify scaffolding then removing leaves nothing.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("throwaway");
        scaffold(&dir, "throwaway", "0.1.0", "", "", &[]).unwrap();
        assert!(dir.exists());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!dir.exists());
    }
}
