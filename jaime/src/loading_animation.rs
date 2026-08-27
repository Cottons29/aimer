use std::time::Duration;

use aimer::animation::{AnimationController, Curve, RotationTransition};
use aimer::style::{FontWeight, TextAlign, TextStyle};
use aimer::{
    AimerApp, BoxAlignment, Column, Container, Dimension, SizedBox, Svg, SvgDocument,
    SvgError, Text, Widget,
};

use crate::theme;

const LOADING_CYCLE: Duration = Duration::from_millis(1500);
const LOADING_ICON_SIZE: f32 = 96.0;

fn loading_controller() -> AnimationController {
    let controller = AnimationController::new(LOADING_CYCLE, Curve::Linear);
    controller.set_repeat(true);
    controller.set_curve(Curve::EaseInOut);
    controller.forward_from_first_tick();
    controller
}

fn loading_icon_document() -> Result<SvgDocument, SvgError> {
    SvgDocument::from_svg(include_bytes!("../assets/loading-1-svgrepo-com.svg"))
}

/// Builds a centered loading indicator driven by an infinitely repeating
/// animation.
///
/// [`RotationTransition`] advances the controller on each rendered frame and
/// rotates the bundled SVG icon around its center. The linear cycle repeats
/// until the application closes, demonstrating a loading state that needs no
/// timers or manual redraw requests.
pub fn loading_animation_example() -> impl Widget {
    let app_theme = theme::app_theme();
    let controller = loading_controller();
    let icon =
        Svg::new(loading_icon_document().expect("the bundled loading icon SVG should be valid"))
            .width(Dimension::Px(LOADING_ICON_SIZE))
            .height(Dimension::Px(LOADING_ICON_SIZE));

    Container::new()
            .width(Dimension::Percent(100.0))
            .height(Dimension::Percent(100.0))
            .color(app_theme.background_color)
            .child(
                Column::new()
                    .horizontal_alignment(BoxAlignment::Center)
                    .vertical_alignment(BoxAlignment::Center)
                    .children(vec![
                        RotationTransition::new(controller, icon).boxed(),
                        SizedBox::new().height(16).boxed(),
                        Text::new("Loading...")
                            .text_align(TextAlign::MidCenter)
                            .text_style(
                                TextStyle::new()
                                    .font_size(24)
                                    .font_weight(FontWeight::Bold)
                                    .color(app_theme.on_background_color),
                            )
                            .boxed(),
                    ]),
            )
}

pub fn start_loading_animation_example() {
    AimerApp::start(crate::theme::provide(loading_animation_example()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_loading_icon_is_valid_svg() {
        assert!(loading_icon_document().is_ok());
    }

    #[test]
    fn loading_controller_starts_and_repeats() {
        let controller = loading_controller();

        assert!(controller.repeat());
        assert!(controller.is_animating());
    }
}
