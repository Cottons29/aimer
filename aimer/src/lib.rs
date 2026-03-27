// re-export all the widgets and utils

pub use animation::*;
pub use attribute::dimension::Dimension;
pub use attribute::position::Vec2d;
pub use attribute::size::{ResolvedSize, Size};
pub use color::prelude::*;
pub use container::flex::row_column::*;
pub use container::flex::*;
pub use container::*;
pub use control::gesture::button::{Button, ButtonStyle};
pub use control::gesture::{AsyncCallback, Callback, CallbackHolder};
pub use control::input::*;
pub use control::*;
pub use quiver;
pub use quiver::AimerApp;
pub use aimer_main::main;
pub use widget;
pub use widget::Widget;
pub use widget::base::BuildContext;
pub use widget::text::Text;
pub use widget::*;
pub use widget::{State, StatefulWidget, StatelessWidget};
pub use media::*;
pub use media::single_frame::image_widget::Image;
pub use media::single_frame::source::ImageSource;

// Macro re-export
pub mod macros {
    pub use constructor::Constructor;
    pub use constructor::WidgetConstructor;
    pub use aimer_main::main;
    pub use widget::widget_attr::widget;
}

// Styling re-export
pub mod style {
    pub use animation::AnimationEffect;
    pub use animation::AnimationStatus;
    pub use animation::curve::Curve;
    pub use color::prelude::{Color, ColorMixer, Colors};
    pub use container::flex::{BoxAlignment, LayoutDirection, OverflowBehavior};
    pub use widget::style::BoxConstraint;
    pub use widget::style::border::*;
    pub use widget::style::layout_spacing::{LayoutSpacing, Spacing};
    pub use widget::style::text_style::*;
    pub use widget::text::{TextAlign, TextOverflow, TextStyle};
}

// utils re-export
pub mod console {
    pub use utils::*;
}

// wasm dependencies
pub use wasm_bindgen;

pub use router;
pub use router::Navigator;
pub use router::NavigatorController;
pub use router::Route;
pub use router::Router;
