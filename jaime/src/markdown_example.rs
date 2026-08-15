use std::rc::Rc;

use aimer::macros::widget;
use aimer::style::{BoxDecoration, FontWeight, LayoutSpacing, Spacing, TextAlign, TextStyle};
use aimer::*;

const JAIME_MARKDOWN: &str = include_str!("../assets/JAIME.md");
const CUSTOM_MARKDOWN: &str = r#"# Custom Markdown Widgets

Custom inline syntax can render a widget inline: {{button:Press me}}.

:::callout
This block is written in Markdown, but rendered as a custom Aimer widget.
:::
"#;

pub fn jaime_markdown_source() -> &'static str {
    JAIME_MARKDOWN
}

pub fn jaime_markdown_content() -> MarkdownViewer {
    MarkdownViewer::new()
        .padding(LayoutSpacing::all(Spacing::Px(16)))
        .theme(MarkdownTheme::default())
        .markdown(jaime_markdown_source())
}

pub fn jaime_markdown_viewer() -> impl Widget {
    Container::new()
        .color(Color::WHITE)
        .child(jaime_markdown_content())
}

fn custom_markdown_content_with_press(on_press: impl Fn() + 'static) -> MarkdownViewer {
    let on_press = Rc::new(on_press);

    MarkdownViewer::new()
        .padding(LayoutSpacing::all(Spacing::Px(16)))
        .theme(MarkdownTheme::default())
        .markdown(CUSTOM_MARKDOWN)
        .custom_block(
            MarkdownBlockRule::new(
                "callout",
                MarkdownBlockSyntax::Paired {
                    opening: ":::callout",
                    closing: ":::",
                },
            ),
            |data| {
                Container::new()
                    .padding(LayoutSpacing::all(Spacing::Px(16)))
                    .color(Color::Rgb(239, 246, 255))
                    .child(
                        Column::new().children([
                            Text::new("Custom block")
                                .text_style(
                                    TextStyle::new()
                                        .font_weight(FontWeight::Bold)
                                        .color(Color::Rgb(30, 64, 175)),
                                )
                                .boxed(),
                            Text::new(data.text.trim().to_owned())
                                .text_style(TextStyle::new().color(Color::Rgb(30, 41, 59)))
                                .boxed(),
                        ]),
                    )
                    .boxed()
            },
        )
        .custom_inline(
            MarkdownInlineRule::new(
                "button",
                MarkdownInlineSyntax::Paired {
                    opening: "{{button:",
                    closing: "}}",
                },
            ),
            move |data| {
                let on_press = on_press.clone();
                Button::new()
                    .decoration(
                        BoxDecoration::new()
                            .background_color(Color::Rgb(37, 99, 235))
                            .border_radius(6),
                    )
                    .hover_decoration(
                        BoxDecoration::new()
                            .background_color(Color::Rgb(29, 78, 216))
                            .border_radius(6),
                    )
                    .press_decoration(
                        BoxDecoration::new()
                            .background_color(Color::Rgb(30, 64, 175))
                            .border_radius(6),
                    )
                    .on_press(move || on_press())
                    .child(
                        Container::new()
                            .width(120)
                            .height(20)
                            .child(Text::new(data.label.clone())
                                .text_align(TextAlign::MidCenter)
                                .text_style(
                                TextStyle::new()

                                    .font_weight(FontWeight::Bold)
                                    .color(Color::WHITE),
                            )),
                    )
                    .boxed()
            },
        )
}

pub fn custom_markdown_content() -> MarkdownViewer {
    custom_markdown_content_with_press(|| {})
}

#[widget(Stateful)]
pub struct CustomMarkdownExample {}

impl CustomMarkdownExample {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct CustomMarkdownExampleState {
    presses: u32,
    updater: StateUpdater<Self>,
}

impl StatefulWidget for CustomMarkdownExample {
    type State = CustomMarkdownExampleState;

    fn create_state(self) -> Self::State {
        CustomMarkdownExampleState {
            presses: 0,
            updater: StateUpdater::empty(),
        }
    }
}

impl State<CustomMarkdownExample> for CustomMarkdownExampleState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, _ctx: &aimer::BuildContext) -> impl Widget {
        let updater = self.updater.clone();
        Column::new().children([
            Text::new(format!("Button presses: {}", self.presses))
                .text_style(TextStyle::new().color(Color::Rgb(30, 41, 59)))
                .boxed(),
            custom_markdown_content_with_press(move || {
                println!("Button pressed");
                updater.set_state(|state| state.presses += 1);
            })
            .boxed(),
        ])
    }
}

pub fn custom_markdown_viewer() -> impl Widget {
    Container::new()
        .color(Color::WHITE)
        .child(CustomMarkdownExample::new())
}

pub fn start_custom_markdown_example() {
    AimerApp::start(custom_markdown_viewer());
}

pub fn start_markdown_example() {
    AimerApp::start(jaime_markdown_viewer());
}

#[cfg(test)]
mod tests {
    use aimer::{MarkdownDocument, Widget};

    use super::{
        custom_markdown_content, jaime_markdown_content, jaime_markdown_source, CUSTOM_MARKDOWN,
    };

    #[test]
    fn bundled_jaime_markdown_is_loaded_and_parseable() {
        let source = jaime_markdown_source();

        assert!(source.starts_with("# AimerMarkdown"));
        assert!(MarkdownDocument::parse(source).is_ok());
        assert_eq!(jaime_markdown_content().text_content(), Some(source));
    }

    #[test]
    fn custom_markdown_source_contains_registered_syntax() {
        assert!(CUSTOM_MARKDOWN.contains("{{button:Press me}}"));
        assert!(CUSTOM_MARKDOWN.contains(":::callout"));
        assert!(custom_markdown_content().text_content().is_some());
    }
}
