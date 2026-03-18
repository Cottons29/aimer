#[cfg(target_arch = "wasm32")]
mod h5canva;
#[cfg(target_arch = "wasm32")]
pub use h5canva::render_ctx::H5CanvasApi;
#[cfg(target_os = "android")]
mod opengles2;
#[cfg(target_os = "android")]
pub use opengles2::render_ctx::OpenGLES2Api;

#[cfg(any(target_os = "ios", target_os = "macos"))]
mod metal;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub use metal::render_ctx::MetalApi;

#[cfg(any(target_os = "ios", target_os = "macos"))]
pub type AimerRenderContext = MetalApi;
#[cfg(target_os = "android")]
pub type AimerRenderContext = OpenGLES2Api;
#[cfg(target_arch = "wasm32")]
pub type AimerRenderContext = H5CanvasApi;






