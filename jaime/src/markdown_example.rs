use aimer::style::{LayoutSpacing, Spacing};
use aimer::{AimerApp, Color, Container, MarkdownTheme, MarkdownViewer, Widget};

const JAIME_MARKDOWN: &str = include_str!("../assets/JAIME.md");

pub fn jaime_markdown_source() -> &'static str {
    JAIME_MARKDOWN
}

pub fn jaime_markdown_content() -> MarkdownViewer {
    MarkdownViewer::new()
        .theme(MarkdownTheme::default())
        .markdown(jaime_markdown_source())
}

pub fn jaime_markdown_viewer() -> impl Widget {
    Container::new()
        .padding(LayoutSpacing::all(Spacing::Px(16)))
        .color(Color::WHITE)
        .child(jaime_markdown_content())
}

pub fn start_markdown_example() {
    AimerApp::start(jaime_markdown_viewer());
}

#[cfg(test)]
mod tests {
    use aimer::{MarkdownDocument, Widget};

    use super::{jaime_markdown_content, jaime_markdown_source};

    #[test]
    fn bundled_jaime_markdown_is_loaded_and_parseable() {
        let source = jaime_markdown_source();

        assert!(source.starts_with("# AimerMarkdown"));
        assert!(MarkdownDocument::parse(source).is_ok());
        assert_eq!(jaime_markdown_content().text_content(), Some(source));
    }
}
