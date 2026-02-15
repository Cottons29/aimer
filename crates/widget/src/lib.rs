use std::any::Any;
mod base_button;
use crate::{base::Vec2d, context::BuildContext, size::Size};
mod widget; 
mod builtin_shapes;
mod context;
mod position;
mod size;



pub mod base {
    pub use crate::context::BuildContext;
    pub use crate::position::Vec2d;
    pub use crate::size::Size;
    pub use crate::base_button::{ButtonTemplate, IntoButton};
}

pub mod buildin {
    pub use crate::builtin_shapes::*;
}


pub use crate::widget::Widget;
pub use crate::widget::stateful::StatefulWidget;
pub use crate::widget::stateless::StatelessWidget;


