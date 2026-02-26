use crate::size::ResolvedSize;

#[derive(Clone, Copy, Debug, Default)]
pub struct Vec2d {
    #[cfg(not(target_arch = "wasm32"))]
    pub x: f32,
    #[cfg(not(target_arch = "wasm32"))]
    pub y: f32,
    #[cfg(target_arch = "wasm32")]
    pub x: f64,
    #[cfg(target_arch = "wasm32")]
    pub y: f64,
}

#[cfg(not(target_arch = "wasm32"))]
impl Vec2d {
    pub fn get_end(&self, size: ResolvedSize) -> Vec2d {
        Self {
            x: self.x + size.width,
            y: self.y + size.height,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Vec2d {
    pub fn get_end(&self, size: ResolvedSize) -> Vec2d {
        Self {
            x: self.x + size.width,
            y: self.y + size.height,
        }
    }
}
