
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
    #[cfg(not(target_arch = "wasm32"))]
    Px(f32),
    #[cfg(not(target_arch = "wasm32"))]
    Percent(f32),
    #[cfg(target_arch = "wasm32")]
    Px(f64),
    #[cfg(target_arch = "wasm32")]
    Percent(f64),
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
        #[cfg(target_arch = "wasm32")]
        {
            Self::Px(v as f64)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::Px(v)
        }
    }
}

impl From<f64> for Dimension {
    fn from(v: f64) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            Self::Px(v)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::Px(v as f32)
        }
    }
}

impl From<i32> for Dimension {
    fn from(v: i32) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            Self::Px(v as f64)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::Px(v as f32)
        }
    }
}

impl From<u32> for Dimension {
    fn from(v: u32) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            Self::Px(v as f64)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::Px(v as f32)
        }
    }
}

impl Dimension {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn resolve(&self, parent_value: f32, scale: f32) -> f32 {
        match self {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => parent_value * (p / 100.0),
            Dimension::Auto => parent_value,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn resolve(&self, parent_value: f64, scale: f64) -> f64 {
        match self {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => parent_value * (p / 100.0),
            Dimension::Auto => parent_value,
        }
    }
}
