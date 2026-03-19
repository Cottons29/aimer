use attribute::Float;

#[derive(Copy, Clone, PartialEq, Default, Debug)]
pub struct Rect {
    /// The x coordinate of the rectangle's left edge.
    pub left: Float,
    /// The y coordinate of the rectangle's top edge.
    pub top: Float,
    /// The x coordinate of the rectangle's right edge.
    pub right: Float,
    /// The y coordinate of the rectangle's bottom edge.
    pub bottom: Float,
}

