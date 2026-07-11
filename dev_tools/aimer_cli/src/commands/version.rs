use colored::{Color, Colorize};
use sha2::{Digest, Sha256};
use std::fmt::Debug;
use std::{env, fs};

#[derive(Debug)]
pub struct VersionCommand;
const VERSION: &str = env!("CARGO_PKG_VERSION");

impl VersionCommand {
    pub(crate) fn execute() {
        use std::thread;

        let cargo_handle = thread::spawn(Self::get_cargo_version);
        let rust_handle = thread::spawn(Self::get_rust_version);
        let binary_hash_handle = thread::spawn(|| {
            let exe = env::current_exe().unwrap();
            let bytes = fs::read(exe).unwrap();
            let hash = Sha256::digest(bytes);
            hex::encode(hash)
        });

        let cargo_version = cargo_handle.join().unwrap();
        let rust_version = rust_handle.join().unwrap();
        let binary_hash = binary_hash_handle.join().unwrap();

        // Rainbow gradient 🌈
        let gradient = [Color::Green, Color::Yellow, Color::Red, Color::Magenta, Color::Blue];

        let rustc_version_line = match rust_version {
            Some(version) => {
                format!("rustc {} ", version.green().bold())
            }
            None => String::new(),
        };

        let current_os_name = env::consts::OS;
        let cargo_version_line = match cargo_version {
            Some(version) => {
                format!("cargo {}, {}", version.green().bold(), rustc_version_line)
            }
            None => String::new(),
        };

        let build_time = option_env!("AIMER_BUILD_TIME").unwrap_or("undefined");
        let formatted_buildtime = format!("Build Time: {}", build_time.green().bold());
        let formatted_version = format!("Current Version is {} ({})", VERSION.to_string().green().bold(), current_os_name.green());

        // let flutter_version_line = String::new();

        let messages = [
            String::new(),
            format!("Welcome to {}!", "Aimer 🎍".green().bold()),
            // format!("A {} cross-platform framework for building gui applications.","Rust".red().bold() ),
            "A cross-platform framework for building pretty gui applications.".into(),
            format!("Aimer are written in {} 🦀", "Rust".red().bold()),
            String::new(),
            formatted_version,
            cargo_version_line,
            formatted_buildtime,
            format!("sha256: {}", binary_hash.green().bold()),
        ];

        let lines = [
            r#"            #▄▄▄▄# x      "#,
            r#"      x     ▄▌    █ x      "#,
            r#"     #▄▄#   █ xx  █x       "#,
            r#"  x ▄█▀▀█▄  █ x░  █        "#,
            r#"   x█ x  ▀▀▀ ░    █x       "#,
            r#"   x█▄ x  x  x ░ x█        "#,
            r#"     █  x    x ░  █        "#,
            r#"   xx▀██▄  xx▒   ▐▀x       "#,
            r#"        ▀█  ▒x █▀▀x        "#,
            r#"         █ ▓ ▓ █          "#,
            r#"▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀"#,
            "\n",
        ];
        let total = lines.len();
        println!();
        for (i, line) in lines.iter().enumerate() {
            let color_index = i * gradient.len() / total;
            println!(" {}     {}", line.color(gradient[color_index]), messages.get(i).unwrap_or(&"".to_string()));
        }
    }

    fn get_rust_version() -> Option<String> {
        let output = std::process::Command::new("rustc")
            .arg("--version")
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let version = stdout.split_whitespace().nth(1)?.to_string();

        Some(version)
    }

    fn get_cargo_version() -> Option<String> {
        let output = std::process::Command::new("cargo")
            .arg("--version")
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let version = stdout.split_whitespace().nth(1)?.to_string();

        Some(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_constant_is_not_empty() {
        assert!(!VERSION.is_empty(), "CARGO_PKG_VERSION should not be empty");
    }

    #[test]
    fn version_constant_is_semver_like() {
        // Should have at least major.minor.patch
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert!(parts.len() >= 3, "VERSION '{}' is not semver-like", VERSION);
    }

    #[test]
    fn get_rust_version_returns_some() {
        let version = VersionCommand::get_rust_version();
        assert!(version.is_some(), "rustc should be available on the system");
        let v = version.unwrap();
        assert!(!v.is_empty());
        // Should look like a version number (contains digits)
        assert!(v.chars().any(|c| c.is_numeric()), "Rust version '{}' should contain digits", v);
    }

    #[test]
    fn get_cargo_version_returns_some() {
        let version = VersionCommand::get_cargo_version();
        assert!(version.is_some(), "cargo should be available on the system");
        let v = version.unwrap();
        assert!(!v.is_empty());
        assert!(v.chars().any(|c| c.is_numeric()), "Cargo version '{}' should contain digits", v);
    }

    #[test]
    fn version_command_debug_format() {
        let debug = format!("{:?}", VersionCommand);
        assert_eq!(debug, "VersionCommand");
    }
}
