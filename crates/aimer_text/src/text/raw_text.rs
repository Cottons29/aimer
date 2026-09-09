use std::cell::Cell;

#[cfg(debug_assertions)]
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_macro::{EventElement, Rebuildable};
use aimer_style::*;
use aimer_widget::base::BuildContext;
use aimer_widget::{TextOverflowMode, *};

use crate::paragraph::geometry::{DecorationPaint, prepare_decoration_paint};
use crate::paragraph::Paragraph;
use crate::text_span::ResolvedTextSpan;
use crate::text_source::TextSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlainDecorationCacheKey {
    width_bits: u32,
    height_bits: u32,
    scale_bits: u32,
    font_size_bits: u32,
    layout_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PlainDecorationLine {
    x: f32,
    width: f32,
    underline_center: f32,
    line_through_center: f32,
    overline_center: f32,
}

struct PlainDecorationCache {
    key: PlainDecorationCacheKey,
    paint: DecorationPaint,
    lines: Vec<PlainDecorationLine>,
}

struct RawTextAuxCache {
    paragraph: Option<Paragraph>,
    plain_decorations: Option<PlainDecorationCache>,
}

impl Default for RawTextAuxCache {
    fn default() -> Self {
        Self {
            paragraph: None,
            plain_decorations: None,
        }
    }
}

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
    pub _typeface: Cell<Option<()>>,
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

    fn with_paragraph<R>(&self, callback: impl FnOnce(&Paragraph) -> R) -> R {
        self.cache.with_extra(|slot: &mut Option<RawTextAuxCache>| {
            let auxiliary = slot.get_or_insert_with(RawTextAuxCache::default);
            let paragraph = auxiliary.paragraph.get_or_insert_with(|| {
                Paragraph::with_layout(
                    vec![ResolvedTextSpan::plain(self.text.to_rc(), self.text_style)],
                    self.text_align,
                    self.text_style.text_overflow,
                    self.line_height,
                    self.text_indent,
                )
            });
            callback(paragraph)
        })
    }

    fn draw_plain_decorations(
        &self,
        ctx: &BuildContext,
        width: f32,
        max_width: f32,
        height: f32,
        y: f32,
        line_height: f32,
        ascent: f32,
        descent: f32,
        font_size: f32,
        font_weight: u16,
    ) {
        let Some(paint) = prepare_decoration_paint(
            self.text_style.text_decoration,
            font_size,
            ctx.scale,
        ) else {
            return;
        };
        let key = PlainDecorationCacheKey {
            width_bits: width.to_bits(),
            height_bits: height.to_bits(),
            scale_bits: ctx.scale.to_bits(),
            font_size_bits: font_size.to_bits(),
            layout_generation: aimer_widget::layout_invalidation_generation(),
        };

        self.cache.with_extra(|slot: &mut Option<RawTextAuxCache>| {
            let auxiliary = slot.get_or_insert_with(RawTextAuxCache::default);
            let cache_miss = auxiliary
                .plain_decorations
                .as_ref()
                .is_none_or(|cached| cached.key != key || cached.paint != paint);
            if cache_miss {
                let line_widths = ctx.canvas.measure_text_line_widths_styled(
                    &self.text,
                    font_size,
                    max_width,
                    self.text_style.font_family,
                    self.text_style.font_style,
                    font_weight,
                );
                let lines = line_widths
                    .into_iter()
                    .enumerate()
                    .map(|(index, line_width)| {
                        let baseline = y + index as f32 * line_height;
                        PlainDecorationLine {
                            x: horizontal_alignment_offset(self.text_align, width, line_width),
                            width: line_width,
                            underline_center: baseline + descent.max(1.0) * 0.5 + paint.offset,
                            line_through_center: baseline - ascent * 0.35 + paint.offset,
                            overline_center: baseline - ascent + paint.offset,
                        }
                    })
                    .collect();
                auxiliary.plain_decorations = Some(PlainDecorationCache { key, paint, lines });
            }

            let cached = auxiliary
                .plain_decorations
                .as_ref()
                .expect("plain decoration cache must be populated");
            let color = cached.paint.dedicated_color.unwrap_or(self.text_style.color);
            for line in &cached.lines {
                let draw_decoration = |center_y: f32| {
                    ctx.canvas.draw_text_decoration(
                        (line.x, center_y - cached.paint.band_height / 2.0).into(),
                        ResolvedSize {
                            width: line.width,
                            height: cached.paint.band_height,
                        },
                        color,
                        cached.paint.style.id(),
                        cached.paint.thickness,
                        cached.paint.period,
                    );
                };
                if cached.paint.lines.contains(TextDecorationLine::UNDERLINE) {
                    draw_decoration(line.underline_center);
                }
                if cached.paint.lines.contains(TextDecorationLine::LINE_THROUGH) {
                    draw_decoration(line.line_through_center);
                }
                if cached.paint.lines.contains(TextDecorationLine::OVERLINE) {
                    draw_decoration(line.overline_center);
                }
            }
        });
    }

    fn draw_paragraph(&self, ctx: &BuildContext) {
        self.with_paragraph(|paragraph| {
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
            if self.text_style.text_shadow.is_some() {
                let mode = paragraph
                    .static_paint_mode()
                    .expect("plain text paragraphs cannot contain interleaved links");
                if !paragraph.draw_cached_static_paint(ctx, &layout, mode) {
                    paragraph.draw_static_spans(ctx, &layout, mode);
                }
            } else {
                paragraph.draw_spans(ctx, &layout, |span| span.style.color, |_, _| {});
            }
            if clipped {
                ctx.canvas.clear_clip();
            }
            ctx.canvas.restore();
        });
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
                {
                    inspector_overlay::set_hovered_widget((self.debug_name(), l_start, l_end));
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

        self.draw_plain_decorations(
            ctx,
            width,
            max_width,
            height,
            y,
            metrics.line_height,
            ascent,
            descent,
            font_size,
            font_weight,
        );
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        // Shadow commands stay on the direct path: their blur/offset payload is
        // intentionally not part of the safe scroll-retention contract. The
        // plain glyph/decorations path has no event geometry or asynchronous
        // state of its own, and the retaining owner still retires it when its
        // layout, scale, or other inputs change.
        self.text_style.text_shadow.is_none()
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
            return self.with_paragraph(|paragraph| paragraph.prepare(ctx).size);
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
        self.cache
            .with_extra(|slot: &mut Option<RawTextAuxCache>| {
                if let Some(auxiliary) = slot.as_mut() {
                    if let Some(paragraph) = auxiliary.paragraph.as_ref() {
                        paragraph.invalidate();
                    }
                    auxiliary.plain_decorations = None;
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use aimer_style::{LineHeight, TextAlign, TextShadow, TextStyle};
    use aimer_widget::{Drawable, LayoutCache, LayoutElement};

    use crate::text_source::TextSource;

    use super::{
        RawTextAuxCache, RawTextWidget, horizontal_alignment_offset, vertical_alignment_baseline,
    };

    #[test]
    fn advanced_text_reuses_its_paragraph_storage() {
        let widget = RawTextWidget {
            text: TextSource::Static("A long transformed paragraph"),
            text_style: TextStyle::default(),
            text_align: TextAlign::TopLeft,
            line_height: LineHeight::Px(24.0),
            text_indent: 8.0,
            cache: LayoutCache::new(),
            _typeface: Cell::new(None),
        };

        let first_ptr = widget.with_paragraph(std::ptr::from_ref);
        let second_ptr = widget.with_paragraph(std::ptr::from_ref);

        assert_eq!(first_ptr, second_ptr);
        widget.invalidate_layout();
        assert_eq!(second_ptr, widget.with_paragraph(std::ptr::from_ref));
        assert!(widget.uses_paragraph_layout());
    }

    #[test]
    fn shadowed_text_stays_on_the_dynamic_paint_path() {
        let widget = RawTextWidget {
            text: TextSource::Static("shadowed"),
            text_style: TextStyle::default().text_shadow(TextShadow::default()),
            text_align: TextAlign::TopLeft,
            line_height: LineHeight::Normal,
            text_indent: 0.0,
            cache: LayoutCache::new(),
            _typeface: Cell::new(None),
        };

        assert!(!widget.is_paint_stable());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn plain_text_decoration_cache_reuses_measured_lines() {
        use aimer_attribute::{ResolvedSize, Vec2d};
        use aimer_canvas::{Canvas, InnerCanvas};
        use aimer_style::{TextDecoration, TextDecorationLine};
        use aimer_widget::base::{BuildContext, WindowHandle};

        let inner = InnerCanvas::new();
        let canvas = Canvas::new(&inner);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let context = BuildContext::new(
            canvas,
            ResolvedSize {
                width: 200.0,
                height: 100.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 100), 1.0),
            runtime.handle().clone(),
        );
        let widget = RawTextWidget {
            text: TextSource::Static("first\nsecond"),
            text_style: TextStyle::default().text_decoration(
                TextDecoration::new().line(TextDecorationLine::UNDERLINE),
            ),
            text_align: TextAlign::TopLeft,
            line_height: LineHeight::Normal,
            text_indent: 0.0,
            cache: LayoutCache::new(),
            _typeface: Cell::new(None),
        };

        widget.draw(&context);
        let first = widget.cache.with_extra(|slot: &mut Option<RawTextAuxCache>| {
            slot.as_ref()
                .and_then(|auxiliary| auxiliary.plain_decorations.as_ref())
                .map(|cache| (cache.lines.as_ptr(), cache.lines.len()))
        });
        #[cfg(debug_assertions)]
        let first_stats = inner.text_cache_stats();

        widget.draw(&context);
        let second = widget.cache.with_extra(|slot: &mut Option<RawTextAuxCache>| {
            slot.as_ref()
                .and_then(|auxiliary| auxiliary.plain_decorations.as_ref())
                .map(|cache| (cache.lines.as_ptr(), cache.lines.len()))
        });
        #[cfg(debug_assertions)]
        let second_stats = inner.text_cache_stats();

        assert_eq!(first.map(|(_, length)| length), Some(2));
        assert_eq!(first, second);
        #[cfg(debug_assertions)]
        {
            assert_eq!(second_stats.0, first_stats.0 + 1);
            assert_eq!(second_stats.1, first_stats.1);
        }
    }

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
