use crate::commands::run::cargo_build::CargoBuildTarget;
use colored::Colorize;
use crossterm::style::Stylize;
use std::env::current_dir;
use std::error::Error;
use std::path::PathBuf;
use std::process::Command;

pub trait LogStyling {
    fn process_log(self) -> String;
}

pub fn get_project_root(allow_workspace: bool) -> Result<PathBuf, Box<dyn Error>> {
    let mut command = Command::new("cargo");
    command.args(["locate-project", "--message-format=plain"]);

    if allow_workspace {
        command.arg("--workspace");
    }

    let output = command.output()?;
    if !output.status.success() {
        return Err("cargo locate-project failed".into());
    }
    let cargo_toml = String::from_utf8(output.stdout)?;
    let root = PathBuf::from(cargo_toml.trim())
        .parent()
        .ok_or("Failed to get workspace root")?
        .to_path_buf();

    Ok(root)
}

pub fn resolve_lib_path(lib_name: &str, rust_target: &str, target: CargoBuildTarget) -> String {
    let extension = match target {
        CargoBuildTarget::Darwin | CargoBuildTarget::Ios {..} => ".a",
        _ => ".so",
    };
    let project_root = get_project_root(true).unwrap_or_else(|_| current_dir().unwrap());

    format!("{}/target/{}/debug/lib{}{extension}", project_root.display(), rust_target, lib_name)
}

impl LogStyling for String {
    fn process_log(self) -> String {
        if self.contains("[ERROR]") {
            self.red().to_string()
        } else if self.contains("[WARN]") {
            self.yellow().to_string()
        } else if self.contains("[DEBUG]") || self.contains("hot-reload") {
            self.green().to_string()
        } else if self.contains("[INFO]") {
            self.bright_cyan().to_string()
        } else {
            self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_finding_project_root() {
        let expected = PathBuf::from(env::var("HOME").unwrap())
            .join("Documents")
            .join("AimerFramework")
            .join("aimer");
        let project_root = get_project_root(true);
        assert!(project_root.is_ok());
        assert_eq!(project_root.unwrap(), expected)
    }

    #[test]
    fn process_log_error_contains_original_text() {
        let result = String::from("[ERROR] something broke").process_log();
        assert!(result.contains("[ERROR] something broke"));
    }

    #[test]
    fn process_log_warn_contains_original_text() {
        let result = String::from("[WARN] be careful").process_log();
        assert!(result.contains("[WARN] be careful"));
    }

    #[test]
    fn process_log_debug_contains_original_text() {
        let result = String::from("[DEBUG] trace info").process_log();
        assert!(result.contains("[DEBUG] trace info"));
    }

    #[test]
    fn process_log_hot_reload_contains_original_text() {
        let result = String::from("hot-reload triggered").process_log();
        assert!(result.contains("hot-reload triggered"));
    }

    #[test]
    fn process_log_info_contains_original_text() {
        let result = String::from("[INFO] all good").process_log();
        assert!(result.contains("[INFO] all good"));
    }

    #[test]
    fn process_log_plain_text_unchanged() {
        let input = "just a normal message";
        let result = input.to_string().process_log();
        assert_eq!(result, input);
    }

    #[test]
    fn process_log_empty_string() {
        let result = String::new().process_log();
        assert_eq!(result, "");
    }

    #[test]
    fn process_log_different_levels_produce_different_output() {
        let error = String::from("[ERROR] bad").process_log();
        let warn = String::from("[WARN] bad").process_log();
        let info = String::from("[INFO] bad").process_log();
        let debug = String::from("[DEBUG] bad").process_log();
        let plain = String::from("bad").process_log();

        // Each branch should produce a distinct styled output
        assert_ne!(error, warn);
        assert_ne!(error, info);
        assert_ne!(error, debug);
        assert_ne!(error, plain);
        assert_ne!(warn, info);
        assert_ne!(warn, debug);
        assert_ne!(info, debug);
        assert_ne!(info, plain);
    }

    #[test]
    fn process_log_error_takes_priority_over_warn() {
        // "[ERROR] [WARN]" should hit the ERROR branch first (not WARN)
        let result = String::from("[ERROR] [WARN] conflict").process_log();
        // Verify it goes through the error branch (red) by checking it's styled
        // and NOT plain text
        assert_ne!(result, "[ERROR] [WARN] conflict");
        // Also confirm a pure WARN is styled differently
        let warn_result = String::from("[WARN] conflict").process_log();
        assert_ne!(result, warn_result);
    }

    #[test]
    fn process_log_hot_reload_in_middle() {
        let result = String::from("something hot-reload happened").process_log();
        // hot-reload is in the DEBUG branch
        let debug = String::from("[DEBUG] something").process_log();
        // Both go through the green branch — verify they're both styled
        assert_ne!(result, "something hot-reload happened");
        assert_ne!(debug, "[DEBUG] something");
    }
}
