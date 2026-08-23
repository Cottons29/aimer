use aimer::AimerApp;

fn launch() {
    AimerApp::new()
        .child(website::portable_proof::hot_reload_proof_root())
        .run();
}

#[aimer::main]
fn main() {
    launch();
}
