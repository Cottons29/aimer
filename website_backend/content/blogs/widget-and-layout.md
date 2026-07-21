# Widgets and Layout in Aimer

Every Aimer interface begins as a tree of widgets. Widgets describe what the interface should look like, while Aimer
turns that description into mounted elements that perform layout, painting, and event handling. The result is a
declarative API that feels natural in Rust: compose small values, configure them with builder methods, and let the type
system catch incomplete widget trees before the application runs.

This post introduces Aimer's widget builder pattern, explains how it benefits from Rust's type system, and builds a
layout with containers, rows, columns, spacing, alignment, and flexible sizing.

## Widgets Describe the Interface

The `Widget` trait is the common boundary for every piece of an Aimer interface. A widget is lightweight configuration;
its `to_element` method creates the runtime element that participates in the mounted tree.

```rust
pub trait Widget {
    fn key(&self) -> Option<Key>;
    fn to_element(&self, ctx: &BuildContext) -> AnyElement;
    fn boxed(self) -> AnyWidget;
}
```

Most application code does not need to call `to_element` directly. Instead, widgets are nested through builder methods
and passed to `AimerApp::start`:

```rust
#[aimer::main]
pub fn start_app() {
    AimerApp::start(
        Container::new()
            .padding(LayoutSpacing::all(Spacing::Px(24)))
            .child(Text::new("Hello from Aimer")),
    );
}
```

The widget tree remains a plain Rust value until the framework mounts it. This separation makes rebuilding cheap: the
new description can be reconciled with the existing element tree while state is preserved for matching widgets.

## A Builder Pattern Backed by Rust's Type System

Aimer's child-last builder pattern does more than make nested code readable. It uses a generic placeholder to represent
an incomplete widget:

```rust
pub struct Panel<W = RequiredChild> {
    child: W,
    width: Dimension,
}

impl Panel {
    pub fn new() -> Self {
        Self {
            child: RequiredChild,
            width: Dimension::Auto,
        }
    }

    pub fn child<W: Widget>(self, child: W) -> Panel<W> {
        Panel {
            child,
            width: self.width,
        }
    }
}
```

`RequiredChild` does not implement `Widget`, so `Panel::new()` cannot be mounted by accident. Calling `child(...)`
changes the type from `Panel<RequiredChild>` into `Panel<W>`, which becomes a valid widget when `W` is a widget. Missing
required children are therefore compile-time errors rather than blank areas or runtime failures.

This pattern is used by widgets such as `Container`, `Button`, `Align`, `Expanded`, `Scrollable`, `AspectRatio`, and
`Opacity`. `Column` applies the same idea with a terminal `children(...)` call.

Keeping the concrete child type also avoids dynamic dispatch in the common case. Type erasure remains available when it
is useful: call `.boxed()` or use a `box_children(...)` builder for branches and heterogeneous collections.

## Build a Box With Container

`Container` combines sizing, spacing, decoration, and a single child. Width and height accept `Dimension` values:

```rust
use aimer::style::{BoxDecoration, LayoutSpacing, Spacing};
use aimer::{Container, Dimension, Text};

let card = Container::new()
    .width(Dimension::Percent(100.0))
    .padding(LayoutSpacing::all(Spacing::Px(16)))
    .margin(LayoutSpacing::all(Spacing::Px(8)))
    .box_decoration(BoxDecoration::new().border_radius(16))
    .child(Text::new("A card built from widgets"));
```

The available dimensions are:

- `Dimension::Px(value)` for a fixed logical size.
- `Dimension::Percent(value)` for a percentage of the available space.
- `Dimension::Auto` to derive size from the child and layout constraints.

Numeric values passed directly to `.width(...)` or `.height(...)` are treated as pixels, so `.height(48)` is a concise
form of `.height(Dimension::Px(48.0))`.

Margin sits outside the painted box, while padding creates space between the decoration and child. `LayoutSpacing`
supports `all`, `vertical`, and `horizontal` constructors, plus side-specific `top`, `bottom`, `left`, and `right`
builders.

When only a fixed area or an intentional gap is needed, use `SizedBox`:

```rust
let spacer = SizedBox::new().width(12).height(12);

let fixed_child = SizedBox::new()
    .width(240)
    .height(48)
    .child(Text::new("Constrained content"));
```

Unlike required-child widgets, `SizedBox::new()` is already a valid widget. Without a child, its automatic dimensions
resolve to zero.

## Arrange Children With Row and Column

`Row` lays children out horizontally and `Column` lays them out vertically. Both support physical-axis alignment, gaps,
and overflow behavior.

```rust
use aimer::style::{BoxAlignment, LayoutSpacing};
use aimer::{Column, Row, Text, Widget};

let profile = Column::new()
    .horizontal_alignment(BoxAlignment::Start)
    .gaps(LayoutSpacing::new().bottom(12))
    .children(vec![
        Text::new("Aimer Developer").boxed(),
        Row::new()
            .vertical_alignment(BoxAlignment::Center)
            .gaps(LayoutSpacing::new().right(8))
            .children([
                Text::new("Rust"),
                Text::new("Cross-platform"),
            ])
            .boxed(),
    ]);
```

For a row, horizontal alignment controls the main axis and vertical alignment controls the cross axis. For a column,
vertical alignment controls the main axis and horizontal alignment controls the cross axis. `BoxAlignment` currently
supports `Start`, `Center`, and `End`.

Rust collections contain one concrete item type. The two `Text` values in the inner row share a type and can be stored
in an array directly. The outer column contains a `Text` and a `Row`, so both are explicitly boxed into `AnyWidget`.
This makes the point where dynamic dispatch enters the tree visible instead of hiding it behind every widget.

By default, overflowing children are hidden. `OverflowBehavior::Wrap` moves overflowing items onto another run, while
`OverflowBehavior::Visible` allows them to paint beyond the flex bounds.

## Share Remaining Space With Expanded

Place `Expanded` children inside a `Row`, `Column`, or `Flex` to divide the remaining space. Every expanded child has a
flex factor; the default is `1.0`.

```rust
use aimer::{Expanded, Row, SizedBox, Text};

let weighted_row = Row::new().children([
    Expanded::new()
        .child(
            SizedBox::new()
                .height(48)
                .child(Text::new("One part")),
        ),
    Expanded::new()
        .flex(2.0)
        .child(
            SizedBox::new()
                .height(48)
                .child(Text::new("Two parts")),
        ),
]);
```

After non-expanded children receive their required size, this row gives one third of the remaining width to the first
child and two thirds to the second. Equal factors create equal shares, while a factor of zero receives none of the
remaining main-axis space.

## Compose a Complete Layout

The same primitives scale from small controls to complete screens:

```rust
use aimer::style::{BoxAlignment, LayoutSpacing, Spacing, TextStyle};
use aimer::{Column, Container, Dimension, Expanded, Row, SizedBox, Text, Widget};

let dashboard_card = Container::new()
    .width(Dimension::Percent(100.0))
    .padding(LayoutSpacing::all(Spacing::Px(16)))
    .child(
        Column::new()
            .horizontal_alignment(BoxAlignment::Start)
            .gaps(LayoutSpacing::new().bottom(12))
            .children(vec![
                Text::new("Layout summary")
                    .text_style(TextStyle::new().font_size(24))
                    .boxed(),
                Row::new()
                    .gaps(LayoutSpacing::new().right(8))
                    .children([
                        Expanded::new().child(
                            SizedBox::new()
                                .height(48)
                                .child(Text::new("Widgets")),
                        ),
                        Expanded::new().flex(2.0).child(
                            SizedBox::new()
                                .height(48)
                                .child(Text::new("Layout")),
                        ),
                    ])
                    .boxed(),
            ]),
    );
```

The tree reads from the outside in: establish the card's bounds and padding, arrange its content vertically, then split
one row into weighted regions. Every required child appears at the end of its builder, and boxing is limited to the one
heterogeneous collection that needs it.

## Declarative Without Hiding Rust

Aimer's declarative syntax is its fluent builder API. It keeps normal Rust expressions, ownership, generics, iterators,
and control flow available while making the visual hierarchy easy to follow. Custom application widgets can use
`#[widget(Stateless)]` or `#[widget(Stateful)]`, then return another composed widget tree from their build method.

This design gives Aimer two useful properties at the same time: interface code remains concise and composable, while
Rust still verifies required children and concrete widget relationships. Start with `Container`, `Row`, `Column`, and
`SizedBox`; introduce `Expanded` where remaining space must be shared; and erase types only where a heterogeneous branch
actually requires it. Those few rules are enough to build layouts that remain readable as an application grows.