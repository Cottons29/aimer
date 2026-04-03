use crate::base::BuildContext;
use crate::style::text_style::TextStyle;
use crate::text::TextAlign;
use crate::{Drawable, Element, LayoutCache, TextOverflow};
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use std::sync::Mutex;

#[allow(dead_code)]
pub struct RawTextWidget {
    pub text: String,
    pub text_style: TextStyle,
    pub text_align: TextAlign,
    pub cache: LayoutCache,
    pub typeface: Mutex<Option<()>>,
}

impl RawTextWidget {
    fn font_size(&self, scale: f32) -> f32 {
        let base = if self.text_style.font_size == 0 { 14.0 } else { self.text_style.font_size as f32 };
        base * scale
    }

    /// Approximate line height as 1.2 × font_size (standard heuristic).
    fn line_height(&self, font_size: f32) -> f32 {
        font_size * 1.2
    }

    /// Approximate ascent as 0.8 × font_size.
    fn ascent(&self, font_size: f32) -> f32 {
        font_size * 0.8
    }

    /// Approximate descent as 0.2 × font_size.
    fn descent(&self, font_size: f32) -> f32 {
        font_size* 0.2
    }
}

/// this is low level TextWidget that covert to element
impl Drawable for RawTextWidget {
    fn draw(&self, ctx: &BuildContext) {
        #[cfg(debug_assertions)]
        {
            if crate::inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx.canvas.get_transform_translation();
                let size = self.content_size(ctx);
                let end_x = start_x + size.width;
                let end_y = start_y + size.height;

                let scale = ctx.scale;
                let l_start = Vec2d { x: start_x / scale, y: start_y / scale };
                let l_end = Vec2d { x: end_x / scale, y: end_y / scale };

                let cp = ctx.cursor_pos;
                if cp.x >= l_start.x && cp.x <= l_end.x && cp.y >= l_start.y && cp.y <= l_end.y {
                    if let Ok(mut hovered) = crate::inspector_overlay::HOVERED_WIDGET.write() {
                        *hovered = Some(("RawTextWidget", l_start, l_end));
                    }
                }
            }
        }
        let canvas = &ctx.canvas;
        let font_size = self.font_size(ctx.scale);
        let text_width = canvas.measure_text(&self.text, font_size);
        let line_height = self.line_height(font_size);
        let ascent = self.ascent(font_size);
        let descent = self.descent(font_size);

        let width = ctx.parent_size.width;
        let height = ctx.parent_size.height;

        let x = match self.text_align {
            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => (width - text_width) / 2.0,
            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => width - text_width,
        };

        let y = match self.text_align {
            TextAlign::TopLeft | TextAlign::TopCenter | TextAlign::TopRight => ascent,
            TextAlign::MidLeft | TextAlign::MidCenter | TextAlign::MidRight => {
                height / 2.0 + (ascent - descent) / 2.0
            }
            TextAlign::BotLeft | TextAlign::BotCenter | TextAlign::BotRight => height - descent,
        };

        let color = self.text_style.color;

        match self.text_style.text_overflow {
            TextOverflow::Clip => {
                canvas.save();
                canvas.set_clip(Vec2d { x: 0.0, y: 0.0 }, ResolvedSize { width, height });
                canvas.draw_text(&self.text, Vec2d { x, y }, font_size, color);
                canvas.clear_clip();
                canvas.restore();
            }
            TextOverflow::Ellipsis => {
                if text_width > width {
                    let ellipsis = "...";
                    let ellipsis_width = canvas.measure_text(ellipsis, font_size);
                    let available_width = width - ellipsis_width;

                    if available_width > 0.0 {
                        let mut truncated = String::new();
                        let mut current_w = 0.0;

                        for c in self.text.chars() {
                            let char_w = canvas.measure_text(&c.to_string(), font_size);
                            if current_w + char_w > available_width {
                                break;
                            }
                            truncated.push(c);
                            current_w += char_w;
                        }

                        truncated.push_str(ellipsis);
                        let display_width = canvas.measure_text(&truncated, font_size);
                        let display_x = match self.text_align {
                            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
                            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                                (width - display_width) / 2.0
                            }
                            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => {
                                width - display_width
                            }
                        };
                        canvas.draw_text(&truncated, Vec2d { x: display_x, y }, font_size, color);
                    } else {
                        canvas.draw_text(ellipsis, Vec2d { x: 0.0, y }, font_size, color);
                    }
                } else {
                    canvas.draw_text(&self.text, Vec2d { x, y }, font_size, color);
                }
            }
            TextOverflow::Wrap => {
                let space_width = canvas.measure_text(" ", font_size);
                let mut lines: Vec<String> = Vec::new();
                let mut current_line = String::new();
                let mut current_line_width = 0.0;

                for word in self.text.split_whitespace() {
                    let word_width = canvas.measure_text(word, font_size);

                    if current_line_width + word_width > width && !current_line.is_empty() {
                        lines.push(current_line);
                        current_line = String::new();
                        current_line_width = 0.0;
                    }

                    if !current_line.is_empty() {
                        current_line.push(' ');
                        current_line_width += space_width;
                    }

                    current_line.push_str(word);
                    current_line_width += word_width;
                }

                if !current_line.is_empty() {
                    lines.push(current_line);
                }

                for (i, line) in lines.iter().enumerate() {
                    let line_width = canvas.measure_text(line, font_size);
                    let line_x = match self.text_align {
                        TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
                        TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                            (width - line_width) / 2.0
                        }
                        TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => width - line_width,
                    };
                    let line_y = y + i as f32 * line_height;
                    canvas.draw_text(line, Vec2d { x: line_x, y: line_y }, font_size, color);
                }
            }
            _ => {
                canvas.draw_text(&self.text, Vec2d { x, y }, font_size, color);
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

        let canvas = &ctx.canvas;
        let font_size = self.font_size(ctx.scale);
        let line_height = self.line_height(font_size);

        let result = match self.text_style.text_overflow {
            TextOverflow::Wrap => {
                let width = if ctx.box_constraint.max_width > 0.0 {
                    ctx.box_constraint.max_width
                } else {
                    ctx.parent_size.width
                };

                let mut lines_count = 0;
                let mut current_line_width = 0.0;
                let space_width = canvas.measure_text(" ", font_size);

                for word in self.text.split_whitespace() {
                    let word_width = canvas.measure_text(word, font_size);

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

                ResolvedSize {
                    width,
                    height: (lines_count as f32 * line_height).ceil(),
                }
            }
            _ => {
                let text_width = canvas.measure_text(&self.text, font_size);
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