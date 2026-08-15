use std::rc::Rc;

use aimer_widget::AnyWidget;

use crate::Document;

/// Describes the delimiters surrounding a custom block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockSyntax {
    /// A block whose opening and closing delimiters occupy their own lines.
    Paired {
        opening: &'static str,
        closing: &'static str,
    },
}

impl BlockSyntax {
    pub(crate) fn delimiters(self) -> (&'static str, &'static str) {
        match self {
            Self::Paired { opening, closing } => (opening, closing),
        }
    }
}

/// Identifies and describes a custom block syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockRule {
    name: String,
    syntax: BlockSyntax,
}

impl BlockRule {
    /// Creates a named custom block rule.
    #[inline]
    pub fn new(name: impl Into<String>, syntax: BlockSyntax) -> Self {
        Self {
            name: name.into(),
            syntax,
        }
    }

    /// Returns the name passed to custom block builders.
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn delimiters(&self) -> (&'static str, &'static str) {
        self.syntax.delimiters()
    }
}

impl From<BlockSyntax> for BlockRule {
    fn from(syntax: BlockSyntax) -> Self {
        let (opening, _) = syntax.delimiters();
        Self::new(opening, syntax)
    }
}

/// Describes the delimiters surrounding a custom inline value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineSyntax {
    /// An inline value surrounded by an opening and closing delimiter.
    Paired {
        opening: &'static str,
        closing: &'static str,
    },
}

impl InlineSyntax {
    pub(crate) fn delimiters(self) -> (&'static str, &'static str) {
        match self {
            Self::Paired { opening, closing } => (opening, closing),
        }
    }
}

/// Identifies and describes a custom inline syntax.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineRule {
    name: String,
    syntax: InlineSyntax,
}

impl InlineRule {
    /// Creates a named custom inline rule.
    #[inline]
    pub fn new(name: impl Into<String>, syntax: InlineSyntax) -> Self {
        Self {
            name: name.into(),
            syntax,
        }
    }

    /// Returns the name passed to custom inline builders.
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn delimiters(&self) -> (&'static str, &'static str) {
        self.syntax.delimiters()
    }
}

impl From<InlineSyntax> for InlineRule {
    fn from(syntax: InlineSyntax) -> Self {
        let (opening, _) = syntax.delimiters();
        Self::new(opening, syntax)
    }
}

/// Parsed data passed to a custom block builder.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomBlockData {
    /// The name of the rule that matched this block.
    pub name: String,
    /// The source between the opening and closing delimiters.
    pub text: String,
    /// The block body parsed as Markdown, including nested custom syntax.
    pub content: Document,
}

/// Parsed data passed to a custom inline builder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomInlineData {
    /// The name of the rule that matched this value.
    pub name: String,
    /// The source between the opening and closing delimiters.
    pub text: String,
    /// An alias for [`CustomInlineData::text`] useful for label-like syntax.
    pub label: String,
}

/// A callback that turns parsed custom block data into a widget.
pub type CustomBlockBuilder = Rc<dyn Fn(&CustomBlockData) -> AnyWidget>;

/// A callback that turns parsed custom inline data into a widget.
pub type CustomInlineBuilder = Rc<dyn Fn(&CustomInlineData) -> AnyWidget>;