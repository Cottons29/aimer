mod attribute;
pub mod components;
mod widget;
pub mod text;
pub mod style;
pub mod base {
    pub use crate::attribute::position::Vec2d;
    pub use crate::attribute::size::Size;
    pub use crate::attribute::dimension::Dimension;
    pub use crate::components::context::BuildContext;
    pub use color::prelude::*;
}
pub use crate::widget::Widget;
pub use crate::components::element::Element;
pub use crate::widget::stateful::{StatefulElement, StatefulWidget, State};
pub use crate::widget::stateless::{StatelessElement, StatelessWidget};
pub use crate::text::Text;
pub use crate::style::text_style::TextStyle;
pub use crate::style::layout_spacing::{LayoutSpacing, Spacing};
pub use widget_attr;
pub use constructor::Constructor;

