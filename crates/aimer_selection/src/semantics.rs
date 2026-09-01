use super::checkbox::CheckboxValue;
use super::state::InteractionState;

/// The semantic role exposed by a choice control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticRole {
    /// A binary or tri-state checkbox.
    Checkbox,
    /// A binary switch.
    Switch,
    /// One radio option.
    Radio,
    /// A group of mutually exclusive radio options.
    RadioGroup,
    /// A closed or expanded select control.
    Select,
    /// A queryable select control.
    Autocomplete,
}

/// Platform-neutral semantics emitted by a selection control.
///
/// W1 can adapt this value to a platform accessibility tree. Keeping the
/// snapshot independent of a renderer means a control remains useful before
/// that adapter is integrated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectionSemantics {
    role: SemanticRole,
    label: Option<String>,
    enabled: bool,
    focused: bool,
    hovered: bool,
    pressed: bool,
    checked: Option<CheckboxValue>,
    selected: Option<bool>,
    expanded: Option<bool>,
    busy: bool,
    error: Option<String>,
}

impl SelectionSemantics {
    /// Returns the semantic role.
    #[inline]
    pub fn role(&self) -> SemanticRole {
        self.role
    }

    /// Returns the accessible label, if one was supplied.
    #[inline]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns whether the control is enabled.
    #[inline]
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns whether the control is focused.
    #[inline]
    pub fn focused(&self) -> bool {
        self.focused
    }

    /// Returns whether the pointer is over the control.
    #[inline]
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    /// Returns whether a pointer press is in progress.
    #[inline]
    pub fn pressed(&self) -> bool {
        self.pressed
    }

    /// Returns the checked or tri-state value, when the role supports it.
    #[inline]
    pub fn checked(&self) -> Option<CheckboxValue> {
        self.checked
    }

    /// Returns the selected state, when the role supports it.
    #[inline]
    pub fn selected(&self) -> Option<bool> {
        self.selected
    }

    /// Returns the popup-expanded state, when the role supports it.
    #[inline]
    pub fn expanded(&self) -> Option<bool> {
        self.expanded
    }

    /// Returns whether the control is waiting for asynchronous options.
    #[inline]
    pub fn busy(&self) -> bool {
        self.busy
    }

    /// Returns the validation error, if any.
    #[inline]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn from_state(
        role: SemanticRole,
        label: &Option<String>,
        state: &InteractionState,
    ) -> Self {
        Self {
            role,
            label: label.clone(),
            enabled: !state.disabled(),
            focused: state.focused(),
            hovered: state.hovered(),
            pressed: state.pressed(),
            checked: None,
            selected: None,
            expanded: None,
            busy: false,
            error: state.error().map(str::to_owned),
        }
    }

    pub(crate) fn with_checked(mut self, checked: CheckboxValue) -> Self {
        self.checked = Some(checked);
        self
    }

    pub(crate) fn with_selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    pub(crate) fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }

    pub(crate) fn with_busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }
}
