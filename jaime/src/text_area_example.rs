//! A focused showcase for Aimer's multiline text editor.

use aimer::style::*;
use aimer::*;

/// Starts the multiline text-area showcase.
pub fn start_text_area_example() {
    AimerApp::start(TextAreaExample::new().boxed())
}

/// Demonstrates wrapped multiline input with bounded vertical growth.
#[derive(Clone, StatelessWidget)]
pub struct TextAreaExample {
    controller: TextEditingController,
}

impl TextAreaExample {
    /// Creates the showcase with editable multiline content.
    #[inline]
    pub fn new() -> Self {
        Self {
            controller: TextEditingController::with_text(
                "Write across multiple lines.\nLong lines wrap within the available width.",
            ),
        }
    }
}

impl Default for TextAreaExample {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl StatelessWidget for TextAreaExample {
    fn build(&self, _: &BuildContext) -> impl Widget {
        let text_style = TextStyle::new().font_size(16).color(Colors::Black);

        Container::new()
            .color(Colors::White.into())
            .padding(LayoutSpacing::all(Spacing::Px(32)))
            .child(
                Column::new().children(vec![
                    Text::new("TextArea")
                        .text_style(
                            TextStyle::new()
                                .font_size(28)
                                .font_weight(FontWeight::Bold)
                                .color(Colors::Black),
                        )
                        .boxed(),
                    Text::new("Multiline input with soft wrapping and vertical scrolling.")
                        .text_style(text_style.color(Colors::Gray))
                        .boxed(),
                    TextArea::new()
                        .controller(self.controller.clone())
                        .hint("Write a longer message")
                        .hint_style(text_style.color(Color::GRAY.with_alpha(0.5)))
                        .text_style(text_style.color(Colors::Black))
                        .min_lines(1)
                        .max_lines(Some(10))
                        .padding(LayoutSpacing::all(Spacing::Px(12)))
                        .on_changed(|text: String| {
                            println!("TextArea contains {} bytes", text.len())
                        })
                        .boxed(),
                ]),
            )
    }
}
