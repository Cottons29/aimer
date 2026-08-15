use aimer::style::{FontWeight, LayoutSpacing, Spacing, TextStyle};
use aimer::{AimerApp, AnyWidget, Color, Column, Container, OverflowBehavior, Row, Text, Widget};

const DEMO_WIDTH: f32 = 360.0;
const DEMO_HEIGHT: f32 = 150.0;
const ITEM_WIDTH: f32 = 150.0;
const ITEM_HEIGHT: f32 = 44.0;

/// Starts a showcase of the three [`OverflowBehavior`] modes.
///
/// Every sample uses the same constrained row and children. `Hidden` clips the
/// overflowing child, `Wrap` moves it to a second line, and `Visible` paints it
/// beyond the row's width.
pub fn start_overflow_behavior_example() {
    AimerApp::start(
        Container::new()
            .padding(LayoutSpacing::all(Spacing::Px(28)))
            .color(Color::Rgb(245, 245, 245))
            .child(
                Column::new()
                    .gaps(LayoutSpacing::all(Spacing::Px(12)))
                    .children([
                        Text::new("OverflowBehavior")
                            .text_style(
                                TextStyle::new()
                                    .font_size(28)
                                    .font_weight(FontWeight::Bold)
                                    .color(Color::BLACK),
                            )
                            .boxed(),
                        Text::new(
                            "The same children are clipped, wrapped, or allowed outside the row.",
                        )
                        .text_style(TextStyle::new().font_size(16).color(Color::Rgb(55, 65, 81)))
                        .boxed(),
                        demo("Hidden", OverflowBehavior::Hidden),
                        demo("Wrap", OverflowBehavior::Wrap),
                        demo("Visible", OverflowBehavior::Visible),
                    ]),
            ),
    );
}

fn demo(label: &'static str, overflow: OverflowBehavior) -> AnyWidget {
    Column::new()
        .gaps(LayoutSpacing::all(Spacing::Px(4)))
        .children([
            Text::new(label)
                .text_style(TextStyle::new().font_size(14).color(Color::Rgb(31, 41, 55)))
                .boxed(),
            Container::new()
                // .width(DEMO_WIDTH)
                // .height(DEMO_HEIGHT)
                .padding(LayoutSpacing::all(Spacing::Px(10)))
                .color(Color::WHITE)
                .child(
                    Row::new()
                        .overflow(overflow)
                        .gaps(LayoutSpacing::all(Spacing::Px(8)))
                        .children([item("A"), item("B"), item("C")]),
                )
                .boxed(),
        ])
        .boxed()
}

fn item(label: &'static str) -> AnyWidget {
    Container::new()
        .width(ITEM_WIDTH)
        .height(ITEM_HEIGHT)
        .color(Color::Rgb(37, 99, 235))
        .child(Text::new(label).text_style(TextStyle::new().font_size(16).color(Color::WHITE)))
        .boxed()
}