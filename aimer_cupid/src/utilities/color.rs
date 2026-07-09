use aimer_color::prelude::Color as AimerColor;
use std::ops::Mul;

#[derive(Debug, Clone, Copy, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: a as f32 / 255.0 }
    }

    pub const fn white() -> Self {
        Self::new(1.0, 1.0, 1.0, 1.0)
    }

    pub const fn black() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0)
    }

    pub const fn red() -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0)
    }

    pub const fn green() -> Self {
        Self::new(0.0, 1.0, 0.0, 1.0)
    }

    pub const fn blue() -> Self {
        Self::new(0.0, 0.0, 1.0, 1.0)
    }

    pub const fn transparent() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

impl Mul<u8> for Color {
    type Output = Self;
    fn mul(self, rhs: u8) -> Self::Output {
        Self { a: self.a * rhs as f32, ..self }
    }
}

impl Color {
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub const fn set_alpha(&mut self, alpha: u8) -> Self {
        self.a = alpha as f32 / 255.0;
        *self
    }
}

impl From<AimerColor> for Color {
    fn from(c: AimerColor) -> Self {
        let argb = c.as_u32();
        let a = ((argb >> 24) & 0xFF) as f32 / 255.0;
        let r = ((argb >> 16) & 0xFF) as f32 / 255.0;
        let g = ((argb >> 8) & 0xFF) as f32 / 255.0;
        let b = (argb & 0xFF) as f32 / 255.0;
        Self { r, g, b, a }
    }
}
