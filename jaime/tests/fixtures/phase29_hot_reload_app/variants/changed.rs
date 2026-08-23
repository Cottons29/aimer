use aimer::AimerApp;

fn launch() {
    AimerApp::new()
        .child(jaime::hot_reload_proof::proof_root_with_label("UPDATED"))
        .run();
}

#[aimer::main]
fn main() {
    launch();
}
