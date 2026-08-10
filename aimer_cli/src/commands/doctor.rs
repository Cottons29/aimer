pub(crate) mod rust_targets;

use std::process::Command;

use colored::Colorize;

use crate::commands::doctor::rust_targets::{
    RUST_TARGETS, install_hint, installed_targets, parse_installed,
};
use crate::errors::AimerError;
use crate::targets::Targets;

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
    Tool {
        bin: "rustc",
        probe: &["--version"],
        purpose: "Rust compiler",
    },
    Tool {
        bin: "cargo",
        probe: &["--version"],
        purpose: "Rust package manager",
    },
    Tool {
        bin: "trunk",
        probe: &["--version"],
        purpose: "Web (wasm) dev server & bundler",
    },
    Tool {
        bin: "xcrun",
        probe: &["--version"],
        purpose: "iOS/macOS toolchain (Xcode)",
    },
    Tool {
        bin: "adb",
        probe: &["--version"],
        purpose: "Android device bridge",
    },
    Tool {
        bin: "gradle",
        probe: &["--version"],
        purpose: "Android project builds",
    },
    #[cfg(target_os = "macos")]
    Tool {
        bin: "llvm-ar",
        probe: &["--version"],
        purpose: "Web (optional for building markdown syntax highlight)"
    },
    #[cfg(target_os = "linux")]
    Tool {
        bin: "cc",
        probe: &["--version"],
        purpose: "Linux desktop (linking the application binary)",
    },
    #[cfg(target_os = "linux")]
    Tool {
        bin: "pkg-config",
        probe: &["--version"],
        purpose: "Linux desktop (locating system libraries)",
    },
    #[cfg(target_os = "windows")]
    Tool {
        bin: "link",
        probe: &["/?"],
        purpose: "Windows desktop (MSVC linker)",
    },
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
    println!(
        "{}",
        "Checking your Aimer development environment...".bold()
    );

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
        println!(
            "  {mark}  {name} {}",
            format!("— {}", tool.purpose).dimmed()
        );
    }

    missing += report_rust_targets();

    println!();
    if missing == 0 {
        println!("{}", "All tools found. You're good to go!".green().bold());
    } else {
        println!(
            "{}",
            format!("{missing} tool(s)/target(s) missing. Some targets may be unavailable.")
                .yellow()
                .bold()
        );
    }

    Ok(())
}

/// Print the `Rust targets` section and return how many triples are missing.
///
/// The section lists every triple in [`RUST_TARGETS`] with the Aimer platform
/// needing it, marks each one installed or missing, and ends with the exact
/// `rustup target add …` line that fixes the gap — `doctor` reports, it never
/// mutates the toolchain.
///
/// When rustup itself is unavailable the section degrades to a single dimmed
/// note and contributes nothing to the summary, so a rustup-less installation
/// never fails the command.
fn report_rust_targets() -> usize {
    println!();
    println!("{}", "Rust targets (rustup)".bold());

    let Some(stdout) = installed_targets() else {
        println!(
            "  {}",
            "rustup not found — skipping Rust target check".dimmed()
        );
        return 0;
    };

    let installed = parse_installed(&stdout);
    let mut missing = Vec::new();
    for target in RUST_TARGETS {
        // Pad before colouring so ANSI codes don't break alignment.
        let triple = format!("{:<28}", target.triple);
        let (mark, triple) = if installed.contains(&target.triple) {
            ("✔".green(), triple.green())
        } else {
            missing.push(target.triple);
            ("✘".red(), triple.red())
        };
        println!(
            "  {mark}  {triple} {}",
            format!("— {}", target.required_by).dimmed()
        );
    }

    // Desktop is a host build, so it needs no extra triple at all.
    if let Some(host) = Targets::host_desktop() {
        println!(
            "  {}  {:<28} {}",
            "•".dimmed(),
            std::env::consts::ARCH,
            format!("— {host} (host toolchain, no extra target needed)").dimmed()
        );
    }

    if let Some(hint) = install_hint(&missing) {
        println!();
        println!("{}", "Install the missing ones with:".bold());
        println!("  {}", hint.cyan());
    }

    missing.len()
}


