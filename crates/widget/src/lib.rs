mod attribute;
pub mod components;
mod widget;
pub mod base {
    pub use crate::attribute::position::Vec2d;
    pub use crate::attribute::size::Size;
    pub use crate::components::context::BuildContext;
    pub use color::prelude::*;
}
pub use crate::widget::Widget;
pub use crate::components::element::Element;
pub use crate::widget::stateful::{StatefulElement, StatefulWidget, State};
pub use crate::widget::stateless::{StatelessElement, StatelessWidget};

pub use widget_attr;
pub use constructor::Constructor;
