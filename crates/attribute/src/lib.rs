pub mod position;
pub mod size;
pub mod dimension;
mod devices;

#[cfg(not(target_arch = "wasm32"))]
///  Float type for rendering
pub type Float = f32;
#[cfg(target_arch = "wasm32")]
///  Float type for rendering
pub type Float = f64;

pub use dimension::Dimension;
pub use dimension::Bounds;
pub use dimension::CacheBounds;