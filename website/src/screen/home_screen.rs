use crate::components::get_started_button::HoverableGetStartedButton;
use crate::components::same_looking::SameLookingSection;
#[cfg(test)]
use crate::router::AppRouter;
use crate::utils::{app_padding, is_mobile, mobile_title, resp_position};
#[cfg(test)]
use crate::{CURRENT_INDEX, TEST_STATE_UPDATED};
#[cfg(test)]
use aimer::router::NavigatorController;
use aimer::style::{
    BorderSlice, BorderStyle, BoxBorder, BoxDecoration, FontWeight, LayoutSpacing, Spacing,
    TextDecoration, TextDecorationLine, TextDecorationStyle, TextOverflow, TextStyle, Theme,
    ThemeData,
};
use aimer::{
    BuildContext, Container, Dimension, Positioned, ScrollController, State, StateUpdater,
    StatefulWidget, Text, Widget, widget, *,
};
use std::sync::atomic::{AtomicBool, Ordering};

pub static SHOW_ICON: AtomicBool = AtomicBool::new(true);

#[widget(Stateful)]
#[derive(Clone)]
pub struct HomePage;

impl HomePage {
    pub fn boxing(_: &BuildContext) -> AnyWidget {
        Self.boxed()
    }
}

pub struct HomePageState {
    pub controller: ScrollController,
    pub updater: StateUpdater<Self>,
}

impl StatefulWidget for HomePage {
    type State = HomePageState;

    fn create_state(&self) -> Self::State {
        HomePageState {
            controller: ScrollController::new(),
            updater: StateUpdater::new(),
        }
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
        let theme = ThemeData::of(ctx);
        #[cfg(test)]
        {
            let is_navigated = TEST_STATE_UPDATED.load(Ordering::Relaxed);
            if !is_navigated {
                let navigator = NavigatorController::<AppRouter>::of(ctx);
                std::thread::spawn(move || {
                    navigator.push(AppRouter::Blog);
                    TEST_STATE_UPDATED.store(true, Ordering::Relaxed);
                });
            }
        }
        Container::new()
            .color(theme.background_color)
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
/// a `Get Started` button and a version label on the themed background.
fn hero_section(ctx: &BuildContext) -> AnyWidget {
    let theme = ThemeData::of(ctx);

    Container::new()
        .padding(app_padding(ctx))
        .color(theme.background_color)
        .child(Column::new()
            .horizontal_alignment(BoxAlignment::Start)
            .children([
                SizedBox::new().height(24).boxed(),
                Text::new("Aimer")
                    .text_style(TextStyle::new()
                        .font_size(72)
                        .color(theme.on_background_color)
                        .font_weight(FontWeight::Bolder)
                        .text_decoration(TextDecoration::Underline))
                    .boxed(),
                SizedBox::new().height(8).boxed(),
                Text::new("“Aimer, c’est choisir avec le cœur „")
                    .text_style(TextStyle::new()
                        .font_size(20)
                        .color(theme.on_background_color.with_opacity(120))
                        .font_weight(FontWeight::Normal)
                        .text_decoration(TextDecoration::new()
                            .line(TextDecorationLine::ITALIC)
                            .style(TextDecorationStyle::Dashed)))
                    .boxed(),
                SizedBox::new().height(24).boxed(),
                Text::new("A cross-platform UI framework built with Rust, inspired by Flutter's widget model. Build native user interfaces from a single codebase using a declarative, composable widget tree.")
                    .text_style(TextStyle::new()
                        .font_size(22)
                        .color(theme.on_background_color.with_opacity(200))
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
                            .color(theme.on_background_color.with_opacity(150))))
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
                .color(if bold {
                    Color::WHITE
                } else {
                    Color::Rgb(180, 180, 180)
                })
                .font_weight(if bold {
                    FontWeight::Bolder
                } else {
                    FontWeight::Normal
                }),
        )
        .boxed()
}

/// A feature block: a bold white title above a body of inline-emphasized text.
fn feature_block(
    title: &str,
    body: AnyWidget,
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
                            SizedBox::new()
                                .height(10)
                                .boxed(),
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
                    SizedBox::new()
                        .height(48)
                        .boxed(),
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
                                    Row::new().box_children(vec![
                                        word("GPU-accelerated rendering via ", false),
                                        word("Cupid", true),
                                        word(" & ", false),
                                        word("wgpu", true),
                                        word(".", false),
                                    ]),
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

fn tooling_grid_tracks(mobile: bool) -> (Vec<GridTrack>, Vec<GridTrack>) {
    if mobile {
        (vec![GridTrack::Fr(1.0)], vec![GridTrack::Auto; 6])
    } else {
        (vec![GridTrack::Fr(1.0); 3], vec![GridTrack::Auto; 2])
    }
}

const TOOLING_FEATURES: [(&str, &str); 6] = [
    (
        "create",
        "Scaffold a new Aimer app with its workspace, assets, and platform files ready.",
    ),
    (
        "run",
        "Launch on desktop, web, or a connected device from one consistent command.",
    ),
    (
        "build",
        "Compile optimized artifacts for the platform and profile you choose.",
    ),
    (
        "doctor",
        "Check toolchains, platform dependencies, and devices with actionable diagnostics.",
    ),
    (
        "assemble",
        "Package release-ready bundles from your compiled Aimer application.",
    ),
    (
        "migrate",
        "Keep existing projects aligned as Aimer templates and APIs evolve.",
    ),
];

fn tooling_card(title: &str, description: &str, mobile: bool) -> AnyWidget {
    let border = BorderSlice::new()
        .stroke(Dimension::Px(1.0))
        .color(Color::WHITE.with_opacity(72))
        .style(BorderStyle::Solid);

    Container::new()
        .height(Dimension::Px(if mobile { 160.0 } else { 200.0 }))
        .padding(LayoutSpacing::all(Spacing::Px(if mobile {
            20
        } else {
            28
        })))
        .box_decoration(BoxDecoration::new().border(BoxBorder::all(border)))
        .child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Start)
                .children([
                    Text::new(title.to_string())
                        .text_style(
                            TextStyle::new()
                                .font_size(if mobile { 21 } else { 24 })
                                .color(Color::YELLOW)
                                .font_weight(FontWeight::Bolder)
                                .text_decoration(TextDecoration::Underline),
                        )
                        .boxed(),
                    SizedBox::new()
                        .height(14)
                        .boxed(),
                    Text::new(description.to_string())
                        .text_style(
                            TextStyle::new()
                                .font_size(if mobile { 15 } else { 17 })
                                .color(Color::WHITE.with_opacity(210))
                                .text_overflow(TextOverflow::Wrap),
                        )
                        .boxed(),
                ]),
        )
        .boxed()
}

/// The `Polished Tooling` section: six `aimer_cli` capabilities in a
/// responsive three-by-two grid that becomes a single column on mobile.
fn polished_tooling_section(ctx: &BuildContext) -> AnyWidget {
    let mobile = is_mobile(ctx);
    let (columns, rows) = tooling_grid_tracks(mobile);

    Container::new()
        .padding(app_padding(ctx))
        .box_decoration(BoxDecoration::new().background_color(Color::Rgb(40, 44, 52)))
        .child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Start)
                .vertical_alignment(BoxAlignment::Start)
                .children(vec![
                    SizedBox::new()
                        .height(12)
                        .boxed(),
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
                    Grid::new()
                        .columns(columns)
                        .rows(rows)
                        .children(
                            TOOLING_FEATURES
                                .into_iter()
                                .map(|(title, description)| {
                                    GridItem::new(tooling_card(title, description, mobile))
                                }),
                        )
                        .boxed(),
                    SizedBox::new()
                        .height(48)
                        .boxed(),
                ]),
        )
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::{TOOLING_FEATURES, tooling_grid_tracks};

    #[test]
    fn tooling_grid_uses_three_columns_on_desktop_and_one_on_mobile() {
        let (desktop_columns, desktop_rows) = tooling_grid_tracks(false);
        let (mobile_columns, mobile_rows) = tooling_grid_tracks(true);

        assert_eq!(desktop_columns.len(), 3);
        assert_eq!(desktop_rows.len(), 2);
        assert_eq!(mobile_columns.len(), 1);
        assert_eq!(mobile_rows.len(), 6);
        assert_eq!(TOOLING_FEATURES.len(), 6);
    }
}
