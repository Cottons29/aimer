pub mod gesture;
mod input_field;
pub mod mouse_region;
/// The generic callback machinery now lives in `aimer_utils` so lower-level
/// crates (e.g. `aimer_container`) can use it without a dependency cycle. It is
/// re-exported here so existing `aimer_input::callback::*` paths keep working.
pub use aimer_utils::callback;
pub mod button;
mod text_button;

pub use text_button::TextButton;

pub mod input {
    pub use crate::input_field::controller::*;
    pub use crate::input_field::raw_fields::*;
    pub use crate::input_field::text_field::*;
}
