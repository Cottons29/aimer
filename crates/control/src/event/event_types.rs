#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum EventType {
    PointerDown = 1,
    PointerUp = 2,
    PointerMove = 3,
    PointerEnter = 4,
    PointerLeave = 5,
    /// Drag event (mouse down + move)
    Drag = 6,
    /// Drag ended (mouse up after drag)
    DragEnd = 7,

    Focus = 10,
    Blur = 11,

    KeyDown = 20,
    KeyUp = 21,
    /// Text input event (for character input, IME composition)
    TextInput = 22,

    Scroll = 30,
    /// Scroll gesture ended (for deceleration/momentum)
    ScrollEnd = 31,
    /// Pinch zoom gesture update
    Pinch = 32,

    Resize = 40,

    // Window lifecycle events
    WindowFocus = 50,
    WindowBlur = 51,

    // Element lifecycle events
    Mount = 60,
    Unmount = 61,

    // Clipboard events
    Cut = 70,
    Copy = 71,
    Paste = 72,

    // Selection events
    SelectAll = 80,
}

impl TryFrom<u8> for EventType {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use EventType::*;
        Ok(match value {
            1 => PointerDown,
            2 => PointerUp,
            3 => PointerMove,
            4 => PointerEnter,
            5 => PointerLeave,
            6 => Drag,
            7 => DragEnd,
            10 => Focus,
            11 => Blur,
            20 => KeyDown,
            21 => KeyUp,
            22 => TextInput,
            30 => Scroll,
            31 => ScrollEnd,
            32 => Pinch,
            40 => Resize,
            50 => WindowFocus,
            51 => WindowBlur,
            60 => Mount,
            61 => Unmount,
            70 => Cut,
            71 => Copy,
            72 => Paste,
            80 => SelectAll,
            _ => return Err(()),
        })
    }
}