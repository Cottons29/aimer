// re-export all the widgets and utils


pub use aimer_attribute::dimension::Dimension;
pub use aimer_attribute::position::Vec2d;
pub use aimer_attribute::size::{ResolvedSize, Size};
pub use aimer_color::prelude::*;
pub use aimer_container::flex::row_column::*;
pub use aimer_container::flex::*;
pub use aimer_container::*;
pub use aimer_input::gesture::button::Button;
pub use aimer_input::callback::{AsyncCallback, RawInnerCallback, CallbackInner};
pub use aimer_input::input::*;
pub use aimer_input::*;
pub use aimer_quiver;
pub use aimer_quiver::AimerApp;
pub use aimer_macro::main;
pub use aimer_widget;
pub use aimer_widget::Widget;
pub use aimer_widget::base::BuildContext;
pub use aimer_widget::*;
pub use aimer_widget::{State, StatefulWidget, StatelessWidget};
pub use aimer_text::Text;
pub use aimer_assets::*;
pub use aimer_assets::img_widget::image_widget::Image;
pub use aimer_assets::img_widget::source::ImageSource;

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
    pub use aimer_macro::Constructor;
    pub use aimer_macro::WidgetConstructor;
    pub use aimer_macro::main;
    pub use aimer_macro::widget;
}

// Styling re-export
pub mod style {
    pub use aimer_animation::AnimationEffect;
    pub use aimer_animation::AnimationStatus;
    pub use aimer_animation::curve::Curve;
    pub use aimer_color::prelude::{Color, ColorMixer, Colors};
    pub use aimer_container::flex::{BoxAlignment, LayoutDirection, OverflowBehavior};
    pub use aimer_style::*;
}

// utils re-export
pub mod console {
    pub use aimer_utils::*;
}

// wasm dependencies
pub use wasm_bindgen;

pub mod router  {
    pub use aimer_router::Navigator;
    pub use aimer_router::NavigatorController;
    pub use aimer_router::Route;
    pub use aimer_router::Router;
    pub use aimer_router::navigator::NavigatorState;
    // pub use aimer_router::router::
}



