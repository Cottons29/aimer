use std::time::Duration;

use aimer::style::*;
use aimer::*;

/// Starts a framework-level animated modal showcase.
pub fn start_modal_example() {
    let page = Container::new().color(Color::Rgb(31, 41, 55)).child(
        Align::new().alignment(Alignment::MidCenter).child(
            Text::new("Content behind the modal")
                .text_style(TextStyle::new().font_size(24).color(Color::WHITE)),
        ),
    );

    let dialog = Container::new()
        .width(420)
        .height(220)
        .padding(LayoutSpacing::all(Spacing::Px(28)))
        .box_decoration(
            BoxDecoration::new()
                .background_color(Color::WHITE)
                .border_radius(18),
        )
        .child(
            Text::new("Modal\n\nThis content is centered above a full-window barrier.")
                .text_align(TextAlign::MidCenter)
                .text_style(TextStyle::new().font_size(20).color(Color::BLACK)),
        );

    Modal::new()
        .barrier_color(Color::BLACK.with_opacity(60))
        .animation(
            ModalAnimation::new()
                .enter_duration(Duration::from_millis(240))
                .exit_duration(Duration::from_millis(160)),
        )
        .child(dialog)
        .show();

    AimerApp::start(page);
}
