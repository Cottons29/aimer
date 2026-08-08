use aimer::{AimerApp, FocusNode, InputType, TextEditingController, TextField};

fn main() {
    let controller = TextEditingController::with_text("Aimer");
    let focus = FocusNode::new();

    AimerApp::start(
        TextField::new()
            .controller(controller)
            .focus_node(focus)
            .input_type(InputType::Text)
            .hint("Your name")
            .max_length(Some(80))
            .on_submitted(|text: String| println!("Submitted: {text}")),
    );
}