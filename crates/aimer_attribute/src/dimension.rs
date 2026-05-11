use crate::position::Vec2d;
use crate::size::ResolvedSize;
use std::ops::{Div, Mul};

///
/// Represents a dimension type that can be used to define sizes in different units.
///
/// The `Dimension` enum supports two primary units:
/// - Pixels (`Px`) - Defines the size in absolute pixel values.
/// - Percent (`Percent`) - Defines the size as a percentage of a parent container.
///
/// It also supports an automatic size value, `Auto`, which can be used when the size
/// should be determined by layout or content rules.
///
/// ### Conditional Compilation
/// The `Px` and `Percent` variants are conditionally compiled to support different
/// architectures:
/// - For non-WebAssembly targets, `Px` and `Percent`
///   use `f32` as the underlying type.
/// - For WebAssembly targets, `Px` and `Percent` use `f64`
///   as the underlying type.
///
/// ### Traits
/// The `Dimension` enum derives the following traits:
/// - `Debug`: Enables formatted output for debugging purposes.
/// - `Clone`: Allows for creating a duplicate of `Dimension` instances.
/// - `Copy`: Allows for `Dimension` to be copied rather than moved.
/// - `PartialEq`: Enables equality comparisons between `Dimension` instances.
///
/// # Example
/// ```
/// use your_crate::Dimension;
///
/// let px_dimension = Dimension::Px(100.0);
/// let percent_dimension = Dimension::Percent(50.0);
/// let auto_dimension = Dimension::Auto;
///
/// assert!(px_dimension != percent_dimension);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    Px(f32),
    Percent(f32),
    Auto,
}
#[allow(clippy::derivable_impls)]
impl Default for Dimension {
    fn default() -> Self {
        Self::Auto
    }
}

impl From<f32> for Dimension {
    fn from(v: f32) -> Self {
        Self::Px(v)
    }
}

impl From<i32> for Dimension {
    fn from(v: i32) -> Self {
        Self::Px(v as f32)
    }
}

impl From<u32> for Dimension {
    fn from(v: u32) -> Self {
        Self::Px(v as f32)
    }
}

impl Dimension {
    pub fn resolve(&self, parent_value: f32, scale: f32) -> f32 {
        match self {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => parent_value * (p / 100.0),
            Dimension::Auto => parent_value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Mul<f32> for Bounds {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self::Output {
        Self { x: self.x * rhs, y: self.y * rhs, width: self.width * rhs, height: self.height * rhs }
    }
}

impl Div<f32> for Bounds {
    type Output = Self;
    fn div(self, rhs: f32) -> Self::Output {
        Self { x: self.x / rhs, y: self.y / rhs, width: self.width / rhs, height: self.height / rhs }
    }
}

impl Bounds {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}
impl Default for Bounds {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CacheBounds {
    bound: Option<Bounds>,
}

impl CacheBounds {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self { bound: None }
    }

    pub const fn with_vec2d(vec2d: Vec2d) -> Self {
        Self { bound: Some(Bounds::new(vec2d.x, vec2d.y, 0.0, 0.0)) }
    }

    pub const fn is_cached(&self) -> bool {
        self.bound.is_some()
    }

    pub const fn get_bounds(&self) -> Option<Bounds> {
        self.bound
    }

    pub fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bound.map(|b| (Vec2d { x: b.x, y: b.y }, Vec2d { x: b.x + b.width, y: b.y + b.height }))
    }

    pub const fn set_bounds(&self, bounds: Bounds) {
        let bound_ptr = &raw const self.bound as *mut Option<Bounds>;
        unsafe {
            *bound_ptr = Some(bounds);
        }
    }

    pub const fn set_size(&self, size: ResolvedSize) {
        let bound_ptr = &raw const self.bound as *mut Option<Bounds>;
        unsafe {
            if let Some(bound) = &mut *bound_ptr {
                bound.width = size.width;
                bound.height = size.height;
            }
        }
    }

    pub const fn save(&self, scale: f32, x: f32, y: f32, width: f32, height: f32) {
        let cache_x = x / scale;
        let cache_y = y / scale;
        let cache_w = width / scale;
        let cache_h = height / scale;
        let bound = Bounds::new(cache_x, cache_y, cache_w, cache_h);
        self.set_bounds(bound);
    }

    pub const fn is_inside(&self, x: f32, y: f32) -> bool {
        let Some(bound) = self.bound else { return false };
        bound.x <= x && x <= bound.x + bound.width && bound.y <= y && y <= bound.y + bound.height
    }
}
