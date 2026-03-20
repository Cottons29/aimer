use std::ops::{Index, Mul};

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
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
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
        Self {
            a: self.a * rhs as f32,
            ..self
        }
    }
}

impl Color {
    pub const fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}
