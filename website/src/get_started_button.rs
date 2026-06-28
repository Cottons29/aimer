
// ---------------------------------------------------------------------------
// Hoverable Get Started button
// ---------------------------------------------------------------------------

use aimer::{widget, BuildContext, State, StateUpdater, StatefulWidget, Widget};
use aimer::*;
use aimer::console::debug;
use aimer::style::{BoxDecoration, FontWeight, TextAlign, TextDecoration, TextStyle};

#[widget(Stateful)]
pub struct HoverableGetStartedButton {}

pub struct HoverableGetStartedButtonState {
    updater: StateUpdater<Self>,
    is_hovered: bool,
}

impl StatefulWidget for HoverableGetStartedButton {
    type State = HoverableGetStartedButtonState;

    fn create_state(&self) -> Self::State {
        HoverableGetStartedButtonState {
            updater: StateUpdater::empty(),
            is_hovered: false,
        }
    }
}

impl State<HoverableGetStartedButton> for HoverableGetStartedButtonState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        let updater = self.updater.clone();
        let updater2 = self.updater.clone();
        let is_hovered = self.is_hovered;

        let label = if is_hovered { "Click me!" } else { "Get Started" };
        let bg_color = if !is_hovered {
            Colors::Green
        } else {
            Colors::Black
        };

        debug!("is_hovered: {}",  is_hovered);

        // The Button sits directly inside the stateful widget — no wrapper
        // that would replace the MouseRegion and lose hover state on rebuild.
        Button!(
            on_press: || {
                println!("Get Started pressed");
            },
            on_hover_enter: move || {
                updater.set_state(|s| { s.is_hovered = true; });
            },
            on_hover_exit: move || {
                updater2.set_state(|s| { s.is_hovered = false; });
            },
            child: Container!(
                color: bg_color,
                // box_decoration: BoxDecoration!(background_color: bg_color),
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
    }
}