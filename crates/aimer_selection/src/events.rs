use std::rc::Rc;

/// A platform-neutral key used by choice controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Activates the focused control.
    Enter,
    /// Activates the focused control and is the canonical toggle key.
    Space,
    /// Moves focus toward the previous choice.
    ArrowUp,
    /// Moves focus toward the next choice.
    ArrowDown,
    /// Moves focus toward the previous choice in a horizontal group.
    ArrowLeft,
    /// Moves focus toward the next choice in a horizontal group.
    ArrowRight,
    /// Moves focus to the first available choice.
    Home,
    /// Moves focus to the last available choice.
    End,
    /// Cancels an open choice surface.
    Escape,
    /// Leaves the control without changing its controlled value.
    Tab,
}

/// A platform-neutral input event consumed by a choice control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    /// Begins a pointer press on the control.
    PointerDown,
    /// Finishes a pointer press and reports whether release was inside the
    /// control's hit bounds.
    PointerUp {
        /// Whether the pointer was released inside the control.
        inside: bool,
    },
    /// Delivers a key press to the control.
    KeyDown(Key),
    /// Delivers a key release to the control.
    KeyUp(Key),
    /// Cancels an in-progress pointer or keyboard interaction.
    Cancel,
}

/// The observable result of processing one choice-control event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlAction<T> {
    /// The event did not apply to the control.
    Ignored,
    /// A pointer press started.
    Pressed,
    /// A pointer press ended without activation.
    Released,
    /// Focus moved to the option at this zero-based index.
    FocusMoved(usize),
    /// The control requests that its owner adopt this new controlled value.
    Activated(T),
    /// A popup or list surface opened.
    Opened,
    /// A popup or list surface closed without a value change.
    Closed,
    /// An open popup or list surface was cancelled without a value change.
    Cancelled,
}

/// A callback invoked when a control proposes a new controlled value.
///
/// The callback is reference-counted so a rebuilt widget can retain the same
/// callback without moving application state into the control. The callback
/// runs synchronously on the caller's thread.
pub type ChangeCallback<T> = Rc<dyn Fn(T)>;

/// A callback invoked when an autocomplete query proposes new text.
pub type QueryCallback = Rc<dyn Fn(String)>;

/// Returns whether a key activates a simple choice control.
pub(crate) fn is_activation_key(key: Key) -> bool {
    matches!(key, Key::Enter | Key::Space)
}

/// Returns whether a key moves focus toward the previous option.
pub(crate) fn is_previous_key(key: Key) -> bool {
    matches!(key, Key::ArrowUp | Key::ArrowLeft)
}

/// Returns whether a key moves focus toward the next option.
pub(crate) fn is_next_key(key: Key) -> bool {
    matches!(key, Key::ArrowDown | Key::ArrowRight)
}

/// Emits a controlled change and returns the value that the owner should
/// adopt. The model itself deliberately remains unchanged.
pub(crate) fn emit_change<T: Clone>(callback: &Option<ChangeCallback<T>>, value: T) -> ControlAction<T> {
    if let Some(callback) = callback {
        callback(value.clone());
    }
    ControlAction::Activated(value)
}

/// Emits a controlled query change. The autocomplete keeps its query owned by
/// the caller, just like its selected value.
pub(crate) fn emit_query(callback: &Option<QueryCallback>, query: String) {
    if let Some(callback) = callback {
        callback(query);
    }
}
