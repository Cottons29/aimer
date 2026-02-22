use color::prelude::Color;
use constructor::Constructor;

use crate::base::Dimension;
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
impl BoxBorder {
    pub fn all(border: BorderSide) -> Self {
        Self { left: border, right: border, top: border, bottom: border }
    }

    pub fn horizontal(border: BorderSide) -> Self {
        Self { top: border, bottom: border, ..Default::default() }
    }

    pub fn vertical(border: BorderSide) -> Self {
        Self { left: border, right: border, ..Default::default() }
    }

    pub fn get_uniform_radius(&self, box_width: f32, box_height: f32, scale: f32) -> Option<f32> {
        let get_r = |dim: Dimension, parent_val: f32| -> f32 {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };
        let left_r = get_r(self.left.radius, box_width);
        let right_r = get_r(self.right.radius, box_width);
        let top_r = get_r(self.top.radius, box_height);
        let bottom_r = get_r(self.bottom.radius, box_height);

        if left_r == right_r && left_r == top_r && left_r == bottom_r && left_r > 0.0 {
            Some(left_r)
        } else {
            None
        }
    }

    pub fn draw(&self, canvas: &skia_safe::Canvas, box_width: f32, box_height: f32, scale: f32) {
        use skia_safe::{Paint, paint::Style, paint::Cap, Color as SkColor, PathBuilder, PathEffect, Rect, RRect};

        let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };

        let left_stroke = get_stroke(self.left.stroke, box_width);
        let right_stroke = get_stroke(self.right.stroke, box_width);
        let top_stroke = get_stroke(self.top.stroke, box_height);
        let bottom_stroke = get_stroke(self.bottom.stroke, box_height);
        
        let left_radius = get_stroke(self.left.radius, box_width);
        let right_radius = get_stroke(self.right.radius, box_width);
        let top_radius = get_stroke(self.top.radius, box_height);
        let bottom_radius = get_stroke(self.bottom.radius, box_height);

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
            
            let rect = Rect::from_xywh(left_stroke / 2.0, left_stroke / 2.0, box_width - left_stroke, box_height - left_stroke);
            if left_radius > 0.0 {
                let stroke_radius = (left_radius - left_stroke / 2.0).max(0.0);
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
                    builder.move_to((0.0, 0.0));
                    builder.line_to((box_width, 0.0));
                    builder.line_to((box_width - right_stroke, top_stroke));
                    builder.line_to((left_stroke, top_stroke));
                    builder.close();
                    canvas.draw_path(&builder.detach(), &paint);
                }
                BorderStyle::Dashed => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(top_stroke);
                    paint.set_path_effect(PathEffect::dash(&[top_stroke * 3.0, top_stroke * 3.0], 0.0).unwrap());
                    canvas.draw_line((0.0, top_stroke / 2.0), (box_width, top_stroke / 2.0), &paint);
                }
                BorderStyle::Dotted => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(top_stroke);
                    paint.set_stroke_cap(Cap::Round);
                    paint.set_path_effect(PathEffect::dash(&[0.1, top_stroke * 2.0], 0.0).unwrap());
                    canvas.draw_line((0.0, top_stroke / 2.0), (box_width, top_stroke / 2.0), &paint);
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
                    builder.move_to((0.0, box_height));
                    builder.line_to((box_width, box_height));
                    builder.line_to((box_width - right_stroke, box_height - bottom_stroke));
                    builder.line_to((left_stroke, box_height - bottom_stroke));
                    builder.close();
                    canvas.draw_path(&builder.detach(), &paint);
                }
                BorderStyle::Dashed => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(bottom_stroke);
                    paint.set_path_effect(PathEffect::dash(&[bottom_stroke * 3.0, bottom_stroke * 3.0], 0.0).unwrap());
                    canvas.draw_line((0.0, box_height - bottom_stroke / 2.0), (box_width, box_height - bottom_stroke / 2.0), &paint);
                }
                BorderStyle::Dotted => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(bottom_stroke);
                    paint.set_stroke_cap(Cap::Round);
                    paint.set_path_effect(PathEffect::dash(&[0.1, bottom_stroke * 2.0], 0.0).unwrap());
                    canvas.draw_line((0.0, box_height - bottom_stroke / 2.0), (box_width, box_height - bottom_stroke / 2.0), &paint);
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
                    builder.move_to((0.0, 0.0));
                    builder.line_to((0.0, box_height));
                    builder.line_to((left_stroke, box_height - bottom_stroke));
                    builder.line_to((left_stroke, top_stroke));
                    builder.close();
                    canvas.draw_path(&builder.detach(), &paint);
                }
                BorderStyle::Dashed => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(left_stroke);
                    paint.set_path_effect(PathEffect::dash(&[left_stroke * 3.0, left_stroke * 3.0], 0.0).unwrap());
                    canvas.draw_line((left_stroke / 2.0, 0.0), (left_stroke / 2.0, box_height), &paint);
                }
                BorderStyle::Dotted => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(left_stroke);
                    paint.set_stroke_cap(Cap::Round);
                    paint.set_path_effect(PathEffect::dash(&[0.1, left_stroke * 2.0], 0.0).unwrap());
                    canvas.draw_line((left_stroke / 2.0, 0.0), (left_stroke / 2.0, box_height), &paint);
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
                    builder.move_to((box_width, 0.0));
                    builder.line_to((box_width, box_height));
                    builder.line_to((box_width - right_stroke, box_height - bottom_stroke));
                    builder.line_to((box_width - right_stroke, top_stroke));
                    builder.close();
                    canvas.draw_path(&builder.detach(), &paint);
                }
                BorderStyle::Dashed => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(right_stroke);
                    paint.set_path_effect(PathEffect::dash(&[right_stroke * 3.0, right_stroke * 3.0], 0.0).unwrap());
                    canvas.draw_line((box_width - right_stroke / 2.0, 0.0), (box_width - right_stroke / 2.0, box_height), &paint);
                }
                BorderStyle::Dotted => {
                    paint.set_style(Style::Stroke);
                    paint.set_stroke_width(right_stroke);
                    paint.set_stroke_cap(Cap::Round);
                    paint.set_path_effect(PathEffect::dash(&[0.1, right_stroke * 2.0], 0.0).unwrap());
                    canvas.draw_line((box_width - right_stroke / 2.0, 0.0), (box_width - right_stroke / 2.0, box_height), &paint);
                }
                _ => {}
            }
        }
    }
}
