use aimer_color::prelude::Color;
use arborium::advanced::Span;

pub trait CaptureColor {
    fn color(input: CaptureSpan) -> Color;
}

/// A highlight capture category, paired with its byte span.
///
/// This mirrors `arborium_highlight::Span`, but replaces the raw
/// `capture: String` with a closed-ish enum of the semantic categories
/// arborium's theme layer normalizes captures into (see `HIGHLIGHT_NAMES`
/// / the HTML tag reference: <a-k>, <a-f>, etc.).
///
/// NOTE: `Other(String)` exists because `Span.capture` is a free-form
/// `String` in the underlying crate — not every possible raw capture name
/// tree-sitter grammars might emit is guaranteed to be covered above,
/// so this variant preserves anything unrecognized instead of dropping it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureSpan {
    // --- Code ---
    Keyword {
        start: u32,
        end: u32,
    },
    Function {
        start: u32,
        end: u32,
    },
    String {
        start: u32,
        end: u32,
    },
    Comment {
        start: u32,
        end: u32,
    },
    Type {
        start: u32,
        end: u32,
    },
    Variable {
        start: u32,
        end: u32,
    },
    Constant {
        start: u32,
        end: u32,
    },
    Number {
        start: u32,
        end: u32,
    },
    Operator {
        start: u32,
        end: u32,
    },
    Punctuation {
        start: u32,
        end: u32,
    },
    Property {
        start: u32,
        end: u32,
    },
    Attribute {
        start: u32,
        end: u32,
    },
    Tag {
        start: u32,
        end: u32,
    },
    Macro {
        start: u32,
        end: u32,
    },
    Label {
        start: u32,
        end: u32,
    },
    Namespace {
        start: u32,
        end: u32,
    },
    Constructor {
        start: u32,
        end: u32,
    },

    // --- Markup (Markdown, AsciiDoc, etc.) ---
    Title {
        start: u32,
        end: u32,
    },
    Strong {
        start: u32,
        end: u32,
    },
    Emphasis {
        start: u32,
        end: u32,
    },
    Link {
        start: u32,
        end: u32,
    },
    Literal {
        start: u32,
        end: u32,
    },
    Strikethrough {
        start: u32,
        end: u32,
    },

    // --- Diff ---
    DiffAdd {
        start: u32,
        end: u32,
    },
    DiffDelete {
        start: u32,
        end: u32,
    },

    // --- Special ---
    Embedded {
        start: u32,
        end: u32,
    },
    Error {
        start: u32,
        end: u32,
    },

    // --- Fallback for anything not covered above ---
    Other {
        start: u32,
        end: u32,
        capture: String,
    },
}

impl CaptureSpan {
    /// Byte offsets, regardless of variant.
    pub fn range(&self) -> (u32, u32) {
        match self {
            CaptureSpan::Keyword { start, end }
            | CaptureSpan::Function { start, end }
            | CaptureSpan::String { start, end }
            | CaptureSpan::Comment { start, end }
            | CaptureSpan::Type { start, end }
            | CaptureSpan::Variable { start, end }
            | CaptureSpan::Constant { start, end }
            | CaptureSpan::Number { start, end }
            | CaptureSpan::Operator { start, end }
            | CaptureSpan::Punctuation { start, end }
            | CaptureSpan::Property { start, end }
            | CaptureSpan::Attribute { start, end }
            | CaptureSpan::Tag { start, end }
            | CaptureSpan::Macro { start, end }
            | CaptureSpan::Label { start, end }
            | CaptureSpan::Namespace { start, end }
            | CaptureSpan::Constructor { start, end }
            | CaptureSpan::Title { start, end }
            | CaptureSpan::Strong { start, end }
            | CaptureSpan::Emphasis { start, end }
            | CaptureSpan::Link { start, end }
            | CaptureSpan::Literal { start, end }
            | CaptureSpan::Strikethrough { start, end }
            | CaptureSpan::DiffAdd { start, end }
            | CaptureSpan::DiffDelete { start, end }
            | CaptureSpan::Embedded { start, end }
            | CaptureSpan::Error { start, end } => (*start, *end),
            CaptureSpan::Other { start, end, .. } => (*start, *end),
        }
    }

    /// Build a `CaptureSpan` from arborium_highlight's raw `Span`.
    ///
    /// Matches on the dotted-prefix convention tree-sitter grammars use
    /// (e.g. "keyword.function" still maps to Keyword), falling back to
    /// `Other` for anything unrecognized.
    pub fn from_raw(span: &Span) -> Self {
        let start = span.start;
        let end = span.end;
        let base = span
            .capture
            .split('.')
            .next()
            .unwrap_or(&span.capture);

        match base {
            "keyword" | "include" | "conditional" | "repeat" => CaptureSpan::Keyword { start, end },
            "function" | "method" => CaptureSpan::Function { start, end },
            "string" | "character" => CaptureSpan::String { start, end },
            "comment" => CaptureSpan::Comment { start, end },
            "type" => CaptureSpan::Type { start, end },
            "variable" | "parameter" => CaptureSpan::Variable { start, end },
            "constant" | "boolean" => CaptureSpan::Constant { start, end },
            "number" | "float" => CaptureSpan::Number { start, end },
            "operator" => CaptureSpan::Operator { start, end },
            "punctuation" => CaptureSpan::Punctuation { start, end },
            "property" | "field" => CaptureSpan::Property { start, end },
            "attribute" | "annotation" => CaptureSpan::Attribute { start, end },
            "tag" => CaptureSpan::Tag { start, end },
            "macro" => CaptureSpan::Macro { start, end },
            "label" => CaptureSpan::Label { start, end },
            "namespace" | "module" => CaptureSpan::Namespace { start, end },
            "constructor" => CaptureSpan::Constructor { start, end },
            "markup" => match span.capture.split('.').nth(1) {
                Some("heading" | "title") => CaptureSpan::Title { start, end },
                Some("bold" | "strong") => CaptureSpan::Strong { start, end },
                Some("italic" | "emphasis") => CaptureSpan::Emphasis { start, end },
                Some("link") => CaptureSpan::Link { start, end },
                Some("raw" | "literal") => CaptureSpan::Literal { start, end },
                Some("strikethrough") => CaptureSpan::Strikethrough { start, end },
                _ => CaptureSpan::Other {
                    start,
                    end,
                    capture: span.capture.clone(),
                },
            },
            "diff" => match span.capture.split('.').nth(1) {
                Some("plus" | "add") => CaptureSpan::DiffAdd { start, end },
                Some("minus" | "delete") => CaptureSpan::DiffDelete { start, end },
                _ => CaptureSpan::Other {
                    start,
                    end,
                    capture: span.capture.clone(),
                },
            },
            "embedded" => CaptureSpan::Embedded { start, end },
            "error" => CaptureSpan::Error { start, end },
            _ => CaptureSpan::Other {
                start,
                end,
                capture: span.capture.clone(),
            },
        }
    }

    pub fn color(&self) -> Color {
        match self {
            // --- Code ---
            CaptureSpan::Keyword { .. } => Color::Rgb(198, 120, 221), // purple
            CaptureSpan::Function { .. } => Color::Rgb(97, 175, 239), // blue
            CaptureSpan::String { .. } => Color::Rgb(152, 195, 121),  // green
            CaptureSpan::Comment { .. } => Color::Rgb(92, 99, 112),   // muted gray
            CaptureSpan::Type { .. } => Color::Rgb(229, 192, 123),    // yellow/gold
            CaptureSpan::Variable { .. } => Color::Rgb(224, 108, 117), // soft red
            CaptureSpan::Constant { .. } => Color::Rgb(209, 154, 102), // orange
            CaptureSpan::Number { .. } => Color::Rgb(209, 154, 102),  // orange
            CaptureSpan::Operator { .. } => Color::Rgb(86, 182, 194), // cyan
            CaptureSpan::Punctuation { .. } => Color::Rgb(171, 178, 191), // light gray
            CaptureSpan::Property { .. } => Color::Rgb(224, 108, 117), // soft red
            CaptureSpan::Attribute { .. } => Color::Rgb(209, 154, 102), // orange
            CaptureSpan::Tag { .. } => Color::Rgb(224, 108, 117),     // soft red
            CaptureSpan::Macro { .. } => Color::Rgb(198, 120, 221),   // purple
            CaptureSpan::Label { .. } => Color::Rgb(198, 120, 221),   // purple
            CaptureSpan::Namespace { .. } => Color::Rgb(229, 192, 123), // yellow/gold
            CaptureSpan::Constructor { .. } => Color::Rgb(97, 175, 239), // blue

            // --- Markup ---
            CaptureSpan::Title { .. } => Color::Rgb(224, 108, 117), // soft red
            CaptureSpan::Strong { .. } => Color::Rgb(229, 192, 123), // yellow/gold
            CaptureSpan::Emphasis { .. } => Color::Rgb(198, 120, 221), // purple
            CaptureSpan::Link { .. } => Color::Rgb(97, 175, 239),   // blue
            CaptureSpan::Literal { .. } => Color::Rgb(152, 195, 121), // green
            CaptureSpan::Strikethrough { .. } => Color::Rgb(92, 99, 112), // muted gray

            // --- Diff ---
            CaptureSpan::DiffAdd { .. } => Color::Rgb(152, 195, 121), // green
            CaptureSpan::DiffDelete { .. } => Color::Rgb(224, 108, 117), // soft red

            // --- Special ---
            CaptureSpan::Embedded { .. } => Color::Rgb(171, 178, 191), // light gray
            CaptureSpan::Error { .. } => Color::Rgb(224, 108, 117),    // soft red (bold-worthy)

            // --- Fallback ---
            CaptureSpan::Other { .. } => Color::Rgb(171, 178, 191), // light gray, neutral default
        }
    }
}

impl From<Span> for CaptureSpan {
    fn from(value: Span) -> Self {
        CaptureSpan::from_raw(&value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arborium::Highlighter;

    #[test]
    fn test_from_raw() {
        let raw = "fn main(){}";
        let span = Highlighter::new()
            .highlight_spans("rust", &raw)
            .unwrap();
        let capture: Vec<CaptureSpan> = span
            .into_iter()
            .map(CaptureSpan::from)
            .collect();

        assert_eq!(
            capture,
            vec![
                CaptureSpan::Keyword { start: 0, end: 2 },
                CaptureSpan::Function { start: 3, end: 7 },
                CaptureSpan::Punctuation { start: 7, end: 8 },
                CaptureSpan::Punctuation { start: 8, end: 9 },
                CaptureSpan::Punctuation { start: 9, end: 10 },
                CaptureSpan::Punctuation { start: 10, end: 11 },
            ]
        );
    }
}
