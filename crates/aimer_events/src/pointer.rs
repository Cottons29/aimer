/// The pointer identifier reserved for a drag coming from the operating system.
///
/// A file drag has no pointer of its own — there is no press to own it — but the
/// events that describe a drag are keyed by one, so file drags are filed under
/// an identifier no real device can produce: a mouse has id `0` and a touch id
/// counts up from there, and neither will ever reach `u64::MAX`.
///
/// # Examples
///
/// ```
/// use aimer_events::pointer::FILE_DRAG_POINTER_ID;
///
/// assert_ne!(FILE_DRAG_POINTER_ID, 0);
/// ```
pub const FILE_DRAG_POINTER_ID: u64 = u64::MAX;

/// Identifies the origin of a pointer event.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum PointerSource {
    Mouse = 0,
    Touch = 1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerPosition {
    pub x: f32,
    pub y: f32,
    pub source: PointerSource,
    /// Touch finger ID (0 for mouse).
    pub id: u64,
}

#[derive(Clone, Debug)]
pub enum PointerEvent {
    Down(PointerPosition),
    Up(PointerPosition),
    Move(PointerPosition),
    Cancel,
    /// Right / secondary mouse button click.
    RightClick(PointerPosition),
    /// Scroll wheel or trackpad gesture.
    Scroll {
        delta_x: f32,
        delta_y: f32,
    },
}
