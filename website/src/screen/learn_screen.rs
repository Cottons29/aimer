use aimer::style::{FontWeight, LayoutSpacing, TextDecoration, TextOverflow, TextStyle};
use aimer::*;
use aimer::{widget, BuildContext, Widget};

use crate::utils::{app_padding, mobile_title};

/// The `Learn` page rendered inside the app shell's content area.
#[widget(Stateless)]
#[derive(Clone)]
#[constructor(crate = "crate::screen::learn_screen")]
pub struct LearnPage {}

impl StatelessWidget for LearnPage {
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
                                "Learn",
                                text_style: TextStyle!(
                                    font_size: mobile_title(ctx),
                                    color: Colors::Black,
                                    font_weight: FontWeight::Bolder,
                                    text_decoration: TextDecoration::Underline,
                                )
                            ),
                            SizedBox!(height: 24),
                            learn_step(
                                "1. Think in Widgets",
                                "Every piece of UI is a widget. Learn how Container, Row and Column combine to build any layout you can imagine."
                            ),
                            learn_step(
                                "2. Make it Reactive",
                                "Hold mutable data in a State, call set_state through a StateUpdater, and Aimer rebuilds only what changed."
                            ),
                            learn_step(
                                "3. Add Navigation",
                                "Wrap your app in a Navigator and move between pages with push and pop, just like a native navigation stack."
                            ),
                            learn_step(
                                "4. Ship Everywhere",
                                "The same widget tree runs on macOS, iOS, Android and the Web from a single Rust codebase."
                            ),
                            SizedBox!(height: 48),
                        ]
                    )
                )
            )
        )
    }
}

/// A single learning step: a bold title above a wrapped body paragraph.
fn learn_step(title: &str, body: &str) -> Box<dyn Widget> {
    Container!(
        padding: LayoutSpacing!(bottom: 24),
        child: Column!(
            horizontal_alignment: BoxAlignment::Start,
            children: [
                Text!(
                    title.to_string(),
                    text_style: TextStyle!(
                        font_size: 26,
                        color: Colors::Blue,
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
