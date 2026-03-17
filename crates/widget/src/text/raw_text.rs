use crate::base::BuildContext;
use crate::style::text_style::TextStyle;
use crate::text::{FontStyle, FontWeight, TextAlign};
use crate::{Drawable, Element, LayoutCache, TextOverflow};
use attribute::size::ResolvedSize;
use skia_safe::font_style::FontStyle as SkFontStyle;
use skia_safe::{Canvas, Color, Font, FontMgr, Paint, TextBlob, Typeface};
use std::sync::Mutex;
use utils::debug;

thread_local! {
    static FONT_MGR: FontMgr = FontMgr::new();
}

/// A contiguous run of text that shares the same font.
#[derive(Clone)]
pub struct TextRun {
    text: String,
    font: Font,
}

/// Segment `text` into runs where each run uses either the primary font or a
/// fallback font obtained via `FontMgr::match_family_style_character`.
fn build_text_runs(text: &str, primary_font: &Font) -> Vec<TextRun> {
    if text.is_empty() {
        return Vec::new();
    }

    let primary_tf = primary_font.typeface();
    let font_size = primary_font.size();
    let style = primary_tf.font_style();

    let mut runs: Vec<TextRun> = Vec::new();
    let mut current_text = String::new();
    let mut current_is_primary = true;
    let mut current_fallback_tf: Option<Typeface> = None;

    for ch in text.chars() {
        let glyph = primary_tf.unichar_to_glyph(ch as i32);
        let has_glyph = glyph != 0;

        if has_glyph {
            if current_is_primary {
                current_text.push(ch);
            } else {
                if !current_text.is_empty() {
                    let font = if let Some(ref tf) = current_fallback_tf {
                        Font::new(tf.clone(), font_size)
                    } else {
                        primary_font.clone()
                    };
                    runs.push(TextRun { text: current_text, font });
                    current_text = String::new();
                }
                current_is_primary = true;
                current_fallback_tf = None;
                current_text.push(ch);
            }
        } else {
            let fallback = FONT_MGR.with(|mgr| mgr.match_family_style_character("", style, &[""], ch as i32));

            if current_is_primary {
                if !current_text.is_empty() {
                    runs.push(TextRun { text: current_text, font: primary_font.clone() });
                    current_text = String::new();
                }
                current_is_primary = false;
                current_fallback_tf = fallback;
                current_text.push(ch);
            } else {
                let same_fallback = match (&current_fallback_tf, &fallback) {
                    (Some(a), Some(b)) => Typeface::equal(a, b),
                    (None, None) => true,
                    _ => false,
                };
                if same_fallback {
                    current_text.push(ch);
                } else {
                    if !current_text.is_empty() {
                        let font = if let Some(ref tf) = current_fallback_tf {
                            Font::new(tf.clone(), font_size)
                        } else {
                            primary_font.clone()
                        };
                        runs.push(TextRun { text: current_text, font });
                        current_text = String::new();
                    }
                    current_fallback_tf = fallback;
                    current_text.push(ch);
                }
            }
        }
    }

    if !current_text.is_empty() {
        let font = if current_is_primary {
            primary_font.clone()
        } else if let Some(ref tf) = current_fallback_tf {
            Font::new(tf.clone(), font_size)
        } else {
            primary_font.clone()
        };
        runs.push(TextRun { text: current_text, font });
    }

    runs
}

fn measure_text_with_fallback(text: &str, primary_font: &Font) -> f32 {
    let runs = build_text_runs(text, primary_font);
    let mut total_width: f32 = 0.0;
    for run in &runs {
        let (w, _) = run.font.measure_text(&run.text, None);
        total_width += w;
    }
    total_width
}

fn draw_text_with_fallback(canvas: &Canvas, text: &str, primary_font: &Font, x: f32, y: f32, paint: &Paint) {
    let runs = build_text_runs(text, primary_font);
    let mut cursor_x = x;
    for run in &runs {
        if let Some(blob) = TextBlob::new(&run.text, &run.font) {
            canvas.draw_text_blob(&blob, (cursor_x, y), paint);
        }
        let (w, _) = run.font.measure_text(&run.text, None);
        cursor_x += w;
    }
}

pub struct RawTextWidget {
    pub text: String,
    pub text_style: TextStyle,
    pub text_align: TextAlign,
    pub cache: LayoutCache,
    pub typeface: Mutex<Option<Typeface>>,
    pub text_runs: Mutex<Option<(f32, Vec<TextRun>)>>,
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
        let typeface = FONT_MGR.with(|mgr| {
            mgr.match_family_style("Arial", sk_font_style)
                .or_else(|| mgr.match_family_style("Helvetica", sk_font_style))
                .or_else(|| mgr.match_family_style("", sk_font_style))
                .expect("Unable to load any typeface")
        });

        *guard = Some(typeface.clone());
        typeface
    }

    fn make_font(&self, scale: f32) -> Font {
        let typeface = self.get_typeface();
        let font_size = if self.text_style.font_size == 0 { 14.0 } else { self.text_style.font_size as f32 };
        let scaled_font_size = font_size * scale;
        Font::new(typeface, scaled_font_size)
    }

    fn get_text_runs(&self, font: &Font) -> (f32, Vec<TextRun>) {
        let mut guard = self.text_runs.lock().unwrap();
        if let Some((cached_size, ref runs)) = *guard {
            if (cached_size - font.size()).abs() < 0.001 {
                return (cached_size, runs.clone());
            }
        }

        let runs = build_text_runs(&self.text, font);
        // let mut total_width: f32 = 0.0;
        // for run in &runs {
        //     let (w, _) = run.font.measure_text(&run.text, None);
        //     total_width += w;
        // }
        *guard = Some((font.size(), runs.clone()));
        (font.size(), runs)
    }

    fn measure_runs(runs: &[TextRun]) -> f32 {
        let mut total_width: f32 = 0.0;
        for run in runs {
            let (w, _) = run.font.measure_text(&run.text, None);
            total_width += w;
        }
        total_width
    }

    fn draw_runs(&self, canvas: &Canvas, runs: &[TextRun], x: f32, y: f32, paint: &Paint) {
        let mut cursor_x = x;
        for run in runs {
            if let Some(blob) = TextBlob::new(&run.text, &run.font) {
                canvas.draw_text_blob(&blob, (cursor_x, y), paint);
            }
            let (w, _) = run.font.measure_text(&run.text, None);
            cursor_x += w;
        }
    }
}

impl Drawable for RawTextWidget {
    fn draw(&self, ctx: &BuildContext) {
        let font = self.make_font(ctx.scale);
        let (_, runs) = self.get_text_runs(&font);
        let text_width = Self::measure_runs(&runs);
        let (_, metrics) = font.metrics();

        let width = ctx.parent_size.width;
        let height = ctx.parent_size.height;

        let x = match self.text_align {
            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => (width - text_width) / 2.0,
            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => width - text_width,
        };

        let y = match self.text_align {
            TextAlign::TopLeft | TextAlign::TopCenter | TextAlign::TopRight => -metrics.ascent,
            TextAlign::MidLeft | TextAlign::MidCenter | TextAlign::MidRight => {
                height / 2.0 - (metrics.ascent + metrics.descent) / 2.0
            }
            TextAlign::BotLeft | TextAlign::BotCenter | TextAlign::BotRight => height - metrics.descent,
        };

        let color = self.text_style.color;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(Color::from(color));

        match self.text_style.text_overflow {
            TextOverflow::Clip => {
                ctx.canvas.save();
                ctx.canvas
                    .clip_rect(skia_safe::Rect::from_xywh(0.0, 0.0, width, height), None, false);
                self.draw_runs(ctx.canvas, &runs, x, y, &paint);
                ctx.canvas.restore();
            }
            TextOverflow::Ellipsis => {
                if text_width > width {
                    let ellipsis = "...";
                    let (ellipsis_width, _) = font.measure_text(ellipsis, None);
                    let available_width = width - ellipsis_width;

                    if available_width > 0.0 {
                        let mut new_runs = Vec::new();
                        let mut current_w = 0.0;
                        let mut done = false;

                        'outer: for run in &runs {
                            let mut run_text = String::new();
                            for c in run.text.chars() {
                                let (char_w, _) = run.font.measure_text(&c.to_string(), None);
                                if current_w + char_w > available_width {
                                    done = true;
                                    break 'outer;
                                }
                                run_text.push(c);
                                current_w += char_w;
                            }
                            new_runs.push(TextRun { text: run_text, font: run.font.clone() });
                        }

                        if done || !new_runs.is_empty() {
                            new_runs.push(TextRun { text: ellipsis.to_string(), font: font.clone() });
                        }

                        let current_total_width = Self::measure_runs(&new_runs);
                        let display_x = match self.text_align {
                            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
                            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                                (width - current_total_width) / 2.0
                            }
                            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => {
                                width - current_total_width
                            }
                        };
                        self.draw_runs(ctx.canvas, &new_runs, display_x, y, &paint);
                    } else {
                        self.draw_runs(
                            ctx.canvas,
                            &[TextRun { text: ellipsis.to_string(), font: font.clone() }],
                            0.0,
                            y,
                            &paint,
                        );
                    }
                } else {
                    self.draw_runs(ctx.canvas, &runs, x, y, &paint);
                }
            }
            TextOverflow::Wrap => {
                let mut lines: Vec<Vec<TextRun>> = Vec::new();
                let mut current_line: Vec<TextRun> = Vec::new();
                let mut current_line_width = 0.0;
                let (space_width, _) = font.measure_text(" ", None);

                for word in self.text.split_whitespace() {
                    let word_runs = build_text_runs(word, &font);
                    let word_width = Self::measure_runs(&word_runs);

                    if current_line_width + word_width > width && !current_line.is_empty() {
                        lines.push(current_line);
                        current_line = Vec::new();
                        current_line_width = 0.0;
                    }

                    if !current_line.is_empty() {
                        current_line.push(TextRun { text: " ".to_string(), font: font.clone() });
                        current_line_width += space_width;
                    }

                    current_line.extend(word_runs);
                    current_line_width += word_width;
                }

                if !current_line.is_empty() {
                    lines.push(current_line);
                }

                let line_height = metrics.bottom - metrics.top;
                for (i, line) in lines.iter().enumerate() {
                    let line_width = Self::measure_runs(line);
                    let line_x = match self.text_align {
                        TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
                        TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                            (width - line_width) / 2.0
                        }
                        TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => width - line_width,
                    };
                    let line_y = y + i as f32 * line_height;
                    self.draw_runs(ctx.canvas, line, line_x, line_y, &paint);
                }
            }
            _ => {
                self.draw_runs(ctx.canvas, &runs, x, y, &paint);
            }
        }
    }
}

impl Element for RawTextWidget {
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
                let mut current_line_width = 0.0;
                let (space_width, _) = font.measure_text(" ", None);

                for word in self.text.split_whitespace() {
                    let word_runs = build_text_runs(word, &font);
                    let word_width = Self::measure_runs(&word_runs);

                    if current_line_width + word_width > width && current_line_width > 0.0 {
                        lines_count += 1;
                        current_line_width = 0.0;
                    }

                    if current_line_width > 0.0 {
                        current_line_width += space_width;
                    }
                    current_line_width += word_width;
                }
                if current_line_width > 0.0 {
                    lines_count += 1;
                }

                ResolvedSize { width, height: (lines_count as f32 * line_height).ceil() }
            }
            _ => {
                let (_, runs) = self.get_text_runs(&font);
                let text_width = Self::measure_runs(&runs);
                ResolvedSize { width: text_width.ceil(), height: line_height.ceil() }
            }
        };

        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);
        result
    }

    fn invalidate_layout(&self) {
        self.cache.invalidate();
    }
}
