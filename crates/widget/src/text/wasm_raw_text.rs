use crate::text::{FontStyle, FontWeight, TextAlign};
use crate::{Drawable, Element, LayoutCache, TextOverflow};
use crate::base::BuildContext;
use crate::style::text_style::TextStyle;
use std::sync::Mutex;
use color::prelude::ColorMixer;

#[allow(dead_code)]
pub struct RawTextWidget {
    pub text: String,
    pub text_style: TextStyle,
    pub text_align: TextAlign,
    pub cache: LayoutCache,
    pub typeface: Mutex<Option<()>>,
}
/// this is low level TextWidget that covert to element
impl RawTextWidget {
    fn get_css_font(&self, scale: f64) -> String {
        let font_size = if self.text_style.font_size == 0 { 14.0 } else { self.text_style.font_size as f64 };
        let scaled_font_size = font_size * scale;
        
        let weight = match self.text_style.font_weight {
            FontWeight::VeryThin => "100",
            FontWeight::Thin => "300",
            FontWeight::Normal => "normal",
            FontWeight::Bold => "bold",
            FontWeight::Bolder => "900",
            FontWeight::Value(v) => "",
        };
        
        let style = match self.text_style.font_style {
            FontStyle::Normal => "normal",
            FontStyle::Italic => "italic",
            FontStyle::Oblique | FontStyle::ObliqueDeg(_) => "oblique",
        };
        
        format!("{} {} {}px Arial, sans-serif", style, weight, scaled_font_size)
    }
}

impl Drawable for RawTextWidget {
    fn draw(&self, ctx: &BuildContext) {
        let canvas = &ctx.canvas;
        let font_str = self.get_css_font(ctx.scale);
        canvas.set_font(&font_str);

        let argb = self.text_style.color.to_u32();
        let a = ((argb >> 24) & 0xFF) as f32 / 255.0;
        let r = (argb >> 16) & 0xFF;
        let g = (argb >> 8) & 0xFF;
        let b = argb & 0xFF;
        let color_str = format!("rgba({}, {}, {}, {})", r, g, b, a);
        canvas.set_fill_style_str(&color_str);

        let width = ctx.parent_size.width;
        let height = ctx.parent_size.height;

        let align_str = match self.text_align {
            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => "left",
            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => "center",
            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => "right",
        };
        canvas.set_text_align(align_str);

        let baseline_str = match self.text_align {
            TextAlign::TopLeft | TextAlign::TopCenter | TextAlign::TopRight => "top",
            TextAlign::MidLeft | TextAlign::MidCenter | TextAlign::MidRight => "middle",
            TextAlign::BotLeft | TextAlign::BotCenter | TextAlign::BotRight => "bottom",
        };
        canvas.set_text_baseline(baseline_str);

        let x = match self.text_align {
            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => width / 2.0,
            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => width,
        };

        let y = match self.text_align {
            TextAlign::TopLeft | TextAlign::TopCenter | TextAlign::TopRight => 0.0,
            TextAlign::MidLeft | TextAlign::MidCenter | TextAlign::MidRight => height / 2.0,
            TextAlign::BotLeft | TextAlign::BotCenter | TextAlign::BotRight => height,
        };

        let _ = canvas.fill_text(&self.text, x as f64, y as f64);
    }
}

impl Element for RawTextWidget {


    fn computed_size(&self, ctx: &BuildContext) -> attribute::size::ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_computed(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let canvas = ctx.canvas;
        let font_str = self.get_css_font(ctx.scale);
        canvas.set_font(&font_str);

        let text_width = match canvas.measure_text(&self.text) {
            Ok(metrics) => metrics.width(),
            Err(_) => 0.0,
        };

        let font_size = if self.text_style.font_size == 0 { 14.0 } else { self.text_style.font_size as f64 };
        let line_height = font_size * ctx.scale;

        let result = match self.text_style.text_overflow {
            TextOverflow::Wrap => {
                let width = if ctx.box_constraint.max_width > 0.0 {
                    ctx.box_constraint.max_width
                } else {
                    ctx.parent_size.width
                };

                let mut lines_count = 0;
                let mut current_line = String::new();
                let words = self.text.split_whitespace();

                for word in words {
                    let test_line = if current_line.is_empty() {
                        word.to_string()
                    } else {
                        format!("{} {}", current_line, word)
                    };

                    let test_width = match canvas.measure_text(&test_line) {
                        Ok(metrics) => metrics.width(),
                        Err(_) => 0.0,
                    };
                    
                    if test_width <= width {
                        current_line = test_line;
                    } else {
                        if !current_line.is_empty() {
                            lines_count += 1;
                        }
                        current_line = word.to_string();
                    }
                }
                if !current_line.is_empty() {
                    lines_count += 1;
                }

                attribute::size::ResolvedSize {
                    width,
                    height: (lines_count as f64 * line_height).ceil(),
                }
            }
            _ => {
                attribute::size::ResolvedSize {
                    width: text_width.ceil(),
                    height: line_height.ceil(),
                }
            }
        };

        self.cache.set_computed(ctx.box_constraint, scale_bits, result);
        result
    }

    fn invalidate_layout(&self) {
        self.cache.invalidate();
    }
}