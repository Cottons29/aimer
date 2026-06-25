use crate::errors::AimerError;
use colored::Colorize;
use std::process::Command;

/// A required or optional external tool the CLI may shell out to.
struct Tool {
    /// Executable name as invoked on the PATH.
    bin: &'static str,
    /// Arguments used to probe for the tool (usually a version flag).
    probe: &'static [&'static str],
    /// Human-readable description of what the tool is used for.
    purpose: &'static str,
}

const TOOLS: &[Tool] = &[
    Tool { bin: "rustc", probe: &["--version"], purpose: "Rust compiler" },
    Tool { bin: "cargo", probe: &["--version"], purpose: "Rust package manager" },
    Tool { bin: "trunk", probe: &["--version"], purpose: "Web (wasm) dev server & bundler" },
    Tool { bin: "xcrun", probe: &["--version"], purpose: "iOS/macOS toolchain (Xcode)" },
    Tool { bin: "adb", probe: &["--version"], purpose: "Android device bridge" },
    Tool { bin: "gradle", probe: &["--version"], purpose: "Android project builds" },
];

/// Return `true` when `bin` can be executed (i.e. it is installed and on PATH).
pub fn is_tool_available(bin: &str, probe: &[&str]) -> bool {
    Command::new(bin)
        .args(probe)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Ensure a required tool is present, returning a typed error otherwise.
pub fn ensure_tool(bin: &str, probe: &[&str]) -> Result<(), AimerError> {
    if is_tool_available(bin, probe) {
        Ok(())
    } else {
        Err(AimerError::MissingToolchain(bin.to_string()))
    }
}

pub fn execute() -> anyhow::Result<()> {
    println!("{}", "Checking your Aimer development environment...\n".bold());

    let mut missing = 0;
    for tool in TOOLS {
        let available = is_tool_available(tool.bin, tool.probe);
        // Pad before colouring so ANSI codes don't break alignment.
        let name = format!("{:<12}", tool.bin);
        let (mark, name) = if available {
            ("✔".green(), name.green())
        } else {
            missing += 1;
            ("✘".red(), name.red())
        };
        println!("  {mark}  {name} {}", format!("— {}", tool.purpose).dimmed());
    }

    println!();
    if missing == 0 {
        println!("{}", "All tools found. You're good to go!".green().bold());
    } else {
        println!(
            "{}",
            format!("{missing} tool(s) missing. Some targets may be unavailable.")
                .yellow()
                .bold()
        );
    }

    Ok(())
}
