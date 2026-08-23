use aimer::AimerApp;

fn launch() {
    AimerApp::new()
        .child(jaime::hot_reload_proof::proof_root())
        .run();
}

#[aimer::main]
fn main() {
    launch();
}
