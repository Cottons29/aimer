
use colored::{Color, Colorize};
use std::error::Error;
use std::fmt::Debug;

#[derive(Debug)]
pub struct VersionCommand;
const VERSION: &str = env!("CARGO_PKG_VERSION");


impl VersionCommand {
    pub(crate) fn execute() {
        let (cargo_version, rust_version) = (Self::get_cargo_version(), Self::get_rust_version());
        // Rainbow gradient 🌈
        let gradient = [Color::Green, Color::Yellow, Color::Red, Color::Magenta, Color::Blue];

        let current_os_name = std::env::consts::OS;
        let cargo_version_line = match cargo_version {
            Some(version) => {
                format!("Cargo version is {}", format!("{}", version.bold()))
            }
            None => String::new(),
        };
        let formatted_version =
            format!("Current Version is {} ({})", VERSION.to_string().green().bold(), current_os_name.green());

        let rustc_version_line = match rust_version {
            Some(version) => {
                format!("Rust version is {}", format!("{} ", version.bold()))
            }
            None => String::new(),
        };

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
            rustc_version_line,
        ];

        let lines = [
            r#"            *+            "#,
            r#"           *##*+          "#,
            r#"        ########*+        "#,
            r#"      *####█#####*        "#,
            r#"     +###\\#█#//## #*     "#,
            r#"    +## █#\\█//#█ ##+     "#,
            r#"     +#\\█#\█/#█###+      "#,
            r#"       #\█#\█ █##         "#,
            r#"          █ █ █           "#,
            r#" `````````````````````````"#,
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
