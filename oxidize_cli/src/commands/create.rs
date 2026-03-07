pub mod android;
pub mod ios;
pub mod linux;
pub mod macos;
pub mod web;
pub mod window;

use inquire::{ui::{Color, RenderConfig, Styled}, Confirm, MultiSelect, Text};
use std::fs;
use std::path::PathBuf;

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

pub fn execute(project_name: &str) {
    let mut config = RenderConfig::default();
    config.prompt_prefix = Styled::new("◆").with_fg(Color::LightCyan);
    config.highlighted_option_prefix = Styled::new("│  ❯").with_fg(Color::LightCyan);
    config.unhighlighted_option_prefix = Styled::new("│   ").with_fg(Color::DarkGrey);
    config.selected_checkbox = Styled::new("●").with_fg(Color::LightGreen);
    config.unselected_checkbox = Styled::new("○").with_fg(Color::DarkGrey);
    inquire::set_global_render_config(config);

    println!("current_dir : {}", std::env::current_dir().unwrap().display());
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
        return;
    }

    let dir = PathBuf::from(project_name);
    if dir.exists() {
        println!("Directory '{}' already exists!", project_name);
        return;
    }

    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("builds")).unwrap();
    fs::create_dir_all(dir.join("builds/web")).unwrap();
    fs::create_dir_all(dir.join("builds/build_src/src")).unwrap();

    if targets.contains(&"android") {
        android::create(&dir);
    }
    if targets.contains(&"ios") {
        ios::create(&dir);
    }
    if targets.contains(&"macos") {
        macos::create(&dir);
    }
    if targets.contains(&"windows") {
        window::create(&dir);
    }
    if targets.contains(&"linux") {
        linux::create(&dir);
    }
    if targets.contains(&"web") {
        web::create(&dir);
    }

    // Oxidize.toml
    fs::write(
        dir.join("Oxidize.toml"),
        format!(
            r#"[package]
name = "{project_name}"
group = "com.example.app"

[build]
dir = "."
"#,

        ),
    )
    .unwrap();

    // Cargo.toml
    fs::write(
        dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{}"
version = "{}"
edition = "2024"

[lib]
crate-type = ["cdylib", "rlib", "staticlib"]

[dependencies]
oxidize = {{path = "/Users/cottons/Documents/oxidize-fw/oxidize/oxidize"}}
"#,
            project_name, version
        ),
    )
    .unwrap();

    // src/lib.rs
    fs::write(
        dir.join("src/lib.rs"),
        include_str!("../../templates/sample.txt"),
    )
    .unwrap();

    // builds/build_src/Cargo.toml
    fs::write(
        dir.join("builds/build_src/Cargo.toml"),
        format!(
            r#"[package]
name = "build_src"
version = "0.1.0"
edition = "2024"

[dependencies]
{} = {{ path = "../../../{}" }}
"#,
            project_name, project_name
        ),
    )
    .unwrap();

    // builds/build_src/src/main.rs
//     fs::write(
//         dir.join("builds/build_src/src/main.rs"),
//         format!(
//             r#"fn main() {{
//     {}::__oxidize_generated_entrance_point()
// }}
// "#,
//             project_name
//         ),
//     )
//     .unwrap();

    fs::write(dir.join("README.md"), format!("# {}\n\n{}", project_name, description)).unwrap();

    println!("Project '{}' created successfully.", project_name);
}
