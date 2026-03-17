use attribute::position::Vec2d;

/// Key actions for keyboard events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Pressed,
    Released,
    Repeat,
}

/// Named (non-text) keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NamedKey {
    Backspace,
    Delete,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    Enter,
    Escape,
    Tab,
    Other(String),
}

/// Pointer and keyboard event types for dispatch.
#[derive(Clone, Debug)]
pub enum ElementEvent {
    PointerDown(Vec2d),
    PointerUp(Vec2d),
    PointerMove(Vec2d),
    Scroll(Vec2d),
    /// A character was typed (text input).
    CharInput { ch: char, action: KeyAction },

    /// A named key was pressed or released.
    KeyInput { key: NamedKey, action: KeyAction },
    Cancel,
}
unsafe impl Send for ElementEvent {}
unsafe impl Sync for ElementEvent {}