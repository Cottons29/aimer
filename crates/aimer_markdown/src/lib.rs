mod cache;
mod document;
mod markdown_theme;
mod renderer;
mod syntax;

// Arborium's debug Tree-sitter runtime references C's `stderr`, which is not
// provided by the `wasm32-unknown-unknown` target. Its optional diagnostics
// accept a null stream, so provide the missing WASM-side storage here.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
static mut stderr: usize = 0;

use std::cell::RefCell;
use std::rc::Rc;

use aimer_container::{Container, ScrollAxis, Scrollable};
use aimer_style::LayoutSpacing;
use aimer_widget::base::BuildContext;
use aimer_widget::{AnyWidget, Element, Widget};

pub use document::{Alignment, Block, Document, Inline, ListItem, MarkdownError, TableRow};
pub use markdown_theme::MarkdownTheme;
pub use renderer::{ImageResolver, LinkHandler, MarkdownImage, default_image_resolver};
pub use syntax::{CaptureSpan, highlight};

use cache::LruCache;

const DOCUMENT_CACHE_CAPACITY: usize = 16;

thread_local! {
    static DOCUMENT_CACHE: RefCell<DocumentCache> = RefCell::new(DocumentCache::new(DOCUMENT_CACHE_CAPACITY));
}

struct DocumentCache {
    entries: LruCache<Rc<str>, Rc<Result<Document, MarkdownError>>>,
}

impl DocumentCache {
    fn new(capacity: usize) -> Self {
        Self { entries: LruCache::new(capacity) }
    }

    fn parse(&mut self, source: Rc<str>) -> Rc<Result<Document, MarkdownError>> {
        self.entries
            .get_or_insert_with(source, |source| Rc::new(Document::parse(source)))
    }
}

fn parse_document(source: Rc<str>) -> Rc<Result<Document, MarkdownError>> {
    DOCUMENT_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .parse(source)
    })
}

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
        let document = parse_document(self.source.clone());
        let content = match document.as_ref() {
            Ok(document) => renderer::render_document(
                document,
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
    use std::rc::Rc;

    use super::{DocumentCache, open_web_link_with};

    #[test]
    fn document_cache_reuses_unchanged_markdown() {
        let mut cache = DocumentCache::new(2);
        let source: Rc<str> = Rc::from("# Cached");

        let first = cache.parse(source.clone());
        let second = cache.parse(Rc::from("# Cached"));

        assert!(Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn document_cache_parses_updated_markdown() {
        let mut cache = DocumentCache::new(2);

        let first = cache.parse(Rc::from("# Before"));
        let second = cache.parse(Rc::from("# After"));

        assert!(!Rc::ptr_eq(&first, &second));
        assert_ne!(first.as_ref(), second.as_ref());
    }

    #[test]
    fn document_cache_reuses_parse_errors() {
        let mut cache = DocumentCache::new(2);

        let first = cache.parse(Rc::from("<div>unsupported</div>"));
        let second = cache.parse(Rc::from("<div>unsupported</div>"));

        assert!(first.is_err());
        assert!(Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn document_cache_evicts_the_least_recently_used_source() {
        let mut cache = DocumentCache::new(2);
        let first = cache.parse(Rc::from("First"));
        cache.parse(Rc::from("Second"));
        cache.parse(Rc::from("Third"));

        let reparsed = cache.parse(Rc::from("First"));

        assert!(!Rc::ptr_eq(&first, &reparsed));
    }

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
