extern crate aimer_widget;
mod ffi_utils;
mod first_frame;

#[macro_use]
pub mod aimer_app;
pub mod handler;
pub use aimer_app::{AimerApp, HeadlessAimerApp, HeadlessOptions};
pub use first_frame::{FIRST_FRAME_RENDERED_EVENT, set_first_frame_rendered_callback};
#[cfg(target_os = "ios")]
mod ios_screen {
    pub use crate::ffi_utils::ios_screen::{
        attach_window_to_active_scene, get_screen_resolution_pixels,
    };
}

mod adapter_detail;
mod render_ctx;
pub mod window_attr;

pub use winit;
