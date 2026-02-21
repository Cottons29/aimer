pub use crate::style::text_style::{TextAlign, TextStyle, FontWeight, FontStyle};
use constructor::Constructor;
use crate::{Widget, Element};
use crate::base::BuildContext;
use skia_safe::{Color, Font, FontStyle as SkFontStyle, Paint, TextBlob, FontMgr};

/// this is a widget for creating the text 
#[allow(dead_code)]
#[derive(Constructor)]
pub struct Text {
    #[constructor(into, first)]
    text: String, 
    #[constructor(default)]
    text_align: Option<TextAlign>,
    #[constructor(default)]
    text_style: Option<TextStyle>,
}

impl Widget for Text {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        let text_style = self.text_style.clone().unwrap_or_default();
        let text_align = self.text_align.unwrap_or(TextAlign::TopLeft);
        
        Box::new(RawTextWidget {
            text: self.text.clone(),
            text_style,
            text_align,
        })
    }
}

/// this is low level TextWidget that covert to element
#[allow(dead_code)]
struct RawTextWidget {
    text: String,
    text_style: TextStyle,
    text_align: TextAlign,
}

impl Element for RawTextWidget {
    fn draw(&self, ctx: &BuildContext) {
        let weight = match self.text_style.font_weight {
            FontWeight::VeryThin => skia_safe::font_style::Weight::EXTRA_LIGHT, 
            FontWeight::Thin => skia_safe::font_style::Weight::THIN,
            FontWeight::Normal => skia_safe::font_style::Weight::NORMAL,
            FontWeight::Bold => skia_safe::font_style::Weight::BOLD,
            FontWeight::Bolder => skia_safe::font_style::Weight::EXTRA_BOLD,
            FontWeight::Value(v) => skia_safe::font_style::Weight::from(v as i32),
        };

        let slant = match self.text_style.font_style {
            FontStyle::Normal => skia_safe::font_style::Slant::Upright,
            FontStyle::Italic => skia_safe::font_style::Slant::Italic,
            FontStyle::Oblique => skia_safe::font_style::Slant::Oblique,
            FontStyle::ObliqueDeg(_) => skia_safe::font_style::Slant::Oblique,
        };
        let scale = ctx.scale;
        let sk_font_style = SkFontStyle::new(weight, skia_safe::font_style::Width::NORMAL, slant);
        
        // Use system default font or fallback
        let font_mgr = FontMgr::new();
        let typeface = font_mgr.match_family_style("Arial", sk_font_style)
             .or_else(|| font_mgr.match_family_style("Helvetica", sk_font_style))
             .or_else(|| font_mgr.match_family_style("", sk_font_style))
             .expect("Unable to load any typeface");
        let font_size = if self.text_style.font_size == 0 { 14.0 } else { self.text_style.font_size as f32};
        let scaled_font_size = font_size * scale;
        let font = Font::new(typeface, scaled_font_size);

        if let Some(blob) = TextBlob::new(&self.text, &font) {
            // Use typographic metrics for true centering
            let (text_width, _) = font.measure_text(&self.text, None);
            let (_, metrics) = font.metrics();

            let width = ctx.parent_size.width as f32;
            let height = ctx.parent_size.height as f32;
             
            let x = match self.text_align {
                TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
                TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => (width - text_width) / 2.0,
                TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => width - text_width,
            };

            let y = match self.text_align {
                 // Align top of the font (ascent) to the top of container
                 TextAlign::TopLeft | TextAlign::TopCenter | TextAlign::TopRight => -metrics.ascent, 
                 
                 // Align center of the font height (ascent + descent) to center of container
                 TextAlign::MidLeft | TextAlign::MidCenter | TextAlign::MidRight => height / 2.0 - (metrics.ascent + metrics.descent) / 2.0,
                 
                 // Align bottom of the font (descent) to bottom of container
                 TextAlign::BotLeft | TextAlign::BotCenter | TextAlign::BotRight => height - metrics.descent,
            };

            let color = self.text_style.color;

            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(Color::from(color));

            ctx.canvas.draw_text_blob(&blob, (x, y), &paint);
        }
    }
}

