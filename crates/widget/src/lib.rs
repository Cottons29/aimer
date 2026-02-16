mod base_button;
mod widget; 
mod builtin_shapes;
mod attribute;
pub mod components;
pub mod base {
    pub use crate::components::context::BuildContext;
    pub use crate::attribute::position::Vec2d;
    pub use crate::attribute::size::Size;
    pub use crate::base_button::{ButtonTemplate, IntoButton};
    pub use color::prelude::*;
}




pub mod buildin {
    pub use crate::builtin_shapes::*;
}


pub use crate::widget::Widget;
pub use crate::widget::stateful::StatefulWidget;
pub use crate::widget::stateless::StatelessWidget;


