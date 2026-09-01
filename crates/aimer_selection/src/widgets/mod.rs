//! Widget adapters for the choice-control models.
//!
//! Each public model implements [`aimer_widget::Widget`] by retaining only
//! transient hover/press/open state. Controlled values stay with the caller.

mod binary;
mod chrome;
mod group;
mod keys;

pub use binary::{CheckboxState, RadioState, SwitchState};
pub use group::{AutocompleteState, RadioGroupState, SelectState};
