use crate::components::get_started_button::HoverableGetStartedButton;
use crate::components::same_looking::SameLookingSection;
use aimer::style::{BoxDecoration, FontWeight, LayoutSpacing, Spacing, TextDecoration, TextDecorationLine, TextDecorationStyle, TextOverflow, TextStyle};
use aimer::*;
use aimer::{widget, BuildContext, Container, Dimension, Positioned, ScrollController, State, StateUpdater, StatefulWidget, Text, Widget};
use crate::utils::{app_padding, is_mobile, mobile_title, resp_position};

#[widget(Stateful)]
#[derive(Clone)]
#[constructor(crate = "crate::screen::home_screen")]
pub struct HomePage {}

pub struct HomePageState {
    pub controller: ScrollController,
    pub updater: StateUpdater<Self>,
}

impl StatefulWidget for HomePage {
    type State = HomePageState;

    fn create_state(&self) -> Self::State {
        let controller = ScrollController::new();
        HomePageState { controller, updater: StateUpdater::new() }
    }
}

impl State<HomePage> for HomePageState {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        // The persistent header lives in the app shell above this page, so the
        // home page only renders its scrollable content in the shell's content
        // area.
        Container!(
            color: Color::WHITE,
            child: Scrollable!(
                controller: self.controller.clone(),
                axis: ScrollAxis::Vertical,
                child: Column!(
                    children: [
                        hero_section(ctx),
                        why_aimer_section(ctx),
                        polished_tooling_section(ctx),
                        Box::new(SameLookingSection{}),
                    ]
                )
            )
        )
    }
}

// Column!(
//     children: [
//         HeaderSection!(),
//         hero_section(ctx),
//         why_aimer_section(ctx),
//         polished_tooling_section(ctx),
//         SameLookingSection!(),
//         //--------------
//
//     ]
// )


/// The hero section: a large underlined `Aimer` title, a tagline paragraph,
/// a `Get Started` button and a version label on a white background.
fn hero_section(ctx: &BuildContext) -> Box<dyn Widget> {
    Container!(
        padding: app_padding(ctx),
        color: Colors::White,
        child: Column!(
            horizontal_alignment: BoxAlignment::Start,
            children: [

                SizedBox!(height: 24),
                Text!(
                    "Aimer",
                    text_style: TextStyle!(
                        font_size: 72,
                        color: Colors::Black,
                        font_weight: FontWeight::Bolder,
                        text_decoration: TextDecoration::Underline,
                    )
                ),

                SizedBox!(height: 8),

                Text!(
                    "“Aimer, c’est choisir avec le cœur „",
                    text_style: TextStyle!(
                        font_size: 20,
                        color: Color::GRAY.with_opacity(120),
                        font_weight: FontWeight::Normal,
                        text_decoration: TextDecoration!(
                            line: TextDecorationLine::ITALIC,
                            style: TextDecorationStyle::Dashed,
                        ),
                    )
                ),


                SizedBox!(height: 24),
                Text!(
                    "A cross-platform UI framework built with Rust, inspired by Flutter's widget model. Build native user interfaces from a single codebase using a declarative, composable widget tree.",
                    text_style: TextStyle!(
                        font_size: 22,
                        color: Color::BLACK.with_opacity(200),
                        text_overflow: TextOverflow::Wrap
                    ),
                ),
                SizedBox!(height: 40),
                Container!(
                    width: Dimension::Px(200.0),
                    height: Dimension::Px(50.0),
                    child: HoverableGetStartedButton{}
                ),
                SizedBox!(height: 14),
                Container!(
                    padding: LayoutSpacing!(
                        left: 60,
                    ),
                    child: Text!(
                        "Version 0.0.1",
                        text_style: TextStyle!(
                            font_size: 14,
                            color: Colors::Gray,
                        )
                    ),
                )
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
            font_weight: if bold { FontWeight::Bolder } else { FontWeight::Normal },
        )
    )
}

/// A feature block: a bold white title above a body of inline-emphasized text.
fn feature_block(title: &str, body: Box<dyn Widget>, top: impl Into<Dimension>, left: impl Into<Dimension>) -> Box<dyn Widget> {
    Positioned!(
        top: top,
        left: left,
        child: Container!(
            // width: 250,
            // height: 100,
            // color: Color::WHITE.with_opacity(5),
            margin: LayoutSpacing!(bottom: Spacing::Px(14)),
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
    )
}

/// The `Why Aimer ?` section: a black background, an underlined white heading
/// and five feature blocks laid out in two columns with bold inline words.
fn why_aimer_section(ctx: &BuildContext) -> Box<dyn Widget> {
    Container!(
        box_decoration: BoxDecoration!(
            background_color: Color::BLACK
        ),
        padding: app_padding(ctx),
        child: Column!(
            horizontal_alignment: BoxAlignment::Start,
            children: [
                Text!(
                    "Why Aimer ?",
                    text_style: TextStyle!(
                        font_size: mobile_title(ctx),
                        color: Colors::White,
                        font_weight: FontWeight::Bolder,
                        text_decoration: TextDecoration::Underline,
                    )
                ),
                SizedBox!(height: 48),
                Container!(
                    height: Dimension::Px(500.0),
                    child: Stack!(
                        // horizontal_alignment: BoxAlignment::Start,
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
                                ),
                                resp_position(ctx, 16.0, 3.0),
                                resp_position(ctx, 12.0, 0.0)
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
                                ),
                                 resp_position(ctx, 45.0, 23.0),
                                resp_position(ctx, 2.0, 0.0)
                            ),
                            feature_block(
                                "Performance",
                                Row!(children: [
                                    word("GPU-accelerated rendering via ", false),
                                    word("Cupid", true),
                                    word(" & ", false),
                                    word("wgpu", true),
                                    word(".", false),
                                ]),
                                resp_position(ctx, 72.0, 46.0),
                                resp_position(ctx, 32.0, 0.0)
                            ),
                            feature_block(
                                "Crates",
                                Row!(children: [
                                    word("Modular crates, available on ", false),
                                    word("crates.io", true),
                                    word(".", false),
                                ]),
                                resp_position(ctx, 34.0, 63.0),
                                resp_position(ctx, 52.0, 0.0)
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
                                ),
                                resp_position(ctx, 2.0, 78.0),
                                resp_position(ctx, 52.0, 0.0)
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
                            font_size: mobile_title(ctx),
                            color: Colors::Yellow,
                            font_weight: FontWeight::Bolder,
                            text_decoration: TextDecoration::Underline,
                        )
                    ),
                ),
                Container!(
                    height : if is_mobile(ctx) { 250 } else { 450 },
                    child: AssetImage!(
                        "assets/polished_tooling.png",
                    )
                ),

                SizedBox!(height: 48)
            ]
        )
    )
}
