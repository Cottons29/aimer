/// Identifies the origin of a pointer event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerSource {
    Mouse,
    Touch,
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
