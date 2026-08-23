use aimer::{AimerApp, Text};

fn launch() {
    let _: u32 = "intentional automatic fixture compile failure";
    AimerApp::new().child(Text::new("compile failure")).run();
}

#[aimer::main]
fn main() {
    launch();
}