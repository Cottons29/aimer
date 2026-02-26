use std::sync::Mutex;
use skia_safe::{Color, Font, FontMgr, Paint, TextBlob, Typeface};
use attribute::size::ResolvedSize;
use crate::text::{FontStyle, FontWeight, TextAlign};
use crate::{Element, LayoutCache, TextOverflow};
use crate::base::BuildContext;
use crate::style::text_style::TextStyle;

/// this is low level TextWidget that covert to element
#[allow(dead_code)]
pub struct RawTextWidget {
    pub text: String,
    pub text_style: TextStyle,
    pub text_align: TextAlign,
    pub cache: LayoutCache,
    pub typeface: Mutex<Option<Typeface>>,
}

impl RawTextWidget {
    fn get_typeface(&self) -> Typeface {
        let mut guard = self.typeface.lock().unwrap();
        if let Some(ref tf) = *guard {
            return tf.clone();
        }

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

        let sk_font_style = SkFontStyle::new(weight, skia_safe::font_style::Width::NORMAL, slant);
        let font_mgr = FontMgr::new();
        let typeface = font_mgr.match_family_style("Arial", sk_font_style)
            .or_else(|| font_mgr.match_family_style("Helvetica", sk_font_style))
            .or_else(|| font_mgr.match_family_style("", sk_font_style))
            .expect("Unable to load any typeface");

        *guard = Some(typeface.clone());
        typeface
    }

    fn make_font(&self, scale: f32) -> Font {
        let typeface = self.get_typeface();
        let font_size = if self.text_style.font_size == 0 { 14.0 } else { self.text_style.font_size as f32 };
        let scaled_font_size = font_size * scale;
        Font::new(typeface, scaled_font_size)
    }
}

impl Element for RawTextWidget {
    fn draw(&self, ctx: &BuildContext) {
        let font = self.make_font(ctx.scale);

        if TextBlob::new(&self.text, &font).is_some() {
            // Use typographic metrics for true centering
            let (text_width, _) = font.measure_text(&self.text, None);
            let (_, metrics) = font.metrics();

            let width = ctx.parent_size.width;
            let height = ctx.parent_size.height;

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

            let mut display_text = self.text.clone();
            let mut display_x = x;

            match self.text_style.text_overflow {
                TextOverflow::Clip => {
                    ctx.canvas.save();
                    ctx.canvas.clip_rect(
                        skia_safe::Rect::from_xywh(0.0, 0.0, width, height),
                        None,
                        false,
                    );
                    if let Some(blob) = TextBlob::new(&display_text, &font) {
                        ctx.canvas.draw_text_blob(&blob, (display_x, y), &paint);
                    }
                    ctx.canvas.restore();
                    return;
                }
                TextOverflow::Ellipsis => {
                    if text_width > width {
                        let ellipsis = "...";
                        let (ellipsis_width, _) = font.measure_text(ellipsis, None);
                        let available_width = width - ellipsis_width;

                        if available_width > 0.0 {
                            let mut current_text = String::new();
                            for c in self.text.chars() {
                                let next_text = format!("{}{}", current_text, c);
                                let (w, _) = font.measure_text(&next_text, None);
                                if w > available_width {
                                    break;
                                }
                                current_text = next_text;
                            }
                            display_text = format!("{}{}", current_text, ellipsis);
                            let (new_width, _) = font.measure_text(&display_text, None);

                            display_x = match self.text_align {
                                TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
                                TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => (width - new_width) / 2.0,
                                TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => width - new_width,
                            };
                        } else {
                            // If not even ellipsis fits, just clip or show nothing?
                            // For now, let's just use ellipsis even if it's too wide
                            display_text = ellipsis.to_string();
                            display_x = 0.0;
                        }
                    }
                }
                TextOverflow::Wrap => {
                    let mut lines = Vec::new();
                    let mut current_line = String::new();
                    let words = self.text.split_whitespace();

                    for word in words {
                        let test_line = if current_line.is_empty() {
                            word.to_string()
                        } else {
                            format!("{} {}", current_line, word)
                        };

                        let (test_width, _) = font.measure_text(&test_line, None);
                        if test_width <= width {
                            current_line = test_line;
                        } else {
                            if !current_line.is_empty() {
                                lines.push(current_line);
                            }
                            current_line = word.to_string();
                        }
                    }
                    if !current_line.is_empty() {
                        lines.push(current_line);
                    }

                    let line_height = metrics.bottom - metrics.top;
                    for (i, line) in lines.iter().enumerate() {
                        let (line_width, _) = font.measure_text(line, None);
                        let line_x = match self.text_align {
                            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
                            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => (width - line_width) / 2.0,
                            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => width - line_width,
                        };
                        let line_y = y + i as f32 * line_height;
                        if let Some(blob) = TextBlob::new(line, &font) {
                            ctx.canvas.draw_text_blob(&blob, (line_x, line_y), &paint);
                        }
                    }
                    return;
                }
                _ => {}
            }

            if let Some(blob) = TextBlob::new(&display_text, &font) {
                ctx.canvas.draw_text_blob(&blob, (display_x, y), &paint);
            }
        }
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_computed(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let font = self.make_font(ctx.scale);
        let (_, metrics) = font.metrics();
        let line_height = metrics.bottom - metrics.top;

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

                    let (test_width, _) = font.measure_text(&test_line, None);
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

                ResolvedSize {
                    width,
                    height: (lines_count as f32 * line_height).ceil(),
                }
            }
            _ => {
                let (text_width, _) = font.measure_text(&self.text, None);
                ResolvedSize {
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

