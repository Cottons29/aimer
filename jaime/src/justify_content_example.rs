use aimer::style::{BorderSlice, BorderStyle, BoxBorder, BoxDecoration, FontWeight, LayoutSpacing, Spacing, TextAlign, TextStyle};
use aimer::{AimerApp, AnyWidget, Color, Column, Container, JustifyContent, Row, Text, Widget};

const DEMO_WIDTH: f32 = 640.0;
const DEMO_HEIGHT: f32 = 56.0;
const ITEM_SIZE: f32 = 36.0;

/// Starts a showcase of the six [`JustifyContent`] modes.
///
/// Each row has the same available width and the same three children, making
/// the difference between positional alignment and free-space distribution
/// visible at a glance.
pub fn start_justify_content_example() {
    AimerApp::start(
        Container::new()
            // .width(700)
            .padding(LayoutSpacing::all(Spacing::Px(28)))
            .color(Color::Rgb(245, 245, 245))
            .child(
                Column::new()
                    .gaps(LayoutSpacing::all(Spacing::Px(12)))
                    .children([
                        Text::new("JustifyContent")
                            .text_style(
                                TextStyle::new()
                                    .font_size(28)
                                    .font_weight(FontWeight::Bold)
                                    .color(Color::BLACK),
                            )
                            .boxed(),
                        Text::new(
                            "The main-axis placement changes while every row keeps the same width.",
                        )
                        .text_style(
                            TextStyle::new()
                                .font_size(16)
                                .color(Color::Rgb(55, 65, 81)),
                        )
                        .boxed(),
                        demo(JustifyContent::Start),
                        demo(JustifyContent::Center),
                        demo(JustifyContent::End),
                        demo(JustifyContent::SpaceBetween),
                        demo(JustifyContent::SpaceAround),
                        demo(JustifyContent::SpaceEvenly),
                    ]),
            ),
    );
}

fn demo(justify_content: JustifyContent) -> AnyWidget {
    Column::new()
        .gaps(LayoutSpacing::all(Spacing::Px(4)))
        .children([
            Text::new(justify_content_name(justify_content))
                .text_style(
                    TextStyle::new()
                        .font_size(14)
                        .color(Color::Rgb(31, 41, 55)),
                )
                .boxed(),
            Container::new()
                // .width(DEMO_WIDTH)
                // .height(DEMO_HEIGHT)
                .padding(LayoutSpacing::all(Spacing::Px(10)))
                .color(Color::WHITE)
                .child(
                    Row::new()
                        .justify_content(justify_content)
                        .children([item("A"), item("B"), item("C")]),
                )
                .boxed(),
        ])
        .boxed()
}

fn item(label: &'static str) -> AnyWidget {
    Container::new()
        .width(ITEM_SIZE)
        .height(ITEM_SIZE)
        .color(Color::PURPLE)
        .box_decoration(
            BoxDecoration::new().border(BoxBorder::all(
                BorderSlice::new()
                    .stroke(1)
                    .color(Color::BLACK)
                    .style(BorderStyle::Solid),
            )),
        )
        .child(
            Text::new(label)
                .text_align(TextAlign::MidCenter)
                .text_style(
                TextStyle::new()
                    .font_size(14)
                    .color(Color::WHITE),
            ),
        )
        .boxed()
}

fn justify_content_name(justify_content: JustifyContent) -> &'static str {
    match justify_content {
        JustifyContent::Start => "Start",
        JustifyContent::Center => "Center",
        JustifyContent::End => "End",
        JustifyContent::SpaceBetween => "SpaceBetween",
        JustifyContent::SpaceAround => "SpaceAround",
        JustifyContent::SpaceEvenly => "SpaceEvenly",
    }
}
