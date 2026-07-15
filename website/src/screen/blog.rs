use aimer::style::{FontWeight, LayoutSpacing, TextDecoration, TextOverflow, TextStyle};
use aimer::{BuildContext, Widget, widget, *};

use crate::utils::{app_padding, mobile_title};

/// The `Docs` page rendered inside the app shell's content area.
#[widget(Stateless)]
#[derive(Clone)]
pub struct DocsPage;

impl DocsPage {
    pub fn boxing(_: &BuildContext) -> Box<dyn Widget> {
        Box::new(Self)
    }
}
impl StatelessWidget for DocsPage {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        Container::new()
            .color(Color::WHITE)
            .child(
                Scrollable::new()
                    .axis(ScrollAxis::Vertical).child(Container::new()
                    .padding(app_padding(ctx))
                    .child(Column::new()
                        .horizontal_alignment(BoxAlignment::Start)
                        .children([
                            SizedBox::new().height(24).boxed(),
                            Text::new("Docs")
                                .text_style(TextStyle::new()
                                    .font_size(mobile_title(ctx))
                                    .color(Color::BLACK)
                                    .font_weight(FontWeight::Bolder)
                                    .text_decoration(TextDecoration::Underline))
                                .boxed(),
                            SizedBox::new().height(24).boxed(),
                            docs_entry(
                                "Getting Started",
                                "Install the Aimer CLI and scaffold a new project in seconds. The CLI handles project creation, running, building and platform setup.",
                            ),
                            docs_entry(
                                "Widgets",
                                "Compose your UI from a declarative widget tree: Container, Row, Column, Text, Button and more. Everything is just a widget.",
                            ),
                            docs_entry(
                                "State Management",
                                "Use StatefulWidget and State with StateUpdater for reactive rebuilds, mirroring Flutter's mental model.",
                            ),
                            docs_entry(
                                "Routing",
                                "Declare routes as an enum, then navigate with the Navigator: named routes, query params, redirects and shell routes are supported.",
                            ),
                            SizedBox::new().height(48).boxed(),
                        ]))
                ))
    }
}

/// A single documentation entry: a bold title above a wrapped body paragraph.
fn docs_entry(title: &str, body: &str) -> Box<dyn Widget> {
    Container::new()
        .padding(LayoutSpacing::new().bottom(24))
        .child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Start)
                .children(vec![
                    Text::new(title.to_string())
                        .text_style(
                            TextStyle::new()
                                .font_size(26)
                                .color(Color::BLACK)
                                .font_weight(FontWeight::Bold),
                        )
                        .boxed(),
                    SizedBox::new().height(8).boxed(),
                    Text::new(body.to_string())
                        .text_style(
                            TextStyle::new()
                                .font_size(18)
                                .color(Color::BLACK.with_opacity(200))
                                .text_overflow(TextOverflow::Wrap),
                        )
                        .boxed(),
                ]),
        )
        .boxed()
}
