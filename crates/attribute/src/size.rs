
use crate::dimension::Dimension;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Size {
    pub width: Dimension,
    pub height: Dimension,
}

impl Default for Size {
    fn default() -> Self {
        Self {
            width: Dimension::Auto,
            height: Dimension::Auto,
        }
    }
}

impl Size {
    pub fn new(width: impl Into<Dimension>, height: impl Into<Dimension>) -> Self {
        Self { width: width.into(), height: height.into() }
    }

    pub fn square(side: impl Into<Dimension>) -> Self {
        let d = side.into();
        Self { width: d, height: d }
    }

    pub fn zero() -> Self {
        Self { width: Dimension::Px(0.0), height: Dimension::Px(0.0) }
    }

    pub fn is_auto_width(&self) -> bool {
        self.width == Dimension::Auto
    }

    pub fn is_auto_height(&self) -> bool {
        self.height == Dimension::Auto
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn resolve(&self, parent: &ResolvedSize, scale: f32) -> ResolvedSize {
        ResolvedSize {
            width: self.width.resolve(parent.width, scale),
            height: self.height.resolve(parent.height, scale),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn resolve(&self, parent: &ResolvedSize, scale: f64) -> ResolvedSize {
        ResolvedSize {
            width: self.width.resolve(parent.width, scale),
            height: self.height.resolve(parent.height, scale),
        }
    }
}

/// # The resolved pixel size after layout.
///
///
/// - f32 for non-wasm32 targets,
///
/// - f64 for wasm32
#[derive(Copy, Clone, Default, Debug, PartialEq)]
pub struct ResolvedSize {
    #[cfg(not(target_arch = "wasm32"))]
    pub width: f32,
    #[cfg(not(target_arch = "wasm32"))]
    pub height: f32,
    #[cfg(target_arch = "wasm32")]
    pub width: f64,
    #[cfg(target_arch = "wasm32")]
    pub height: f64,
}
