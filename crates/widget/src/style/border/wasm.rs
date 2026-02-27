use attribute::dimension::Dimension;
use constructor::Constructor;
use crate::style::border::{BorderSide, BorderStyle};
use color::prelude::ColorMixer;

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

    pub fn get_uniform_radius(&self, box_width: f64, box_height: f64, scale: f64) -> Option<f64> {
        let get_r = |dim: Dimension, parent_val: f64| -> f64 {
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

    pub fn draw(&self, canvas: &web_sys::CanvasRenderingContext2d, box_width: f64, box_height: f64, scale: f64) {
        let get_stroke = |dim: Dimension, parent_val: f64| -> f64 {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };

        let left_stroke = get_stroke(self.left.stroke, box_width);
        
        if left_stroke > 0.0 && self.left.style != BorderStyle::None {
            let color_str = self.left.color.to_css_color();
            canvas.set_stroke_style_str(&color_str);
            canvas.set_line_width(left_stroke);
            
            if let Some(radius) = self.get_uniform_radius(box_width, box_height, scale) {
                canvas.begin_path();
                let _ = canvas.round_rect_with_f64(
                    left_stroke / 2.0,
                    left_stroke / 2.0,
                    box_width - left_stroke,
                    box_height - left_stroke,
                    radius,
                );
                canvas.stroke();
            } else {
                canvas.stroke_rect(
                    left_stroke / 2.0,
                    left_stroke / 2.0,
                    box_width - left_stroke,
                    box_height - left_stroke
                );
            }
        }
    }
}