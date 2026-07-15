use std::sync::atomic::{AtomicBool, Ordering};

use aimer::style::{
    BoxDecoration, FontWeight, LayoutSpacing, Spacing, TextDecoration, TextDecorationLine,
    TextDecorationStyle, TextOverflow, TextStyle,
};
use aimer::{
    BuildContext, Container, Dimension, Positioned, ScrollController, State, StateUpdater,
    StatefulWidget, Text, Widget, widget, *,
};

use crate::components::get_started_button::HoverableGetStartedButton;
use crate::components::same_looking::SameLookingSection;
use crate::utils::{app_padding, is_mobile, mobile_title, resp_position};

pub static SHOW_ICON: AtomicBool = AtomicBool::new(true);

#[widget(Stateful)]
#[derive(Clone)]
pub struct HomePage;

impl HomePage {
    pub fn boxing(_: &BuildContext) -> AnyWidget {
        Box::new(Self)
    }
}

pub struct HomePageState {
    pub controller: ScrollController,
    pub updater: StateUpdater<Self>,
}

impl StatefulWidget for HomePage {
    type State = HomePageState;

    fn create_state(&self) -> Self::State {
        HomePageState { controller: ScrollController::new(), updater: StateUpdater::new() }
    }
}

impl State<HomePage> for HomePageState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        let controller = &self.controller;
        let updater_clone = updater.clone();
        controller.on_scroll(move |item: Vec2d| {
            let has_change = SHOW_ICON.load(Ordering::Relaxed);
            if item.y > 150.0 {
                SHOW_ICON.store(true, Ordering::Relaxed);
            } else {
                SHOW_ICON.store(false, Ordering::Relaxed);
            }
            if has_change != SHOW_ICON.load(Ordering::Relaxed) {
                updater_clone.set_state(|_| {})
            }
        });
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        Container::new()
            .color(Color::WHITE)
            .child(
                Scrollable::new()
                    .key(key!())
                    .controller(self.controller.clone())
                    .axis(ScrollAxis::Vertical)
                    .child(Column::new().children(vec![
                        hero_section(ctx),
                        why_aimer_section(ctx),
                        polished_tooling_section(ctx),
                        SameLookingSection { key: Some("same-looking-section".into()) }.boxed(),
                        // SameLookingSection.boxed(),
                    ])),
            )
    }
}

/// The hero section: a large underlined `Aimer` title, a tagline paragraph,
/// a `Get Started` button and a version label on a white background.
fn hero_section(ctx: &BuildContext) -> AnyWidget {
    Container::new()
        .padding(app_padding(ctx))
        .color(Color::WHITE)
        .child(Column::new()
            .horizontal_alignment(BoxAlignment::Start)
            .children([
                SizedBox::new().height(24).boxed(),
                Text::new("Aimer")
                    .text_style(TextStyle::new()
                        .font_size(72)
                        .color(Color::BLACK)
                        .font_weight(FontWeight::Bolder)
                        .text_decoration(TextDecoration::Underline))
                    .boxed(),
                SizedBox::new().height(8).boxed(),
                Text::new("“Aimer, c’est choisir avec le cœur „")
                    .text_style(TextStyle::new()
                        .font_size(20)
                        .color(Color::GRAY.with_opacity(120))
                        .font_weight(FontWeight::Normal)
                        .text_decoration(TextDecoration::new()
                            .line(TextDecorationLine::ITALIC)
                            .style(TextDecorationStyle::Dashed)))
                    .boxed(),
                SizedBox::new().height(24).boxed(),
                Text::new("A cross-platform UI framework built with Rust, inspired by Flutter's widget model. Build native user interfaces from a single codebase using a declarative, composable widget tree.")
                    .text_style(TextStyle::new()
                        .font_size(22)
                        .color(Color::BLACK.with_opacity(200))
                        .text_overflow(TextOverflow::Wrap))
                    .boxed(),
                SizedBox::new().height(40).boxed(),
                Container::new()
                    .width(Dimension::Px(200.0))
                    .height(Dimension::Px(50.0))
                    .child(HoverableGetStartedButton {})
                    .boxed(),
                SizedBox::new().height(14).boxed(),
                Container::new()
                    .padding(LayoutSpacing::new()
                        .left(60))
                    .child(Text::new("Version 0.0.1")
                        .text_style(TextStyle::new()
                            .font_size(14)
                            .color(Color::GRAY)))
                    .boxed(),
            ])
        )
        .boxed()
}

/// A single inline word. `bold` words are rendered white (and bold), normal
/// words a lighter gray, so the emphasis reads even where the canvas font
/// weight is not visually distinct.
fn word(text: &str, bold: bool) -> AnyWidget {
    Text::new(text.to_string())
        .text_style(
            TextStyle::new()
                .font_size(16)
                .color(if bold { Color::WHITE } else { Color::Rgb(180, 180, 180) })
                .font_weight(if bold { FontWeight::Bolder } else { FontWeight::Normal }),
        )
        .boxed()
}

/// A feature block: a bold white title above a body of inline-emphasized text.
fn feature_block(
    title: &str,
    body: Box<dyn Widget>,
    top: impl Into<Dimension>,
    left: impl Into<Dimension>,
) -> AnyWidget {
    Positioned::new()
        // .layer(1)
        .top(top)
        .left(left)
        .child(
            Container::new()
                // width: 250,
                // height: 100,
                // .color( Color::WHITE.with_opacity(50))
                .margin(LayoutSpacing::new().bottom(Spacing::Px(14)))
                .child(
                    Column::new()
                        .horizontal_alignment(BoxAlignment::Start)
                        .children(vec![
                            Text::new(title.to_string())
                                .text_style(
                                    TextStyle::new()
                                        .font_size(24)
                                        .color(Color::WHITE)
                                        .font_weight(FontWeight::Bold),
                                )
                                .boxed(),
                            SizedBox::new().height(10).boxed(),
                            body,
                        ]),
                ),
        )
        .boxed()
}

/// The `Why Aimer ?` section: a black background, an underlined white heading
/// and five feature blocks laid out in two columns with bold inline words.
fn why_aimer_section(ctx: &BuildContext) -> AnyWidget {
    Container::new()
        .box_decoration(BoxDecoration::new().background_color(Color::BLACK))
        .padding(app_padding(ctx))
        .child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Start)
                .children([
                    Text::new("Why Aimer ?")
                        .text_style(
                            TextStyle::new()
                                .font_size(mobile_title(ctx))
                                .color(Color::WHITE)
                                .font_weight(FontWeight::Bolder)
                                .text_decoration(TextDecoration::Underline),
                        )
                        .boxed(),
                    SizedBox::new().height(48).boxed(),
                    Container::new()
                        .height(Dimension::Px(500.0))
                        .child(
                            Stack::new().children([
                                feature_block(
                                    "Type Safety",
                                    Column::new()
                                        .overflow(OverflowBehavior::Wrap)
                                        .horizontal_alignment(BoxAlignment::Start)
                                        .children(vec![
                                            Row::new().children(vec![
                                                word("Build UIs with ", false),
                                                word("Rust's", true),
                                                word(" type system.", false),
                                            ]),
                                            Row::new().children(vec![
                                                word("Catch errors at ", false),
                                                word("compile time", true),
                                                word(".", false),
                                            ]),
                                        ])
                                        .boxed(),
                                    resp_position(ctx, 16.0, 3.0),
                                    resp_position(ctx, 12.0, 0.0),
                                ),
                                feature_block(
                                    "Mobile & Desktop",
                                    Column::new()
                                        .horizontal_alignment(BoxAlignment::Start)
                                        .children([
                                            Row::new().children(vec![
                                                word("Runs on ", false),
                                                word("macOS", true),
                                                word(", ", false),
                                                word("iOS", true),
                                                word(", ", false),
                                                word("Android", true),
                                                word(",", false),
                                            ]),
                                            Row::new().children(vec![
                                                word("and ", false),
                                                word("Web", true),
                                                word(". ", false),
                                                word("Windows", true),
                                                word(" & ", false),
                                                word("Linux", true),
                                                word(" soon.", false),
                                            ]),
                                        ])
                                        .boxed(),
                                    resp_position(ctx, 45.0, 23.0),
                                    resp_position(ctx, 2.0, 0.0),
                                ),
                                feature_block(
                                    "Performance",
                                    Row::new()
                                        .children(vec![
                                            word("GPU-accelerated rendering via ", false),
                                            word("Cupid", true),
                                            word(" & ", false),
                                            word("wgpu", true),
                                            word(".", false),
                                        ])
                                        .boxed(),
                                    resp_position(ctx, 72.0, 46.0),
                                    resp_position(ctx, 32.0, 0.0),
                                ),
                                feature_block(
                                    "Crates",
                                    Row::new()
                                        .children(vec![
                                            word("Modular crates, available on ", false),
                                            word("crates.io", true),
                                            word(".", false),
                                        ])
                                        .boxed(),
                                    resp_position(ctx, 34.0, 63.0),
                                    resp_position(ctx, 52.0, 0.0),
                                ),
                                feature_block(
                                    "Consistence Looking",
                                    Column::new()
                                        .horizontal_alignment(BoxAlignment::Start)
                                        .children(vec![
                                            Row::new().children(vec![
                                                word("The same widget tree looks ", false),
                                                word("identical", true),
                                            ]),
                                            Row::new()
                                                .children(vec![word("everywhere it runs.", false)]),
                                        ])
                                        .boxed(),
                                    resp_position(ctx, 2.0, 78.0),
                                    resp_position(ctx, 52.0, 0.0),
                                ),
                            ]),
                        )
                        .boxed(),
                ]),
        )
        .boxed()
}

/// The `Polished Tooling` section: a dark-slate background with a yellow
/// underlined heading, the TUI screenshot on the left and a description with
/// bold inline words on the right.
fn polished_tooling_section(ctx: &BuildContext) -> AnyWidget {
    Container::new()
        .padding(app_padding(ctx))
        .box_decoration(BoxDecoration::new().background_color(Color::Rgb(40, 44, 52)))
        .child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Start)
                .vertical_alignment(BoxAlignment::Start)
                .children(vec![
                    SizedBox::new().height(12).boxed(),
                    Container::new()
                        .height(100)
                        .child(
                            Text::new("Polished Tooling").text_style(
                                TextStyle::new()
                                    .font_size(mobile_title(ctx))
                                    .color(Color::YELLOW)
                                    .font_weight(FontWeight::Bolder)
                                    .text_decoration(TextDecoration::Underline),
                            ),
                        )
                        .boxed(),
                    Container::new()
                        .height(if is_mobile(ctx) { 250 } else { 450 })
                        .child(AssetImage::new("assets/polished_tooling.png"))
                        .boxed(),
                    SizedBox::new().height(48).boxed(),
                ]),
        )
        .boxed()
}
