use attribute::dimension::Dimension;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
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
pub struct BorderSlice {
    #[constructor(default)]
    pub style: BorderStyle,
    #[constructor(default, into)]
    pub stroke: Stroke,
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
    pub left: BorderSlice,
    #[constructor(default)]
    pub right: BorderSlice,
    #[constructor(default)]
    pub top: BorderSlice,
    #[constructor(default)]
    pub bottom: BorderSlice,
}


#[allow(dead_code)]
#[derive(Default, Clone, Copy, Constructor)]
pub struct BoxOutline {
    #[constructor(default)]
    pub left: BorderSlice,
    #[constructor(default)]
    pub right: BorderSlice,
    #[constructor(default)]
    pub top: BorderSlice,
    #[constructor(default)]
    pub bottom: BorderSlice,
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy)]
pub(crate) struct RawBoxBorder {
    pub left: BorderSlice,
    pub right: BorderSlice,
    pub top: BorderSlice,
    pub bottom: BorderSlice,
    pub mode: BorderMode,
}

impl RawBoxBorder {
    #[allow(dead_code)]
    pub fn get_uniform_radius(&self, _box_width: f32, _box_height: f32, _scale: f32) -> Option<f32> {
        None
    }

    /// Returns per-corner radii [top-left, top-right, bottom-right, bottom-left].
    /// Each corner radius is the minimum of its two adjacent side radii.
    /// Returns None if all radii are zero.
    #[allow(dead_code)]
    pub fn get_per_corner_radii(&self, _box_width: f32, _box_height: f32, _scale: f32) -> Option<[f32; 4]> {
        None
    }
}

impl BoxBorder {
    pub fn all(border: BorderSlice) -> Self {
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

    pub fn horizontal(border: BorderSlice) -> Self {
        Self { top: border, bottom: border, ..Default::default() }
    }

    pub fn vertical(border: BorderSlice) -> Self {
        Self { left: border, right: border, ..Default::default() }
    }

    pub fn get_uniform_radius(&self, _box_width: f32, _box_height: f32, _scale: f32) -> Option<f32> {
        None
    }

    /// Returns per-corner radii [top-left, top-right, bottom-right, bottom-left].
    /// Each corner radius is the minimum of its two adjacent side radii.
    /// Returns None if all radii are zero.
    pub fn get_per_corner_radii(&self, _box_width: f32, _box_height: f32, _scale: f32) -> Option<[f32; 4]> {
        None
    }
}

impl BoxOutline {
    pub fn all(border: BorderSlice) -> Self {
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

    pub fn horizontal(border: BorderSlice) -> Self {
        Self { top: border, bottom: border, ..Default::default() }
    }

    pub fn vertical(border: BorderSlice) -> Self {
        Self { left: border, right: border, ..Default::default() }
    }

    pub fn get_uniform_radius(&self, _box_width: f32, _box_height: f32, _scale: f32) -> Option<f32> {
        None
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



#[allow(dead_code)]
impl Drawable for RawBoxBorder {
    fn draw(&self, ctx: &BuildContext) {
        let canvas = &ctx.canvas;
        let box_width = ctx.parent_size.width;
        let box_height = ctx.parent_size.height;
        let scale = ctx.scale;
        let is_outline = self.mode == BorderMode::Outside;

        let left_stroke = resolve_dim(self.left.stroke, box_width, scale);
        let right_stroke = resolve_dim(self.right.stroke, box_width, scale);
        let top_stroke = resolve_dim(self.top.stroke, box_height, scale);
        let bottom_stroke = resolve_dim(self.bottom.stroke, box_height, scale);

        let is_uniform_style = self.left.style == self.right.style && self.left.style == self.top.style && self.left.style == self.bottom.style;
        let is_uniform_stroke = left_stroke == right_stroke && left_stroke == top_stroke && left_stroke == bottom_stroke;
        let is_uniform_color = self.left.color == self.right.color && self.left.color == self.top.color && self.left.color == self.bottom.color;

        // Uniform border: single stroke_rect call
        if is_uniform_style && is_uniform_stroke && is_uniform_color && left_stroke > 0.0 && self.left.style != BorderStyle::None {
            let (x, y, w, h) = if is_outline {
                (-left_stroke / 2.0, -left_stroke / 2.0, box_width + left_stroke, box_height + left_stroke)
            } else {
                (left_stroke / 2.0, left_stroke / 2.0, box_width - left_stroke, box_height - left_stroke)
            };
            let stroke_radius = 0.0;
            canvas.stroke_rect(
                Vec2d { x, y },
                ResolvedSize { width: w, height: h },
                self.left.color,
                left_stroke,
                stroke_radius,
            );
            return;
        }

        // Per-side borders with per-corner radii using the new per-side API.
        // When all colors are the same we can use a single stroke_rect_per_side call.
        if is_uniform_color && self.left.style != BorderStyle::None {
            let border_radius = [0.0, 0.0, 0.0, 0.0];
            let border_width = [
                top_stroke,
                right_stroke,
                bottom_stroke,
                left_stroke,
            ];

            canvas.stroke_rect_per_side(
                Vec2d { x: 0.0, y: 0.0 },
                ResolvedSize { width: box_width, height: box_height },
                self.left.color,
                border_width,
                border_radius,
            );
            return;
        }

        // Fallback: draw each side as a filled rectangle
        // Top border
        if self.top.style != BorderStyle::None && top_stroke > 0.0 {
            let (x, y, w, h) = if is_outline {
                (-left_stroke, -top_stroke, box_width + left_stroke + right_stroke, top_stroke)
            } else {
                (0.0, 0.0, box_width, top_stroke)
            };
            canvas.fill_color_rect(
                Vec2d { x, y },
                ResolvedSize { width: w, height: h },
                self.top.color,
                0.0,
            );
        }

        // Bottom border
        if self.bottom.style != BorderStyle::None && bottom_stroke > 0.0 {
            let (x, y, w, h) = if is_outline {
                (-left_stroke, box_height, box_width + left_stroke + right_stroke, bottom_stroke)
            } else {
                (0.0, box_height - bottom_stroke, box_width, bottom_stroke)
            };
            canvas.fill_color_rect(
                Vec2d { x, y },
                ResolvedSize { width: w, height: h },
                self.bottom.color,
                0.0,
            );
        }

        // Left border
        if self.left.style != BorderStyle::None && left_stroke > 0.0 {
            let (x, y, w, h) = if is_outline {
                (-left_stroke, -top_stroke, left_stroke, box_height + top_stroke + bottom_stroke)
            } else {
                (0.0, 0.0, left_stroke, box_height)
            };
            canvas.fill_color_rect(
                Vec2d { x, y },
                ResolvedSize { width: w, height: h },
                self.left.color,
                0.0,
            );
        }

        // Right border
        if self.right.style != BorderStyle::None && right_stroke > 0.0 {
            let (x, y, w, h) = if is_outline {
                (box_width, -top_stroke, right_stroke, box_height + top_stroke + bottom_stroke)
            } else {
                (box_width - right_stroke, 0.0, right_stroke, box_height)
            };
            canvas.fill_color_rect(
                Vec2d { x, y },
                ResolvedSize { width: w, height: h },
                self.right.color,
                0.0,
            );
        }
    }
}
