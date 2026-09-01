//! Controlled choice and selection controls.
//!
//! The models remain platform-neutral: values are caller-owned, and activation
//! proposes a new value through [`events::ChangeCallback`]. Each model also
//! implements [`aimer_widget::Widget`] so pointer, keyboard, and focus adapters
//! can drive the same contract. Semantic snapshots stay independent of
//! `aimer_accessibility`; that crate may adapt [`SelectionSemantics`] when it
//! is present.
//!
//! Portable lowering is intentionally unsupported. Native widgets use the
//! default [`aimer_widget::PortableWidget`] diagnostic until a schema is added.

mod autocomplete;
mod checkbox;
mod events;
mod option;
mod radio;
mod select;
mod semantics;
mod state;
mod switch;
mod widgets;

pub use autocomplete::Autocomplete;
pub use checkbox::{Checkbox, CheckboxValue, CheckboxVisualBuilder, CheckboxVisualState};
pub use events::{ChangeCallback, ControlAction, InputEvent, Key, QueryCallback};
pub use option::{ChoiceOption, OptionError};
pub use radio::{Radio, RadioGroup};
pub use select::Select;
pub use semantics::{SelectionSemantics, SemanticRole};
pub use state::InteractionState;
pub use switch::Switch;
pub use widgets::{
    AutocompleteState, CheckboxState, RadioGroupState, RadioState, SelectState, SwitchState,
};
