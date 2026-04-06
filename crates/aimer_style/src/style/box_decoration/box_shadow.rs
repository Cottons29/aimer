use aimer_color::prelude::{Color, Colors};
use aimer_macro::Constructor;




///
/// Represents the sides or edges of a border, with additional options for
/// angles and custom ranges.
///
/// This enumeration provides a way to define which specific side(s) of a
/// border are being referred to, as well as options for specifying angles
/// or numeric ranges. It derives common traits such as `Debug`, `Clone`,
/// `Copy`, `PartialEq`, and `Default` for convenience.
///
/// # Variants
///
/// - `Left`: Represents the left side of the border.
/// - `Right`: Represents the right side of the border.
/// - `Top`: Represents the top side of the border.
/// - `Bottom`: Represents the bottom side of the border.
/// - `Vertical`: Represents both the top and bottom sides of the border.
/// - `Horizontal`: Represents both the left and right sides of the border.
/// - `All`: Represents all sides of the border. This is the default variant.
/// - `Angle(f32)`: Specifies a border side using an angle, where the `f32` value
///   represents the angle in degrees.
/// - `Range(f32, f32)`: Specifies a range for the border sides, where the two
///   `f32` values represent the start and end of the range.
///
/// # Auto Traits
/// - `Debug`: Allows formatting the enum for output and debugging purposes.
/// - `Clone`: Enables creating an identical copy of the enum instance.
/// - `Copy`: Allows bit-level copying of the enum when moved.
/// - `PartialEq`: Enables comparison of enum instances for equality.
/// - `Default`: Provides a default value for the enum, which is `All`.
///
/// # Examples
///
/// ```
/// use ::widget::BorderSide;
///
/// let side = BorderSide::Left;
/// assert_eq!(side, BorderSide::Left);
///
/// let angle = BorderSide::Angle(45.0);
/// assert!(matches!(angle, BorderSide::Angle(a) if a == 45.0));
///
/// let range = BorderSide::Range(0.0, 90.0);
/// assert!(matches!(range, BorderSide::Range(start, end) if start == 0.0 && end == 90.0));
///
/// let default_side = BorderSide::default();
/// assert_eq!(default_side, BorderSide::All);
/// ```
///
/// This enum can be used in applications where borders or specific directions
/// need to be expressed with additional precision (e.g., angles or ranges).
///
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BorderSide {
    Left,
    Right,
    Top,
    Bottom,
    Vertical,
    Horizontal,
    #[default]
    All,
    Angle(f32),
    Range(f32, f32),
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
/// * `side` - Specifies the side or border details associated with the shadow. Defaults to `BorderSide`'s default value.
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
#[derive(Debug, Clone, Copy, PartialEq, Constructor)]
pub struct BoxShadow {
    /// Horizontal offset
    #[constructor(default = 0.0)]
    pub offset_x: f32,
    /// Vertical offset
    #[constructor(default = 0.0)]
    pub offset_y: f32,

    /// Blur radius
    #[constructor(default = 0.0)]
    pub blur: f32,

    /// Spread radius (grow/shrink before blur)
    #[constructor(default = 0.0)]
    pub spread: f32,

    /// RGBA color
    #[constructor(default = BoxShadow::DEFAULT_COLOR, into)]
    pub color: Color,

    /// Inner shadow instead of outer
    #[constructor(default = false)]
    pub inset: bool,

    #[constructor(default)]
    pub side: BorderSide,
}

impl BoxShadow {
    pub const DEFAULT_COLOR: Colors = Colors::Black;
}
