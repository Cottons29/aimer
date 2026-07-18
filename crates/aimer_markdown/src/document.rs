use std::fmt::{Display, Formatter};

use markdown::mdast::{AlignKind, Node};
use markdown::{Constructs, ParseOptions};

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading { depth: u8, content: Vec<Inline> },
    Paragraph(Vec<Inline>),
    Blockquote(Vec<Block>),
    List { ordered: bool, start: Option<u32>, items: Vec<ListItem> },
    Code { value: String, language: Option<String>, meta: Option<String> },
    ThematicBreak,
    Table { alignments: Vec<Alignment>, rows: Vec<TableRow> },
    FootnoteDefinition { identifier: String, blocks: Vec<Block> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableRow {
    pub cells: Vec<Vec<Inline>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    SoftBreak,
    HardBreak,
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Delete(Vec<Inline>),
    InlineCode(String),
    Link { url: String, title: Option<String>, content: Vec<Inline> },
    Image { url: String, title: Option<String>, alt: String },
    FootnoteReference { identifier: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownError {
    message: String,
}

impl MarkdownError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for MarkdownError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for MarkdownError {}

impl Document {
    pub fn parse(source: &str) -> Result<Self, MarkdownError> {
        let options = ParseOptions {
            constructs: Constructs::gfm(),
            gfm_strikethrough_single_tilde: false,
            ..ParseOptions::default()
        };
        let root = markdown::to_mdast(source, &options)
            .map_err(|error| MarkdownError::new(error.to_string()))?;
        let Node::Root(root) = root else {
            return Err(MarkdownError::new("Markdown parser did not produce a document root"));
        };
        Ok(Self { blocks: convert_blocks(&root.children)? })
    }
}

fn convert_blocks(nodes: &[Node]) -> Result<Vec<Block>, MarkdownError> {
    nodes
        .iter()
        .map(convert_block)
        .collect()
}

fn convert_block(node: &Node) -> Result<Block, MarkdownError> {
    match node {
        Node::Heading(heading) => Ok(Block::Heading {
            depth: heading.depth,
            content: convert_inlines(&heading.children)?,
        }),
        Node::Paragraph(paragraph) => Ok(Block::Paragraph(convert_inlines(&paragraph.children)?)),
        Node::Blockquote(quote) => Ok(Block::Blockquote(convert_blocks(&quote.children)?)),
        Node::List(list) => {
            let items = list
                .children
                .iter()
                .map(|node| {
                    let Node::ListItem(item) = node else {
                        return Err(MarkdownError::new(
                            "Markdown list contains a non-list-item node",
                        ));
                    };
                    Ok(ListItem { checked: item.checked, blocks: convert_blocks(&item.children)? })
                })
                .collect::<Result<_, MarkdownError>>()?;
            Ok(Block::List { ordered: list.ordered, start: list.start, items })
        }
        Node::Code(code) => Ok(Block::Code {
            value: code
                .value
                .clone(),
            language: code
                .lang
                .clone(),
            meta: code
                .meta
                .clone(),
        }),
        Node::ThematicBreak(_) => Ok(Block::ThematicBreak),
        Node::Table(table) => {
            let rows = table
                .children
                .iter()
                .map(|node| {
                    let Node::TableRow(row) = node else {
                        return Err(MarkdownError::new("Markdown table contains a non-row node"));
                    };
                    let cells = row
                        .children
                        .iter()
                        .map(|node| {
                            let Node::TableCell(cell) = node else {
                                return Err(MarkdownError::new(
                                    "Markdown table row contains a non-cell node",
                                ));
                            };
                            convert_inlines(&cell.children)
                        })
                        .collect::<Result<_, MarkdownError>>()?;
                    Ok(TableRow { cells })
                })
                .collect::<Result<_, MarkdownError>>()?;
            let alignments = table
                .align
                .iter()
                .copied()
                .map(Alignment::from)
                .collect();
            Ok(Block::Table { alignments, rows })
        }
        Node::FootnoteDefinition(footnote) => Ok(Block::FootnoteDefinition {
            identifier: footnote
                .identifier
                .clone(),
            blocks: convert_blocks(&footnote.children)?,
        }),
        Node::Html(_) => Err(MarkdownError::new("Raw HTML is not supported in MarkdownViewer")),
        other => Err(MarkdownError::new(format!(
            "Unsupported Markdown block node: {}",
            node_name(other)
        ))),
    }
}

fn convert_inlines(nodes: &[Node]) -> Result<Vec<Inline>, MarkdownError> {
    let mut result = Vec::new();
    for node in nodes {
        match node {
            Node::Text(text) => push_text_with_soft_breaks(&mut result, &text.value),
            Node::Break(_) => result.push(Inline::HardBreak),
            Node::Emphasis(emphasis) => {
                result.push(Inline::Emphasis(convert_inlines(&emphasis.children)?))
            }
            Node::Strong(strong) => result.push(Inline::Strong(convert_inlines(&strong.children)?)),
            Node::Delete(delete) => result.push(Inline::Delete(convert_inlines(&delete.children)?)),
            Node::InlineCode(code) => result.push(Inline::InlineCode(
                code.value
                    .clone(),
            )),
            Node::Link(link) => result.push(Inline::Link {
                url: link
                    .url
                    .clone(),
                title: link
                    .title
                    .clone(),
                content: convert_inlines(&link.children)?,
            }),
            Node::Image(image) => result.push(Inline::Image {
                url: image
                    .url
                    .clone(),
                title: image
                    .title
                    .clone(),
                alt: image
                    .alt
                    .clone(),
            }),
            Node::FootnoteReference(reference) => result.push(Inline::FootnoteReference {
                identifier: reference
                    .identifier
                    .clone(),
            }),
            Node::Html(_) => {
                return Err(MarkdownError::new("Raw HTML is not supported in MarkdownViewer"));
            }
            other => {
                return Err(MarkdownError::new(format!(
                    "Unsupported Markdown inline node: {}",
                    node_name(other)
                )));
            }
        }
    }
    Ok(result)
}

fn push_text_with_soft_breaks(result: &mut Vec<Inline>, value: &str) {
    let mut parts = value
        .split('\n')
        .peekable();
    while let Some(part) = parts.next() {
        push_extended_image_text(result, part);
        if parts
            .peek()
            .is_some()
        {
            result.push(Inline::SoftBreak);
        }
    }
}

fn push_extended_image_text(result: &mut Vec<Inline>, value: &str) {
    let mut remaining = value;
    while let Some(start) = remaining.find("![") {
        let Some(alt_end_relative) = remaining[start + 2..].find("](") else {
            break;
        };
        let alt_end = start + 2 + alt_end_relative;
        let destination_start = alt_end + 2;
        let Some(destination_end_relative) = remaining[destination_start..].find(')') else {
            break;
        };
        let destination_end = destination_start + destination_end_relative;
        let destination = remaining[destination_start..destination_end].trim();
        if !destination.contains(' ') || destination.is_empty() {
            break;
        }
        if start > 0 {
            result.push(Inline::Text(remaining[..start].to_string()));
        }
        result.push(Inline::Image {
            url: destination.to_string(),
            title: None,
            alt: remaining[start + 2..alt_end].to_string(),
        });
        remaining = &remaining[destination_end + 1..];
    }
    if !remaining.is_empty() {
        result.push(Inline::Text(remaining.to_string()));
    }
}

fn node_name(node: &Node) -> &'static str {
    match node {
        Node::Root(_) => "root",
        Node::Blockquote(_) => "blockquote",
        Node::FootnoteDefinition(_) => "footnote definition",
        Node::MdxJsxFlowElement(_) => "MDX flow element",
        Node::List(_) => "list",
        Node::MdxjsEsm(_) => "MDX ESM",
        Node::Toml(_) => "TOML",
        Node::Yaml(_) => "YAML",
        Node::Break(_) => "break",
        Node::InlineCode(_) => "inline code",
        Node::InlineMath(_) => "inline math",
        Node::Delete(_) => "delete",
        Node::Emphasis(_) => "emphasis",
        Node::MdxTextExpression(_) => "MDX text expression",
        Node::FootnoteReference(_) => "footnote reference",
        Node::Html(_) => "HTML",
        Node::Image(_) => "image",
        Node::ImageReference(_) => "image reference",
        Node::MdxJsxTextElement(_) => "MDX text element",
        Node::Link(_) => "link",
        Node::LinkReference(_) => "link reference",
        Node::Strong(_) => "strong",
        Node::Text(_) => "text",
        Node::Code(_) => "code",
        Node::Math(_) => "math",
        Node::MdxFlowExpression(_) => "MDX flow expression",
        Node::Heading(_) => "heading",
        Node::Table(_) => "table",
        Node::ThematicBreak(_) => "thematic break",
        Node::TableRow(_) => "table row",
        Node::TableCell(_) => "table cell",
        Node::ListItem(_) => "list item",
        Node::Definition(_) => "definition",
        Node::Paragraph(_) => "paragraph",
    }
}

impl From<AlignKind> for Alignment {
    fn from(value: AlignKind) -> Self {
        match value {
            AlignKind::None => Self::None,
            AlignKind::Left => Self::Left,
            AlignKind::Center => Self::Center,
            AlignKind::Right => Self::Right,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Document {
        Document::parse(source).expect("fixture should parse")
    }

    fn inline_text(inlines: &[Inline]) -> String {
        inlines
            .iter()
            .map(|inline| match inline {
                Inline::Text(value) | Inline::InlineCode(value) => value.clone(),
                Inline::SoftBreak | Inline::HardBreak => "\n".to_string(),
                Inline::Emphasis(children)
                | Inline::Strong(children)
                | Inline::Delete(children) => inline_text(children),
                Inline::Link { content, .. } => inline_text(content),
                Inline::Image { alt, .. } => alt.clone(),
                Inline::FootnoteReference { identifier } => identifier.clone(),
            })
            .collect()
    }

    #[test]
    fn parses_headings_and_all_emphasis_forms() {
        let document = parse(
            "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6\n\n*italic* **bold** ***both*** ~~gone~~",
        );
        for (index, block) in document.blocks[..6]
            .iter()
            .enumerate()
        {
            assert!(matches!(block, Block::Heading { depth, .. } if *depth == index as u8 + 1));
        }
        let Block::Paragraph(inlines) = &document.blocks[6] else { panic!("expected paragraph") };
        assert!(matches!(&inlines[0], Inline::Emphasis(_)));
        assert!(
            inlines
                .iter()
                .any(|inline| matches!(inline, Inline::Strong(_)))
        );
        assert!(
            inlines
                .iter()
                .any(|inline| matches!(inline, Inline::Delete(_)))
        );
        assert!(
            matches!(inlines.iter().find(|inline| inline_text(std::slice::from_ref(inline)) == "both"), Some(Inline::Emphasis(children)) if matches!(children.as_slice(), [Inline::Strong(_)]))
        );
    }

    #[test]
    fn parses_lists_tasks_and_nested_blocks() {
        let document = parse("- plain\n- [x] done\n- [ ] todo\n  1. nested\n  2. second");
        let Block::List { ordered, start, items } = &document.blocks[0] else {
            panic!("expected list")
        };
        assert!(!ordered);
        assert_eq!(*start, None);
        assert_eq!(
            items
                .iter()
                .map(|item| item.checked)
                .collect::<Vec<_>>(),
            [None, Some(true), Some(false)]
        );
        assert!(matches!(
            items[2]
                .blocks
                .last(),
            Some(Block::List { ordered: true, start: Some(1), .. })
        ));
    }

    #[test]
    fn parses_links_images_autolinks_and_footnotes() {
        let document = parse(
            "[plain](https://example.com) [titled](https://example.com \"title\") <https://a.test> ![alt](image.jpg \"caption\") ref[^One].\n\n[^One]: Footnote *text*.",
        );
        let Block::Paragraph(inlines) = &document.blocks[0] else { panic!("expected paragraph") };
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Link { url, title: None, .. } if url == "https://example.com")));
        assert!(inlines.iter().any(
            |inline| matches!(inline, Inline::Link { title: Some(title), .. } if title == "title")
        ));
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Image { url, title: Some(title), alt } if url == "image.jpg" && title == "caption" && alt == "alt")));
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::FootnoteReference { identifier } if identifier == "one")));
        assert!(
            matches!(&document.blocks[1], Block::FootnoteDefinition { identifier, .. } if identifier == "one")
        );
    }

    #[test]
    fn accepts_the_reference_image_destination_with_spaces() {
        let document = parse("![alt text](image line here)");
        assert!(matches!(
            &document.blocks[0],
            Block::Paragraph(inlines)
                if matches!(inlines.as_slice(), [Inline::Image { url, alt, .. }] if url == "image line here" && alt == "alt text")
        ));
    }

    #[test]
    fn parses_quotes_code_rules_breaks_and_escapes() {
        let document = parse(
            "> outer\n>> inner\n\n`inline`\n\n```python title=demo\nprint('ok')\n```\n\n    indented\n\n---\n\nline one  \nline two\nsoft\nline\n\n\\*literal\\*",
        );
        assert!(
            matches!(&document.blocks[0], Block::Blockquote(children) if matches!(children.get(1), Some(Block::Blockquote(_))))
        );
        assert!(
            matches!(&document.blocks[1], Block::Paragraph(inlines) if matches!(inlines.as_slice(), [Inline::InlineCode(value)] if value == "inline"))
        );
        assert!(
            matches!(&document.blocks[2], Block::Code { language: Some(language), meta: Some(meta), value } if language == "python" && meta == "title=demo" && value == "print('ok')")
        );
        assert!(
            matches!(&document.blocks[3], Block::Code { language: None, value, .. } if value == "indented")
        );
        assert!(matches!(&document.blocks[4], Block::ThematicBreak));
        assert!(
            matches!(&document.blocks[5], Block::Paragraph(inlines) if inlines.iter().any(|inline| matches!(inline, Inline::HardBreak)) && inlines.iter().any(|inline| matches!(inline, Inline::SoftBreak)))
        );
        assert!(
            matches!(&document.blocks[6], Block::Paragraph(inlines) if inline_text(inlines) == "*literal*")
        );
    }

    #[test]
    fn parses_table_alignment_and_inline_cell_content() {
        let document = parse(
            "| Left | Center | Right | None |\n|:-----|:------:|------:|------|\n| *a* | **b** | `c` | d |",
        );
        let Block::Table { alignments, rows } = &document.blocks[0] else {
            panic!("expected table")
        };
        assert_eq!(
            alignments,
            &[Alignment::Left, Alignment::Center, Alignment::Right, Alignment::None]
        );
        assert_eq!(rows.len(), 2);
        assert!(matches!(rows[1].cells[0].as_slice(), [Inline::Emphasis(_)]));
        assert!(matches!(rows[1].cells[1].as_slice(), [Inline::Strong(_)]));
        assert!(matches!(rows[1].cells[2].as_slice(), [Inline::InlineCode(value)] if value == "c"));
    }

    #[test]
    fn rejects_raw_html_without_silently_dropping_it() {
        let error = Document::parse("<script>alert('no')</script>")
            .expect_err("raw HTML is intentionally unsupported");
        assert!(
            error
                .to_string()
                .contains("HTML")
        );
    }
}
