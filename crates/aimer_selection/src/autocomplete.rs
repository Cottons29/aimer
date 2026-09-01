use super::events::{
    ChangeCallback, ControlAction, InputEvent, Key, QueryCallback, emit_change, emit_query,
    is_activation_key, is_next_key, is_previous_key,
};
use super::option::{ChoiceOption, OptionError, validate_options};
use super::semantics::{SelectionSemantics, SemanticRole};
use super::state::InteractionState;

/// A controlled autocomplete model with stable option keys.
///
/// Query text and selected value are both caller-owned. [`Self::change_query`]
/// and [`Self::select_key`] emit proposals without mutating those controlled
/// values. The model retains only popup focus, loading, and interaction state.
pub struct Autocomplete<T> {
    options: Vec<ChoiceOption<T>>,
    query: String,
    selected: Option<T>,
    focused_index: Option<usize>,
    label: Option<String>,
    state: InteractionState,
    open: bool,
    loading: bool,
    on_change: Option<ChangeCallback<T>>,
    on_query_change: Option<QueryCallback>,
}

impl<T> Default for Autocomplete<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Autocomplete<T> {
    /// Creates a closed autocomplete with an empty query and no options.
    #[inline]
    pub fn new() -> Self {
        Self {
            options: Vec::new(),
            query: String::new(),
            selected: None,
            focused_index: None,
            label: None,
            state: InteractionState::new(),
            open: false,
            loading: false,
            on_change: None,
            on_query_change: None,
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

    /// Returns the options in source order.
    #[inline]
    pub fn options(&self) -> &[ChoiceOption<T>] {
        &self.options
    }

    /// Returns the options whose labels contain the controlled query,
    /// case-insensitively. Stable keys remain available on each returned
    /// option even when labels repeat.
    pub fn visible_options(&self) -> impl Iterator<Item = &ChoiceOption<T>> {
        let query = self.query.to_lowercase();
        self.options.iter().filter(move |option| {
            query.is_empty() || option.label().to_lowercase().contains(&query)
        })
    }

    /// Returns a copy of the controlled query.
    #[inline]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Returns an autocomplete configured with the caller-owned query.
    #[inline]
    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = query.into();
        self
    }

    /// Returns an autocomplete configured with the caller-owned selected
    /// value.
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

    /// Returns an autocomplete with an accessible label.
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

    /// Registers the callback that receives proposed query text.
    #[inline]
    pub fn on_query_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(String) + 'static,
    {
        self.on_query_change = Some(std::rc::Rc::new(callback));
        self
    }

    /// Proposes query text without changing the controlled query.
    pub fn change_query(&self, query: impl Into<String>) -> Option<String> {
        if self.state.disabled() {
            return None;
        }
        let query = query.into();
        emit_query(&self.on_query_change, query.clone());
        Some(query)
    }

    /// Returns an autocomplete with the asynchronous-loading flag set.
    #[inline]
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Returns whether options are currently loading.
    #[inline]
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// Returns an autocomplete with the supplied interaction state.
    #[inline]
    pub fn with_state(mut self, state: InteractionState) -> Self {
        self.state = state;
        self
    }

    /// Returns an autocomplete with the disabled flag set.
    #[inline]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self
    }

    /// Returns an autocomplete with the focus flag set.
    #[inline]
    pub fn focused(mut self, focused: bool) -> Self {
        self.state.set_focused(focused);
        self
    }

    /// Returns an autocomplete with the hover flag set.
    #[inline]
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.state.set_hovered(hovered);
        self
    }

    /// Returns an autocomplete with a validation error.
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

    /// Returns whether the suggestions surface is open.
    #[inline]
    pub fn is_open(&self) -> bool {
        self.open
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

    /// Opens the suggestions surface and focuses the selected or first visible
    /// enabled option.
    pub fn open_menu(&mut self) -> ControlAction<T>
    where
        T: PartialEq,
    {
        if self.state.disabled() || self.open {
            return ControlAction::Ignored;
        }
        self.open = true;
        if self
            .focused_index
            .is_none_or(|index| !self.is_available(index))
        {
            self.focused_index = self.selected_index().or_else(|| self.first_available());
        }
        ControlAction::Opened
    }

    /// Closes the suggestions surface without changing its controlled value.
    pub fn close_menu(&mut self) -> ControlAction<T> {
        if !self.open {
            return ControlAction::Ignored;
        }
        self.open = false;
        self.focused_index = None;
        ControlAction::Closed
    }

    /// Cancels the suggestions surface without changing its controlled value.
    pub fn cancel(&mut self) -> ControlAction<T> {
        if !self.open {
            return ControlAction::Ignored;
        }
        self.open = false;
        self.focused_index = None;
        ControlAction::Cancelled
    }

    /// Selects an enabled visible option by stable key while suggestions are
    /// open. Selection is unavailable while options are loading.
    pub fn select_key(&mut self, key: &str) -> ControlAction<T>
    where
        T: Clone + PartialEq,
    {
        if self.state.disabled() || !self.open || self.loading {
            return ControlAction::Ignored;
        }
        let Some(index) = self
            .options
            .iter()
            .position(|option| option.key() == key && self.matches_query(option))
        else {
            return ControlAction::Ignored;
        };
        self.select_index(index)
    }

    /// Applies a platform-neutral input event to the autocomplete trigger and
    /// suggestions surface.
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
        SelectionSemantics::from_state(SemanticRole::Autocomplete, &self.label, &self.state)
            .with_expanded(self.open)
            .with_busy(self.loading)
    }

    fn matches_query(&self, option: &ChoiceOption<T>) -> bool {
        let query = self.query.to_lowercase();
        query.is_empty() || option.label().to_lowercase().contains(&query)
    }

    fn is_available(&self, index: usize) -> bool {
        self.options
            .get(index)
            .is_some_and(|option| !option.disabled() && self.matches_query(option))
    }

    fn first_available(&self) -> Option<usize> {
        self.options
            .iter()
            .position(|option| !option.disabled() && self.matches_query(option))
    }

    fn selected_index(&self) -> Option<usize>
    where
        T: PartialEq,
    {
        self.selected.as_ref().and_then(|selected| {
            self.options.iter().position(|option| {
                option.value() == selected && !option.disabled() && self.matches_query(option)
            })
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
            .filter(|index| self.is_available(*index))
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
            self.options
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, option)| {
                    (!option.disabled() && self.matches_query(option)).then_some(index)
                })
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
        if option.disabled() || !self.matches_query(option) || self.loading {
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
