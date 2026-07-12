use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_color::prelude::Color;
use aimer_widget::Drawable;
use aimer_widget::base::BuildContext;

#[allow(dead_code)]
#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub enum BorderStyle {
    Solid,
    Dashed,
    Dotted,
    #[default]
    None,
}

#[allow(dead_code)]
#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub enum BorderMode {
    #[default]
    Inside,
    Outside,
}

pub type Stroke = Dimension;

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct BorderSlice {
    pub style: BorderStyle,
    pub stroke: Stroke,
    pub color: Color,
}

impl BorderSlice {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn style(mut self, style: BorderStyle) -> Self {
        self.style = style;
        self
    }

    pub fn stroke(mut self, stroke: impl Into<Stroke>) -> Self {
        self.stroke = stroke.into();
        self
    }

    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }
}

pub fn resolve_dim(dim: Dimension, parent_val: f32, scale: f32) -> f32 {
    match dim {
        Dimension::Px(w) => w * scale,
        Dimension::Percent(p) => parent_val * (p / 100.0),
        Dimension::Auto => 0.0,
    }
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct BoxBorder {
    pub left: BorderSlice,
    pub right: BorderSlice,
    pub top: BorderSlice,
    pub bottom: BorderSlice,
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct BoxOutline {
    pub left: BorderSlice,
    pub right: BorderSlice,
    pub top: BorderSlice,
    pub bottom: BorderSlice,
}

#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct RawBoxBorder {
    pub left: BorderSlice,
    pub right: BorderSlice,
    pub top: BorderSlice,
    pub bottom: BorderSlice,
    pub mode: BorderMode,
    pub radius: [f32; 4],
}

impl RawBoxBorder {
    #[allow(dead_code)]
    pub(crate) fn new(
        left: BorderSlice,
        right: BorderSlice,
        top: BorderSlice,
        bottom: BorderSlice,
        mode: BorderMode,
        radius: [f32; 4],
    ) -> Self {
        Self { left, right, top, bottom, mode, radius }
    }
}

impl BoxBorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn left(mut self, left: BorderSlice) -> Self {
        self.left = left;
        self
    }

    pub fn right(mut self, right: BorderSlice) -> Self {
        self.right = right;
        self
    }

    pub fn top(mut self, top: BorderSlice) -> Self {
        self.top = top;
        self
    }

    pub fn bottom(mut self, bottom: BorderSlice) -> Self {
        self.bottom = bottom;
        self
    }

    pub fn all(border: BorderSlice) -> Self {
        Self { left: border, right: border, top: border, bottom: border }
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

    /// Returns the color to paint the border with.
    ///
    /// The per-side border GPU pipeline currently supports only a single
    /// uniform border color, so we cannot honor a different color per side.
    /// Picking `left.color` unconditionally is wrong when only another side is
    /// set (e.g. a `bottom`-only border): `left.color` is then the default
    /// `Color::Transparent` and the border renders invisibly. Instead, return
    /// the color of the first side that is actually visible (non-`None` style
    /// and non-zero stroke), falling back to `left.color`.
    pub fn effective_color(&self, box_width: f32, box_height: f32, scale: f32) -> Color {
        let (l, t, r, b) = self.strokes(box_width, box_height, scale);
        if l > 0.0 && self.left.style != BorderStyle::None {
            self.left.color
        } else if t > 0.0 && self.top.style != BorderStyle::None {
            self.top.color
        } else if r > 0.0 && self.right.style != BorderStyle::None {
            self.right.color
        } else if b > 0.0 && self.bottom.style != BorderStyle::None {
            self.bottom.color
        } else {
            self.left.color
        }
    }

    pub fn horizontal(border: BorderSlice) -> Self {
        Self { top: border, bottom: border, ..Default::default() }
    }

    pub fn vertical(border: BorderSlice) -> Self {
        Self { left: border, right: border, ..Default::default() }
    }
}

impl BoxOutline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn left(mut self, left: BorderSlice) -> Self {
        self.left = left;
        self
    }

    pub fn right(mut self, right: BorderSlice) -> Self {
        self.right = right;
        self
    }

    pub fn top(mut self, top: BorderSlice) -> Self {
        self.top = top;
        self
    }

    pub fn bottom(mut self, bottom: BorderSlice) -> Self {
        self.bottom = bottom;
        self
    }

    pub fn all(border: BorderSlice) -> Self {
        Self { left: border, right: border, top: border, bottom: border }
    }

    /// Returns true if any side has a non-None style and non-zero stroke.
    pub fn has_visible_outline(&self, box_width: f32, box_height: f32, scale: f32) -> bool {
        let (l, t, r, b) = self.strokes(box_width, box_height, scale);
        (l > 0.0 && self.left.style != BorderStyle::None)
            || (t > 0.0 && self.top.style != BorderStyle::None)
            || (r > 0.0 && self.right.style != BorderStyle::None)
            || (b > 0.0 && self.bottom.style != BorderStyle::None)
    }

    /// Returns the color to paint the outline with. See
    /// [`BoxBorder::effective_color`] — the per-side outline pipeline supports a
    /// single uniform color, so we pick the color of the first visible side and
    /// fall back to `left.color`.
    pub fn effective_color(&self, box_width: f32, box_height: f32, scale: f32) -> Color {
        let (l, t, r, b) = self.strokes(box_width, box_height, scale);
        if l > 0.0 && self.left.style != BorderStyle::None {
            self.left.color
        } else if t > 0.0 && self.top.style != BorderStyle::None {
            self.top.color
        } else if r > 0.0 && self.right.style != BorderStyle::None {
            self.right.color
        } else if b > 0.0 && self.bottom.style != BorderStyle::None {
            self.bottom.color
        } else {
            self.left.color
        }
    }

    pub fn horizontal(border: BorderSlice) -> Self {
        Self { top: border, bottom: border, ..Default::default() }
    }

    pub fn vertical(border: BorderSlice) -> Self {
        Self { left: border, right: border, ..Default::default() }
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

        let is_uniform_style =
            self.left.style == self.right.style && self.left.style == self.top.style && self.left.style == self.bottom.style;
        let is_uniform_stroke = left_stroke == right_stroke && left_stroke == top_stroke && left_stroke == bottom_stroke;
        let is_uniform_color =
            self.left.color == self.right.color && self.left.color == self.top.color && self.left.color == self.bottom.color;

        // Uniform border: single stroke_rect call
        if is_uniform_style && is_uniform_stroke && is_uniform_color && left_stroke > 0.0 && self.left.style != BorderStyle::None {
            let (x, y, w, h) = if is_outline {
                (-left_stroke / 2.0, -left_stroke / 2.0, box_width + left_stroke, box_height + left_stroke)
            } else {
                (left_stroke / 2.0, left_stroke / 2.0, box_width - left_stroke, box_height - left_stroke)
            };
            canvas.stroke_rect(Vec2d { x, y }, ResolvedSize { width: w, height: h }, self.left.color, left_stroke, self.radius);
            return;
        }

        // Per-side borders with per-corner radii using the new per-side API.
        // When all colors are the same we can use a single stroke_rect_per_side call.
        if is_uniform_color && self.left.style != BorderStyle::None {
            let border_radius = self.radius;
            let border_width = [top_stroke, right_stroke, bottom_stroke, left_stroke];

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
            canvas.fill_color_rect(Vec2d { x, y }, ResolvedSize { width: w, height: h }, self.top.color, self.radius);
        }

        // Bottom border
        if self.bottom.style != BorderStyle::None && bottom_stroke > 0.0 {
            let (x, y, w, h) = if is_outline {
                (-left_stroke, box_height, box_width + left_stroke + right_stroke, bottom_stroke)
            } else {
                (0.0, box_height - bottom_stroke, box_width, bottom_stroke)
            };
            canvas.fill_color_rect(Vec2d { x, y }, ResolvedSize { width: w, height: h }, self.bottom.color, self.radius);
        }

        // Left border
        if self.left.style != BorderStyle::None && left_stroke > 0.0 {
            let (x, y, w, h) = if is_outline {
                (-left_stroke, -top_stroke, left_stroke, box_height + top_stroke + bottom_stroke)
            } else {
                (0.0, 0.0, left_stroke, box_height)
            };
            canvas.fill_color_rect(Vec2d { x, y }, ResolvedSize { width: w, height: h }, self.left.color, self.radius);
        }

        // Right border
        if self.right.style != BorderStyle::None && right_stroke > 0.0 {
            let (x, y, w, h) = if is_outline {
                (box_width, -top_stroke, right_stroke, box_height + top_stroke + bottom_stroke)
            } else {
                (box_width - right_stroke, 0.0, right_stroke, box_height)
            };
            canvas.fill_color_rect(Vec2d { x, y }, ResolvedSize { width: w, height: h }, self.right.color, self.radius);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(stroke: f32, color: Color) -> BorderSlice {
        BorderSlice { style: BorderStyle::Solid, stroke: Dimension::Px(stroke), color }
    }

    /// Regression for the website header: a `bottom`-only border must paint with
    /// the color set on that side, not the default (transparent) `left` color.
    #[test]
    fn effective_color_uses_bottom_only_side() {
        let border = BoxBorder { bottom: slice(8.0, Color::BLACK), ..Default::default() };
        // Before the fix this returned `left.color` == Transparent (invisible).
        assert_eq!(border.effective_color(200.0, 60.0, 1.0), Color::BLACK);
    }

    #[test]
    fn effective_color_prefers_left_when_visible() {
        let border = BoxBorder { left: slice(4.0, Color::RED), bottom: slice(8.0, Color::BLACK), ..Default::default() };
        assert_eq!(border.effective_color(200.0, 60.0, 1.0), Color::RED);
    }

    #[test]
    fn effective_color_falls_back_to_left_when_nothing_visible() {
        // No side has a stroke: fall back to the (default) left color.
        let border = BoxBorder::default();
        assert_eq!(border.effective_color(200.0, 60.0, 1.0), border.left.color);
    }

    #[test]
    fn effective_color_ignores_none_style_side() {
        // `left` has a stroke but style None → skip it, use the visible `top`.
        let border = BoxBorder {
            left: BorderSlice { style: BorderStyle::None, stroke: Dimension::Px(4.0), color: Color::RED },
            top: slice(2.0, Color::BLACK),
            ..Default::default()
        };
        assert_eq!(border.effective_color(200.0, 60.0, 1.0), Color::BLACK);
    }

    #[test]
    fn outline_effective_color_uses_visible_side() {
        let outline = BoxOutline { right: slice(3.0, Color::BLACK), ..Default::default() };
        assert_eq!(outline.effective_color(200.0, 60.0, 1.0), Color::BLACK);
    }
}
