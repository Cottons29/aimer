
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use std::sync::Mutex;
use aimer_style::*;
use aimer_widget::*;
use aimer_widget::base::BuildContext;

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

}

impl Drawable for RawTextWidget {
    fn draw(&self, ctx: &BuildContext) {
        #[cfg(debug_assertions)]
        {
            if inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx.canvas.get_transform_translation();
                let size = self.content_size(ctx);
                let end_x = start_x + size.width;
                let end_y = start_y + size.height;

                let scale = ctx.scale;
                let l_start = Vec2d { x: start_x / scale, y: start_y / scale };
                let l_end = Vec2d { x: end_x / scale, y: end_y / scale };

                let cp = ctx.cursor_pos;
                if cp.x >= l_start.x && cp.x <= l_end.x && cp.y >= l_start.y && cp.y <= l_end.y {
                    if let Ok(mut hovered) = inspector_overlay::HOVERED_WIDGET.write() {
                        *hovered = Some((self.debug_name(), l_start, l_end));
                    }
                }
            }
        }
        let font_size = self.font_size(ctx.scale);
        let width = ctx.parent_size.width;
        let height = ctx.parent_size.height;
        let max_width = if matches!(self.text_style.text_overflow, TextOverflow::Wrap) {
            width
        } else {
            0.0
        };
        let metrics = ctx.canvas.measure_text_metrics(&self.text, font_size, max_width);
        let text_width = metrics.width;
        let line_height = metrics.line_height;
        let ascent = metrics.ascent;
        let descent = -metrics.descent;

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
                ctx.canvas.save();
                ctx.canvas.set_clip((0.0, 0.0).into(), ResolvedSize { width, height });
                ctx.canvas.draw_text(&self.text, (x, y).into(), font_size, color);
                ctx.canvas.clear_clip();
                ctx.canvas.restore();
            }
            TextOverflow::Ellipsis => {
                if text_width > width {
                    let ellipsis = "…";
                    let ellipsis_width = ctx.canvas.measure_text(ellipsis, font_size);
                    let available_width = width - ellipsis_width;

                    if available_width > 0.0 {
                        let mut truncated = String::new();
                        let mut current_w = 0.0;

                        for cluster in unicode_segmentation::UnicodeSegmentation::graphemes(self.text.as_str(), true) {
                            let char_w = ctx.canvas.measure_text(cluster, font_size);
                            if current_w + char_w > available_width {
                                break;
                            }
                            truncated.push_str(cluster);
                            current_w += char_w;
                        }

                        truncated.push_str(ellipsis);
                        let display_width = ctx.canvas.measure_text(&truncated, font_size);
                        let display_x = match self.text_align {
                            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
                            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                                (width - display_width) / 2.0
                            }
                            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => {
                                width - display_width
                            }
                        };
                        ctx.canvas.draw_text(&truncated, (display_x, y).into(), font_size, color);
                    } else {
                        ctx.canvas.draw_text(ellipsis, (0.0, y).into(), font_size, color);
                    }
                } else {
                    ctx.canvas.draw_text(&self.text, (x, y).into(), font_size, color);
                }
            }
            TextOverflow::Wrap => {
                let mut lines: Vec<String> = Vec::new();
                let mut current_line = String::new();
                let mut current_line_width = 0.0;

                for cluster in unicode_segmentation::UnicodeSegmentation::graphemes(self.text.as_str(), true) {
                    if cluster == "\n" {
                        lines.push(current_line);
                        current_line = String::new();
                        current_line_width = 0.0;
                        continue;
                    }

                    let cluster_width = ctx.canvas.measure_text(cluster, font_size);

                    if current_line_width + cluster_width > width && !current_line.is_empty() {
                        lines.push(current_line);
                        current_line = String::new();
                        current_line_width = 0.0;
                    }

                    current_line.push_str(cluster);
                    current_line_width += cluster_width;
                }

                if !current_line.is_empty() {
                    lines.push(current_line);
                }

                for (i, line) in lines.iter().enumerate() {
                    let line_width = ctx.canvas.measure_text(line, font_size);
                    let line_x = match self.text_align {
                        TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
                        TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                            (width - line_width) / 2.0
                        }
                        TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => width - line_width,
                    };
                    let line_y = y + i as f32 * line_height;
                    ctx.canvas.draw_text(line, (line_x, line_y).into(), font_size, color);
                }
            }
            _ => {
                ctx.canvas.draw_text(&self.text, (x, y).into(), font_size, color);
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

        let font_size = self.font_size(ctx.scale);

        let result = match self.text_style.text_overflow {
            TextOverflow::Wrap => {
                let width = if ctx.box_constraint.max_width > 0.0 {
                    ctx.box_constraint.max_width
                } else {
                    ctx.parent_size.width
                };
                let metrics = ctx.canvas.measure_text_metrics(&self.text, font_size, width);

                ResolvedSize { width, height: metrics.height.ceil() }
            }
            _ => {
                let metrics = ctx.canvas.measure_text_metrics(&self.text, font_size, 0.0);
                ResolvedSize { width: metrics.width.ceil(), height: metrics.height.ceil() }
            }
        };

        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);
        result
    }
    fn invalidate_layout(&self) {
        self.cache.invalidate();
    }

    fn debug_name(&self) -> &'static str {
        "RawTextWidget"
    }
}
