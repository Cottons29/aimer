#[allow(unused)]
#[macro_use]
extern crate widget;

#[macro_use]
pub mod aimer_app;
pub mod render;
pub use aimer_app::AimerApp;
#[cfg(target_os = "ios")]
mod ios_screen;
pub mod window_attr;
pub mod window_event;

pub use winit;




