use super::events::{
    ChangeCallback, ControlAction, InputEvent, Key, emit_change, is_activation_key, is_next_key,
    is_previous_key,
};
use super::option::{ChoiceOption, OptionError, validate_options};
use super::semantics::{SelectionSemantics, SemanticRole};
use super::state::InteractionState;

/// A controlled select model with stable option keys.
///
/// Opening and focus are transient control state. The selected value remains
/// owned by the caller; [`Self::select_key`] invokes the callback and returns
/// the candidate value without changing [`Self::selected`].
pub struct Select<T> {
    options: Vec<ChoiceOption<T>>,
    selected: Option<T>,
    focused_index: Option<usize>,
    label: Option<String>,
    state: InteractionState,
    open: bool,
    on_change: Option<ChangeCallback<T>>,
}

impl<T> Default for Select<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Select<T> {
    /// Creates a closed select with no options.
    #[inline]
    pub fn new() -> Self {
        Self {
            options: Vec::new(),
            selected: None,
            focused_index: None,
            label: None,
            state: InteractionState::new(),
            open: false,
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

    /// Returns a select configured with the caller-owned selected value.
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

    /// Returns a select with an initial focus index when that option is enabled.
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

    /// Returns the stable key of the transiently focused option.
    #[inline]
    pub fn focused_key(&self) -> Option<&str> {
        self.focused_index
            .and_then(|index| self.options.get(index))
            .map(ChoiceOption::key)
    }

    /// Returns whether the select surface is currently open.
    #[inline]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Returns a select with an accessible label.
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

    /// Returns a select with the supplied interaction state.
    #[inline]
    pub fn with_state(mut self, state: InteractionState) -> Self {
        self.state = state;
        self
    }

    /// Returns a select with the disabled flag set.
    #[inline]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self
    }

    /// Returns a select with the focus flag set.
    #[inline]
    pub fn focused(mut self, focused: bool) -> Self {
        self.state.set_focused(focused);
        self
    }

    /// Returns a select with the hover flag set.
    #[inline]
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.state.set_hovered(hovered);
        self
    }

    /// Returns a select with a validation error.
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

    /// Opens the select and focuses the selected or first enabled option.
    pub fn open_menu(&mut self) -> ControlAction<T>
    where
        T: PartialEq,
    {
        if self.state.disabled() || self.open {
            return ControlAction::Ignored;
        }
        self.open = true;
        if self.focused_index.is_none_or(|index| !self.is_available(index)) {
            self.focused_index = self.selected_index().or_else(|| self.first_available());
        }
        ControlAction::Opened
    }

    /// Closes an open select without changing its controlled value.
    pub fn close_menu(&mut self) -> ControlAction<T> {
        if !self.open {
            return ControlAction::Ignored;
        }
        self.open = false;
        self.focused_index = None;
        ControlAction::Closed
    }

    /// Cancels an open select without changing its controlled value.
    pub fn cancel(&mut self) -> ControlAction<T> {
        if !self.open {
            return ControlAction::Ignored;
        }
        self.open = false;
        self.focused_index = None;
        ControlAction::Cancelled
    }

    /// Selects an enabled option by stable key while the surface is open.
    pub fn select_key(&mut self, key: &str) -> ControlAction<T>
    where
        T: Clone + PartialEq,
    {
        if self.state.disabled() || !self.open {
            return ControlAction::Ignored;
        }
        let Some(index) = self.options.iter().position(|option| option.key() == key) else {
            return ControlAction::Ignored;
        };
        self.select_index(index)
    }

    /// Applies a platform-neutral input event to the select trigger and menu.
    pub fn handle_event(&mut self, event: InputEvent) -> ControlAction<T>
    where
        T: Clone + PartialEq,
    {
        if self.state.disabled() {
            self.state.set_pressed(false);
            self.open = false;
            self.focused_index = None;
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
                    if self.open {
                        self.close_menu()
                    } else {
                        self.open_menu()
                    }
                } else {
                    ControlAction::Released
                }
            }
            InputEvent::KeyDown(Key::Escape) => self.cancel(),
            InputEvent::KeyDown(Key::Tab) => self.close_menu(),
            InputEvent::KeyDown(key) if is_activation_key(key) => {
                if self.open {
                    let index = self.focused_index.or_else(|| self.first_available());
                    index.map_or(ControlAction::Ignored, |index| self.select_index(index))
                } else {
                    self.open_menu()
                }
            }
            InputEvent::KeyDown(key) if is_next_key(key) => {
                if self.open {
                    self.move_focus(true).map_or(ControlAction::Ignored, ControlAction::FocusMoved)
                } else {
                    self.open_menu()
                }
            }
            InputEvent::KeyDown(key) if is_previous_key(key) => {
                if self.open {
                    self.move_focus(false).map_or(ControlAction::Ignored, ControlAction::FocusMoved)
                } else {
                    self.open_menu()
                }
            }
            InputEvent::KeyDown(Key::Home) if self.open => {
                self.focus_edge(false).map_or(ControlAction::Ignored, ControlAction::FocusMoved)
            }
            InputEvent::KeyDown(Key::End) if self.open => {
                self.focus_edge(true).map_or(ControlAction::Ignored, ControlAction::FocusMoved)
            }
            InputEvent::Cancel => {
                self.state.set_pressed(false);
                self.cancel()
            }
            InputEvent::KeyDown(_) | InputEvent::KeyUp(_) => ControlAction::Ignored,
        }
    }

    /// Builds a platform-neutral semantic snapshot.
    pub fn semantics(&self) -> SelectionSemantics {
        SelectionSemantics::from_state(SemanticRole::Select, &self.label, &self.state)
            .with_expanded(self.open)
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
            .or_else(|| self.first_available())?;
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
        Some(current)
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

    fn select_index(&mut self, index: usize) -> ControlAction<T>
    where
        T: Clone + PartialEq,
    {
        let Some(option) = self.options.get(index) else {
            return ControlAction::Ignored;
        };
        if option.disabled() {
            return ControlAction::Ignored;
        }
        let value = option.value().clone();
        self.focused_index = Some(index);
        self.open = false;
        if self.selected.as_ref().is_some_and(|selected| selected == &value) {
            return ControlAction::Closed;
        }
        emit_change(&self.on_change, value)
    }
}
