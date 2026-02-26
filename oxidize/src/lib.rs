pub mod widget {
    pub use widget::*;
    pub use constructor::Constructor;
    pub use container::*;
    pub use control::*;
    pub use control::gesture::button::*;
    pub use control::gesture::{AsyncCallback, Callback, CallbackHolder};
}

use ::color::prelude::Colors;
use ::widget::{Text, TextStyle};
use ::widget::text::TextAlign;
pub use engine::OxidizeApp;
pub mod color {
    pub use color::prelude::*;
}

pub fn start() {
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