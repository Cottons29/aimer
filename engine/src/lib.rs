#[allow(unused)]
#[macro_use]
extern crate widget;

#[macro_use]
pub mod oxidize;
pub mod render;
pub mod inspector;
pub use oxidize::OxidizeApp;
#[cfg(target_os = "ios")]
mod ios_screen;
pub mod window_attr;
pub mod window_event;

pub use winit;




