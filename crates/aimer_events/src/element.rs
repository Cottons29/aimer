pub use winit::event::TouchPhase;
use aimer_attribute::position::Vec2d;
use crate::pointer::PointerSource;

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
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    PageUp,
    PageDown,
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
    /// Pointer down. The `u64` is the touch finger ID (0 for mouse).
    PointerDown(Vec2d, PointerSource, u64),
    /// Pointer up. The `u64` is the touch finger ID (0 for mouse).
    PointerUp(Vec2d, PointerSource, u64),
    /// Pointer move. The `u64` is the touch finger ID (0 for mouse).
    PointerMove(Vec2d, PointerSource, u64),
    Scroll{delta: Vec2d, phase: TouchPhase},
    /// A character was typed (text input).
    CharInput { ch: char, action: KeyAction, modifiers: Modifiers },

    /// A named key was pressed or released.
    KeyInput { key: NamedKey, action: KeyAction, modifiers: Modifiers },
    /// IME pre-edit (composition in progress). `text` is the composing string.
    /// `cursor` is the byte range of the active composing segment.
    ImePreedit { text: String, cursor: Option<(usize, usize)> },
    Cancel,
}

impl ElementEvent {
    pub fn get_pointer_pos(&self) -> Option<Vec2d> {
        match self {
            ElementEvent::PointerDown(p, _, _) | ElementEvent::PointerUp(p, _, _) | ElementEvent::PointerMove(p, _, _) => Some(*p),
            _ => None,
        }
    }
}

unsafe impl Send for ElementEvent {}
unsafe impl Sync for ElementEvent {}