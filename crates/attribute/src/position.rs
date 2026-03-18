use crate::Float;
use crate::size::ResolvedSize;

#[derive(Clone, Copy, Debug, Default)]
pub struct Vec2d {
    pub x: Float,
    pub y: Float,
}

impl Vec2d {
    pub fn get_end(&self, size: ResolvedSize) -> Vec2d {
        Self {
            x: self.x + size.width,
            y: self.y + size.height,
        }
    }
}