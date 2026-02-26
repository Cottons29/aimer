#[cfg(not(target_arch = "wasm32"))]
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Default, Debug)]
pub struct Rect {
    /// The x coordinate of the rectangle's left edge.
    pub left: f32,
    /// The y coordinate of the rectangle's top edge.
    pub top: f32,
    /// The x coordinate of the rectangle's right edge.
    pub right: f32,
    /// The y coordinate of the rectangle's bottom edge.
    pub bottom: f32,
}


#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Default, Debug)]
pub struct Rect {
    /// The x coordinate of the rectangle's left edge.
    pub left: f64,
    /// The y coordinate of the rectangle's top edge.
    pub top: f64,
    /// The x coordinate of the rectangle's right edge.
    pub right: f64,
    /// The y coordinate of the rectangle's bottom edge.
    pub bottom: f64,
}
