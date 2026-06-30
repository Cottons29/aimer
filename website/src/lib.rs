pub mod get_started_button;

use crate::get_started_button::HoverableGetStartedButton;
use aimer::animation::{Curve as AnimCurve, MorphTransition, Rgba};
use aimer::console::{debug, info};
use aimer::macros::widget;
use aimer::provider::media_query::MediaQuery;
use aimer::style::*;
use aimer::*;
use std::time::Duration;

// this is the entry point of the app
#[main]
pub fn my_app() {
    AimerApp::start(HomePage {})
}

#[widget(Stateless)]
struct HomePage {}

fn app_padding(ctx: &BuildContext) -> LayoutSpacing {
    let window_size = MediaQuery::of(ctx).size;
    // info!("window size: {:?}", window_size);
    let horizontal_padding = 20f64;
    LayoutSpacing!(
        left: horizontal_padding.into(),
        right: horizontal_padding.into(),
        top: Spacing::Px(90),
        bottom: Spacing::Px(90)
    )
}

impl StatelessWidget for HomePage {
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        Container!(
            child: Scrollable!(
                axis: ScrollAxis::Vertical,
                // horizontal_scroll_bar: ScrollBar!(),
                child: Column!(
                    children: [
                        hero_section(ctx),
                        why_aimer_section(ctx),
                        polished_tooling_section(ctx),
                        same_looking_section(ctx),
                        // SizedBox!(height: 500),
                        // hero_section(ctx),
                        // why_aimer_section(ctx),
                        // polished_tooling_section(ctx),
                        // same_looking_section(ctx),
                        // SizedBox!(height: 500),
                        // hero_section(ctx),
                        // why_aimer_section(ctx),
                        // polished_tooling_section(ctx),
                        // same_looking_section(ctx),
                        // SizedBox!(height: 500),
                        // hero_section(ctx),
                        // why_aimer_section(ctx),
                        // polished_tooling_section(ctx),
                        // same_looking_section(ctx),
                        // SizedBox!(height: 500),
                        // hero_section(ctx),
                    ]
                )
            )
        )
    }
}

/// The hero section: a large underlined `Aimer` title, a tagline paragraph,
/// a `Get Started` button and a version label on a white background.
fn hero_section(ctx: &BuildContext) -> Box<dyn Widget> {
    let string = String::from("Aimer");
    Container!(
        padding: app_padding(ctx),
        color: Colors::White,
        child: Column!(
            horizontal_alignment: BoxAlignment::Start,
            children: [
                Text!(
                    string.as_str(),
                    text_style: TextStyle!(
                        font_size: 72,
                        color: Colors::Black,
                        font_weight: FontWeight::Bolder,
                        text_decoration: TextDecoration::Underline,
                    )
                ),
                SizedBox!(height: 24),
                Text!(
                    "A cross-platform UI framework built with Rust, inspired by Flutter's widget model. Build native user interfaces from a single codebase using a declarative, composable widget tree.",
                    text_style: TextStyle!(
                        font_size: 22,
                        color: Colors::Gray,
                        text_overflow: TextOverflow::Wrap
                    ),
                ),
                SizedBox!(height: 40),
                Container!(
                    width: Dimension::Px(200.0),
                    height: Dimension::Px(156.0),
                    child: HoverableGetStartedButton!()
                ),
                SizedBox!(height: 14),
                Text!(
                    "Version 0.0.1",
                    text_style: TextStyle!(
                        font_size: 14,
                        color: Colors::Gray,
                    )
                ),
            ]
        )
    )
}

/// A single inline word. `bold` words are rendered white (and bold), normal
/// words a lighter gray, so the emphasis reads even where the canvas font
/// weight is not visually distinct.
fn word(text: &str, bold: bool) -> Box<dyn Widget> {
    Text!(
        text.to_string(),
        text_style: TextStyle!(
            font_size: 16,
            color: if bold { Color::Basic(Colors::White) } else { Color::Rgb(180, 180, 180) },
            font_weight: if bold { FontWeight::Bold } else { FontWeight::Normal },
        )
    )
}

/// A feature block: a bold white title above a body of inline-emphasized text.
fn feature_block(title: &str, body: Box<dyn Widget>) -> Box<dyn Widget> {
    Container!(
        margin: LayoutSpacing!(bottom: Spacing::Px(34)),
        child: Column!(
            horizontal_alignment: BoxAlignment::Start,
            children: [
                Text!(
                    title.to_string(),
                    text_style: TextStyle!(
                        font_size: 24,
                        color: Colors::White,
                        font_weight: FontWeight::Bold,
                    )
                ),
                SizedBox!(height: 10),
                body,
            ]
        )
    )
}

/// The `Why Aimer ?` section: a black background, an underlined white heading
/// and five feature blocks laid out in two columns with bold inline words.
fn why_aimer_section(ctx: &BuildContext) -> Box<dyn Widget> {
    Container!(
        box_decoration: BoxDecoration!(background_color: Colors::Black),
        padding: app_padding(ctx),
        child: Column!(
            horizontal_alignment: BoxAlignment::Start,
            children: [
                Text!(
                    "Why Aimer ?",
                    text_style: TextStyle!(
                        font_size: 48,
                        color: Colors::White,
                        font_weight: FontWeight::Bolder,
                        text_decoration: TextDecoration::Underline,
                    )
                ),
                SizedBox!(height: 48),
                // The two feature columns are layered inside a Stack and placed
                // with Positioned: the first column is anchored to the left edge
                // and the second is offset to the horizontal centre. The Stack is
                // wrapped in a fixed-height Container because Positioned children
                // do not contribute to the Stack's intrinsic size.
                Container!(
                    height: Dimension::Px(400.0),
                    child: Stack!(
                        children: [
                            Positioned!(
                                top: 0.0,
                                left: 0.0,
                                child: Container!(
                                    width: Dimension::Percent(50.0),
                                    padding: LayoutSpacing!(right: Spacing::Px(24)),
                                    child: Column!(
                                        horizontal_alignment: BoxAlignment::Start,
                                        children: [
                                            feature_block(
                                                "Type Safety",
                                                Column!(
                                                    horizontal_alignment: BoxAlignment::Start,
                                                    children: [
                                                        Row!(children: [
                                                            word("Build UIs with ", false),
                                                            word("Rust's", true),
                                                            word(" type system.", false),
                                                        ]),
                                                        Row!(children: [
                                                            word("Catch errors at ", false),
                                                            word("compile time", true),
                                                            word(".", false),
                                                        ]),
                                                    ]
                                                )
                                            ),
                                            feature_block(
                                                "Mobile & Desktop",
                                                Column!(
                                                    horizontal_alignment: BoxAlignment::Start,
                                                    children: [
                                                        Row!(children: [
                                                            word("Runs on ", false),
                                                            word("macOS", true),
                                                            word(", ", false),
                                                            word("iOS", true),
                                                            word(", ", false),
                                                            word("Android", true),
                                                            word(",", false),
                                                        ]),
                                                        Row!(children: [
                                                            word("and ", false),
                                                            word("Web", true),
                                                            word(". ", false),
                                                            word("Windows", true),
                                                            word(" & ", false),
                                                            word("Linux", true),
                                                            word(" soon.", false),
                                                        ]),
                                                    ]
                                                )
                                            ),
                                            feature_block(
                                                "Performance",
                                                Row!(children: [
                                                    word("GPU-accelerated rendering via ", false),
                                                    word("Cupid", true),
                                                    word(" & ", false),
                                                    word("wgpu", true),
                                                    word(".", false),
                                                ])
                                            ),
                                        ]
                                    )
                                )
                            ),
                            Positioned!(
                                top: 0.0,
                                left: Dimension::Percent(50.0),
                                child: Container!(
                                    width: Dimension::Percent(50.0),
                                    padding: LayoutSpacing!(left: Spacing::Px(24)),
                                    child: Column!(
                                        horizontal_alignment: BoxAlignment::Start,
                                        children: [
                                            feature_block(
                                                "Crates",
                                                Row!(children: [
                                                    word("Modular crates, available on ", false),
                                                    word("crates.io", true),
                                                    word(".", false),
                                                ])
                                            ),
                                            feature_block(
                                                "Consistence Looking",
                                                Column!(
                                                    horizontal_alignment: BoxAlignment::Start,
                                                    children: [
                                                        Row!(children: [
                                                            word("The same widget tree looks ", false),
                                                            word("identical", true),
                                                        ]),
                                                        Row!(children: [
                                                            word("everywhere it runs.", false),
                                                        ]),
                                                    ]
                                                )
                                            ),
                                        ]
                                    )
                                )
                            ),
                        ]
                    )
                ),
            ]
        )
    )
}

/// The `Polished Tooling` section: a dark-slate background with a yellow
/// underlined heading, the TUI screenshot on the left and a description with
/// bold inline words on the right.
fn polished_tooling_section(ctx: &BuildContext) -> Box<dyn Widget> {
    Container!(
        padding: app_padding(ctx),
        box_decoration: BoxDecoration!(background_color: Color::Rgb(40, 44, 52)),
        child: Column!(
            horizontal_alignment: BoxAlignment::Start,
            vertical_alignment: BoxAlignment::Start,
            children: [
                SizedBox!(height: 12),
                Container!(
                    height: 100,
                    child: Text!(
                        "Polished Tooling",
                        text_style: TextStyle!(
                            font_size: 48,
                            color: Colors::Yellow,
                            font_weight: FontWeight::Bolder,
                            text_decoration: TextDecoration::Underline,
                        )
                    ),
                ),
                Container!(
                    height : 450,
                    child: AssetImage!(
                        "assets/polished_tooling.png",
                    )
                ),

                SizedBox!(height: 48)
            ]
        )
    )
}

/// A small window-control dot for the browser-mock title bar.
fn window_dot(color: Color) -> Box<dyn Widget> {
    Box::new(Container!(
        width: Dimension::Px(12.0),
        height: Dimension::Px(12.0),
        margin: LayoutSpacing!(left: Spacing::Px(8)),
        box_decoration: BoxDecoration!(
            background_color: color,
            border_radius: (6, 6, 6, 6),
        ),
        child: SizedBox!()
    ))
}

/// A platform name in the footer row; the active platform (`Web`) is bold/black.
fn platform_label(text: &str, active: bool) -> Box<dyn Widget> {
    Text!(
        text.to_string(),
        text_style: TextStyle!(
            font_size: if active {28} else {18},
            color: if active { Color::Basic(Colors::Black) } else { Color::Rgb(150, 150, 150) },
            font_weight: if active { FontWeight::Bolder } else { FontWeight::Normal },
        )
    )
}


/// The `Same Looking Everywhere` section: an underlined heading, a rounded
/// browser-mock frame embedding the live counter demo and a platform row.
fn same_looking_section(ctx: &BuildContext) -> Box<dyn Widget> {
    Container!(
        color: Colors::White,
        padding: app_padding(ctx),
        child: Column!(
            horizontal_alignment: BoxAlignment::Center,
            children: [
                Container!(
                    height: 100,
                    child: Text!(
                        "Same Looking Everywhere",
                        text_style: TextStyle!(
                            font_size: 44,
                            color: Colors::Black,
                            font_weight: FontWeight::Bolder,
                            text_decoration: TextDecoration::Underline,
                        )
                    ),
                ),
                SizedBox!(height: 24),
                Container!(
                    height: 600,
                    child: AssetImage!(
                        "assets/web_screenshot.png",
                    )
                ),
                SizedBox!(height: 40),
                Row!(
                    horizontal_alignment: BoxAlignment::Center,
                    vertical_alignment: BoxAlignment::Center,
                    gaps: LayoutSpacing::horizontal(Spacing::Px(16)),
                    children: [
                        platform_label("macOS", false),
                        platform_label("iOS", false),
                        platform_label("Web", true),
                        platform_label("Android", false),
                        platform_label("Windows", false),
                    ]
                ),
            ]
        )
    )
}
