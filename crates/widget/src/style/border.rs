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
use crate::base::BuildContext;
use crate::Drawable;

#[cfg(target_arch = "wasm32")]
pub(crate) type Float = f64;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type Float = f32;

#[allow(dead_code)]
#[derive(Default, Clone, Copy, PartialEq)]
pub enum BorderStyle {
    Solid,
    Dashed,
    Dotted,
    #[default]
    None,
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy, PartialEq)]
pub enum BorderMode {
    #[default]
    Inside,
    Outside,
}

pub type Stroke = Dimension;

#[allow(dead_code)]
#[derive(Default, Clone, Copy, constructor::Constructor)]
pub struct BorderSide {
    #[constructor(default)]
    pub style: BorderStyle,
    #[constructor(default, into)]
    pub stroke: Stroke,
    #[constructor(default, into)]
    pub radius: Dimension,
    #[constructor(default, into)]
    pub color: Color,
}

pub(crate) fn resolve_dim(dim: Dimension, parent_val: Float, scale: Float) -> Float {
    match dim {
        Dimension::Px(w) => w * scale,
        Dimension::Percent(p) => parent_val * (p / 100.0),
        Dimension::Auto => 0.0,
    }
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy, Constructor)]
pub struct BoxBorder {
    #[constructor(default)]
    pub left: BorderSide,
    #[constructor(default)]
    pub right: BorderSide,
    #[constructor(default)]
    pub top: BorderSide,
    #[constructor(default)]
    pub bottom: BorderSide,
}


#[allow(dead_code)]
#[derive(Default, Clone, Copy, Constructor)]
pub struct BoxOutline {
    #[constructor(default)]
    pub left: BorderSide,
    #[constructor(default)]
    pub right: BorderSide,
    #[constructor(default)]
    pub top: BorderSide,
    #[constructor(default)]
    pub bottom: BorderSide,
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy)]
pub(crate) struct RawBoxBorder {
    pub left: BorderSide,
    pub right: BorderSide,
    pub top: BorderSide,
    pub bottom: BorderSide,
    pub mode: BorderMode,
}

impl RawBoxBorder {
    pub fn get_uniform_radius(&self, box_width: Float, box_height: Float, scale: Float) -> Option<Float> {
        let left_r = resolve_dim(self.left.radius, box_width, scale);
        let right_r = resolve_dim(self.right.radius, box_width, scale);
        let top_r = resolve_dim(self.top.radius, box_height, scale);
        let bottom_r = resolve_dim(self.bottom.radius, box_height, scale);

        if left_r == right_r && left_r == top_r && left_r == bottom_r && left_r > 0.0 {
            Some(left_r)
        } else {
            None
        }
    }
}

impl BoxBorder {
    pub fn all(border: BorderSide) -> Self {
        Self { left: border, right: border, top: border, bottom: border, ..Default::default() }
    }

    pub fn horizontal(border: BorderSide) -> Self {
        Self { top: border, bottom: border, ..Default::default() }
    }

    pub fn vertical(border: BorderSide) -> Self {
        Self { left: border, right: border, ..Default::default() }
    }

    pub fn get_uniform_radius(&self, box_width: Float, box_height: Float, scale: Float) -> Option<Float> {
        let left_r = resolve_dim(self.left.radius, box_width, scale);
        let right_r = resolve_dim(self.right.radius, box_width, scale);
        let top_r = resolve_dim(self.top.radius, box_height, scale);
        let bottom_r = resolve_dim(self.bottom.radius, box_height, scale);

        if left_r == right_r && left_r == top_r && left_r == bottom_r && left_r > 0.0 {
            Some(left_r)
        } else {
            None
        }
    }
}

impl BoxOutline {
    pub fn all(border: BorderSide) -> Self {
        Self { left: border, right: border, top: border, bottom: border, ..Default::default() }
    }

    pub fn horizontal(border: BorderSide) -> Self {
        Self { top: border, bottom: border, ..Default::default() }
    }

    pub fn vertical(border: BorderSide) -> Self {
        Self { left: border, right: border, ..Default::default() }
    }

    pub fn get_uniform_radius(&self, box_width: Float, box_height: Float, scale: Float) -> Option<Float> {
        let left_r = resolve_dim(self.left.radius, box_width, scale);
        let right_r = resolve_dim(self.right.radius, box_width, scale);
        let top_r = resolve_dim(self.top.radius, box_height, scale);
        let bottom_r = resolve_dim(self.bottom.radius, box_height, scale);

        if left_r == right_r && left_r == top_r && left_r == bottom_r && left_r > 0.0 {
            Some(left_r)
        } else {
            None
        }
    }

    /// Returns the resolved outline stroke for each side: (left, top, right, bottom).
    pub fn strokes(&self, box_width: Float, box_height: Float, scale: Float) -> (Float, Float, Float, Float) {
        (
            resolve_dim(self.left.stroke, box_width, scale),
            resolve_dim(self.top.stroke, box_height, scale),
            resolve_dim(self.right.stroke, box_width, scale),
            resolve_dim(self.bottom.stroke, box_height, scale),
        )
    }
}

impl Drawable for BoxOutline {
    fn draw(&self, ctx: &BuildContext) {
        RawBoxBorder::from(*self).draw(ctx)
    }
}

impl Drawable for BoxBorder {
    fn draw(&self, ctx: &BuildContext) {
        RawBoxBorder::from(*self).draw(ctx)
    }
}

impl From<BoxBorder> for RawBoxBorder {
    #[inline]
    fn from(value: BoxBorder) -> Self {
        Self{
            left: value.left,
            right: value.right,
            top: value.top,
            bottom: value.bottom,
            mode: BorderMode::Inside,
        }
    }
}

impl From<BoxOutline> for RawBoxBorder {
    #[inline]
    fn from(value: BoxOutline) -> Self {
        Self{
            left: value.left,
            right: value.right,
            top: value.top,
            bottom: value.bottom,
            mode: BorderMode::Outside,
        }
    }
}



