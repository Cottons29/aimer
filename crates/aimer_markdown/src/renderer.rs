use std::rc::Rc;

use aimer_assets::{AssetImage, NetworkImage};
use aimer_color::prelude::Color;
use aimer_container::flex::row_column::{Column, Row};
use aimer_container::flex::{BoxAlignment, Expanded};
use aimer_container::{Container, Grid, GridItem, GridTrack, ScrollAxis, Scrollable, SizedBox};
use aimer_style::{
    BorderSlice, BorderStyle, BoxBorder, BoxDecoration, FontStyle, FontWeight, LayoutSpacing,
    TextAlign, TextDecoration, TextDecorationLine, TextStyle,
};
use aimer_svg::{Svg, SvgDocument, SvgStyle};
use aimer_text::{RichText, SpanStyle, Text, TextSpan};
use aimer_widget::{AnyWidget, Widget};

pub(crate) use crate::markdown_theme::MarkdownTheme;
use crate::syntax::highlight_cached;
use crate::{Alignment, Block, CaptureSpan, Document, Inline, TableRow};

const TICK_SVG_DATA: &'static [u8] = include_bytes!("../tick-checkbox-svgrepo-com.svg");

#[derive(Clone, Debug, PartialEq, Eq)]
/// Metadata passed to a [`ImageResolver`] for each image node.
pub struct MarkdownImage {
    pub source: String,
    pub alt: String,
    pub title: Option<String>,
}

/// Shared callback invoked when a rendered link is activated.
pub type LinkHandler = Rc<dyn Fn(Rc<str>)>;
/// Shared resolver that maps image metadata to a native Aimer widget.
pub type ImageResolver = Rc<dyn Fn(&MarkdownImage) -> AnyWidget>;

fn build_tick_box(ticked: bool) -> AnyWidget {
    match SvgDocument::from_svg(TICK_SVG_DATA) {
        Ok(doc) => Container::new()
            // .margin(LayoutSpacing::new().top(4))
            .width(16)
            .height(16)
            .child(Svg::new(doc).style(
                "#tick",
                SvgStyle::new().fill(if ticked {
                    Color::BLACK
                } else {
                    Color::Transparent
                }),
            ))
            .boxed(),
        Err(_) => Text::new(if ticked {
            "[✔]".to_string()
        } else {
            "[ ]".to_string()
        })
        .boxed(),
    }
}

/// Resolves network URLs with `NetworkImage` and other sources with `AssetImage`.
pub fn default_image_resolver(image: &MarkdownImage) -> AnyWidget {
    if image
        .source
        .starts_with("http://")
        || image
            .source
            .starts_with("https://")
    {
        NetworkImage::new(image.source.clone()).boxed()
    } else {
        AssetImage::new(image.source.clone()).boxed()
    }
}

pub(crate) fn inline_spans(inlines: &[Inline], theme: &MarkdownTheme) -> TextSpan {
    TextSpan::root(
        inlines
            .iter()
            .map(|inline| inline_span(inline, theme)),
    )
}

fn inline_span(inline: &Inline, theme: &MarkdownTheme) -> TextSpan {
    match inline {
        Inline::Text(text) => TextSpan::new(text.clone()),
        Inline::SoftBreak => TextSpan::new(" "),
        Inline::HardBreak => TextSpan::new("\n"),
        Inline::Emphasis(children) => TextSpan::new("")
            .style(SpanStyle::new().font_style(FontStyle::Italic))
            .children(
                children
                    .iter()
                    .map(|inline| inline_span(inline, theme)),
            ),
        Inline::Strong(children) => TextSpan::new("")
            .style(SpanStyle::new().font_weight(FontWeight::Bold))
            .children(
                children
                    .iter()
                    .map(|inline| inline_span(inline, theme)),
            ),
        Inline::Delete(children) => TextSpan::new("")
            .style(
                SpanStyle::new()
                    .text_decoration(TextDecoration::new().line(TextDecorationLine::LINE_THROUGH)),
            )
            .children(
                children
                    .iter()
                    .map(|inline| inline_span(inline, theme)),
            ),
        Inline::InlineCode(code) => TextSpan::new(code.clone()).style(theme.inline_code),
        Inline::Link { url, content, .. } => TextSpan::new("")
            .style(theme.link)
            .children(
                content
                    .iter()
                    .map(|inline| inline_span(inline, theme)),
            )
            .link(url.clone()),
        Inline::Image { alt, .. } => {
            TextSpan::new(format!("[{alt}]")).style(SpanStyle::new().font_style(FontStyle::Italic))
        }
        Inline::FootnoteReference { identifier } => TextSpan::new(format!("[{identifier}]"))
            .style(theme.link)
            .link(format!("#footnote-{identifier}")),
    }
}

pub(crate) fn render_document(
    document: &Document,
    theme: &MarkdownTheme,
    link_handler: Option<&LinkHandler>,
    image_resolver: &ImageResolver,
) -> AnyWidget {
    render_blocks(&document.blocks, theme, link_handler, image_resolver)
}

fn render_blocks(
    blocks: &[Block],
    theme: &MarkdownTheme,
    link_handler: Option<&LinkHandler>,
    image_resolver: &ImageResolver,
) -> AnyWidget {
    let children = blocks
        .iter()
        .map(|block| render_block(block, theme, link_handler, image_resolver))
        .collect::<Vec<_>>();
    Column::new()
        .horizontal_alignment(BoxAlignment::Start)
        .gaps(LayoutSpacing::all(theme.block_spacing.into()))
        .children(children)
        .boxed()
}

fn table_text_align(alignment: Alignment) -> TextAlign {
    match alignment {
        Alignment::Left | Alignment::None => TextAlign::TopLeft,
        Alignment::Center => TextAlign::TopCenter,
        Alignment::Right => TextAlign::TopRight,
    }
}

fn table_tracks(column_count: usize) -> Vec<GridTrack> {
    std::iter::repeat_n(GridTrack::Fr(1.0), column_count).collect()
}

fn render_table(
    alignments: &[Alignment],
    rows: &[TableRow],
    theme: &MarkdownTheme,
    link_handler: Option<&LinkHandler>,
) -> AnyWidget {
    let column_count = rows
        .iter()
        .map(|row| row.cells.len())
        .chain(std::iter::once(alignments.len()))
        .max()
        .unwrap_or(1)
        .max(1);
    let cells = rows
        .iter()
        .enumerate()
        .flat_map(|(row_index, row)| {
            (0..column_count).map(move |column_index| {
                let content = row
                    .cells
                    .get(column_index)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let alignment = alignments
                    .get(column_index)
                    .copied()
                    .unwrap_or(Alignment::None);
                let rich = RichText::new(inline_spans(content, theme))
                    .text_style(theme.body)
                    .text_align(table_text_align(alignment))
                    .wrapped()
                    .link_hover_color(theme.link_hover_color)
                    .selectable();
                let content: AnyWidget = match link_handler {
                    Some(handler) => {
                        let handler = (*handler).clone();
                        rich.on_link(move |target: Rc<str>| handler(target))
                            .boxed()
                    }
                    None => rich.boxed(),
                };
                let background = if row_index == 0 {
                    theme.table_header_background
                } else {
                    theme.table_cell_background
                };
                GridItem::new(
                    Container::new()
                        .padding(LayoutSpacing::all(8_u32.into()))
                        .color(background)
                        .child(content)
                        .boxed(),
                )
                .at(row_index, column_index)
            })
        })
        .collect::<Vec<_>>();
    Container::new()
        .padding(LayoutSpacing::all(1_u32.into()))
        .color(theme.rule_color)
        .child(
            Grid::new()
                .columns(table_tracks(column_count))
                .gap(1.0)
                .children(cells),
        )
        .boxed()
}

fn render_block(
    block: &Block,
    theme: &MarkdownTheme,
    link_handler: Option<&LinkHandler>,
    image_resolver: &ImageResolver,
) -> AnyWidget {
    match block {
        Block::Heading { depth, content } => rich_text(
            content,
            theme.headings[usize::from(*depth)
                .saturating_sub(1)
                .min(5)],
            theme,
            link_handler,
        ),
        Block::Paragraph(content) => {
            render_paragraph(content, theme.body, theme, link_handler, image_resolver)
        }
        Block::Blockquote(blocks) => Container::new()
            .padding(LayoutSpacing::all(8_u32.into()))
            .box_decoration(
                BoxDecoration::new()
                    .background_color(theme.quote_background)
                    .border(
                        BoxBorder::new().left(
                            BorderSlice::new()
                                .stroke(4)
                                .color(theme.rule_color)
                                .style(BorderStyle::Solid),
                        ),
                    ),
            )
            .box_child(render_blocks_with_style(
                blocks,
                theme.blockquote,
                theme,
                link_handler,
                image_resolver,
            )),
        Block::List {
            ordered,
            start,
            items,
        } => {
            let start = start.unwrap_or(1);
            let rows = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let mut is_complete = false;
                    let marker = match item.checked {
                        Some(true) => {
                            is_complete = true;
                            None
                        }
                        Some(false) => None,
                        None if *ordered => Some(format!("{}.", start + index as u32)),
                        None => Some("•".to_string()),
                    };

                    Row::new()
                        .gaps(LayoutSpacing::all(8_u32.into()))
                        .vertical_alignment(BoxAlignment::Start)
                        .children(vec![
                            match marker {
                                Some(marker) => Text::new(marker)
                                    .text_style(
                                        theme
                                            .body
                                            .font_weight(FontWeight::Bold),
                                    )
                                    .boxed(),
                                None => build_tick_box(is_complete),
                            },
                            Expanded::new().box_child(render_blocks(
                                &item.blocks,
                                theme,
                                link_handler,
                                image_resolver,
                            )),
                        ])
                        .boxed()
                })
                .collect::<Vec<_>>();
            Column::new()
                .horizontal_alignment(BoxAlignment::Start)
                .gaps(LayoutSpacing::all(6_u32.into()))
                .children(rows)
                .boxed()
        }
        Block::Code {
            value, language, ..
        } => {
            let spans = highlighted_code_spans(value, language.as_deref());
            Container::new()
                .padding(LayoutSpacing::all(12_u32.into()))
                .box_decoration(
                    BoxDecoration::new()
                        .background_color(theme.code_background)
                        .border_radius(8),
                )
                .child(
                    Scrollable::new()
                        .axis(ScrollAxis::Horizontal)
                        .vertical_scroll_bar(None)
                        .horizontal_scroll_bar(None)
                        .child(
                            RichText::new(TextSpan::root(spans))
                                .text_style(theme.code_block)
                                .selectable(),
                        ),
                )
                .boxed()
        }
        Block::ThematicBreak => SizedBox::new()
            .height(1.0)
            .color(theme.rule_color)
            .boxed(),
        Block::Table { alignments, rows } => render_table(alignments, rows, theme, link_handler),
        Block::FootnoteDefinition { identifier, blocks } => Row::new()
            .gaps(LayoutSpacing::all(6_u32.into()))
            .children(vec![
                Text::new(format!("[{identifier}]"))
                    .text_style(
                        theme
                            .body
                            .font_weight(FontWeight::Bold),
                    )
                    .boxed(),
                render_blocks(blocks, theme, link_handler, image_resolver),
            ])
            .boxed(),
    }
}

fn highlighted_code_spans(value: &str, language: Option<&str>) -> Vec<TextSpan> {
    let mut offset = 0;
    let mut spans = Vec::new();
    for capture in highlight_cached(value, language).iter() {
        let (start, end) = capture.range();
        let (start, end) = (start as usize, end as usize);
        if start < offset
            || end <= start
            || end > value.len()
            || !value.is_char_boundary(start)
            || !value.is_char_boundary(end)
        {
            continue;
        }
        if offset < start {
            spans.push(TextSpan::new(&value[offset..start]));
        }
        let mut style = SpanStyle::new().color(capture.color());
        if matches!(capture, CaptureSpan::Keyword { .. }) {
            style = style.font_weight(FontWeight::Bold);
        } else if matches!(capture, CaptureSpan::Comment { .. }) {
            style = style.font_style(FontStyle::Italic);
        }
        spans.push(TextSpan::new(&value[start..end]).style(style));
        offset = end;
    }
    if offset < value.len() {
        spans.push(TextSpan::new(&value[offset..]));
    }
    spans
}

fn render_blocks_with_style(
    blocks: &[Block],
    style: TextStyle,
    theme: &MarkdownTheme,
    link_handler: Option<&LinkHandler>,
    image_resolver: &ImageResolver,
) -> AnyWidget {
    let children = blocks
        .iter()
        .map(|block| match block {
            Block::Paragraph(inlines) => {
                render_paragraph(inlines, style, theme, link_handler, image_resolver)
            }
            _ => render_block(block, theme, link_handler, image_resolver),
        })
        .collect::<Vec<_>>();
    Column::new()
        .horizontal_alignment(BoxAlignment::Start)
        .gaps(LayoutSpacing::all(8_u32.into()))
        .box_children(children)
}

fn render_paragraph(
    inlines: &[Inline],
    style: TextStyle,
    theme: &MarkdownTheme,
    link_handler: Option<&LinkHandler>,
    image_resolver: &ImageResolver,
) -> AnyWidget {
    if !inlines
        .iter()
        .any(|inline| matches!(inline, Inline::Image { .. }))
    {
        return rich_text(inlines, style, theme, link_handler);
    }
    let mut children = Vec::new();
    let mut text = Vec::new();
    for inline in inlines {
        if let Inline::Image { url, title, alt } = inline {
            if !text.is_empty() {
                children.push(rich_text(&text, style, theme, link_handler));
                text.clear();
            }
            children.push(image_resolver(&MarkdownImage {
                source: url.clone(),
                alt: alt.clone(),
                title: title.clone(),
            }));
        } else {
            text.push(inline.clone());
        }
    }
    if !text.is_empty() {
        children.push(rich_text(&text, style, theme, link_handler));
    }
    Column::new()
        .horizontal_alignment(BoxAlignment::Start)
        .gaps(LayoutSpacing::all(8_u32.into()))
        .children(children)
        .boxed()
}

fn rich_text(
    inlines: &[Inline],
    style: TextStyle,
    theme: &MarkdownTheme,
    link_handler: Option<&LinkHandler>,
) -> AnyWidget {
    let rich = RichText::new(inline_spans(inlines, theme))
        .text_style(style)
        .wrapped()
        .link_hover_color(theme.link_hover_color)
        .selectable();
    match link_handler {
        Some(handler) => {
            let handler = (*handler).clone();
            rich.on_link(move |target: Rc<str>| handler(target))
                .boxed()
        }
        None => rich.boxed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aimer_attribute::{BoxConstraint, ResolvedSize};
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_style::{FontFamily, TextAlign, TextDecorationLine, TextStyle};
    use aimer_widget::base::{BuildContext, WindowHandle};
    use std::cell::Cell;
    use std::sync::OnceLock;

    #[cfg(not(target_arch = "wasm32"))]
    fn dummy_async_handle() -> tokio::runtime::Handle {
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        });
        let _guard = runtime.enter();
        tokio::runtime::Handle::current()
    }

    fn layout_context(width: f32, height: f32) -> BuildContext<'static> {
        let canvas = Canvas::new(Box::leak(Box::new(InnerCanvas::new())));
        let size = ResolvedSize { width, height };
        let mut ctx = BuildContext::new(
            canvas,
            size,
            1.0,
            Default::default(),
            Default::default(),
            WindowHandle::headless(
                winit::dpi::PhysicalSize::new(width as u32, height as u32),
                1.0,
            ),
            #[cfg(not(target_arch = "wasm32"))]
            dummy_async_handle(),
        );
        ctx.box_constraint = BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: width,
            max_height: height,
        };
        ctx
    }

    #[test]
    fn single_line_blockquote_uses_bounded_width_and_natural_height() {
        let resolver: ImageResolver = Rc::new(default_image_resolver);
        let quote = Document::parse("> one line").unwrap();
        let quote_widget = render_document(&quote, &MarkdownTheme::default(), None, &resolver);
        let quote_ctx = layout_context(320.0, 200.0);
        let quote_size = quote_widget
            .to_element(&quote_ctx)
            .computed_size(&quote_ctx);

        assert!(
            quote_size.height < 100.0,
            "single-line blockquote height was {}",
            quote_size.height
        );

        let document = Document::parse("> one line\n\nafter").unwrap();
        let widget = render_document(&document, &MarkdownTheme::default(), None, &resolver);
        let ctx = layout_context(320.0, 200.0);

        let size = widget
            .to_element(&ctx)
            .computed_size(&ctx);

        assert!(size.width.is_finite());
        assert!(size.height.is_finite());
        assert!(
            size.width <= 320.0,
            "blockquote document width was {}",
            size.width
        );
        assert!(
            size.height < 100.0,
            "blockquote document height was {}",
            size.height
        );
    }

    #[test]
    fn nested_blockquote_increases_the_outer_quote_height() {
        let resolver: ImageResolver = Rc::new(default_image_resolver);
        let ctx = layout_context(320.0, 200.0);
        let single_quote = Document::parse("> outer").unwrap();
        let nested_quote = Document::parse("> outer\n>> inner").unwrap();

        let single_height =
            render_document(&single_quote, &MarkdownTheme::default(), None, &resolver)
                .to_element(&ctx)
                .computed_size(&ctx)
                .height;
        let nested_height =
            render_document(&nested_quote, &MarkdownTheme::default(), None, &resolver)
                .to_element(&ctx)
                .computed_size(&ctx)
                .height;

        assert!(
            nested_height > single_height + 30.0,
            "nested blockquote height {nested_height} did not reserve space for its inner text; single quote height was {single_height}"
        );
    }

    #[test]
    fn list_content_wraps_within_the_space_remaining_after_its_marker() {
        let resolver: ImageResolver = Rc::new(default_image_resolver);
        let theme = MarkdownTheme::default();
        let ctx = layout_context(320.0, 200.0);
        let paragraph = Document::parse("Use CodeGraph to understand code safely").unwrap();
        let list = Document::parse("- Use CodeGraph to understand code safely").unwrap();

        let paragraph_size = render_document(&paragraph, &theme, None, &resolver)
            .to_element(&ctx)
            .computed_size(&ctx);
        let list_size = render_document(&list, &theme, None, &resolver)
            .to_element(&ctx)
            .computed_size(&ctx);

        assert!(list_size.width <= ctx.box_constraint.max_width);
        assert!(
            list_size.height < ctx.box_constraint.max_height,
            "a list row expanded to the viewport height: {}",
            list_size.height
        );
        assert!(
            list_size.height > paragraph_size.height,
            "list content did not wrap in its reduced width: list height {}, paragraph height {}",
            list_size.height,
            paragraph_size.height
        );
    }

    #[test]
    fn table_columns_share_the_available_width() {
        assert_eq!(
            table_tracks(2),
            vec![GridTrack::Fr(1.0), GridTrack::Fr(1.0)]
        );
    }

    #[test]
    fn default_link_hover_color_is_brighter_than_the_link_color() {
        let theme = MarkdownTheme::default();
        let link_color = theme
            .link
            .color
            .expect("default links have a color");

        assert_eq!(theme.link_hover_color, link_color.lighten(0.48));
        assert_ne!(theme.link_hover_color, link_color);
    }

    #[test]
    fn table_alignment_maps_to_rich_text_alignment() {
        assert!(matches!(
            table_text_align(Alignment::Left),
            TextAlign::TopLeft
        ));
        assert!(matches!(
            table_text_align(Alignment::Center),
            TextAlign::TopCenter
        ));
        assert!(matches!(
            table_text_align(Alignment::Right),
            TextAlign::TopRight
        ));
        assert!(matches!(
            table_text_align(Alignment::None),
            TextAlign::TopLeft
        ));
    }

    #[test]
    fn highlighted_code_spans_preserve_all_source_text() {
        for (source, language) in [
            ("fn café() { \"你好\" } // done\n", Some("rust")),
            ("anything <goes>\n", Some("unknown")),
            ("unlabelled\n", None),
        ] {
            let text = TextSpan::root(highlighted_code_spans(source, language))
                .flatten(&TextStyle::default())
                .into_iter()
                .map(|span| span.text.to_string())
                .collect::<String>();
            assert_eq!(text, source);
        }
    }

    #[test]
    fn code_blocks_scroll_horizontally_without_wrapping_long_lines() {
        let resolver: ImageResolver = Rc::new(default_image_resolver);
        let document = Document::parse(
            "```rust\nlet message = \"this line is deliberately wider than the narrow viewport\";\n```",
        )
        .unwrap();

        let narrow_ctx = layout_context(180.0, 200.0);
        let narrow_size = render_document(&document, &MarkdownTheme::default(), None, &resolver)
            .to_element(&narrow_ctx)
            .computed_size(&narrow_ctx);
        let wide_ctx = layout_context(800.0, 200.0);
        let wide_size = render_document(&document, &MarkdownTheme::default(), None, &resolver)
            .to_element(&wide_ctx)
            .computed_size(&wide_ctx);

        assert!(
            narrow_size.width <= 180.0,
            "code block width was {}",
            narrow_size.width
        );
        assert!(
            narrow_size.height < 100.0,
            "code block height should follow its content, but was {}",
            narrow_size.height
        );
        assert_eq!(narrow_size.height, wide_size.height);
    }

    #[test]
    fn maps_nested_inline_styles_links_breaks_and_footnotes() {
        let theme = MarkdownTheme::default();
        let inlines = vec![
            Inline::Text("plain ".into()),
            Inline::Emphasis(vec![Inline::Strong(vec![Inline::Text("both".into())])]),
            Inline::Text(" ".into()),
            Inline::Delete(vec![Inline::Text("gone".into())]),
            Inline::SoftBreak,
            Inline::InlineCode("code".into()),
            Inline::Link {
                url: "https://example.com".into(),
                title: None,
                content: vec![Inline::Text("link".into())],
            },
            Inline::FootnoteReference {
                identifier: "note".into(),
            },
        ];
        let resolved = inline_spans(&inlines, &theme).flatten(&TextStyle::default());

        assert_eq!(
            resolved
                .iter()
                .map(|span| span.text.as_ref())
                .collect::<String>(),
            "plain both gone codelink[note]"
        );
        let both = resolved
            .iter()
            .find(|span| span.text.as_ref() == "both")
            .unwrap();
        assert_eq!(both.style.font_style, FontStyle::Italic);
        assert_eq!(both.style.font_weight, FontWeight::Bold);
        let gone = resolved
            .iter()
            .find(|span| span.text.as_ref() == "gone")
            .unwrap();
        assert!(
            gone.style
                .text_decoration
                .line
                .contains(TextDecorationLine::LINE_THROUGH)
        );
        assert!(
            resolved
                .iter()
                .any(|span| span.link.as_deref() == Some("https://example.com"))
        );
        assert!(
            resolved
                .iter()
                .any(|span| span.link.as_deref() == Some("#footnote-note"))
        );
        assert_eq!(
            resolved
                .iter()
                .find(|span| span.text.as_ref() == "code")
                .unwrap()
                .style
                .font_family,
            FontFamily::MONOSPACE
        );
    }

    #[test]
    fn soft_breaks_flow_as_spaces_while_hard_breaks_remain_newlines() {
        let theme = MarkdownTheme::default();
        let document = Document::parse("first\nsecond\n\nthird  \nfourth").unwrap();
        let rendered_paragraphs = document
            .blocks
            .iter()
            .map(|block| {
                let Block::Paragraph(inlines) = block else {
                    panic!("expected paragraph")
                };
                inline_spans(inlines, &theme)
                    .flatten(&TextStyle::default())
                    .into_iter()
                    .map(|span| span.text.to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();

        assert_eq!(rendered_paragraphs, ["first second", "third\nfourth"]);
    }

    #[test]
    fn document_rendering_invokes_the_configured_image_resolver() {
        let document = Document::parse("before ![alt](asset.png) after").unwrap();
        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let resolver: ImageResolver = Rc::new(move |image| {
            assert_eq!(image.source, "asset.png");
            assert_eq!(image.alt, "alt");
            observed.set(observed.get() + 1);
            SizedBox::new().boxed()
        });

        let _widget = render_document(&document, &MarkdownTheme::default(), None, &resolver);
        assert_eq!(calls.get(), 1);
    }
}
