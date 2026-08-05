use std::time::Duration;

use aimer::animation::{AnimatedBuilder, AnimationController, Curve, RotationTransition};
use aimer::console::info;
use aimer::style::{FontWeight, TextAlign, TextStyle};
use aimer::{
    AimerApp, AnyWidget, BoxAlignment, Color, Column, Container, Dimension, Opacity, SizedBox, Svg,
    SvgDocument, SvgError, Text, Widget,
};

const LOADING_CYCLE: Duration = Duration::from_millis(800);
const LOADING_ICON_SIZE: f32 = 32.0;

fn loading_controller() -> AnimationController {
    let controller = AnimationController::new(LOADING_CYCLE, Curve::Linear);
    controller.set_repeat(true);
    controller.set_curve(Curve::EaseInOut);
    controller.forward_from_first_tick();
    controller
}

fn loading_icon_document() -> Result<SvgDocument, SvgError> {
    SvgDocument::from_svg(include_bytes!(
        "../../assets/loading-spinner-svgrepo-com.svg"
    ))
}

/// Starts a centered loading indicator driven by an infinitely repeating
/// animation.
///
/// [`RotationTransition`] advances the controller on each rendered frame and
/// rotates the bundled SVG icon around its center. The linear cycle repeats
/// until the application closes, demonstrating a loading state that needs no
/// timers or manual redraw requests.
pub fn build_loading_indicator(text: &'static str) -> AnyWidget {
    let controller = loading_controller();
    let icon =
        Svg::new(loading_icon_document().expect("the bundled loading icon SVG should be valid"))
            .width(Dimension::Px(LOADING_ICON_SIZE))
            .height(Dimension::Px(LOADING_ICON_SIZE));

    const LOADING_CYCLE: Duration = Duration::from_millis(400);

    let loop_controller = AnimationController::new(LOADING_CYCLE, Curve::Linear);
    loop_controller.set_repeat(true);
    loop_controller.set_auto_reverse(true);
    loop_controller.forward_from_first_tick();

    let loading = AnimatedBuilder::new(loop_controller, move |item| {
        // info!("Item: {item}");
        Opacity::new().opacity(item).child(
            Text::new(text).text_align(TextAlign::MidCenter).text_style(
                TextStyle::new()
                    .font_size(13)
                    .font_weight(FontWeight::Bold)
                    .color(Color::BLACK),
            ),
        )
    });

    Container::new()
        .height(LOADING_ICON_SIZE * 2f32)
        .color(Color::Transparent)
        .box_child(
            Column::new()
                .horizontal_alignment(BoxAlignment::Center)
                .vertical_alignment(BoxAlignment::Center)
                .children(vec![
                    RotationTransition::new(controller, icon).boxed(),
                    SizedBox::new().height(12).boxed(),
                    loading.boxed(),
                ]),
        )
}
