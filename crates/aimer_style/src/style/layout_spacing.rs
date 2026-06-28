use aimer_macro::Constructor;

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

    pub fn value(&self, total: f32, scale: f32) -> f32 {
        match self {
            Spacing::Px(px) => *px as f32 * scale,
            Spacing::Percent(p) => total * (*p as f32 / 100.0),
            Spacing::None => 0.0,
        }
    }
}


impl From<f64> for Spacing {
    fn from(value: f64) -> Self {
        Spacing::Px(value as u32)
    }
}