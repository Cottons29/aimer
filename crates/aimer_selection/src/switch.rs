use super::checkbox::CheckboxValue;
use super::events::{ChangeCallback, ControlAction, InputEvent, emit_change, is_activation_key};
use super::semantics::{SelectionSemantics, SemanticRole};
use super::state::InteractionState;

/// A controlled binary switch.
///
/// `Switch` is the canonical API name; the crate intentionally does not
/// define a duplicate `Toggle` model.
pub struct Switch {
    pub(crate) value: bool,
    pub(crate) label: Option<String>,
    pub(crate) state: InteractionState,
    pub(crate) on_change: Option<ChangeCallback<bool>>,
}

impl Default for Switch {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Switch {
    /// Creates an off switch with no callback or accessible label.
    #[inline]
    pub fn new() -> Self {
        Self {
            value: false,
            label: None,
            state: InteractionState::new(),
            on_change: None,
        }
    }

    /// Returns a switch configured with the caller-owned value.
    #[inline]
    pub fn with_value(mut self, value: bool) -> Self {
        self.value = value;
        self
    }

    /// Returns the current controlled value.
    #[inline]
    pub fn value(&self) -> bool {
        self.value
    }

    /// Returns a switch with an accessible label.
    #[inline]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Registers the callback that receives proposed controlled values.
    #[inline]
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(bool) + 'static,
    {
        self.on_change = Some(std::rc::Rc::new(callback));
        self
    }

    /// Returns a switch with the supplied interaction state.
    #[inline]
    pub fn with_state(mut self, state: InteractionState) -> Self {
        self.state = state;
        self
    }

    /// Returns a switch with the disabled flag set.
    #[inline]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self
    }

    /// Returns a switch with the focus flag set.
    #[inline]
    pub fn focused(mut self, focused: bool) -> Self {
        self.state.set_focused(focused);
        self
    }

    /// Returns a switch with the hover flag set.
    #[inline]
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.state.set_hovered(hovered);
        self
    }

    /// Returns a switch with a validation error.
    #[inline]
    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.state.set_error(Some(error.into()));
        self
    }

    /// Returns the live interaction state.
    #[inline]
    pub fn interaction_state(&self) -> &InteractionState {
        &self.state
    }

    /// Proposes the opposite value without changing the current value.
    pub fn activate(&self) -> ControlAction<bool> {
        if self.state.disabled() {
            return ControlAction::Ignored;
        }
        emit_change(&self.on_change, !self.value)
    }

    /// Applies a platform-neutral input event.
    pub fn handle_event(&mut self, event: InputEvent) -> ControlAction<bool> {
        if self.state.disabled() {
            self.state.set_pressed(false);
            return ControlAction::Ignored;
        }

        match event {
            InputEvent::PointerDown => {
                self.state.set_pressed(true);
                ControlAction::Pressed
            }
            InputEvent::PointerUp { inside } => {
                let was_pressed = self.state.pressed();
                self.state.set_pressed(false);
                if was_pressed && inside {
                    self.activate()
                } else {
                    ControlAction::Released
                }
            }
            InputEvent::KeyDown(key) if is_activation_key(key) => self.activate(),
            InputEvent::Cancel => {
                self.state.set_pressed(false);
                ControlAction::Cancelled
            }
            InputEvent::KeyDown(_) | InputEvent::KeyUp(_) => ControlAction::Ignored,
        }
    }

    /// Builds a platform-neutral semantic snapshot.
    pub fn semantics(&self) -> SelectionSemantics {
        let checked = if self.value {
            CheckboxValue::Checked
        } else {
            CheckboxValue::Unchecked
        };
        SelectionSemantics::from_state(SemanticRole::Switch, &self.label, &self.state)
            .with_checked(checked)
    }
}
