use constructor::Constructor;

#[derive(Constructor, Default, Clone, Copy)]
pub struct LayoutSpacing {
    #[constructor(default)]
    pub top: Spacing,
    #[constructor(default)]
    pub bottom: Spacing,
    #[constructor(default)]
    pub left: Spacing,
    #[constructor(default)]
    pub right: Spacing,
}

#[cfg(target_arch = "wasm32")]
type Float = f64;
#[cfg(not(target_arch = "wasm32"))]
type Float = f32;

impl LayoutSpacing {
    /// For Top and Bottom
    pub const fn vertical(space: Spacing) -> Self {
        Self { top: space, bottom: space, left: Spacing::DEFAULT_VALUE, right: Spacing::DEFAULT_VALUE }
    }

    /// For Left and right
    pub const fn horizontal(space: Spacing) -> Self {
        Self { left: space, right: space, top: Spacing::DEFAULT_VALUE, bottom: Spacing::DEFAULT_VALUE }
    }

    pub const fn all(space: Spacing) -> Self {
        Self { left: space, right: space, top: space, bottom: space }
    }
}

#[derive(Default, Clone, Copy)]
pub enum Spacing {
    Px(u32),
    Percent(u32),
    #[default]
    None,
}

impl Spacing {
    pub const DEFAULT_VALUE: Spacing = Spacing::None;

    pub fn value(&self, total: Float, scale: Float) -> Float {
        match self {
            Spacing::Px(px) => *px as Float * scale,
            Spacing::Percent(p) => total * (*p as Float / 100.0),
            Spacing::None => 0.0,
        }
    }
}
