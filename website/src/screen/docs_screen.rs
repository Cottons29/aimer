use aimer::style::{FontWeight, LayoutSpacing, TextDecoration, TextOverflow, TextStyle};
use aimer::*;
use aimer::{widget, BuildContext, Widget};

use crate::utils::{app_padding, mobile_title};

/// The `Docs` page rendered inside the app shell's content area.
#[widget(Stateless)]
#[derive(Clone)]
#[constructor(crate = "crate::screen::docs_screen")]
pub struct DocsPage {}

impl StatelessWidget for DocsPage {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        Container!(
            color: Colors::White,
            child: Scrollable!(
                axis: ScrollAxis::Vertical,
                child: Container!(
                    padding: app_padding(ctx),
                    child: Column!(
                        horizontal_alignment: BoxAlignment::Start,
                        children: [
                            SizedBox!(height: 24),
                            Text!(
                                "Docs",
                                text_style: TextStyle!(
                                    font_size: mobile_title(ctx),
                                    color: Colors::Black,
                                    font_weight: FontWeight::Bolder,
                                    text_decoration: TextDecoration::Underline,
                                )
                            ),
                            SizedBox!(height: 24),
                            docs_entry(
                                "Getting Started",
                                "Install the Aimer CLI and scaffold a new project in seconds. The CLI handles project creation, running, building and platform setup."
                            ),
                            docs_entry(
                                "Widgets",
                                "Compose your UI from a declarative widget tree: Container, Row, Column, Text, Button and more. Everything is just a widget."
                            ),
                            docs_entry(
                                "State Management",
                                "Use StatefulWidget and State with StateUpdater for reactive rebuilds, mirroring Flutter's mental model."
                            ),
                            docs_entry(
                                "Routing",
                                "Declare routes as an enum, then navigate with the Navigator: named routes, query params, redirects and shell routes are supported."
                            ),
                            SizedBox!(height: 48),
                        ]
                    )
                )
            )
        )
    }
}

/// A single documentation entry: a bold title above a wrapped body paragraph.
fn docs_entry(title: &str, body: &str) -> Box<dyn Widget> {
    Container!(
        padding: LayoutSpacing!(bottom: 24),
        child: Column!(
            horizontal_alignment: BoxAlignment::Start,
            children: [
                Text!(
                    title.to_string(),
                    text_style: TextStyle!(
                        font_size: 26,
                        color: Colors::Black,
                        font_weight: FontWeight::Bold,
                    )
                ),
                SizedBox!(height: 8),
                Text!(
                    body.to_string(),
                    text_style: TextStyle!(
                        font_size: 18,
                        color: Color::BLACK.with_opacity(200),
                        text_overflow: TextOverflow::Wrap,
                    )
                ),
            ]
        )
    )
}
