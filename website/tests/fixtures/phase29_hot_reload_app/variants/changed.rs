use aimer::AimerApp;

fn launch() {
    AimerApp::new()
        .child(website::portable_proof::hot_reload_proof_root_with_label("UPDATED"))
        .run();
}

#[aimer::main]
fn main() {
    launch();
}
