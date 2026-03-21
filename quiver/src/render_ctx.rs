#[cfg(target_arch = "wasm32")]
mod h5canva;
#[cfg(target_arch = "wasm32")]
pub use h5canva::render_ctx::H5CanvasApi;

#[cfg(not(target_arch = "wasm32"))]
mod wgpu_ctx;
#[cfg(not(target_arch = "wasm32"))]
pub use wgpu_ctx::render_ctx::WgpuApi;

#[cfg(not(target_arch = "wasm32"))]
pub type AimerRenderContext = WgpuApi;
#[cfg(target_arch = "wasm32")]
pub type AimerRenderContext = H5CanvasApi;






