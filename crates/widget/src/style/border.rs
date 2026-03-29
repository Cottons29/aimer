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
#[derive(Default, Clone, Copy, Constructor)]
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

pub fn resolve_dim(dim: Dimension, parent_val: f32, scale: f32) -> f32 {
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
    #[allow(dead_code)]
    pub fn get_uniform_radius(&self, box_width: f32, box_height: f32, scale: f32) -> Option<f32> {
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

    /// Returns per-corner radii [top-left, top-right, bottom-right, bottom-left].
    /// Each corner radius is the minimum of its two adjacent side radii.
    /// Returns None if all radii are zero.
    #[allow(dead_code)]
    pub fn get_per_corner_radii(&self, box_width: f32, box_height: f32, scale: f32) -> Option<[f32; 4]> {
        let left_r = resolve_dim(self.left.radius, box_width, scale);
        let right_r = resolve_dim(self.right.radius, box_width, scale);
        let top_r = resolve_dim(self.top.radius, box_height, scale);
        let bottom_r = resolve_dim(self.bottom.radius, box_height, scale);

        let tl = left_r.min(top_r);
        let tr = right_r.min(top_r);
        let br = right_r.min(bottom_r);
        let bl = left_r.min(bottom_r);

        if tl == 0.0 && tr == 0.0 && br == 0.0 && bl == 0.0 {
            None
        } else {
            Some([tl, tr, br, bl])
        }
    }
}

impl BoxBorder {
    pub fn all(border: BorderSide) -> Self {
        Self { left: border, right: border, top: border, bottom: border, ..Default::default() }
    }

    /// Returns the resolved border stroke for each side: (left, top, right, bottom).
    pub fn strokes(&self, box_width: f32, box_height: f32, scale: f32) -> (f32, f32, f32, f32) {
        (
            resolve_dim(self.left.stroke, box_width, scale),
            resolve_dim(self.top.stroke, box_height, scale),
            resolve_dim(self.right.stroke, box_width, scale),
            resolve_dim(self.bottom.stroke, box_height, scale),
        )
    }

    /// Returns true if any side has a non-None style and non-zero stroke.
    pub fn has_visible_border(&self, box_width: f32, box_height: f32, scale: f32) -> bool {
        let (l, t, r, b) = self.strokes(box_width, box_height, scale);
        (l > 0.0 && self.left.style != BorderStyle::None)
            || (t > 0.0 && self.top.style != BorderStyle::None)
            || (r > 0.0 && self.right.style != BorderStyle::None)
            || (b > 0.0 && self.bottom.style != BorderStyle::None)
    }

    pub fn horizontal(border: BorderSide) -> Self {
        Self { top: border, bottom: border, ..Default::default() }
    }

    pub fn vertical(border: BorderSide) -> Self {
        Self { left: border, right: border, ..Default::default() }
    }

    pub fn get_uniform_radius(&self, box_width: f32, box_height: f32, scale: f32) -> Option<f32> {
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

    /// Returns per-corner radii [top-left, top-right, bottom-right, bottom-left].
    /// Each corner radius is the minimum of its two adjacent side radii.
    /// Returns None if all radii are zero.
    pub fn get_per_corner_radii(&self, box_width: f32, box_height: f32, scale: f32) -> Option<[f32; 4]> {
        let left_r = resolve_dim(self.left.radius, box_width, scale);
        let right_r = resolve_dim(self.right.radius, box_width, scale);
        let top_r = resolve_dim(self.top.radius, box_height, scale);
        let bottom_r = resolve_dim(self.bottom.radius, box_height, scale);

        let tl = left_r.min(top_r);
        let tr = right_r.min(top_r);
        let br = right_r.min(bottom_r);
        let bl = left_r.min(bottom_r);

        if tl == 0.0 && tr == 0.0 && br == 0.0 && bl == 0.0 {
            None
        } else {
            Some([tl, tr, br, bl])
        }
    }
}

impl BoxOutline {
    pub fn all(border: BorderSide) -> Self {
        Self { left: border, right: border, top: border, bottom: border, ..Default::default() }
    }

    /// Returns true if any side has a non-None style and non-zero stroke.
    pub fn has_visible_outline(&self, box_width: f32, box_height: f32, scale: f32) -> bool {
        let (l, t, r, b) = self.strokes(box_width, box_height, scale);
        (l > 0.0 && self.left.style != BorderStyle::None)
            || (t > 0.0 && self.top.style != BorderStyle::None)
            || (r > 0.0 && self.right.style != BorderStyle::None)
            || (b > 0.0 && self.bottom.style != BorderStyle::None)
    }

    pub fn horizontal(border: BorderSide) -> Self {
        Self { top: border, bottom: border, ..Default::default() }
    }

    pub fn vertical(border: BorderSide) -> Self {
        Self { left: border, right: border, ..Default::default() }
    }

    pub fn get_uniform_radius(&self, box_width: f32, box_height: f32, scale: f32) -> Option<f32> {
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
    pub fn strokes(&self, box_width: f32, box_height: f32, scale: f32) -> (f32, f32, f32, f32) {
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



