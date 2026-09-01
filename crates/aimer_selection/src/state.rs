/// Interaction state shared by every choice control.
///
/// `disabled`, `focused`, `hovered`, and `error` are safe to rebuild from the
/// owning application model. `pressed` is transient and is updated by
/// [`crate::InputEvent`] handling.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InteractionState {
    disabled: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
    error: Option<String>,
}

impl InteractionState {
    /// Creates the enabled, unfocused, non-hovered, non-pressed state.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether interaction is disabled.
    #[inline]
    pub fn disabled(&self) -> bool {
        self.disabled
    }

    /// Returns whether the control currently owns keyboard focus.
    #[inline]
    pub fn focused(&self) -> bool {
        self.focused
    }

    /// Returns whether the pointer is currently over the control.
    #[inline]
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    /// Returns whether a pointer press is in progress.
    #[inline]
    pub fn pressed(&self) -> bool {
        self.pressed
    }

    /// Returns the validation error associated with the control, if any.
    #[inline]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Returns a copy of this state with its disabled flag set.
    #[inline]
    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        if disabled {
            self.pressed = false;
        }
        self
    }

    /// Returns a copy of this state with its focus flag set.
    #[inline]
    pub fn with_focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Returns a copy of this state with its hover flag set.
    #[inline]
    pub fn with_hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    /// Returns a copy of this state carrying a validation error.
    #[inline]
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Returns a copy of this state without a validation error.
    #[inline]
    pub fn without_error(mut self) -> Self {
        self.error = None;
        self
    }

    /// Updates the disabled flag in a retained control state.
    #[inline]
    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
        if disabled {
            self.pressed = false;
        }
    }

    /// Updates the focus flag in a retained control state.
    #[inline]
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Updates the hover flag in a retained control state.
    #[inline]
    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    /// Sets or clears the validation error in a retained control state.
    #[inline]
    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    pub(crate) fn set_pressed(&mut self, pressed: bool) {
        self.pressed = pressed;
    }
}
