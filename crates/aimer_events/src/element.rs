use std::path::PathBuf;
use aimer_attribute::position::Vec2d;
pub use winit::event::TouchPhase;

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

/// Identifies whether a scroll delta came from a stepped wheel or a
/// pixel-precise input device such as a trackpad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollDeltaKind {
    /// A discrete line-based mouse-wheel delta.
    Line,
    /// A pixel-precise trackpad or smooth-wheel delta.
    Pixel,
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
    /// Pointer left the window. The `u64` is the pointer ID (0 for mouse).
    PointerExited(PointerSource, u64),
    Scroll {
        delta: Vec2d,
        phase: TouchPhase,
        kind: ScrollDeltaKind,
        /// Whether the user is still physically controlling the scroll.
        ///
        /// Trackpad contact sets this to `true` until finger lift. Native
        /// momentum events emitted after lift retain their scroll phase but set
        /// this to `false`. Mouse-wheel input is never a direct manipulation.
        is_direct_manipulation: bool,
    },
    /// A character was typed (text input).
    CharInput {
        ch: char,
        action: KeyAction,
        modifiers: Modifiers,
    },

    /// A multi-character text payload was produced by the platform, such as an
    /// IME commit of a composed phrase or a dead-key sequence.
    ///
    /// The whole payload travels in one event so a receiver inserts it as a
    /// single edit: one undo entry, one change notification, and one cursor
    /// advance. Single typed characters keep arriving as [`Self::CharInput`].
    TextInput {
        text: String,
        action: KeyAction,
        modifiers: Modifiers,
    },

    /// A named key was pressed or released.
    KeyInput {
        key: NamedKey,
        action: KeyAction,
        modifiers: Modifiers,
    },
    /// IME pre-edit (composition in progress). `text` is the composing string.
    /// `cursor` is the byte range of the active composing segment.
    ImePreedit {
        text: String,
        cursor: Option<(usize, usize)>,
    },
    HoveredFile{path: PathBuf},
    DroppedFile{path: PathBuf},
    HoveredFileCancelled,
    Cancel,
}

impl ElementEvent {
    pub fn get_pointer_pos(&self) -> Option<Vec2d> {
        match self {
            ElementEvent::PointerDown(p, _, _)
            | ElementEvent::PointerUp(p, _, _)
            | ElementEvent::PointerMove(p, _, _) => Some(*p),
            _ => None,
        }
    }

    /// Returns whether the event belongs to the focused element instead of the
    /// element under the pointer.
    ///
    /// Text and composition events are produced by the keyboard and the input
    /// method, which know nothing about the pointer, so they must reach the
    /// focused element even while the pointer rests over another widget or has
    /// left the window. Named keys stay positional: scrollables scroll the
    /// hovered viewport and text spans resolve shortcuts against the hovered
    /// span.
    ///
    /// # Examples
    ///
    /// ```
    /// use aimer_events::element::{ElementEvent, KeyAction, Modifiers, NamedKey};
    ///
    /// let commit = ElementEvent::TextInput {
    ///     text: "你好".into(),
    ///     action: KeyAction::Pressed,
    ///     modifiers: Modifiers::default(),
    /// };
    /// let arrow = ElementEvent::KeyInput {
    ///     key: NamedKey::ArrowDown,
    ///     action: KeyAction::Pressed,
    ///     modifiers: Modifiers::default(),
    /// };
    ///
    /// assert!(commit.is_focus_directed());
    /// assert!(!arrow.is_focus_directed());
    /// ```
    pub fn is_focus_directed(&self) -> bool {
        matches!(
            self,
            ElementEvent::CharInput { .. }
                | ElementEvent::TextInput { .. }
                | ElementEvent::ImePreedit { .. }
        )
    }
}

unsafe impl Send for ElementEvent {}
unsafe impl Sync for ElementEvent {}
