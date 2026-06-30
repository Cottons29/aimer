// ---------------------------------------------------------------------------
// Hoverable Get Started button
// ---------------------------------------------------------------------------

use aimer::console::debug;
use aimer::style::{BoxDecoration, FontWeight, TextAlign, TextDecoration, TextStyle};
use aimer::*;
use aimer::{BuildContext, State, StateUpdater, StatefulWidget, Widget, widget};
use aimer::mouse_region::RawMouseRegion;

#[widget(Stateful)]
pub struct HoverableGetStartedButton {}

pub struct HoverableGetStartedButtonState {
    updater: StateUpdater<Self>,
    is_hovered: bool,
    is_inside: bool,
}

impl StatefulWidget for HoverableGetStartedButton {
    type State = HoverableGetStartedButtonState;

    fn create_state(&self) -> Self::State {
        HoverableGetStartedButtonState { updater: StateUpdater::empty(), is_hovered: false, is_inside: false }
    }
}

impl State<HoverableGetStartedButton> for HoverableGetStartedButtonState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        let is_hovered = self.is_hovered;
        let bg_color = if is_hovered { Colors::Black } else { Colors::Green };
        let label = format!("Is inside ({})", self.is_inside);


        Column!(
            children: [
                Container!(
                    // box_decoration: BoxDecoration!(background_color: bg_color),
                    child: Text!(
                        &*label,
                        text_align: TextAlign::MidCenter,
                        text_style: TextStyle!(
                            color: Colors::Black,
                            font_size: 18,
                            font_weight: FontWeight::Bold,
                            text_decoration: TextDecoration::Underline,
                        )
                    )
                ),
                Container!(
                    child: Button!(
                        on_press: move || {
                            println!("Button pressed");
                        },
                        child: Container!(
                            box_decoration: BoxDecoration!(background_color: bg_color),
                            child: Text!(
                                label,
                                text_align: TextAlign::MidCenter,
                                text_style: TextStyle!(
                                    color: Colors::White,
                                    font_size: 18,
                                    font_weight: FontWeight::Bold,
                                    text_decoration: TextDecoration::Underline,
                                )
                            )
                        )
                    )
                )
            ]
        )


    }
}
