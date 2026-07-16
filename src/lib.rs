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
pub use aimer_macro::{key, main};
pub use aimer_quiver as quiver;
pub use aimer_quiver::{
    self, AimerApp, FIRST_FRAME_RENDERED_EVENT, HeadlessAimerApp, HeadlessOptions,
    set_first_frame_rendered_callback,
};
pub use aimer_svg::{
    RawSvg, Svg, SvgCallback, SvgColor, SvgDiagnostic, SvgDocument, SvgError, SvgFillRule, SvgHit,
    SvgLimits, SvgLoadState, SvgLoader, SvgNodeId, SvgNodeMetadata, SvgPath, SvgSelector,
    SvgSource, SvgStyle, SvgTransform,
};
pub use aimer_text::{RichText, SpanStyle, Text, TextButton, TextSpan};
pub use aimer_widget::base::BuildContext;
pub use aimer_widget::{self, Key, State, StatefulWidget, StatelessWidget, Widget, *};

pub mod widget {
    pub use aimer_widget::base::BuildContext;
    pub use aimer_widget::{State, StatefulWidget, StatelessWidget, Widget, *};
}

pub mod animation {
    pub use aimer_animation::*;
}

// Macro re-export
pub mod macros {
    pub use aimer_macro::{key, main, widget};
}

// Styling re-export
pub mod style {
    pub use aimer_animation::primitives::curve::Curve;
    pub use aimer_animation::{AnimationEffect, AnimationStatus};
    pub use aimer_color::prelude::{Color, Colors};
    pub use aimer_container::flex::{BoxAlignment, LayoutDirection, OverflowBehavior};
    pub use aimer_style::*;
}

// utils re-export
pub mod console {
    pub use aimer_utils::*;
}

// wasm dependencies
pub use aimer_provider as provider;
pub use aimer_router as router;
pub use wasm_bindgen;
