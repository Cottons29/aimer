use crate::base::BuildContext;
use crate::components::drawable::Drawable;
use crate::style::border::{resolve_dim, BorderMode, BorderStyle, BoxBorder, RawBoxBorder};
use color::prelude::ColorMixer;

#[allow(dead_code)]
impl Drawable for RawBoxBorder {
    fn draw(&self, ctx: &BuildContext) {
        let canvas = ctx.canvas;
        let box_width = ctx.parent_size.width;
        let box_height = ctx.parent_size.height;
        let scale = ctx.scale;
        let is_outline = self.mode == BorderMode::Outside;

        let left_stroke = resolve_dim(self.left.stroke, box_width, scale);

        if left_stroke > 0.0 && self.left.style != BorderStyle::None {
            let color_str = self.left.color.to_css_color();
            canvas.set_stroke_style_str(&color_str);
            canvas.set_line_width(left_stroke);

            let (x, y, w, h) = if is_outline {
                (-left_stroke / 2.0, -left_stroke / 2.0, box_width + left_stroke, box_height + left_stroke)
            } else {
                (left_stroke / 2.0, left_stroke / 2.0, box_width - left_stroke, box_height - left_stroke)
            };

            if let Some(radius) = self.get_uniform_radius(box_width, box_height, scale) {
                let stroke_radius = if is_outline {
                    radius + left_stroke / 2.0
                } else {
                    (radius - left_stroke / 2.0).max(0.0)
                };
                canvas.begin_path();
                let _ = canvas.round_rect_with_f64(x, y, w, h, stroke_radius);
                canvas.stroke();
            } else {
                canvas.stroke_rect(x, y, w, h);
            }
        }
    }
}