pub use aimer_assets::img_widget::image_widget::Image;
pub use aimer_assets::img_widget::source::ImageSource;
pub use aimer_assets::*;
pub use aimer_attribute::dimension::Dimension;
pub use aimer_attribute::position::Vec2d;
pub use aimer_attribute::size::{ResolvedSize, Size};
pub use aimer_color::prelude::*;
pub use aimer_container::flex::row_column::*;
pub use aimer_container::flex::*;
pub use aimer_container::*;
pub use aimer_input::button::Button;
pub use aimer_input::callback::{AsyncCallback, CallbackInner, RawInnerCallback};
pub use aimer_input::input::*;
pub use aimer_input::*;
pub use aimer_macro::key;
pub use aimer_macro::main;
pub use aimer_quiver as quiver;
pub use aimer_quiver;
pub use aimer_quiver::AimerApp;
pub use aimer_text::Text;
pub use aimer_widget;
pub use aimer_widget::Key;
pub use aimer_widget::Widget;
pub use aimer_widget::base::BuildContext;
pub use aimer_widget::*;
pub use aimer_widget::{State, StatefulWidget, StatelessWidget};

pub mod widget {
    pub use aimer_widget::Widget;
    pub use aimer_widget::base::BuildContext;
    pub use aimer_widget::*;

    pub use aimer_widget::{State, StatefulWidget, StatelessWidget};
}

pub mod animation {
    pub use aimer_animation::*;
}

// Macro re-export
pub mod macros {
    pub use aimer_macro::key;
    pub use aimer_macro::main;
    pub use aimer_macro::widget;
}

// Styling re-export
pub mod style {
    pub use aimer_animation::AnimationEffect;
    pub use aimer_animation::AnimationStatus;
    pub use aimer_animation::curve::Curve;
    pub use aimer_color::prelude::{Color, Colors};
    pub use aimer_container::flex::{BoxAlignment, LayoutDirection, OverflowBehavior};
    pub use aimer_style::*;
}

// utils re-export
pub mod console {
    pub use aimer_utils::*;
}

// wasm dependencies
pub use wasm_bindgen;

pub use aimer_provider as provider;
pub use aimer_router as router;
