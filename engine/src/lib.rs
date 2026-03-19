#[allow(unused)]
#[macro_use]
extern crate widget;
mod ffi_utils;


#[macro_use]
pub mod aimer_app;
pub mod handler;
pub use aimer_app::AimerApp;
#[cfg(target_os = "ios")]
mod ios_screen {
    pub use crate::ffi_utils::ios_screen::get_screen_resolution_pixels;
}
pub mod window_attr;
pub mod window_event;
mod render_ctx;

pub use winit;




