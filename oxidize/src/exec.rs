pub mod run;
pub mod platform;

pub enum Commands {
    Run,
    Build,
    Create
}


impl From<&str> for Commands {
    fn from(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "run" => Commands::Run,
            "build" => Commands::Build,
            _ => panic!("Unknown command: {}", s),
        }
    }
}
