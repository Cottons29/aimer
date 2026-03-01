#[allow(unused)]
#[macro_use]
extern crate widget;

#[macro_use]
pub mod oxidize;
pub mod render;
pub use oxidize::OxidizeApp;
#[cfg(target_os = "ios")]
mod ios_screen;
pub use winit;




