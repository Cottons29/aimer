
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

impl From<f64> for Dimension {
    fn from(v: f64) -> Self {
        Self::Px(v as f32)
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
