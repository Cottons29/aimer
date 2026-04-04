pub mod color;
pub mod mat3;

pub use color::*;
pub use mat3::*;

/// Common types used throughout the cupid render engine.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Vec2d {
    pub x: f32,
    pub y: f32,
}

impl Vec2d {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

pub type TextureId = u32;
