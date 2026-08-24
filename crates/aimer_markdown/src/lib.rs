mod cache;
mod custom;
mod document;
mod markdown_theme;
mod renderer;
mod syntax;

// Arborium's debug Tree-sitter runtime references C's `stderr`, which is not
// provided by the `wasm32-unknown-unknown` target. Its optional diagnostics
// accept a null stream, so provide the missing WASM-side storage here. The
// default `synoptic` highlighter is pure Rust and needs none of this.
#[cfg(all(target_arch = "wasm32", feature = "arborium"))]
#[unsafe(no_mangle)]
static mut stderr: usize = 0;

use std::cell::RefCell;
use std::rc::Rc;

use aimer_container::Container;
use aimer_scroll::{ScrollAxis, Scrollable};
use aimer_style::LayoutSpacing;
use aimer_widget::base::BuildContext;
use aimer_widget::{AnyElement, AnyWidget, Key, Widget};
use cache::LruCache;
pub use document::{Alignment, Block, Document, Inline, ListItem, MarkdownError, TableRow};
pub use custom::{
    BlockRule, BlockSyntax, CustomBlock, CustomBlockBuilder, CustomBlockData, CustomBlockInput,
    CustomInline, CustomInlineBuilder, CustomInlineData, InlineRule, InlineSyntax,
};
pub use markdown_theme::MarkdownTheme;
pub use renderer::{ImageResolver, LinkHandler, MarkdownImage, default_image_resolver};
pub use syntax::{CaptureSpan, highlight};

const DOCUMENT_CACHE_CAPACITY: usize = 16;

thread_local! {
    static DOCUMENT_CACHE: RefCell<DocumentCache> = RefCell::new(DocumentCache::new(DOCUMENT_CACHE_CAPACITY));
}

struct DocumentCache {
    entries: LruCache<Rc<str>, Rc<Result<Document, MarkdownError>>>,
}

impl DocumentCache {
    fn new(capacity: usize) -> Self {
        Self {
            entries: LruCache::new(capacity),
        }
    }

    fn parse(&mut self, source: Rc<str>) -> Rc<Result<Document, MarkdownError>> {
        self.entries
            .get_or_insert_with(source, |source| Rc::new(Document::parse(source)))
    }
}

fn parse_document(source: Rc<str>) -> Rc<Result<Document, MarkdownError>> {
    DOCUMENT_CACHE.with(|cache| cache.borrow_mut().parse(source))
}

#[cfg(not(aimer_portable_guest))]
fn open_web_link_with<E>(
    target: &str,
    opener: impl FnOnce(&str) -> Result<(), E>,
) -> Option<Result<(), E>> {
    if !target.starts_with("https://") && !target.starts_with("http://") {
        return None;
    }
    Some(opener(target))
}

#[cfg(not(aimer_portable_guest))]
fn open_web_link(target: Rc<str>) {
    if let Some(Err(error)) = open_web_link_with(&target, webbrowser::open) {
        eprintln!("Failed to open Markdown link '{target}': {error}");
    }
}

#[cfg(aimer_portable_guest)]
fn open_web_link(_target: Rc<str>) {}

/// A scrollable Markdown document rendered with native Aimer widgets.
///
/// Create an empty viewer with [`MarkdownViewer::new`], then provide source
/// with [`MarkdownViewer::markdown`]. Fenced code blocks display their language
/// in a header and provide a copy control that writes the complete code source
/// to the platform clipboard. Unlabelled fences keep the header without showing
/// a language name.
#[derive(Clone, aimer_widget::PortableWidget)]
#[portable_widget(id = "aimer_markdown::MarkdownViewer", schema_only)]
pub struct MarkdownViewer {
    #[portable_skip]
    source: Rc<str>,
    #[portable_skip]
    theme: MarkdownTheme,
    #[portable_skip]
    link_handler: Option<LinkHandler>,
    #[portable_skip]
    image_resolver: ImageResolver,
    #[portable_skip]
    custom_blocks: Vec<(BlockRule, CustomBlockBuilder)>,
    #[portable_skip]
    custom_inlines: Vec<(InlineRule, CustomInlineBuilder)>,
    #[portable_skip]
    typed_custom_blocks: Vec<(BlockRule, custom::TypedCustomBlockBuilder)>,
    #[portable_skip]
    typed_custom_inlines: Vec<(InlineRule, custom::TypedCustomInlineBuilder)>,
    #[portable_skip]
    padding: LayoutSpacing,
    scrollable: bool,
    #[portable_skip]
    key: Key,
}

impl Default for MarkdownViewer {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownViewer {
    /// Creates an empty viewer with the default theme and image resolver.
    ///
    /// Activated HTTP and HTTPS links open in the system web browser by
    /// default.
    pub fn new() -> Self {
        Self {
            source: Rc::from(""),
            theme: MarkdownTheme::default(),
            link_handler: Some(Rc::new(open_web_link)),
            image_resolver: Rc::new(default_image_resolver),
            custom_blocks: Vec::new(),
            custom_inlines: Vec::new(),
            typed_custom_blocks: Vec::new(),
            typed_custom_inlines: Vec::new(),
            padding: Default::default(),
            key: Key::unique(),
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

    /// Registers a paired custom block and its widget builder.
    #[inline]
    pub fn custom_block(
        mut self,
        rule: impl Into<BlockRule>,
        builder: impl Fn(&CustomBlockData) -> AnyWidget + 'static,
    ) -> Self {
        self.custom_blocks.push((rule.into(), Rc::new(builder)));
        self
    }

    /// Registers a paired custom inline value and its widget builder.
    #[inline]
    pub fn custom_inline(
        mut self,
        rule: impl Into<InlineRule>,
        builder: impl Fn(&CustomInlineData) -> AnyWidget + 'static,
    ) -> Self {
        self.custom_inlines.push((rule.into(), Rc::new(builder)));
        self
    }

    /// Registers a strongly typed custom block.
    #[inline]
    pub fn typed_block<T: CustomBlock>(mut self) -> Self {
        let rule = BlockRule::new(
            T::NAME,
            BlockSyntax::Paired {
                opening: T::OPENING,
                closing: T::CLOSING,
            },
        );
        let builder = Rc::new(|data: &CustomBlockData, ctx: &BuildContext| {
            match T::parse(CustomBlockInput {
                raw: &data.text,
                content: &data.content,
            }) {
                Ok(props) => T::build(&props, ctx),
                Err(error) => aimer_text::Text::new(format!(
                    "custom block '{}': {error}",
                    T::NAME
                ))
                .boxed(),
            }
        });
        self.typed_custom_blocks.push((rule, builder));
        self
    }

    /// Registers a strongly typed custom inline element.
    #[inline]
    pub fn typed_inline<T: CustomInline>(mut self) -> Self {
        let rule = InlineRule::new(
            T::NAME,
            InlineSyntax::Paired {
                opening: T::OPENING,
                closing: T::CLOSING,
            },
        );
        let builder = Rc::new(|data: &CustomInlineData, ctx: &BuildContext| {
            match T::parse(&data.text) {
                Ok(props) => T::build(&props, ctx),
                Err(error) => aimer_text::Text::new(format!(
                    "custom inline '{}': {error}",
                    T::NAME
                ))
                .boxed(),
            }
        });
        self.typed_custom_inlines.push((rule, builder));
        self
    }

    /// Add a key for widget
    pub fn key(mut self, key: Key) -> Self {
        self.key = key;
        self
    }
}

impl Widget for MarkdownViewer {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let block_rules = self
            .custom_blocks
            .iter()
            .map(|(rule, _)| rule.clone())
            .collect::<Vec<_>>();
        let inline_rules = self
            .custom_inlines
            .iter()
            .map(|(rule, _)| rule.clone())
            .collect::<Vec<_>>();
        let typed_block_rules = self
            .typed_custom_blocks
            .iter()
            .map(|(rule, _)| rule.clone())
            .collect::<Vec<_>>();
        let typed_inline_rules = self
            .typed_custom_inlines
            .iter()
            .map(|(rule, _)| rule.clone())
            .collect::<Vec<_>>();
        let block_rules = block_rules
            .into_iter()
            .chain(typed_block_rules)
            .collect::<Vec<_>>();
        let inline_rules = inline_rules
            .into_iter()
            .chain(typed_inline_rules)
            .collect::<Vec<_>>();
        let document = if block_rules.is_empty() && inline_rules.is_empty() {
            parse_document(self.source.clone())
        } else {
            Rc::new(Document::parse_with_rules(
                &self.source,
                &block_rules,
                &inline_rules,
            ))
        };
        let content = match document.as_ref() {
            Ok(document) => renderer::render_document_with_context(
                document,
                &self.theme,
                self.link_handler.as_ref(),
                &self.image_resolver,
                &self.custom_blocks,
                &self.custom_inlines,
                &self.typed_custom_blocks,
                &self.typed_custom_inlines,
                Some(ctx),
            ),
            Err(error) => aimer_text::Text::new(error.to_string())
                .text_style(self.theme.body)
                .boxed(),
        };

        if self.scrollable {
            Scrollable::new()
                .key(self.key.clone())
                .axis(ScrollAxis::Vertical)
                .child(Container::new().padding(self.padding).child(content))
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

    use super::*;

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
            opened.borrow_mut().push(url.to_owned());
            Ok::<(), ()>(())
        });

        assert!(matches!(handled, Some(Ok(()))));
        assert_eq!(opened.into_inner(), ["https://aimer.dev/docs"]);
    }

    #[test]
    fn document_anchors_are_not_forwarded_to_the_browser() {
        let opened = RefCell::new(Vec::new());

        let handled = open_web_link_with("#footnote-guide", |url| {
            opened.borrow_mut().push(url.to_owned());
            Ok::<(), ()>(())
        });

        assert!(handled.is_none());
        assert!(opened.into_inner().is_empty());
    }

    #[test]
    fn viewer_registers_typed_custom_rules() {
        struct Alert;

        impl CustomBlock for Alert {
            const NAME: &'static str = "alert";
            const OPENING: &'static str = ":::alert";

            type Props = String;

            fn parse(input: CustomBlockInput<'_>) -> Result<Self::Props, MarkdownError> {
                Ok(input.raw.to_owned())
            }

            fn build(_props: &Self::Props, _ctx: &BuildContext) -> AnyWidget {
                aimer_text::Text::new("alert").boxed()
            }
        }

        struct Mention;

        impl CustomInline for Mention {
            const NAME: &'static str = "mention";
            const OPENING: &'static str = "@{";
            const CLOSING: &'static str = "}";

            type Props = String;

            fn parse(raw: &str) -> Result<Self::Props, MarkdownError> {
                Ok(raw.to_owned())
            }

            fn build(_props: &Self::Props, _ctx: &BuildContext) -> AnyWidget {
                aimer_text::Text::new("mention").boxed()
            }
        }

        let _viewer = MarkdownViewer::new()
            .typed_block::<Alert>()
            .typed_inline::<Mention>();
    }
}
