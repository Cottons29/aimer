use attribute::position::Vec2d;

/// Key actions for keyboard events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Pressed,
    Released,
    Repeat,
}

/// Modifier key state carried with keyboard events.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
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
    CharInput { ch: char, action: KeyAction, modifiers: Modifiers },

    /// A named key was pressed or released.
    KeyInput { key: NamedKey, action: KeyAction, modifiers: Modifiers },
    Cancel,
}

impl ElementEvent {
    pub fn get_pointer_pos(&self) -> Option<Vec2d> {
        match self {
            ElementEvent::PointerDown(p) | ElementEvent::PointerUp(p) | ElementEvent::PointerMove(p) => Some(*p),
            _ => None,
        }
    }
}

unsafe impl Send for ElementEvent {}
unsafe impl Sync for ElementEvent {}