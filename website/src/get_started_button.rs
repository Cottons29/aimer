// ---------------------------------------------------------------------------
// Hoverable Get Started button
// ---------------------------------------------------------------------------

use aimer::style::{BoxDecoration, FontWeight, LayoutSpacing, TextAlign, TextDecoration, TextStyle};
use aimer::*;
use aimer::{BuildContext, State, StateUpdater, StatefulWidget, Widget, widget};

#[widget(Stateful)]
pub struct HoverableGetStartedButton {}

pub struct HoverableGetStartedButtonState {
    updater: StateUpdater<Self>,
}

impl StatefulWidget for HoverableGetStartedButton {
    type State = HoverableGetStartedButtonState;

    fn create_state(&self) -> Self::State {
        HoverableGetStartedButtonState { updater: StateUpdater::empty() }
    }
}

impl State<HoverableGetStartedButton> for HoverableGetStartedButtonState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        Container!(
            child: Button!(
                decoration: BoxDecoration!(
                    background_color: Colors::Black,
                    border_radius: 8,
                ),
                on_press: {
                    move || {
                        println!("Button pressed");
                        let url = "https://github.com/Cottons29/aimer";
                        if let Err(e) = webbrowser::open(url) {
                            eprintln!("Failed to open browser: {}", e);
                        }
                    }
                },
                child: Row!(
                    vertical_alignment: BoxAlignment::Center,
                    horizontal_alignment: BoxAlignment::Center,
                    children: [
                        Box::new(AssetImage!(
                            "assets/github-svgrepo-com.png",
                            width: 24,
                            height: 24,
                        )),

                        SizedBox!(width: 20),

                        Text!(
                            "Get Started!",
                            text_align: TextAlign::MidCenter,
                            text_style: TextStyle!(
                                color: Colors::White,
                                font_size: 18,
                                font_weight: FontWeight::Bold,
                                // text_decoration: TextDecoration::Underline,
                            )
                        )
                    ]
                )
            )
        )
    }
}
