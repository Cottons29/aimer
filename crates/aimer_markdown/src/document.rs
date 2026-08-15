use std::fmt::{Display, Formatter};

use aimer_utils::error;
use markdown::mdast::{AlignKind, Node};
use markdown::{Constructs, ParseOptions};

use crate::custom::{BlockRule, CustomBlockData, CustomInlineData, InlineRule};

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading {
        depth: u8,
        content: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
    Blockquote(Vec<Block>),
    List {
        ordered: bool,
        start: Option<u32>,
        items: Vec<ListItem>,
    },
    Code {
        value: String,
        language: Option<String>,
        meta: Option<String>,
    },
    ThematicBreak,
    Table {
        alignments: Vec<Alignment>,
        rows: Vec<TableRow>,
    },
    FootnoteDefinition {
        identifier: String,
        blocks: Vec<Block>,
    },
    Custom(CustomBlockData),
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
    Code(String),
    Link {
        url: String,
        title: Option<String>,
        content: Vec<Inline>,
    },
    Image {
        url: String,
        title: Option<String>,
        alt: String,
    },
    FootnoteReference {
        identifier: String,
    },
    Custom(CustomInlineData),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownError {
    message: String,
}

impl MarkdownError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
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

struct PreparedSource {
    source: String,
    blocks: Vec<(String, CustomBlockData)>,
}

fn validate_rules(
    block_rules: &[BlockRule],
    inline_rules: &[InlineRule],
) -> Result<(), MarkdownError> {
    for (index, rule) in block_rules.iter().enumerate() {
        let (opening, closing) = rule.delimiters();
        if opening.is_empty() || closing.is_empty() || opening == closing {
            return Err(MarkdownError::new(format!(
                "Custom block rule '{}' has invalid delimiters",
                rule.name()
            )));
        }
        if block_rules[..index]
            .iter()
            .any(|other| other.name() == rule.name() || other.delimiters() == (opening, closing))
        {
            return Err(MarkdownError::new(format!(
                "Duplicate custom block rule '{}'",
                rule.name()
            )));
        }
    }
    for (index, rule) in inline_rules.iter().enumerate() {
        let (opening, closing) = rule.delimiters();
        if opening.is_empty() || closing.is_empty() || opening == closing {
            return Err(MarkdownError::new(format!(
                "Custom inline rule '{}' has invalid delimiters",
                rule.name()
            )));
        }
        if inline_rules[..index]
            .iter()
            .any(|other| other.name() == rule.name() || other.delimiters() == (opening, closing))
        {
            return Err(MarkdownError::new(format!(
                "Duplicate custom inline rule '{}'",
                rule.name()
            )));
        }
    }
    Ok(())
}

fn prepare_source(
    source: &str,
    block_rules: &[BlockRule],
    inline_rules: &[InlineRule],
) -> Result<PreparedSource, MarkdownError> {
    let mut prepared = String::with_capacity(source.len());
    let mut blocks = Vec::new();
    let mut lines = source.split_inclusive('\n').peekable();
    let mut token_index = 0;

    while let Some(line) = lines.next() {
        let line_content = line.trim_matches(['\r', '\n']);
        let Some(rule) = block_rules
            .iter()
            .find(|rule| rule.delimiters().0 == line_content.trim())
        else {
            prepared.push_str(line);
            continue;
        };

        let (_, closing) = rule.delimiters();
        let mut body = String::new();
        let mut nesting = 1_usize;
        let mut closed = false;
        while let Some(inner_line) = lines.next() {
            let inner_content = inner_line.trim_matches(['\r', '\n']);
            if inner_content.trim() == rule.delimiters().0 {
                nesting += 1;
                body.push_str(inner_line);
            } else if inner_content.trim() == closing {
                nesting -= 1;
                if nesting == 0 {
                    closed = true;
                    break;
                }
                body.push_str(inner_line);
            } else {
                body.push_str(inner_line);
            }
        }
        if !closed {
            return Err(MarkdownError::new(format!(
                "Unclosed custom block '{}'",
                rule.name()
            )));
        }

        let body = body.trim_end_matches(['\r', '\n']);
        let content = Document::parse_with_rules(body, block_rules, inline_rules)?;
        let token = loop {
            let token = format!("AIMER_CUSTOM_BLOCK_{token_index}");
            token_index += 1;
            if !source.contains(&token) {
                break token;
            }
        };
        prepared.push_str(&token);
        prepared.push('\n');
        blocks.push((token, CustomBlockData {
            name: rule.name().to_string(),
            text: body.to_string(),
            content,
        }));
    }

    Ok(PreparedSource { source: prepared, blocks })
}

fn replace_custom_blocks(blocks: &mut [Block], captures: &[(String, CustomBlockData)]) {
    for block in blocks {
        match block {
            Block::Paragraph(inlines) if inlines.len() == 1 => {
                let Some(Inline::Text(token)) = inlines.first() else {
                    continue;
                };
                if let Some((_, data)) = captures.iter().find(|(candidate, _)| candidate == token) {
                    *block = Block::Custom(data.clone());
                }
            }
            Block::Blockquote(children) => replace_custom_blocks(children, captures),
            Block::List { items, .. } => items
                .iter_mut()
                .for_each(|item| replace_custom_blocks(&mut item.blocks, captures)),
            Block::FootnoteDefinition { blocks, .. } => replace_custom_blocks(blocks, captures),
            _ => {}
        }
    }
}

fn split_custom_inlines(
    result: &mut Vec<Inline>,
    mut remaining: &str,
    rules: &[InlineRule],
) -> Result<(), MarkdownError> {
    while let Some((start, rule)) = find_custom_opening(remaining, rules) {
        if start > 0 {
            split_custom_inlines(result, &remaining[..start], rules)?;
        }
        let (opening, closing) = rule.delimiters();
        let value_start = start + opening.len();
        let Some(relative_end) = find_unescaped(remaining[value_start..].as_bytes(), closing) else {
            return Err(MarkdownError::new(format!(
                "Unclosed custom inline '{}'",
                rule.name()
            )));
        };
        let value_end = value_start + relative_end;
        let value = &remaining[value_start..value_end];
        if rules.iter().any(|nested| {
            find_unescaped(value.as_bytes(), nested.delimiters().0).is_some()
        }) {
            return Err(MarkdownError::new(format!(
                "Nested custom inline '{}' is not supported",
                rule.name()
            )));
        }
        result.push(Inline::Custom(CustomInlineData {
            name: rule.name().to_string(),
            text: value.to_string(),
            label: value.to_string(),
        }));
        remaining = &remaining[value_end + closing.len()..];
    }
    if !remaining.is_empty() {
        result.push(Inline::Text(remaining.to_string()));
    }
    Ok(())
}

fn find_custom_opening<'a>(value: &str, rules: &'a [InlineRule]) -> Option<(usize, &'a InlineRule)> {
    rules
        .iter()
        .filter_map(|rule| {
            find_unescaped(value.as_bytes(), rule.delimiters().0).map(|start| (start, rule))
        })
        .min_by_key(|(start, _)| *start)
}

fn find_unescaped(value: &[u8], needle: &str) -> Option<usize> {
    let needle = needle.as_bytes();
    let mut offset = 0;
    while offset + needle.len() <= value.len() {
        let Some(relative) = value[offset..].windows(needle.len()).position(|window| window == needle)
        else {
            return None;
        };
        let position = offset + relative;
        let mut slash_count = 0;
        let mut slash = position;
        while slash > 0 && value[slash - 1] == b'\\' {
            slash_count += 1;
            slash -= 1;
        }
        if slash_count % 2 == 0 {
            return Some(position);
        }
        offset = position + needle.len();
    }
    None
}

impl Document {
    pub fn parse(source: &str) -> Result<Self, MarkdownError> {
        Self::parse_with_rules(source, &[], &[])
    }

    pub(crate) fn parse_with_rules(
        source: &str,
        block_rules: &[BlockRule],
        inline_rules: &[InlineRule],
    ) -> Result<Self, MarkdownError> {
        validate_rules(block_rules, inline_rules)?;
        let prepared = prepare_source(source, block_rules, inline_rules)?;
        let options = ParseOptions {
            constructs: Constructs::gfm(),
            gfm_strikethrough_single_tilde: false,
            ..ParseOptions::default()
        };
        let root = markdown::to_mdast(&prepared.source, &options)
            .map_err(|error| MarkdownError::new(error.to_string()))?;
        let Node::Root(root) = root else {
            return Err(MarkdownError::new(
                "Markdown parser did not produce a document root",
            ));
        };
        let mut blocks = convert_blocks(&root.children, inline_rules)?;
        replace_custom_blocks(&mut blocks, &prepared.blocks);
        Ok(Self { blocks })
    }
}

fn convert_blocks(nodes: &[Node], inline_rules: &[InlineRule]) -> Result<Vec<Block>, MarkdownError> {
    nodes
        .iter()
        .map(|node| convert_block(node, inline_rules))
        .collect()
}

fn convert_block(node: &Node, inline_rules: &[InlineRule]) -> Result<Block, MarkdownError> {
    match node {
        Node::Heading(heading) => Ok(Block::Heading {
            depth: heading.depth,
            content: convert_inlines(&heading.children, inline_rules)?,
        }),
        Node::Paragraph(paragraph) => Ok(Block::Paragraph(convert_inlines(
            &paragraph.children,
            inline_rules,
        )?)),
        Node::Blockquote(quote) => Ok(Block::Blockquote(convert_blocks(
            &quote.children,
            inline_rules,
        )?)),
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
                    Ok(ListItem {
                        checked: item.checked,
                        blocks: convert_blocks(&item.children, inline_rules)?,
                    })
                })
                .collect::<Result<_, MarkdownError>>()?;
            Ok(Block::List {
                ordered: list.ordered,
                start: list.start,
                items,
            })
        }
        Node::Code(code) => Ok(Block::Code {
            value: code.value.clone(),
            language: code.lang.clone(),
            meta: code.meta.clone(),
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
                            convert_inlines(&cell.children, inline_rules)
                        })
                        .collect::<Result<_, MarkdownError>>()?;
                    Ok(TableRow { cells })
                })
                .collect::<Result<_, MarkdownError>>()?;
            let alignments = table.align.iter().copied().map(Alignment::from).collect();
            Ok(Block::Table { alignments, rows })
        }
        Node::FootnoteDefinition(footnote) => Ok(Block::FootnoteDefinition {
            identifier: footnote.identifier.clone(),
            blocks: convert_blocks(&footnote.children, inline_rules)?,
        }),
        Node::Html(_) => Err(MarkdownError::new(
            "Raw HTML is not supported in MarkdownViewer",
        )),
        other => Err(MarkdownError::new(format!(
            "Unsupported Markdown block node: {}",
            node_name(other)
        ))),
    }
}

fn convert_inlines(nodes: &[Node], inline_rules: &[InlineRule]) -> Result<Vec<Inline>, MarkdownError> {
    let mut result = Vec::new();
    for node in nodes {
        match node {
            Node::Text(text) => push_text_with_soft_breaks(&mut result, &text.value, inline_rules)?,
            Node::Break(_) => result.push(Inline::HardBreak),
            Node::Emphasis(emphasis) => {
                result.push(Inline::Emphasis(convert_inlines(&emphasis.children, inline_rules)?))
            }
            Node::Strong(strong) => result.push(Inline::Strong(convert_inlines(&strong.children, inline_rules)?)),
            Node::Delete(delete) => result.push(Inline::Delete(convert_inlines(&delete.children, inline_rules)?)),
            Node::InlineCode(code) => result.push(Inline::Code(code.value.clone())),
            Node::Link(link) => result.push(Inline::Link {
                url: link.url.clone(),
                title: link.title.clone(),
                content: convert_inlines(&link.children, inline_rules)?,
            }),
            Node::Image(image) => result.push(Inline::Image {
                url: image.url.clone(),
                title: image.title.clone(),
                alt: image.alt.clone(),
            }),
            Node::FootnoteReference(reference) => result.push(Inline::FootnoteReference {
                identifier: reference.identifier.clone(),
            }),
            Node::Html(item) => {
                error!("Raw HTML is not supported in MarkdownViewer : {:?}", item);
                return Err(MarkdownError::new(
                    "Raw HTML is not supported in MarkdownViewer",
                ));
            }
            other => {
                error!("Unsupported Markdown inline node: {}", node_name(other));
                return Err(MarkdownError::new(format!(
                    "Unsupported Markdown inline node: {}",
                    node_name(other)
                )));
            }
        }
    }
    Ok(result)
}

fn push_text_with_soft_breaks(
    result: &mut Vec<Inline>,
    value: &str,
    inline_rules: &[InlineRule],
) -> Result<(), MarkdownError> {
    let mut parts = value.split('\n').peekable();
    while let Some(part) = parts.next() {
        push_extended_image_text(result, part, inline_rules)?;
        if parts.peek().is_some() {
            result.push(Inline::SoftBreak);
        }
    }
    Ok(())
}

fn push_extended_image_text(
    result: &mut Vec<Inline>,
    value: &str,
    inline_rules: &[InlineRule],
) -> Result<(), MarkdownError> {
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
        split_custom_inlines(result, remaining, inline_rules)?;
    }
    Ok(())
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
    use crate::{BlockRule, BlockSyntax, InlineRule, InlineSyntax};

    fn parse(source: &str) -> Document {
        Document::parse(source).expect("fixture should parse")
    }

    #[test]
    fn parses_paired_custom_blocks_and_inlines() {
        let block_rule = BlockRule::new(
            "alert",
            BlockSyntax::Paired {
                opening: ":::alert",
                closing: ":::",
            },
        );
        let inline_rule = InlineRule::new(
            "button",
            InlineSyntax::Paired {
                opening: "{{button:",
                closing: "}}",
            },
        );

        let document = Document::parse_with_rules(
            ":::alert\n**Important**\n:::\n\nClick {{button:continue}}.",
            &[block_rule],
            &[inline_rule],
        )
        .expect("custom Markdown should parse");

        assert!(matches!(
            &document.blocks[0],
            Block::Custom(data)
                if data.name == "alert"
                    && data.text == "**Important**"
                    && matches!(data.content.blocks.as_slice(), [Block::Paragraph(_)])
        ));
        let Block::Paragraph(inlines) = &document.blocks[1] else {
            panic!("expected custom inline paragraph")
        };
        assert!(matches!(
            inlines.as_slice(),
            [Inline::Text(prefix), Inline::Custom(data), Inline::Text(suffix)]
                if prefix == "Click " && data.name == "button" && data.text == "continue" && suffix == "."
        ));
    }

    #[test]
    fn rejects_duplicate_and_unclosed_custom_rules() {
        let rule = BlockRule::new(
            "alert",
            BlockSyntax::Paired {
                opening: ":::alert",
                closing: ":::",
            },
        );
        let duplicate = Document::parse_with_rules(
            ":::alert\nbody\n:::",
            &[rule.clone(), rule.clone()],
            &[],
        )
        .expect_err("duplicate rules must be rejected");
        assert!(duplicate.message().contains("Duplicate custom block rule"));

        let unclosed = Document::parse_with_rules(
            ":::alert\nbody",
            &[rule],
            &[],
        )
        .expect_err("unclosed blocks must be rejected");
        assert!(unclosed.message().contains("Unclosed custom block"));

        let unclosed_inline = Document::parse_with_rules(
            "Click {{button:value",
            &[],
            &[InlineRule::new(
                "button",
                InlineSyntax::Paired {
                    opening: "{{button:",
                    closing: "}}",
                },
            )],
        )
        .expect_err("unclosed inline values must be rejected");
        assert!(unclosed_inline.message().contains("Unclosed custom inline"));
    }

    fn inline_text(inlines: &[Inline]) -> String {
        inlines
            .iter()
            .map(|inline| match inline {
                Inline::Text(value) | Inline::Code(value) => value.clone(),
                Inline::SoftBreak | Inline::HardBreak => "\n".to_string(),
                Inline::Emphasis(children)
                | Inline::Strong(children)
                | Inline::Delete(children) => inline_text(children),
                Inline::Link { content, .. } => inline_text(content),
                Inline::Image { alt, .. } => alt.clone(),
                Inline::FootnoteReference { identifier } => identifier.clone(),
                Inline::Custom(data) => data.text.clone(),
            })
            .collect()
    }

    #[test]
    fn parses_headings_and_all_emphasis_forms() {
        let document = parse(
            "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6\n\n*italic* **bold** ***both*** ~~gone~~",
        );
        for (index, block) in document.blocks[..6].iter().enumerate() {
            assert!(matches!(block, Block::Heading { depth, .. } if *depth == index as u8 + 1));
        }
        let Block::Paragraph(inlines) = &document.blocks[6] else {
            panic!("expected paragraph")
        };
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
        let Block::List {
            ordered,
            start,
            items,
        } = &document.blocks[0]
        else {
            panic!("expected list")
        };
        assert!(!ordered);
        assert_eq!(*start, None);
        assert_eq!(
            items.iter().map(|item| item.checked).collect::<Vec<_>>(),
            [None, Some(true), Some(false)]
        );
        assert!(matches!(
            items[2].blocks.last(),
            Some(Block::List {
                ordered: true,
                start: Some(1),
                ..
            })
        ));
    }

    #[test]
    fn parses_links_images_autolinks_and_footnotes() {
        let document = parse(
            "[plain](https://example.com) [titled](https://example.com \"title\") <https://a.test> ![alt](image.jpg \"caption\") ref[^One].\n\n[^One]: Footnote *text*.",
        );
        let Block::Paragraph(inlines) = &document.blocks[0] else {
            panic!("expected paragraph")
        };
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
            matches!(&document.blocks[1], Block::Paragraph(inlines) if matches!(inlines.as_slice(), [Inline::Code(value)] if value == "inline"))
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
            &[
                Alignment::Left,
                Alignment::Center,
                Alignment::Right,
                Alignment::None
            ]
        );
        assert_eq!(rows.len(), 2);
        assert!(matches!(rows[1].cells[0].as_slice(), [Inline::Emphasis(_)]));
        assert!(matches!(rows[1].cells[1].as_slice(), [Inline::Strong(_)]));
        assert!(matches!(rows[1].cells[2].as_slice(), [Inline::Code(value)] if value == "c"));
    }

    #[test]
    fn rejects_raw_html_without_silently_dropping_it() {
        let error = Document::parse("<script>alert('no')</script>")
            .expect_err("raw HTML is intentionally unsupported");
        assert!(error.to_string().contains("HTML"));
    }
}
