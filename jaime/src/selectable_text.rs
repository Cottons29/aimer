use aimer::style::{LayoutSpacing, Spacing, TextOverflow, TextStyle};
use aimer::{
    AimerApp, Color, Container, Dimension, Flex, FontStyle, FontWeight, LayoutDirection, RichText,
    SizedBox, SpanStyle, Text, TextSpan, Widget,
};

pub fn selectable_text_example() -> impl Widget {
    Container::new()
        .color(Color::WHITE)
        .padding(LayoutSpacing::all(Spacing::Px(32)))
        .child(
            Flex::new()
                .direction(LayoutDirection::Column)
                .children([
                    Text::new("Selectable RichText")
                        .text_style(TextStyle::new().font_size(30).color(Color::BLACK))
                        .boxed(),
                    SizedBox::new().height(16).boxed(),
                    Text::new(
                        "Drag across the text below to select it. Press Cmd/Ctrl+A to select all and Cmd/Ctrl+C to copy.",
                    )
                    .text_style(TextStyle::new().font_size(16).color(Color::GRAY))
                    .boxed(),
                    SizedBox::new().height(24).boxed(),
                    Container::new()
                        // .width(Dimension::Px(680.0))
                        .padding(LayoutSpacing::all(Spacing::Px(20)))
                        .color(Color::Rgb(245, 247, 250))
                        .child(
                            RichText::new(TextSpan::root([
                                TextSpan::new("Selection works across "),
                                TextSpan::new("bold text")
                                    .style(SpanStyle::new().font_weight(FontWeight::Bold)),
                                TextSpan::new(", "),
                                TextSpan::new("italic text")
                                    .style(SpanStyle::new().font_style(FontStyle::Italic)),
                                TextSpan::new(", colors, wrapped lines, and Unicode: "),
                                TextSpan::new("Aimer • 你好 • សួស្តី • 👩‍💻")
                                    .style(SpanStyle::new().color(Color::BLUE)),
                                TextSpan::new(
                                    "\n\nThe copied value is plain text, without style metadata.",
                                ),
                            ]))
                                .text_overflow(TextOverflow::Wrap)
                            .text_style(TextStyle::new().font_size(20).color(Color::BLACK))
                            .selectable(),
                        )
                        .boxed(),
                ]),
        )
}

pub fn start_selectable_text_example() {
    AimerApp::start(selectable_text_example());
}

#[cfg(test)]
mod tests {
    use aimer::Widget;

    use super::selectable_text_example;

    #[test]
    fn selectable_text_example_builds_a_demo_screen() {
        fn assert_widget(_: impl Widget) {}

        assert_widget(selectable_text_example());
    }
}
