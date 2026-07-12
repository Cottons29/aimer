use aimer_color::prelude::Color;

/// Specifies which side(s) of the box the shadow is visible on.
///
/// By default, the shadow is visible on all sides (`All`). Use specific
/// variants to restrict the shadow to particular edges or angular ranges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShadowSide {
    /// Shadow visible on all sides (default)
    All,
    /// Shadow visible only on the top edge
    Top,
    /// Shadow visible only on the right edge
    Right,
    /// Shadow visible only on the bottom edge
    Bottom,
    /// Shadow visible only on the left edge
    Left,
    /// Shadow visible on top and bottom edges
    Vertical,
    /// Shadow visible on left and right edges
    Horizontal,
    /// Shadow visible on top and left edges with blended corner
    TopLeft,
    /// Shadow visible on top and right edges with blended corner
    TopRight,
    /// Shadow visible on bottom and right edges with blended corner
    BottomRight,
    /// Shadow visible on bottom and left edges with blended corner
    BottomLeft,
    /// Shadow visible within an angular range (in radians, measured from positive X axis)
    /// The range is specified as (start_angle, end_angle).
    Range(f32, f32),
}
#[allow(clippy::derivable_impls)]
impl Default for ShadowSide {
    fn default() -> Self {
        ShadowSide::All
    }
}

impl ShadowSide {
    /// Encodes the side as two f32 values for the GPU shader.
    /// Returns (side_type, side_param):
    /// - Side_type: 0.0 = All, 1.0 = Top, 2.0 = Right, 3.0 = Bottom, 4.0 = Left,
    ///   5.0 = Vertical, 6.0 = Horizontal, 7.0 = Range,
    ///   8.0 = TopLeft, 9.0 = TopRight, 10.0 = BottomRight, 11.0 = BottomLeft
    /// - Side_param: unused for most variants; for Range, encodes start angle
    /// - Side_param2: unused for most variants; for Range, encodes end angle
    pub fn to_shader_params(self) -> (f32, f32, f32) {
        match self {
            ShadowSide::All => (0.0, 0.0, 0.0),
            ShadowSide::Top => (1.0, 0.0, 0.0),
            ShadowSide::Right => (2.0, 0.0, 0.0),
            ShadowSide::Bottom => (3.0, 0.0, 0.0),
            ShadowSide::Left => (4.0, 0.0, 0.0),
            ShadowSide::Vertical => (5.0, 0.0, 0.0),
            ShadowSide::Horizontal => (6.0, 0.0, 0.0),
            ShadowSide::TopLeft => (8.0, 0.0, 0.0),
            ShadowSide::TopRight => (9.0, 0.0, 0.0),
            ShadowSide::BottomRight => (10.0, 0.0, 0.0),
            ShadowSide::BottomLeft => (11.0, 0.0, 0.0),
            ShadowSide::Range(start, end) => (7.0, start, end),
        }
    }
}

///
/// A struct representing a box shadow with various customizable properties.
///
/// Box shadows are commonly used in UI design to create the illusion of depth
/// by adding shadow effects to UI elements such as buttons, cards, and containers.
///
/// # Fields
///
/// * `offset_x` - Horizontal offset of the shadow. Defaults to `0.0`.
/// * `offset_y` - Vertical offset of the shadow. Defaults to `0.0`.
/// * `blur` - Radius of the blur effect for the shadow, controlling how soft or sharp the edges of the shadow are. Defaults to `0.0`.
/// * `spread` - Spread radius, governing how the shadow grows or shrinks before applying the blur. Negative values shrink the shadow, while positive values expand it. Defaults to `0.0`.
/// * `color` - The color of the shadow in RGBA format. Defaults to `BoxShadow::DEFAULT_COLOR`.
/// * `inset` - Whether the shadow appears as an inner shadow (`true`) or an outer shadow (`false`). Defaults to `false`.
/// * `side` - Which side(s) of the box the shadow is visible on. Defaults to `ShadowSide::All`.
///
/// # Derives
///
/// This struct derives several traits:
///
/// * `Debug` - Allows for formatting the struct for debugging purposes.
/// * `Clone` - Enables cloning of the struct.
/// * `Copy` - Allows the struct to be copied.
/// * `PartialEq` - Enables equality comparisons between two `BoxShadow` instances.
/// * `Constructor` - Simplifies the creation of the struct by providing default values for fields.
///
///
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxShadow {
    /// Horizontal offset
    pub offset_x: f32,
    /// Vertical offset
    pub offset_y: f32,

    /// Blur radius
    pub blur: f32,

    /// Spread radius (grow/shrink before blur)
    pub spread: f32,

    /// RGBA color (default: semi-transparent black)
    pub color: Color,

    /// Inner shadow instead of outer
    pub inset: bool,

    /// Which side(s) of the box the shadow is visible on
    pub side: ShadowSide,
}

impl BoxShadow {
    /// Default shadow color: semi-transparent black (~50% opacity),
    /// closer to typical CSS usage than fully opaque black.
    pub const DEFAULT_COLOR: Color = Color::Rgba(0, 0, 0, 128);

    pub fn new() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: 0.0,
            color: Self::DEFAULT_COLOR,
            inset: false,
            side: ShadowSide::All,
        }
    }

    pub fn offset_x(mut self, offset_x: f32) -> Self {
        self.offset_x = offset_x;
        self
    }

    pub fn offset_y(mut self, offset_y: f32) -> Self {
        self.offset_y = offset_y;
        self
    }

    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }

    pub fn spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }

    pub fn inset(mut self, inset: bool) -> Self {
        self.inset = inset;
        self
    }

    pub fn side(mut self, side: ShadowSide) -> Self {
        self.side = side;
        self
    }
}
