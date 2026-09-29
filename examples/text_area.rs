use aimer::input::{TextArea, TextEditingController};
use aimer::AimerApp;

fn main() {
    let controller = TextEditingController::new();

    AimerApp::start(
        TextArea::new()
            .controller(controller)
            .hint("Write a message")
            .min_lines(4)
            .max_lines(Some(10))
            .on_changed(|text: String| println!("{} bytes", text.len())),
    );
}
