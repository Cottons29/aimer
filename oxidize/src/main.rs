use color::prelude::Colors;
use engine::OxidizeApp;
use widget::{Text, TextStyle};
use widget::text::TextAlign;

fn main() {
    OxidizeApp::start(
        Text!(
            "Hello World",
            text_align: TextAlign::MidCenter,
            text_style: TextStyle!(
                font_size: 30,
                color: Colors::Green,
            )
        )
    );
}
