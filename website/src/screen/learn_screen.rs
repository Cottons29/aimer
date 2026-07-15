use aimer::style::{FontWeight, LayoutSpacing, TextDecoration, TextOverflow, TextStyle};
use aimer::{BuildContext, Widget, widget, *};

use crate::utils::{app_padding, mobile_title};

/// The `Learn` page rendered inside the app shell's content area.
#[widget(Stateless)]
#[derive(Clone)]
pub struct LearnPage;

impl LearnPage {
    pub fn boxing(_: &BuildContext) -> Box<dyn Widget> {
        Box::new(Self)
    }
}

impl StatelessWidget for LearnPage {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        Container::new()
            .color(Color::WHITE)
            .child(
                Scrollable::new()
                .axis(ScrollAxis::Vertical)
                    .child(Container::new()
                        .padding(app_padding(ctx))
                        .child(Column::new()
                            .horizontal_alignment(BoxAlignment::Start)
                            .children(vec![
                                SizedBox::new().height(24).boxed(),
                                Text::new("Learn")
                                    .text_style(TextStyle::new()
                                        .font_size(mobile_title(ctx))
                                        .color(Color::BLACK)
                                        .font_weight(FontWeight::Bolder)
                                        .text_decoration(TextDecoration::Underline)
                                    )
                                    .boxed(),
                                SizedBox::new().height(24).boxed(),
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
                                SizedBox::new().height(48).boxed(),
                            ]
                            )
                        )
                    )
            )
    }
}

/// A single learning step: a bold title above a wrapped body paragraph.
fn learn_step(title: &str, body: &str) -> Box<dyn Widget> {
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
                                .color(Color::BLUE)
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
