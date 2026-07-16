use aimer::style::{LayoutSpacing, Spacing, TextStyle};
use aimer::{
    AimerApp, BoxAlignment, Color, Container, Dimension, Flex, FontError, FontFamily,
    FontRegistration, FontRegistry, FontStyle, FontWeight, LayoutDirection, SizedBox, Text, Widget,
};

const CUSTOM_FONT_FAMILY: &str = "Jaime JetBrains Mono";
const CUSTOM_FONT_BYTES: &[u8] = include_bytes!("../assets/JetBrainsMono-Regular.ttf");

fn register_custom_font() -> Result<FontFamily, FontError> {
    if let Some(family) = FontRegistry::family(CUSTOM_FONT_FAMILY) {
        return Ok(family);
    }

    FontRegistry::register(FontRegistration {
        family: CUSTOM_FONT_FAMILY,
        bytes: CUSTOM_FONT_BYTES,
        weight: FontWeight::Normal,
        style: FontStyle::Normal,
    })
}

pub fn start_custom_font_example() {
    let custom_font = register_custom_font().expect("the embedded custom font should be valid");

    AimerApp::start(
        Container::new()
            .color(Color::WHITE)
            .padding(LayoutSpacing::all(Spacing::Px(32)))
            .child(
                Flex::new()
                    .direction(LayoutDirection::Column)
                    .vertical_alignment(BoxAlignment::Center)
                    .horizontal_alignment(BoxAlignment::Center)
                    .children([
                        Text::new("Text with a custom font")
                            .text_style(TextStyle::new().font_size(30).color(Color::BLACK))
                            .boxed(),
                        SizedBox::new().height(24).boxed(),
                        Text::new("JetBrains Mono\nThe quick brown fox jumps over the lazy dog.\n0123456789  {}[]() =>")
                            .text_style(
                                TextStyle::new()
                                    .font_family(custom_font)
                                    .font_size(22)
                                    .color(Color::BLUE),
                            )
                            .boxed(),
                        SizedBox::new().height(24).boxed(),
                        Container::new()
                            .width(Dimension::Px(520.0))
                            .child(
                                Text::new("The font is embedded with include_bytes!, registered before AimerApp::start, and selected through TextStyle::font_family.")
                                    .text_style(TextStyle::new().font_size(16).color(Color::GRAY)),
                            )
                            .boxed(),
                    ]),
            ),
    )
}

#[cfg(test)]
mod tests {
    use aimer::FontRegistry;

    use super::*;

    #[test]
    fn custom_font_example_registers_embedded_family() {
        let family = register_custom_font().expect("the embedded custom font should be valid");

        assert_eq!(FontRegistry::family(CUSTOM_FONT_FAMILY), Some(family));
    }
}
