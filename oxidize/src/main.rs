use std::path::Path;
use crate::exec::run::RunCommand;

mod exec;



fn main() -> std::io::Result<()> {
    let args = std::env::args().collect::<Vec<String>>();
    let playground_path = Path::new("playground");
    if playground_path.exists() {
        std::fs::create_dir(playground_path.join("hello"))?;
    }
    println!("{:?}", args);
    RunCommand::run()
}