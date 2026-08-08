 //! A focused showcase for Aimer's single-line text editor.

use aimer::style::*;
use aimer::*;

/// Starts the single-line text-field showcase.
pub fn start_text_field_example() {
    AimerApp::start(TextFieldExample::new().boxed())
}

/// Demonstrates a controlled single-line field with submission handling.
pub struct TextFieldExample {
    controller: TextEditingController,
}

impl TextFieldExample {
    /// Creates the showcase with an editable greeting.
    #[inline]
    pub fn new() -> Self {
        Self {
            controller: TextEditingController::with_text("Hello, Aimer!"),
        }
    }
}

impl Default for TextFieldExample {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for TextFieldExample {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        Container::new()
            .color(Colors::White.into())
            .padding(LayoutSpacing::all(Spacing::Px(32)))
            .child(
                Column::new().children(vec![
                    Text::new("TextField")
                        .text_style(
                            TextStyle::new()
                                .font_size(28)
                                .font_weight(FontWeight::Bold)
                                .color(Colors::Black),
                        )
                        .boxed(),
                    Text::new("Single-line input. Press Return to submit.")
                        .text_style(TextStyle::new().font_size(16).color(Colors::Gray))
                        .boxed(),
                    TextField::new()
                        .controller(self.controller.clone())
                        .input_type(InputType::Text)
                        .hint("Type a short message")
                        .max_length(Some(80))
                        .padding(LayoutSpacing::all(Spacing::Px(12)))
                        .on_submitted(|text: String| println!("Submitted: {text}"))
                        .boxed(),
                ]),
            )
            .to_element(ctx)
    }

    fn debug_name(&self) -> &'static str {
        "TextFieldExample"
    }
}