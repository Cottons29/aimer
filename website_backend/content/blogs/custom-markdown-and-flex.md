# Custom Markdown Widgets and the New Flex Layout

Markdown is a great format for writing documentation, release notes, and long-form content. It is deliberately good at
describing text, but an application often needs more than text: a callout, an interactive button, or a layout that
responds to the available space. Aimer's Markdown renderer now gives applications a small extension point for those
cases, and the new `Flex` widget brings one layout API to rows, columns, weighted children, and data-driven lists.

This post introduces both features and shows how they fit together: Markdown remains portable source text, while the
application decides which custom fragments become native Aimer widgets.

## Start With Ordinary Markdown

`MarkdownViewer` parses Markdown into Aimer's document tree and renders it with the same widget and theme system as the
rest of an application. There is no web view to style separately, and no HTML bridge is needed for headings, emphasis,
links, lists, code blocks, tables, or images.

```rust
use aimer::style::{LayoutSpacing, Spacing};
use aimer::{Container, MarkdownTheme, MarkdownViewer};

const DOCUMENT: &str = include_str!("guide.md");

let viewer = Container::new().child(
    MarkdownViewer::new()
        .padding(LayoutSpacing::all(Spacing::Px(16)))
        .theme(MarkdownTheme::default())
        .markdown(DOCUMENT),
);
```

The source can stay in a `.md` file, a string loaded from a server, or content supplied by a CMS. Keeping the source
as Markdown means it remains readable outside the application. The renderer turns it into native widgets only at the
point where it is displayed.

## Extending Markdown With Custom Syntax

An application can register paired block and inline rules on a viewer. A block rule uses delimiters on their own lines;
an inline rule surrounds a value inside a paragraph. The delimiters are application-defined, so the syntax can describe
the domain without pretending that every interactive component is a standard Markdown feature.

Here is a document containing one custom block and one custom inline value:

````markdown
# Release notes

:::callout
This message is rendered by the application's callout widget.
:::

Read the next example: {{button:Open the demo}}.
````

The `:::callout` and `{{button:...}}` forms are not global extensions. They only have meaning in a viewer that registers
matching rules. This keeps the base Markdown parser predictable and lets different applications assign different widget
semantics to their own documents.

### Custom Blocks

Register a block with `MarkdownBlockRule::new` and `MarkdownBlockSyntax::Paired`. The builder receives
`MarkdownCustomBlockData`, which includes the matched rule name, the raw text between the delimiters, and the nested
Markdown `Document`. The nested document is useful when a custom panel should still support headings, emphasis, or
lists inside its body.

```rust
use aimer::style::{BoxDecoration, LayoutSpacing, Spacing, TextStyle};
use aimer::{
    Column, Container, MarkdownBlockRule, MarkdownBlockSyntax, MarkdownViewer, Text,
};

let viewer = MarkdownViewer::new()
    .markdown(SOURCE)
    .custom_block(
        MarkdownBlockRule::new(
            "callout",
            MarkdownBlockSyntax::Paired {
                opening: ":::callout",
                closing: ":::",
            },
        ),
        |data| {
            Container::new()
                .padding(LayoutSpacing::all(Spacing::Px(16)))
                .box_decoration(BoxDecoration::new().border_radius(8))
                .child(
                    Column::new().children([
                        Text::new("Callout")
                            .text_style(TextStyle::new())
                            .boxed(),
                        Text::new(data.text.trim().to_owned()).boxed(),
                    ]),
                )
                .boxed()
        },
    );
```

The callback is a normal Rust function or closure, so it can construct a card, warning panel, media block, or any other
widget tree. `data.content` retains the parsed nested document when the block needs to render structured Markdown
rather than only its raw text.

### Custom Inline Values

Inline rules use the same idea at a smaller granularity. `MarkdownCustomInlineData` exposes `text` and `label`, making
it convenient for label-like values such as buttons, badges, mentions, or links into an application's own router.

```rust
use aimer::{Button, MarkdownInlineRule, MarkdownInlineSyntax, MarkdownViewer, Text};

let viewer = MarkdownViewer::new()
    .markdown("Try {{button:Open the demo}} from this paragraph.")
    .custom_inline(
        MarkdownInlineRule::new(
            "button",
            MarkdownInlineSyntax::Paired {
                opening: "{{button:",
                closing: "}}",
            },
        ),
        |data| Button::new().child(Text::new(data.label.clone())).boxed(),
    );
```

The callback can capture an `Rc` or another application-owned handle when the widget needs to perform an action. The
Markdown parser supplies the value; the callback owns the behavior. This separation keeps parsing deterministic and
prevents the document format from knowing about application state.

## Why Flex?

Before `Flex`, applications commonly chose between `Row` and `Column` and then added wrappers when a layout needed a
different direction or a particular overflow policy. `Flex` provides the shared primitive underneath those layouts. Its
direction can be `FlexDirection::Row`, `FlexDirection::Column`, or `FlexDirection::Inherit`, while the same builder
also exposes alignment, justification, gaps, and overflow behavior.

```rust
use aimer::style::{BoxAlignment, LayoutSpacing, Spacing};
use aimer::{Flex, FlexDirection, JustifyContent, OverflowBehavior, Text};

let toolbar = Flex::new()
    .direction(FlexDirection::Row)
    .horizontal_alignment(BoxAlignment::Center)
    .justify_content(JustifyContent::SpaceBetween)
    .gaps(LayoutSpacing::horizontal(Spacing::Px(12)))
    .overflow(OverflowBehavior::Hidden)
    .children([
        Text::new("Aimer").boxed(),
        Text::new("Docs").boxed(),
        Text::new("Blog").boxed(),
    ]);
```

`gaps` expresses spacing in logical pixels. `OverflowBehavior::Hidden` clips to the flex bounds, `Visible` allows
children to paint outside them, and `Wrap` continues the layout in additional rows or columns. `justify_content`
controls the main axis with values such as `Start`, `Center`, `End`, `SpaceBetween`, `SpaceAround`, and `SpaceEvenly`.

`Row` and `Column` remain useful concise names for the two common directions. `Flex` is most helpful when the direction
is selected by shared code, when one component needs the same configuration in either orientation, or when an
application wants to use the data-source API.

## Share Remaining Space With Expanded

`Expanded` marks a child as flexible. Its `flex` value is a weight used to divide the free space left after ordinary
children are measured. One child with the default factor `1.0` receives all remaining space; children with factors
`1.0` and `2.0` receive one third and two thirds of that space.

```rust
use aimer::style::BoxDecoration;
use aimer::{Container, Expanded, Flex, FlexDirection, Text};

let dashboard = Flex::new()
    .direction(FlexDirection::Row)
    .children([
        Container::new()
            .width(180)
            .child(Text::new("Navigation"))
            .boxed(),
        Expanded::new()
            .child(
                Container::new()
                    .box_decoration(BoxDecoration::new().border_radius(12))
                    .child(Text::new("Content")),
            )
            .boxed(),
        Expanded::new()
            .flex(2.0)
            .child(Text::new("Details"))
            .boxed(),
    ]);
```

The fixed navigation panel is measured first. The two `Expanded` children then divide whatever horizontal space is left
according to their weights. A negative factor is clamped to zero, and a zero-weight child receives no share of the
remaining space. This makes weighted layouts explicit without requiring application code to measure the window.

## Build Large Lists From Data

For a short static collection, `Flex::children` is direct and readable. For a long list, `Flex::list` keeps the data
source and maps each item through `FlexList::builder` instead of asking the caller to materialize one widget per item.

```rust
use aimer::{Flex, FlexDirection, Text};

let messages = Flex::new()
    .direction(FlexDirection::Column)
    .list(0..120_000)
    .builder(|index| Text::new(format!("Message {index}")));
```

The source is replayable and indexable, which lets the flex element build the children it needs during layout. The list
builder can also define an item extent and stable keys when an application knows the row size or when rows carry state
that must follow their data after insertion, removal, or reordering.

## Markdown and Flex Together

These features are deliberately composable. A custom Markdown block can return a `Flex`, and a `MarkdownViewer` can be
placed inside an `Expanded` child so it occupies the remaining space in a page. For example, a documentation screen can
keep its navigation at a fixed width while the Markdown content expands with the window:

```rust
use aimer::{Column, Expanded, Flex, FlexDirection, MarkdownViewer, Text};

let documentation = Column::new().children([
    Text::new("Aimer documentation").boxed(),
    Expanded::new()
        .child(
            Flex::new()
                .direction(FlexDirection::Row)
                .children([
                    Text::new("Sections").boxed(),
                    Expanded::new()
                        .child(MarkdownViewer::new().markdown(DOCUMENT))
                        .boxed(),
                ]),
        )
        .boxed(),
]);
```

The document stays content-focused, custom syntax stays owned by the application, and layout stays in the widget tree.
That division gives each layer a small responsibility while still allowing a Markdown document to participate in a
fully native, responsive Aimer interface.

## Closing Thoughts

Custom Markdown syntax is a bridge between portable content and interactive UI. Use ordinary Markdown for content that
should remain universally readable, then add a named block or inline rule when the application needs native behavior.
Use `Flex` when the layout needs one configurable primitive, `Expanded` when children should share free space, and
`Flex::list` when data should remain data until the layout needs to materialize it.

Together, these APIs make it possible to build documentation, dashboards, release notes, and content-driven screens
without leaving Aimer's widget model.