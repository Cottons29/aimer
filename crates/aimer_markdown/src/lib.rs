mod document;
mod markdown_theme;
mod renderer;
mod syntax;

use std::rc::Rc;

use aimer_container::{Container, ScrollAxis, Scrollable};
use aimer_style::LayoutSpacing;
use aimer_widget::base::BuildContext;
use aimer_widget::{AnyWidget, Element, Widget};

pub use document::{Alignment, Block, Document, Inline, ListItem, MarkdownError, TableRow};
pub use markdown_theme::MarkdownTheme;
pub use renderer::{ImageResolver, LinkHandler, MarkdownImage, default_image_resolver};
pub use syntax::{CaptureSpan, highlight};

fn open_web_link_with<E>(
    target: &str,
    opener: impl FnOnce(&str) -> Result<(), E>,
) -> Option<Result<(), E>> {
    if !target.starts_with("https://") && !target.starts_with("http://") {
        return None;
    }
    Some(opener(target))
}

fn open_web_link(target: Rc<str>) {
    if let Some(Err(error)) = open_web_link_with(&target, webbrowser::open) {
        eprintln!("Failed to open Markdown link '{target}': {error}");
    }
}

/// A scrollable Markdown document rendered with native Aimer widgets.
///
/// Create an empty viewer with [`MarkdownViewer::new`], then provide source
/// with [`MarkdownViewer::markdown`].
#[derive(Clone)]
pub struct MarkdownViewer {
    source: Rc<str>,
    theme: MarkdownTheme,
    link_handler: Option<LinkHandler>,
    image_resolver: ImageResolver,
    padding: LayoutSpacing,
    scrollable: bool,
}

impl Default for MarkdownViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownViewer {
    /// Creates an empty viewer with the default theme and image resolver.
    ///
    /// Activated HTTP and HTTPS links open in the system web browser by default.
    pub fn new() -> Self {
        Self {
            source: Rc::from(""),
            theme: MarkdownTheme::default(),
            link_handler: Some(Rc::new(open_web_link)),
            image_resolver: Rc::new(default_image_resolver),
            padding: Default::default(),
            scrollable: true,
        }
    }

    pub fn padding(mut self, padding: LayoutSpacing) -> Self {
        self.padding = padding;
        self
    }

    /// Sets the Markdown source rendered by this viewer.
    pub fn markdown(mut self, source: impl Into<Rc<str>>) -> Self {
        self.source = source.into();
        self
    }

    /// Sets whether the viewer should be scrollable.
    pub fn scrollable(mut self, scrollable: bool) -> Self {
        self.scrollable = scrollable;
        self
    }

    /// Replaces the complete visual theme.
    pub fn theme(mut self, theme: MarkdownTheme) -> Self {
        self.theme = theme;
        self
    }

    /// Replaces the default browser opener with a custom link handler.
    ///
    /// The handler receives every activated target, including `#footnote-*`
    /// references.
    pub fn on_link(mut self, handler: impl Fn(Rc<str>) + 'static) -> Self {
        self.link_handler = Some(Rc::new(handler));
        self
    }

    /// Resolves each Markdown image into an arbitrary Aimer widget.
    pub fn image_resolver(
        mut self,
        resolver: impl Fn(&MarkdownImage) -> AnyWidget + 'static,
    ) -> Self {
        self.image_resolver = Rc::new(resolver);
        self
    }
}

impl Widget for MarkdownViewer {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let content = match Document::parse(&self.source) {
            Ok(document) => renderer::render_document(
                &document,
                &self.theme,
                self.link_handler
                    .as_ref(),
                &self.image_resolver,
            ),
            Err(error) => aimer_text::Text::new(error.to_string())
                .text_style(self.theme.body)
                .boxed(),
        };

        if self.scrollable {
            Scrollable::new()
                .axis(ScrollAxis::Vertical)
                .child(
                    Container::new()
                        .padding(self.padding)
                        .child(content),
                )
                .to_element(ctx)
        } else {
            Container::new()
                .padding(self.padding)
                .child(content)
                .to_element(ctx)
        }
    }

    fn debug_name(&self) -> &'static str {
        "MarkdownViewer"
    }

    fn text_content(&self) -> Option<&str> {
        Some(&self.source)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::open_web_link_with;

    #[test]
    fn web_links_are_forwarded_to_the_browser() {
        let opened = RefCell::new(Vec::new());

        let handled = open_web_link_with("https://aimer.dev/docs", |url| {
            opened
                .borrow_mut()
                .push(url.to_owned());
            Ok::<(), ()>(())
        });

        assert!(matches!(handled, Some(Ok(()))));
        assert_eq!(opened.into_inner(), ["https://aimer.dev/docs"]);
    }

    #[test]
    fn document_anchors_are_not_forwarded_to_the_browser() {
        let opened = RefCell::new(Vec::new());

        let handled = open_web_link_with("#footnote-guide", |url| {
            opened
                .borrow_mut()
                .push(url.to_owned());
            Ok::<(), ()>(())
        });

        assert!(handled.is_none());
        assert!(
            opened
                .into_inner()
                .is_empty()
        );
    }
}
