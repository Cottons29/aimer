use crate::base::BuildContext;
use crate::components::drawable::Drawable;
use crate::style::border::{resolve_dim, BorderMode, BorderStyle, BoxBorder, RawBoxBorder};
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;

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

        let left_radius = resolve_dim(self.left.radius, box_width, scale);
        let right_radius = resolve_dim(self.right.radius, box_width, scale);
        let top_radius = resolve_dim(self.top.radius, box_height, scale);
        let bottom_radius = resolve_dim(self.bottom.radius, box_height, scale);

        let is_uniform_style = self.left.style == self.right.style && self.left.style == self.top.style && self.left.style == self.bottom.style;
        let is_uniform_stroke = left_stroke == right_stroke && left_stroke == top_stroke && left_stroke == bottom_stroke;
        let is_uniform_color = self.left.color == self.right.color && self.left.color == self.top.color && self.left.color == self.bottom.color;
        let is_uniform_radius = left_radius == right_radius && left_radius == top_radius && left_radius == bottom_radius;

        // Uniform border: single stroke_rect call
        if is_uniform_style && is_uniform_stroke && is_uniform_color && is_uniform_radius && left_stroke > 0.0 && self.left.style != BorderStyle::None {
            let (x, y, w, h) = if is_outline {
                (-left_stroke / 2.0, -left_stroke / 2.0, box_width + left_stroke, box_height + left_stroke)
            } else {
                (left_stroke / 2.0, left_stroke / 2.0, box_width - left_stroke, box_height - left_stroke)
            };
            let stroke_radius = if left_radius > 0.0 {
                if is_outline {
                    left_radius + left_stroke / 2.0
                } else {
                    (left_radius - left_stroke / 2.0).max(0.0)
                }
            } else {
                0.0
            };
            canvas.stroke_rect(
                (x, y).into(),
                ResolvedSize { width: w, height: h },
                self.left.color,
                left_stroke,
                stroke_radius,
            );
            return;
        }

        // Per-side borders: draw each side as a filled rectangle
        // Top border
        if self.top.style != BorderStyle::None && top_stroke > 0.0 {
            let (x, y, w, h) = if is_outline {
                (-left_stroke, -top_stroke, box_width + left_stroke + right_stroke, top_stroke)
            } else {
                (0.0, 0.0, box_width, top_stroke)
            };
            canvas.fill_color_rect(
                (x, y).into(),
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
                (x, y).into(),
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
                (x, y).into(),
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
                (x, y).into(),
                ResolvedSize { width: w, height: h },
                self.right.color,
                0.0,
            );
        }
    }
}