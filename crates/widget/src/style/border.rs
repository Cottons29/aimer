#[cfg(not(target_arch = "wasm32"))]
mod non_wasm;
#[cfg(target_arch = "wasm32")]
mod wasm;

#[allow(unused_imports)]
#[cfg(not(target_arch = "wasm32"))]
pub use non_wasm::*;

#[allow(unused_imports)]
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

use attribute::dimension::Dimension;
use color::prelude::Color;
use constructor::Constructor;
#[allow(dead_code)]
#[derive(Default, Clone, Copy, PartialEq)]
pub enum BorderStyle {
    Solid,
    Dashed,
    Dotted,
    #[default]
    None,
}

pub type Stroke = Dimension;

#[allow(dead_code)]
#[derive(Default, Clone, Copy, constructor::Constructor)]
pub struct BorderSide {
    #[constructor(default)]
    pub style: BorderStyle,
    #[constructor(default)]
    pub stroke: Stroke,
    #[constructor(default)]
    pub radius: Dimension,
    #[constructor(default, into)]
    pub color: Color,
}



