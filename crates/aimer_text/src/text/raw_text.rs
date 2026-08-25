use std::sync::Mutex;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_macro::{EventElement, Rebuildable};
use aimer_style::*;
use aimer_widget::base::BuildContext;
use aimer_widget::{TextOverflowMode, *};

use crate::paragraph::Paragraph;
use crate::text_span::ResolvedTextSpan;
use crate::text_source::TextSource;

/// Low-level element that lays out and paints one run of text.
///
/// [`crate::Text`] is the usual constructor. Direct construction requires
/// callers to provide a fresh [`LayoutCache`] and typeface slot in addition to
/// the text, style, and alignment.
#[derive(Rebuildable, EventElement)]
pub struct RawTextWidget {
    pub text: TextSource,
    pub text_style: TextStyle,
    pub text_align: TextAlign,
    pub line_height: LineHeight,
    pub text_indent: f32,
    pub cache: LayoutCache,
    pub _typeface: Mutex<Option<()>>,
}

impl RawTextWidget {
    pub(crate) fn font_size(&self, scale: f32) -> f32 {
        let base = if self.text_style.font_size == 0 {
            14.0
        } else {
            self.text_style.font_size as f32
        };
        base * scale
    }

    fn uses_paragraph_layout(&self) -> bool {
        self.line_height != LineHeight::Normal
            || self.text_indent != 0.0
            || self.text_style.text_transform != TextTransform::None
            || self.text_style.letter_spacing != 0.0
            || self.text_style.word_spacing != 0.0
            || self.text_style.text_shadow.is_some()
    }

    fn paragraph(&self) -> Paragraph {
        Paragraph::with_layout(
            vec![ResolvedTextSpan::plain(self.text.to_rc(), self.text_style)],
            self.text_align,
            self.text_style.text_overflow,
            self.line_height,
            self.text_indent,
        )
    }

    fn draw_paragraph(&self, ctx: &BuildContext) {
        let paragraph = self.paragraph();
        let layout = paragraph.prepare(ctx);
        let vertical_offset = match self.text_align {
            TextAlign::TopLeft | TextAlign::TopCenter | TextAlign::TopRight => 0.0,
            TextAlign::MidLeft | TextAlign::MidCenter | TextAlign::MidRight => {
                (ctx.parent_size.height - layout.size.height).max(0.0) / 2.0
            }
            TextAlign::BotLeft | TextAlign::BotCenter | TextAlign::BotRight => {
                (ctx.parent_size.height - layout.size.height).max(0.0)
            }
        };

        ctx.canvas.save();
        if vertical_offset != 0.0 {
            ctx.canvas.translate((0.0, vertical_offset).into());
        }
        let clipped = paragraph.needs_clip();
        if clipped {
            ctx.canvas.set_clip(
                (0.0, 0.0).into(),
                ResolvedSize {
                    width: paragraph.available_width(ctx),
                    height: ctx.parent_size.height,
                },
            );
        }
        paragraph.draw_spans(ctx, &layout, |span| span.style.color, |_, _| {});
        if clipped {
            ctx.canvas.clear_clip();
        }
        ctx.canvas.restore();
    }
}

impl Drawable for RawTextWidget {
    fn draw(&self, ctx: &BuildContext) {
        // println!("Drawing text widget : {:?}", self.text);
        #[cfg(debug_assertions)]
        {
            if inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx.canvas.get_transform_translation();
                let size = self.content_size(ctx);
                let end_x = start_x + size.width;
                let end_y = start_y + size.height;

                let scale = ctx.scale;
                let l_start = Vec2d {
                    x: start_x / scale,
                    y: start_y / scale,
                };
                let l_end = Vec2d {
                    x: end_x / scale,
                    y: end_y / scale,
                };
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
        if self.uses_paragraph_layout() {
            self.draw_paragraph(ctx);
            return;
        }
        let font_size = self.font_size(ctx.scale);
        let width = ctx.parent_size.width;
        let height = ctx.parent_size.height;
        let max_width = if matches!(
            self.text_style.text_overflow,
            TextOverflow::Clip | TextOverflow::Wrap
        ) {
            width
        } else {
            0.0
        };
        let metrics = ctx.canvas.measure_text_metrics_styled(
            &self.text,
            font_size,
            max_width,
            self.text_style.font_family,
            self.text_style.font_style,
            self.text_style.font_weight.numeric(),
        );
        let ascent = metrics.ascent;
        let descent = -metrics.descent;
        let x = 0.0;
        let y = vertical_alignment_baseline(self.text_align, height, metrics.height, ascent);
        let horizontal_align = match self.text_align {
            TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => {
                TextHorizontalAlign::Left
            }
            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                TextHorizontalAlign::Center
            }
            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => {
                TextHorizontalAlign::Right
            }
        };

        let color = self.text_style.color;
        let font_weight = self.text_style.font_weight.numeric();

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
                ctx.canvas.draw_text_aligned_with_overflow_styled(
                    &self.text,
                    (x, y).into(),
                    font_size,
                    color,
                    width,
                    height,
                    TextOverflowMode::Wrap,
                    horizontal_align,
                    self.text_style.font_family,
                    self.text_style.font_style,
                    font_weight,
                );
                ctx.canvas.clear_clip();
                ctx.canvas.restore();
            }
            TextOverflow::Ellipsis => {
                ctx.canvas.draw_text_aligned_with_overflow_styled(
                    &self.text,
                    (x, y).into(),
                    font_size,
                    color,
                    width,
                    height,
                    TextOverflowMode::Ellipsis,
                    horizontal_align,
                    self.text_style.font_family,
                    self.text_style.font_style,
                    font_weight,
                );
            }
            TextOverflow::Wrap => {
                ctx.canvas.draw_text_aligned_with_overflow_styled(
                    &self.text,
                    (x, y).into(),
                    font_size,
                    color,
                    width,
                    height,
                    TextOverflowMode::Wrap,
                    horizontal_align,
                    self.text_style.font_family,
                    self.text_style.font_style,
                    font_weight,
                );
            }
            _ => {
                ctx.canvas.draw_text_aligned_with_overflow_styled(
                    &self.text,
                    (x, y).into(),
                    font_size,
                    color,
                    width,
                    height,
                    TextOverflowMode::Clip,
                    horizontal_align,
                    self.text_style.font_family,
                    self.text_style.font_style,
                    font_weight,
                );
            }
        }

        if is_italic {
            ctx.canvas.set_italic(false);
        }

        let decoration = self.text_style.text_decoration;
        if !decoration.line.is_none() {
            let line_widths = ctx.canvas.measure_text_line_widths_styled(
                &self.text,
                font_size,
                max_width,
                self.text_style.font_family,
                self.text_style.font_style,
                font_weight,
            );
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

            let emit = |center_y: f32, line_width: f32| {
                let band_top = center_y - band_height / 2.0;
                let line_x = horizontal_alignment_offset(self.text_align, width, line_width);
                ctx.canvas.draw_text_decoration(
                    (line_x, band_top).into(),
                    ResolvedSize {
                        width: line_width,
                        height: band_height,
                    },
                    deco_color,
                    style_id,
                    thickness,
                    period,
                );
            };

            for (index, line_width) in line_widths.into_iter().enumerate() {
                let baseline = y + index as f32 * metrics.line_height;
                if decoration.line.contains(TextDecorationLine::UNDERLINE) {
                    emit(baseline + descent.max(1.0) * 0.5 + offset, line_width);
                }
                if decoration.line.contains(TextDecorationLine::LINE_THROUGH) {
                    emit(baseline - ascent * 0.35 + offset, line_width);
                }
                if decoration.line.contains(TextDecorationLine::OVERLINE) {
                    emit(baseline - ascent + offset, line_width);
                }
            }
        }
    }
}

impl VisitorElement for RawTextWidget {
    fn debug_name(&self) -> &'static str {
        "RawTextWidget"
    }
}

fn horizontal_alignment_offset(alignment: TextAlign, width: f32, text_width: f32) -> f32 {
    let remaining_width = (width - text_width).max(0.0);
    match alignment {
        TextAlign::TopLeft | TextAlign::MidLeft | TextAlign::BotLeft => 0.0,
        TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => remaining_width / 2.0,
        TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => remaining_width,
    }
}

fn vertical_alignment_baseline(
    alignment: TextAlign,
    height: f32,
    text_height: f32,
    ascent: f32,
) -> f32 {
    match alignment {
        TextAlign::TopLeft | TextAlign::TopCenter | TextAlign::TopRight => ascent,
        TextAlign::MidLeft | TextAlign::MidCenter | TextAlign::MidRight => {
            (height - text_height).max(0.0) / 2.0 + ascent
        }
        TextAlign::BotLeft | TextAlign::BotCenter | TextAlign::BotRight => {
            (height - text_height).max(0.0) + ascent
        }
    }
}

impl LayoutElement for RawTextWidget {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        if self.uses_paragraph_layout() {
            return self.paragraph().prepare(ctx).size;
        }
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
                let metrics = ctx.canvas.measure_text_metrics_styled(
                    &self.text,
                    font_size,
                    width,
                    self.text_style.font_family,
                    self.text_style.font_style,
                    self.text_style.font_weight.numeric(),
                );

                ResolvedSize {
                    width,
                    height: metrics.height.ceil(),
                }
            }
            _ => {
                let metrics = ctx.canvas.measure_text_metrics_styled(
                    &self.text,
                    font_size,
                    0.0,
                    self.text_style.font_family,
                    self.text_style.font_style,
                    self.text_style.font_weight.numeric(),
                );
                ResolvedSize {
                    width: metrics.width.ceil(),
                    height: metrics.height.ceil(),
                }
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

#[cfg(test)]
mod tests {
    use aimer_style::TextAlign;

    use super::{horizontal_alignment_offset, vertical_alignment_baseline};

    #[test]
    fn top_center_does_not_place_oversized_text_before_the_left_edge() {
        assert_eq!(
            horizontal_alignment_offset(TextAlign::TopCenter, 200.0, 260.0),
            0.0
        );
    }

    #[test]
    fn mid_center_centers_the_entire_multiline_block() {
        assert_eq!(
            vertical_alignment_baseline(TextAlign::MidCenter, 200.0, 60.0, 16.0),
            86.0
        );
    }

    #[test]
    fn bottom_alignment_places_the_entire_multiline_block_at_the_bottom() {
        assert_eq!(
            vertical_alignment_baseline(TextAlign::BotCenter, 200.0, 60.0, 16.0),
            156.0
        );
    }
}
