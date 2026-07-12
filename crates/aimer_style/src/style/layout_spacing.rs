#[derive(Default, Clone, Copy)]
pub struct LayoutSpacing {
    pub top: Spacing,
    pub bottom: Spacing,
    pub left: Spacing,
    pub right: Spacing,
}

impl LayoutSpacing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn top(mut self, top: impl Into<Spacing>) -> Self {
        self.top = top.into();
        self
    }

    pub fn bottom(mut self, bottom: impl Into<Spacing>) -> Self {
        self.bottom = bottom.into();
        self
    }

    pub fn left(mut self, left: impl Into<Spacing>) -> Self {
        self.left = left.into();
        self
    }

    pub fn right(mut self, right: impl Into<Spacing>) -> Self {
        self.right = right.into();
        self
    }

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

impl From<i32> for Spacing {
    fn from(value: i32) -> Self {
        Spacing::Px(value as u32)
    }
}

impl From<u32> for Spacing {
    fn from(value: u32) -> Self {
        Spacing::Px(value)
    }
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
