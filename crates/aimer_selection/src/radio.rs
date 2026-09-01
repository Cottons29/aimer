use super::events::{
    ChangeCallback, ControlAction, InputEvent, Key, emit_change, is_activation_key, is_next_key,
    is_previous_key,
};
use super::option::{ChoiceOption, OptionError, validate_options};
use super::semantics::{SelectionSemantics, SemanticRole};
use super::state::InteractionState;

/// One controlled radio option.
///
/// A radio only proposes its value when it is not already selected. The
/// selected value remains owned by the parent or by [`RadioGroup`].
pub struct Radio<T> {
    pub(crate) value: T,
    pub(crate) selected: bool,
    pub(crate) label: Option<String>,
    pub(crate) state: InteractionState,
    pub(crate) on_change: Option<ChangeCallback<T>>,
}

impl<T> Radio<T> {
    /// Creates an unselected radio for `value`.
    #[inline]
    pub fn new(value: T) -> Self {
        Self {
            value,
            selected: false,
            label: None,
            state: InteractionState::new(),
            on_change: None,
        }
    }

    /// Returns a radio configured with its controlled selected flag.
    #[inline]
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Returns the radio's option value.
    #[inline]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns whether this radio is selected.
    #[inline]
    pub fn selected(&self) -> bool {
        self.selected
    }

    /// Returns a radio with an accessible label.
    #[inline]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Registers the callback that receives this radio's proposed value.
    #[inline]
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(T) + 'static,
    {
        self.on_change = Some(std::rc::Rc::new(callback));
        self
    }

    /// Returns a radio with the supplied interaction state.
    #[inline]
    pub fn with_state(mut self, state: InteractionState) -> Self {
        self.state = state;
        self
    }

    /// Returns a radio with the disabled flag set.
    #[inline]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self
    }

    /// Returns a radio with the focus flag set.
    #[inline]
    pub fn focused(mut self, focused: bool) -> Self {
        self.state.set_focused(focused);
        self
    }

    /// Returns a radio with the hover flag set.
    #[inline]
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.state.set_hovered(hovered);
        self
    }

    /// Returns a radio with a validation error.
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

    /// Proposes this radio's value without changing its selected flag.
    pub fn activate(&self) -> ControlAction<T>
    where
        T: Clone,
    {
        if self.state.disabled() || self.selected {
            return ControlAction::Ignored;
        }
        emit_change(&self.on_change, self.value.clone())
    }

    /// Applies a platform-neutral input event.
    pub fn handle_event(&mut self, event: InputEvent) -> ControlAction<T>
    where
        T: Clone,
    {
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
        SelectionSemantics::from_state(SemanticRole::Radio, &self.label, &self.state)
            .with_selected(self.selected)
    }
}

/// A controlled group of mutually exclusive radio options.
///
/// The group keeps only transient focus and press state. `selected` is a
/// controlled value: selecting an option invokes the callback and returns the
/// proposed value, but does not mutate the current selection.
pub struct RadioGroup<T> {
    options: Vec<ChoiceOption<T>>,
    selected: Option<T>,
    focused_index: Option<usize>,
    label: Option<String>,
    state: InteractionState,
    on_change: Option<ChangeCallback<T>>,
}

impl<T> Default for RadioGroup<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> RadioGroup<T> {
    /// Creates an empty radio group.
    #[inline]
    pub fn new() -> Self {
        Self {
            options: Vec::new(),
            selected: None,
            focused_index: None,
            label: None,
            state: InteractionState::new(),
            on_change: None,
        }
    }

    /// Replaces the options after validating their stable keys.
    pub fn try_options<I>(mut self, options: I) -> Result<Self, OptionError>
    where
        I: IntoIterator<Item = ChoiceOption<T>>,
    {
        let options: Vec<_> = options.into_iter().collect();
        validate_options(&options)?;
        self.options = options;
        self.focused_index = None;
        Ok(self)
    }

    /// Returns the options in display order.
    #[inline]
    pub fn options(&self) -> &[ChoiceOption<T>] {
        &self.options
    }

    /// Returns a group configured with its controlled selected value.
    #[inline]
    pub fn with_selected(mut self, selected: Option<T>) -> Self {
        self.selected = selected;
        self
    }

    /// Returns the current controlled selected value.
    #[inline]
    pub fn selected(&self) -> Option<&T> {
        self.selected.as_ref()
    }

    /// Returns a group with an initial focus index when that option is enabled.
    #[inline]
    pub fn with_focus_index(mut self, index: Option<usize>) -> Self {
        self.focused_index = index.filter(|index| self.is_available(*index));
        self
    }

    /// Returns the transient focused option index.
    #[inline]
    pub fn focused_index(&self) -> Option<usize> {
        self.focused_index
    }

    /// Returns a group with an accessible label.
    #[inline]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Registers the callback that receives proposed selected values.
    #[inline]
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(T) + 'static,
    {
        self.on_change = Some(std::rc::Rc::new(callback));
        self
    }

    /// Returns a group with the supplied interaction state.
    #[inline]
    pub fn with_state(mut self, state: InteractionState) -> Self {
        self.state = state;
        self
    }

    /// Returns a group with the disabled flag set.
    #[inline]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self
    }

    /// Returns a group with the focus flag set.
    #[inline]
    pub fn focused(mut self, focused: bool) -> Self {
        self.state.set_focused(focused);
        self
    }

    /// Returns a group with the hover flag set.
    #[inline]
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.state.set_hovered(hovered);
        self
    }

    /// Returns a group with a validation error.
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

    /// Activates an enabled option by its stable key.
    pub fn activate_key(&mut self, key: &str) -> ControlAction<T>
    where
        T: Clone + PartialEq,
    {
        if self.state.disabled() {
            return ControlAction::Ignored;
        }
        let Some(index) = self.options.iter().position(|option| option.key() == key) else {
            return ControlAction::Ignored;
        };
        self.focused_index = Some(index);
        self.activate_index(index)
    }

    /// Applies a platform-neutral input event, including arrow-key traversal.
    pub fn handle_event(&mut self, event: InputEvent) -> ControlAction<T>
    where
        T: Clone + PartialEq,
    {
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
                    let index = self.focused_index.or_else(|| self.first_available());
                    index.map_or(ControlAction::Ignored, |index| self.activate_index(index))
                } else {
                    ControlAction::Released
                }
            }
            InputEvent::KeyDown(key) if is_next_key(key) => {
                self.move_focus(true).map_or(ControlAction::Ignored, |index| {
                    match self.activate_index(index) {
                        ControlAction::Ignored => ControlAction::FocusMoved(index),
                        action => action,
                    }
                })
            }
            InputEvent::KeyDown(key) if is_previous_key(key) => {
                self.move_focus(false).map_or(ControlAction::Ignored, |index| {
                    match self.activate_index(index) {
                        ControlAction::Ignored => ControlAction::FocusMoved(index),
                        action => action,
                    }
                })
            }
            InputEvent::KeyDown(Key::Home) => {
                self.focus_edge(false).map_or(ControlAction::Ignored, ControlAction::FocusMoved)
            }
            InputEvent::KeyDown(Key::End) => {
                self.focus_edge(true).map_or(ControlAction::Ignored, ControlAction::FocusMoved)
            }
            InputEvent::KeyDown(key) if is_activation_key(key) => {
                let index = self.focused_index.or_else(|| self.first_available());
                index.map_or(ControlAction::Ignored, |index| self.activate_index(index))
            }
            InputEvent::Cancel => {
                self.state.set_pressed(false);
                ControlAction::Cancelled
            }
            InputEvent::KeyDown(_) | InputEvent::KeyUp(_) => ControlAction::Ignored,
        }
    }

    /// Builds a platform-neutral semantic snapshot for the group.
    pub fn semantics(&self) -> SelectionSemantics {
        SelectionSemantics::from_state(SemanticRole::RadioGroup, &self.label, &self.state)
    }

    fn is_available(&self, index: usize) -> bool {
        self.options.get(index).is_some_and(|option| !option.disabled())
    }

    fn first_available(&self) -> Option<usize> {
        self.options.iter().position(|option| !option.disabled())
    }

    fn selected_index(&self) -> Option<usize>
    where
        T: PartialEq,
    {
        self.selected.as_ref().and_then(|selected| {
            self.options
                .iter()
                .position(|option| option.value() == selected && !option.disabled())
        })
    }

    fn move_focus(&mut self, forward: bool) -> Option<usize>
    where
        T: PartialEq,
    {
        let length = self.options.len();
        if length == 0 {
            return None;
        }
        let current = self
            .focused_index
            .or_else(|| self.selected_index())
            .unwrap_or(if forward { length - 1 } else { 0 });
        for step in 1..=length {
            let index = if forward {
                (current + step) % length
            } else {
                (current + length - step % length) % length
            };
            if self.is_available(index) {
                self.focused_index = Some(index);
                return Some(index);
            }
        }
        None
    }

    fn focus_edge(&mut self, last: bool) -> Option<usize> {
        let index = if last {
            self.options.iter().rposition(|option| !option.disabled())
        } else {
            self.first_available()
        }?;
        self.focused_index = Some(index);
        Some(index)
    }

    fn activate_index(&self, index: usize) -> ControlAction<T>
    where
        T: Clone + PartialEq,
    {
        let Some(option) = self.options.get(index) else {
            return ControlAction::Ignored;
        };
        if option.disabled() || self.selected.as_ref().is_some_and(|selected| selected == option.value()) {
            return ControlAction::Ignored;
        }
        emit_change(&self.on_change, option.value().clone())
    }
}
