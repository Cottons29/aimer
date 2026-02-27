// re-export all the widgets and utils

pub mod widget {
    pub use widget::*;
    pub use constructor::Constructor;
    pub use container::*;
    pub use control::*;
    pub use control::gesture::button::*;
    pub use control::gesture::{AsyncCallback, Callback, CallbackHolder};
}




pub mod color {
    pub use color::prelude::*;
}

