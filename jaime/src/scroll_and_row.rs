use aimer::{AimerApp, Color, Container, Row, ScrollAxis, Scrollable, ZeroSizedBox};

pub fn test_scroll_and_row() {
    AimerApp::start(
        Scrollable::new()
            .axis(ScrollAxis::Vertical)
            .child(
                Row::new().children([Container::new()
                    .width(100)
                    .color(Color::GREEN)
                    .child(ZeroSizedBox)]),
            ),
    );
}
