use std::rc::Rc;

use aimer_assets::{AssetImage, NetworkImage};
use aimer_color::prelude::Color;
use aimer_container::flex::BoxAlignment;
use aimer_container::flex::row_column::{Column, Row};
use aimer_container::{Container, Grid, GridItem, GridTrack, SizedBox};
use aimer_style::{
    BoxDecoration, FontStyle, FontWeight, LayoutSpacing, TextAlign, TextDecoration,
    TextDecorationLine, TextStyle,
};
use aimer_svg::{Svg, SvgDocument, SvgStyle};
use aimer_text::{RichText, SpanStyle, Text, TextSpan};
use aimer_widget::{AnyWidget, Widget};

pub(crate) use crate::markdown_theme::MarkdownTheme;
use crate::{Alignment, Block, CaptureSpan, Document, Inline, TableRow, highlight};

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
        Ok(doc) => Svg::new(doc)
            .width(16)
            .height(16)
            .style(
                "#tick",
                SvgStyle::new().fill(if ticked { Color::BLACK } else { Color::Transparent }),
            )
            .boxed(),
        Err(_) => Text::new(if ticked { "[✔]".to_string() } else { "[ ]".to_string() }).boxed(),
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
        NetworkImage::new(
            image
                .source
                .clone(),
        )
        .boxed()
    } else {
        AssetImage::new(
            image
                .source
                .clone(),
        )
        .boxed()
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
        Inline::SoftBreak | Inline::HardBreak => TextSpan::new("\n"),
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
        .gaps(LayoutSpacing::all(
            theme
                .block_spacing
                .into(),
        ))
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
        .map(|row| {
            row.cells
                .len()
        })
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
        Block::Blockquote(blocks) => Row::new()
            .gaps(LayoutSpacing::all(10_u32.into()))
            .children(vec![
                SizedBox::new()
                    .width(4.0)
                    .color(theme.rule_color)
                    .boxed(),
                Container::new()
                    .padding(LayoutSpacing::all(8_u32.into()))
                    .color(theme.quote_background)
                    .child(render_blocks_with_style(
                        blocks,
                        theme.blockquote,
                        theme,
                        link_handler,
                        image_resolver,
                    ))
                    .boxed(),
            ])
            .boxed(),
        Block::List { ordered, start, items } => {
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
                        .vertical_alignment(BoxAlignment::Center)
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
                            render_blocks(&item.blocks, theme, link_handler, image_resolver),
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
        Block::Code { value, language, .. } => {
            let spans = highlighted_code_spans(value, language.as_deref());
            Container::new()
                .padding(LayoutSpacing::all(12_u32.into()))
                .box_decoration(
                    BoxDecoration::new()
                        .background_color(theme.code_background)
                        .border_radius(8),
                )
                .child(
                    RichText::new(TextSpan::root(spans))
                        .text_style(theme.code_block)
                        .wrapped()
                        .selectable(),
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
    for capture in highlight(value, language) {
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
        .children(children)
        .boxed()
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
    use aimer_style::{FontFamily, TextAlign, TextDecorationLine, TextStyle};
    use std::cell::Cell;

    #[test]
    fn table_columns_share_the_available_width() {
        assert_eq!(table_tracks(2), vec![GridTrack::Fr(1.0), GridTrack::Fr(1.0)]);
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
        assert!(matches!(table_text_align(Alignment::Left), TextAlign::TopLeft));
        assert!(matches!(table_text_align(Alignment::Center), TextAlign::TopCenter));
        assert!(matches!(table_text_align(Alignment::Right), TextAlign::TopRight));
        assert!(matches!(table_text_align(Alignment::None), TextAlign::TopLeft));
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
                .map(|span| {
                    span.text
                        .to_string()
                })
                .collect::<String>();
            assert_eq!(text, source);
        }
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
            Inline::FootnoteReference { identifier: "note".into() },
        ];
        let resolved = inline_spans(&inlines, &theme).flatten(&TextStyle::default());

        assert_eq!(
            resolved
                .iter()
                .map(|span| span
                    .text
                    .as_ref())
                .collect::<String>(),
            "plain both gone\ncodelink[note]"
        );
        let both = resolved
            .iter()
            .find(|span| {
                span.text
                    .as_ref()
                    == "both"
            })
            .unwrap();
        assert_eq!(
            both.style
                .font_style,
            FontStyle::Italic
        );
        assert_eq!(
            both.style
                .font_weight,
            FontWeight::Bold
        );
        let gone = resolved
            .iter()
            .find(|span| {
                span.text
                    .as_ref()
                    == "gone"
            })
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
                .any(|span| span
                    .link
                    .as_deref()
                    == Some("https://example.com"))
        );
        assert!(
            resolved
                .iter()
                .any(|span| span
                    .link
                    .as_deref()
                    == Some("#footnote-note"))
        );
        assert_eq!(
            resolved
                .iter()
                .find(|span| span
                    .text
                    .as_ref()
                    == "code")
                .unwrap()
                .style
                .font_family,
            FontFamily::MONOSPACE
        );
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
