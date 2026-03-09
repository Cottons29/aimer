use crate::base::BuildContext;
use crate::components::drawable::Drawable;
use crate::style::border::{resolve_dim, BorderMode, BorderStyle, BoxBorder, RawBoxBorder};

#[allow(dead_code)]
impl Drawable for RawBoxBorder {
    fn draw(&self, ctx: &BuildContext) {
        use skia_safe::{paint::Cap, paint::Style, Color as SkColor, Paint, PathBuilder, PathEffect, RRect, Rect};

        let canvas = ctx.canvas;
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

        if is_uniform_style && is_uniform_stroke && is_uniform_color && is_uniform_radius && left_stroke > 0.0 && self.left.style != BorderStyle::None {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_style(Style::Stroke);
            paint.set_color(SkColor::from(self.left.color));
            paint.set_stroke_width(left_stroke);

            match self.left.style {
                BorderStyle::Dashed => {
                    paint.set_path_effect(PathEffect::dash(&[left_stroke * 3.0, left_stroke * 3.0], 0.0).unwrap());
                }
                BorderStyle::Dotted => {
                    paint.set_stroke_cap(Cap::Round);
                    paint.set_path_effect(PathEffect::dash(&[0.1, left_stroke * 2.0], 0.0).unwrap());
                }
                _ => {}
            }

            let rect = if is_outline {
                Rect::from_xywh(-left_stroke / 2.0, -left_stroke / 2.0, box_width + left_stroke, box_height + left_stroke)
            } else {
                Rect::from_xywh(left_stroke / 2.0, left_stroke / 2.0, box_width - left_stroke, box_height - left_stroke)
            };
            if left_radius > 0.0 {
                let stroke_radius = if is_outline {
                    left_radius + left_stroke / 2.0
                } else {
                    (left_radius - left_stroke / 2.0).max(0.0)
                };
                let rrect = RRect::new_rect_xy(rect, stroke_radius, stroke_radius);
                canvas.draw_rrect(rrect, &paint);
            } else {
                canvas.draw_rect(rect, &paint);
            }
            return;
        }

        // Top border
        if self.top.style != BorderStyle::None && top_stroke > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(SkColor::from(self.top.color));
            match self.top.style {
                BorderStyle::Solid => {
                    paint.set_style(Style::Fill);
                    let mut builder = PathBuilder::new();
                    if is_outline {
                        builder.move_to((-left_stroke, -top_stroke));
                        builder.line_to((box_width + right_stroke, -top_stroke));
                        builder.line_to((box_width, 0.0));
                        builder.line_to((0.0, 0.0));
                    } else {
                        builder.move_to((0.0, 0.0));
                        builder.line_to((box_width, 0.0));
                        builder.line_to((box_width - right_stroke, top_stroke));
                        builder.line_to((left_stroke, top_stroke));
                    }
                    builder.close();
                    canvas.draw_path(&builder.detach(), &paint);
                }
                BorderStyle::Dashed => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(top_stroke);
                    paint.set_path_effect(PathEffect::dash(&[top_stroke * 3.0, top_stroke * 3.0], 0.0).unwrap());
                    let y = if is_outline { -top_stroke / 2.0 } else { top_stroke / 2.0 };
                    canvas.draw_line((0.0, y), (box_width, y), &paint);
                }
                BorderStyle::Dotted => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(top_stroke);
                    paint.set_stroke_cap(Cap::Round);
                    paint.set_path_effect(PathEffect::dash(&[0.1, top_stroke * 2.0], 0.0).unwrap());
                    let y = if is_outline { -top_stroke / 2.0 } else { top_stroke / 2.0 };
                    canvas.draw_line((0.0, y), (box_width, y), &paint);
                }
                _ => {}
            }
        }

        // Bottom border
        if self.bottom.style != BorderStyle::None && bottom_stroke > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(SkColor::from(self.bottom.color));
            match self.bottom.style {
                BorderStyle::Solid => {
                    paint.set_style(Style::Fill);
                    let mut builder = PathBuilder::new();
                    if is_outline {
                        builder.move_to((-left_stroke, box_height + bottom_stroke));
                        builder.line_to((box_width + right_stroke, box_height + bottom_stroke));
                        builder.line_to((box_width, box_height));
                        builder.line_to((0.0, box_height));
                    } else {
                        builder.move_to((0.0, box_height));
                        builder.line_to((box_width, box_height));
                        builder.line_to((box_width - right_stroke, box_height - bottom_stroke));
                        builder.line_to((left_stroke, box_height - bottom_stroke));
                    }
                    builder.close();
                    canvas.draw_path(&builder.detach(), &paint);
                }
                BorderStyle::Dashed => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(bottom_stroke);
                    paint.set_path_effect(PathEffect::dash(&[bottom_stroke * 3.0, bottom_stroke * 3.0], 0.0).unwrap());
                    let y = if is_outline { box_height + bottom_stroke / 2.0 } else { box_height - bottom_stroke / 2.0 };
                    canvas.draw_line((0.0, y), (box_width, y), &paint);
                }
                BorderStyle::Dotted => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(bottom_stroke);
                    paint.set_stroke_cap(Cap::Round);
                    paint.set_path_effect(PathEffect::dash(&[0.1, bottom_stroke * 2.0], 0.0).unwrap());
                    let y = if is_outline { box_height + bottom_stroke / 2.0 } else { box_height - bottom_stroke / 2.0 };
                    canvas.draw_line((0.0, y), (box_width, y), &paint);
                }
                _ => {}
            }
        }

        // Left border
        if self.left.style != BorderStyle::None && left_stroke > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(SkColor::from(self.left.color));
            match self.left.style {
                BorderStyle::Solid => {
                    paint.set_style(Style::Fill);
                    let mut builder = PathBuilder::new();
                    if is_outline {
                        builder.move_to((-left_stroke, -top_stroke));
                        builder.line_to((-left_stroke, box_height + bottom_stroke));
                        builder.line_to((0.0, box_height));
                        builder.line_to((0.0, 0.0));
                    } else {
                        builder.move_to((0.0, 0.0));
                        builder.line_to((0.0, box_height));
                        builder.line_to((left_stroke, box_height - bottom_stroke));
                        builder.line_to((left_stroke, top_stroke));
                    }
                    builder.close();
                    canvas.draw_path(&builder.detach(), &paint);
                }
                BorderStyle::Dashed => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(left_stroke);
                    paint.set_path_effect(PathEffect::dash(&[left_stroke * 3.0, left_stroke * 3.0], 0.0).unwrap());
                    let x = if is_outline { -left_stroke / 2.0 } else { left_stroke / 2.0 };
                    canvas.draw_line((x, 0.0), (x, box_height), &paint);
                }
                BorderStyle::Dotted => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(left_stroke);
                    paint.set_stroke_cap(Cap::Round);
                    paint.set_path_effect(PathEffect::dash(&[0.1, left_stroke * 2.0], 0.0).unwrap());
                    let x = if is_outline { -left_stroke / 2.0 } else { left_stroke / 2.0 };
                    canvas.draw_line((x, 0.0), (x, box_height), &paint);
                }
                _ => {}
            }
        }

        // Right border
        if self.right.style != BorderStyle::None && right_stroke > 0.0 {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(SkColor::from(self.right.color));
            match self.right.style {
                BorderStyle::Solid => {
                    paint.set_style(Style::Fill);
                    let mut builder = PathBuilder::new();
                    if is_outline {
                        builder.move_to((box_width + right_stroke, -top_stroke));
                        builder.line_to((box_width + right_stroke, box_height + bottom_stroke));
                        builder.line_to((box_width, box_height));
                        builder.line_to((box_width, 0.0));
                    } else {
                        builder.move_to((box_width, 0.0));
                        builder.line_to((box_width, box_height));
                        builder.line_to((box_width - right_stroke, box_height - bottom_stroke));
                        builder.line_to((box_width - right_stroke, top_stroke));
                    }
                    builder.close();
                    canvas.draw_path(&builder.detach(), &paint);
                }
                BorderStyle::Dashed => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(right_stroke);
                    paint.set_path_effect(PathEffect::dash(&[right_stroke * 3.0, right_stroke * 3.0], 0.0).unwrap());
                    let x = if is_outline { box_width + right_stroke / 2.0 } else { box_width - right_stroke / 2.0 };
                    canvas.draw_line((x, 0.0), (x, box_height), &paint);
                }
                BorderStyle::Dotted => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(right_stroke);
                    paint.set_stroke_cap(Cap::Round);
                    paint.set_path_effect(PathEffect::dash(&[0.1, right_stroke * 2.0], 0.0).unwrap());
                    let x = if is_outline { box_width + right_stroke / 2.0 } else { box_width - right_stroke / 2.0 };
                    canvas.draw_line((x, 0.0), (x, box_height), &paint);
                }
                _ => {}
            }
        }
    }
}