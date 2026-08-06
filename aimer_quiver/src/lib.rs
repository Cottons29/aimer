extern crate aimer_widget;
mod ffi_utils;
mod first_frame;

#[macro_use]
pub mod aimer_app;
pub mod handler;
pub use aimer_app::{AimerApp, HeadlessAimerApp, HeadlessOptions};
pub use aimer_cupid::AntiAlias;
pub use first_frame::{FIRST_FRAME_RENDERED_EVENT, set_first_frame_rendered_callback};
#[cfg(target_os = "ios")]
mod ios_screen {
    pub use crate::ffi_utils::ios_screen::{
        attach_window_to_active_scene, get_screen_resolution_pixels,
    };
}

mod adapter_detail;
/// The native application menu, and the shortcuts macOS routes through it.
pub mod menu;
pub mod frame_stats;
/// Where the platform's light / dark appearance comes from.
mod system_appearance;
/// Off-thread rasterization. Native only: the browser has no thread the WebGPU
/// objects could move to, so the web backend presents inline.
#[cfg(not(target_arch = "wasm32"))]
pub mod raster;
pub mod render_ctx;
pub mod window_attr;

pub use winit;
