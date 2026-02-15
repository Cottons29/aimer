use crate::size::Size;

#[derive(Clone, Copy, Debug)]
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
    pub fn get_end(&self, size: Size) -> Vec2d {
        Self {
            x: self.x + size.width as f32,
            y: self.y + size.height as f32,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Vec2d {
    pub fn get_end(&self, size: Size) -> Vec2d {
        Self {
            x: self.x + size.width as f64,
            y: self.y + size.height as f64,
        }
    }
}
