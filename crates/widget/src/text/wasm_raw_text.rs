use crate::text::{FontStyle, FontWeight, TextAlign};
use crate::{Element, LayoutCache, TextOverflow};
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
            FontWeight::Value(_v) => "normal",
        };
        
        let style = match self.text_style.font_style {
            FontStyle::Normal => "normal",
            FontStyle::Italic => "italic",
            FontStyle::Oblique | FontStyle::ObliqueDeg(_) => "oblique",
        };
        
        format!("{} {} {}px Arial, sans-serif", style, weight, scaled_font_size)
    }
}

impl Element for RawTextWidget {
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
        canvas.set_fill_style(&wasm_bindgen::JsValue::from_str(&color_str));
        
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