use std::rc::Rc;
use std::sync::Mutex;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_macro::{EventElement, Rebuildable};
use aimer_style::*;
use aimer_widget::base::BuildContext;
use aimer_widget::{TextOverflowMode, *};

#[derive(Rebuildable, EventElement)]
pub struct RawTextWidget {
    pub text: Rc<str>,
    pub text_style: TextStyle,
    pub text_align: TextAlign,
    pub cache: LayoutCache,
    pub _typeface: Mutex<Option<()>>,
}

impl RawTextWidget {
    fn font_size(&self, scale: f32) -> f32 {
        let base =
            if self.text_style.font_size == 0 { 14.0 } else { self.text_style.font_size as f32 };
        base * scale
    }
}

impl Drawable for RawTextWidget {
    fn draw(&self, ctx: &BuildContext) {
        // println!("Drawing text widget : {:?}", self.text);
        #[cfg(debug_assertions)]
        {
            if inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx
                    .canvas
                    .get_transform_translation();
                let size = self.content_size(ctx);
                let end_x = start_x + size.width;
                let end_y = start_y + size.height;

                let scale = ctx.scale;
                let l_start = Vec2d { x: start_x / scale, y: start_y / scale };
                let l_end = Vec2d { x: end_x / scale, y: end_y / scale };
                let cp = ctx.cursor_pos;
                if cp.x >= l_start.x
                    && cp.x <= l_end.x
                    && cp.y >= l_start.y
                    && cp.y <= l_end.y
                    && let Ok(mut hovered) = inspector_overlay::HOVERED_WIDGET.write()
                {
                    *hovered = Some((self.debug_name(), l_start, l_end));
                }
            }
        }
        let font_size = self.font_size(ctx.scale);
        let width = ctx.parent_size.width;
        let height = ctx.parent_size.height;
        let max_width =
            if matches!(self.text_style.text_overflow, TextOverflow::Wrap) { width } else { 0.0 };
        let metrics = ctx
            .canvas
            .measure_text_metrics(&self.text, font_size, max_width);
        let text_width = metrics.width;
        let ascent = metrics.ascent;
        let descent = -metrics.descent;

        let x = match self.text_align {
            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                (width - text_width) / 2.0
            }
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
        let font_weight = self
            .text_style
            .font_weight
            .numeric();

        // Synthetic italic is carried on the decoration line set; enable it on the
        // canvas so the glyphs are sheared, then reset it after the text is drawn.
        let is_italic = self
            .text_style
            .text_decoration
            .line
            .contains(TextDecorationLine::ITALIC);
        if is_italic {
            ctx.canvas.set_italic(true);
        }

        match self.text_style.text_overflow {
            TextOverflow::Clip => {
                ctx.canvas.save();
                let width = ctx.parent_size.width;
                ctx.canvas
                    .set_clip((0.0, 0.0).into(), ResolvedSize { width, height });
                ctx.canvas.draw_text_wrapped(
                    &self.text,
                    (x, y).into(),
                    font_size,
                    color,
                    width,
                    font_weight,
                );
                ctx.canvas.clear_clip();
                ctx.canvas.restore();
            }
            TextOverflow::Ellipsis => {
                ctx.canvas.draw_text_with_overflow(
                    &self.text,
                    (x, y).into(),
                    font_size,
                    color,
                    width,
                    height,
                    TextOverflowMode::Ellipsis,
                    font_weight,
                );
            }
            TextOverflow::Wrap => {
                ctx.canvas.draw_text_wrapped(
                    &self.text,
                    (x, y).into(),
                    font_size,
                    color,
                    width,
                    font_weight,
                );
            }
            _ => {
                ctx.canvas
                    .draw_text(&self.text, (x, y).into(), font_size, color, font_weight);
            }
        }

        if is_italic {
            ctx.canvas.set_italic(false);
        }

        let decoration = self.text_style.text_decoration;
        if !decoration.line.is_none() {
            let scale = ctx.scale;
            // Dedicated decoration color, else inherit the text color.
            let deco_color = decoration.color.unwrap_or(color);
            // User thickness/offset are logical px; scale them like the font.
            let thickness = decoration
                .thickness
                .map(|t| t * scale)
                .unwrap_or((font_size * 0.06).max(1.0));
            let offset = decoration.offset * scale;
            let style_id = decoration.style.id();

            // The band must be tall enough to hold the styled stroke: wavy needs
            // room for its amplitude, double needs room for two strokes + gap.
            let (band_height, period) = match decoration.style {
                TextDecorationStyle::Double => (thickness * 3.0, 1.0),
                TextDecorationStyle::Dotted => (thickness, (thickness * 2.0).max(2.0)),
                TextDecorationStyle::Dashed => (thickness, (thickness * 4.0).max(2.0)),
                TextDecorationStyle::Wavy => (thickness * 4.0, (thickness * 6.0).max(4.0)),
                TextDecorationStyle::Solid => (thickness, 1.0),
            };

            let emit = |center_y: f32| {
                let band_top = center_y - band_height / 2.0;
                ctx.canvas.draw_text_decoration(
                    (x, band_top).into(),
                    ResolvedSize { width: text_width, height: band_height },
                    deco_color,
                    style_id,
                    thickness,
                    period,
                );
            };

            if decoration
                .line
                .contains(TextDecorationLine::UNDERLINE)
            {
                emit(y + descent.max(1.0) * 0.5 + offset);
            }
            if decoration
                .line
                .contains(TextDecorationLine::LINE_THROUGH)
            {
                emit(y - ascent * 0.35 + offset);
            }
            if decoration
                .line
                .contains(TextDecorationLine::OVERLINE)
            {
                emit(y - ascent + offset);
            }
        }
    }
}

impl VisitorElement for RawTextWidget {
    fn debug_name(&self) -> &'static str {
        "RawTextWidget"
    }
}

impl LayoutElement for RawTextWidget {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self
            .cache
            .get_computed(ctx.box_constraint, scale_bits)
        {
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
                let metrics = ctx
                    .canvas
                    .measure_text_metrics(&self.text, font_size, width);

                ResolvedSize { width, height: metrics.height.ceil() }
            }
            _ => {
                let metrics = ctx
                    .canvas
                    .measure_text_metrics(&self.text, font_size, 0.0);
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
}
