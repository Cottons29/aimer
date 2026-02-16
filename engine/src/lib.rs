#[macro_use]
extern crate widget;

#[macro_use]
mod oxidize;
mod event;
pub mod render;
pub mod container;
pub use container::MyStatefulWidget;
pub use oxidize::OxidizeApp;

