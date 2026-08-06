pub(crate) mod geometry;

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use aimer_attribute::ResolvedSize;
use aimer_style::{FontStyle, TextAlign, TextDecorationLine, TextOverflow};
use aimer_widget::base::{BuildContext, Color};
use unicode_segmentation::UnicodeSegmentation;

use crate::paragraph::geometry::{
    PreparedBackground, prepare_background_runs, vertical_span_is_visible,
};
use crate::text_span::{ResolvedTextSpan, ellipsize_first_line, layout_resolved_spans};

/// One painted run of text: a maximal slice of a single span that shares a
/// line, in element-local physical pixels.
pub(crate) struct PreparedFragment {
    pub span_index: usize,
    pub text: String,
    pub source_range: Option<Range<usize>>,
    pub line: usize,
    pub x: f32,
    pub baseline: f32,
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
    pub descent: f32,
}

/// Cached geometry of one grapheme cluster, in element-local physical pixels.
///
/// Measuring graphemes is the expensive part of hit testing and of painting a
/// selection, so it happens once per layout instead of once per frame.
pub(crate) struct GraphemeBox {
    pub source_range: Range<usize>,
    pub fragment_index: usize,
    pub x: f32,
    pub width: f32,
}

/// A hard line break, which owns no glyphs but is still selectable and
/// hit-testable up to the end of its line.
pub(crate) struct PreparedLineBreak {
    pub source_range: Range<usize>,
    pub line: usize,
    pub x: f32,
    pub y: f32,
    pub hit_width: f32,
    pub selection_width: f32,
    pub height: f32,
}

/// A complete, immutable paragraph layout for one width and one device pixel
/// ratio.
pub(crate) struct PreparedLayout {
    pub fragments: Vec<PreparedFragment>,
    pub graphemes: Vec<GraphemeBox>,
    pub backgrounds: Vec<PreparedBackground>,
    pub line_breaks: Vec<PreparedLineBreak>,
    pub line_heights: Vec<f32>,
    pub size: ResolvedSize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct PreparedLayoutKey {
    width_bits: u32,
    scale_bits: u32,
}

/// Resolves the color a span is painted with, letting a hovered link override
/// its own color only.
pub(crate) fn display_color(
    span: &ResolvedTextSpan,
    hovered_link: Option<&Rc<str>>,
    link_hover_color: Option<Color>,
) -> Color {
    if hovered_link.is_some() && span.link.as_ref() == hovered_link {
        link_hover_color.unwrap_or(span.style.color)
    } else {
        span.style.color
    }
}

/// A laid-out run of resolved spans with a one-entry layout cache.
///
/// The paragraph owns everything that does not depend on interaction: the
/// spans, their alignment, their overflow mode and the cached
/// [`PreparedLayout`]. Elements add selection, links and hover on top.
///
/// # Examples
///
/// ```ignore
/// let paragraph = Paragraph::new(spans, TextAlign::TopLeft, TextOverflow::Wrap);
/// let layout = paragraph.prepare(ctx);
/// paragraph.draw_backgrounds(ctx, &layout);
/// paragraph.draw_spans(ctx, &layout, |span| span.style.color, |_, _| {});
/// ```
pub(crate) struct Paragraph {
    spans: Vec<ResolvedTextSpan>,
    text_align: TextAlign,
    overflow: TextOverflow,
    layout_cache: RefCell<Option<(PreparedLayoutKey, Rc<PreparedLayout>)>>,
}

impl Paragraph {
    /// Creates a paragraph over already resolved spans.
    #[inline]
    pub fn new(spans: Vec<ResolvedTextSpan>, text_align: TextAlign, overflow: TextOverflow) -> Self {
        Self {
            spans,
            text_align,
            overflow,
            layout_cache: RefCell::new(None),
        }
    }

    /// Reports whether the paragraph paints outside its own width and therefore
    /// needs a clip.
    #[inline]
    pub const fn needs_clip(&self) -> bool {
        matches!(self.overflow, TextOverflow::Clip | TextOverflow::Ellipsis)
    }

    /// The width lines are laid out against, preferring an explicit constraint
    /// over the parent's size.
    pub fn available_width(&self, ctx: &BuildContext) -> f32 {
        if ctx.box_constraint.max_width > 0.0 && ctx.box_constraint.max_width < f32::MAX {
            ctx.box_constraint.max_width
        } else {
            ctx.parent_size.width
        }
    }

    /// Drops the cached layout so the next [`Paragraph::prepare`] recomputes it.
    #[inline]
    pub fn invalidate(&self) {
        self.layout_cache.borrow_mut().take();
    }

    /// Returns the layout for the current width and scale, computing it only
    /// when the cached one no longer applies.
    pub fn prepare(&self, ctx: &BuildContext) -> Rc<PreparedLayout> {
        let width = self.wrap_width(ctx);
        let key = PreparedLayoutKey {
            width_bits: width.to_bits(),
            scale_bits: ctx.scale.to_bits(),
        };
        if let Some((cached_key, layout)) = self.layout_cache.borrow().as_ref()
            && *cached_key == key
        {
            return Rc::clone(layout);
        }

        let layout = Rc::new(self.compute_layout(ctx));
        *self.layout_cache.borrow_mut() = Some((key, Rc::clone(&layout)));
        layout
    }

    fn wrap_width(&self, ctx: &BuildContext) -> f32 {
        if matches!(self.overflow, TextOverflow::Wrap | TextOverflow::Ellipsis) {
            self.available_width(ctx)
        } else {
            0.0
        }
    }

    fn compute_layout(&self, ctx: &BuildContext) -> PreparedLayout {
        let wrap_width = self.wrap_width(ctx);
        let mut layout = layout_resolved_spans(&self.spans, wrap_width, |text, style| {
            let font_size = style.font_size.max(1) as f32 * ctx.scale;
            ctx.canvas.measure_text_styled(
                text,
                font_size,
                style.font_family,
                style.font_style,
                style.font_weight.numeric(),
            )
        });
        if matches!(self.overflow, TextOverflow::Ellipsis) {
            ellipsize_first_line(&mut layout, &self.spans, wrap_width, |text, style| {
                ctx.canvas.measure_text_styled(
                    text,
                    style.font_size.max(1) as f32 * ctx.scale,
                    style.font_family,
                    style.font_style,
                    style.font_weight.numeric(),
                )
            });
        }

        let mut line_ascent = vec![0.0_f32; layout.line_count];
        let mut line_descent = vec![0.0_f32; layout.line_count];
        let mut line_gap = vec![0.0_f32; layout.line_count];
        let mut line_width = vec![0.0_f32; layout.line_count];
        for fragment in &layout.fragments {
            let style = self.spans[fragment.span_index].style;
            let metrics = ctx.canvas.measure_text_metrics_styled(
                &fragment.text,
                style.font_size.max(1) as f32 * ctx.scale,
                0.0,
                style.font_family,
                style.font_style,
                style.font_weight.numeric(),
            );
            line_ascent[fragment.line] = line_ascent[fragment.line].max(metrics.ascent);
            line_descent[fragment.line] = line_descent[fragment.line].max(-metrics.descent);
            line_gap[fragment.line] = line_gap[fragment.line].max(metrics.line_gap);
            line_width[fragment.line] = line_width[fragment.line].max(fragment.x + fragment.width);
        }
        for line_break in &layout.line_breaks {
            let style = self.spans[line_break.span_index].style;
            let metrics = ctx.canvas.measure_text_metrics_styled(
                " ",
                style.font_size.max(1) as f32 * ctx.scale,
                0.0,
                style.font_family,
                style.font_style,
                style.font_weight.numeric(),
            );
            for line in line_break.line..=(line_break.line + 1).min(layout.line_count - 1) {
                line_ascent[line] = line_ascent[line].max(metrics.ascent);
                line_descent[line] = line_descent[line].max(-metrics.descent);
                line_gap[line] = line_gap[line].max(metrics.line_gap);
            }
        }

        let mut line_top = vec![0.0; layout.line_count];
        for line in 1..layout.line_count {
            line_top[line] = line_top[line - 1]
                + line_ascent[line - 1]
                + line_descent[line - 1]
                + line_gap[line - 1];
        }
        let height = layout
            .line_count
            .checked_sub(1)
            .map(|last| line_top[last] + line_ascent[last] + line_descent[last])
            .unwrap_or(0.0);
        let natural_width = line_width.iter().copied().fold(0.0, f32::max);
        let width = if matches!(self.overflow, TextOverflow::Wrap) {
            wrap_width
        } else {
            natural_width
        };

        let fragments = layout
            .fragments
            .into_iter()
            .map(|fragment| {
                let line_offset = self.line_offset(width, line_width[fragment.line]);
                PreparedFragment {
                    span_index: fragment.span_index,
                    text: fragment.text,
                    source_range: fragment.source_range,
                    line: fragment.line,
                    x: fragment.x + line_offset,
                    baseline: line_top[fragment.line] + line_ascent[fragment.line],
                    width: fragment.width,
                    height: line_ascent[fragment.line] + line_descent[fragment.line],
                    ascent: line_ascent[fragment.line],
                    descent: line_descent[fragment.line],
                }
            })
            .collect::<Vec<_>>();
        let graphemes = self.measure_graphemes(ctx, &fragments);
        let backgrounds = prepare_background_runs(&fragments, &self.spans);
        let line_heights = (0..layout.line_count)
            .map(|line| {
                if line + 1 < layout.line_count {
                    line_top[line + 1] - line_top[line]
                } else {
                    line_ascent[line] + line_descent[line]
                }
            })
            .collect::<Vec<_>>();
        let line_breaks = layout
            .line_breaks
            .into_iter()
            .map(|line_break| {
                let line_offset = self.line_offset(width, line_width[line_break.line]);
                let x = line_width[line_break.line] + line_offset;
                PreparedLineBreak {
                    source_range: line_break.source_range,
                    line: line_break.line,
                    x,
                    y: line_top[line_break.line],
                    hit_width: (width - x).max(ctx.scale),
                    selection_width: ctx.scale,
                    height: line_heights[line_break.line],
                }
            })
            .collect();

        PreparedLayout {
            fragments,
            graphemes,
            backgrounds,
            line_breaks,
            line_heights,
            size: ResolvedSize { width, height },
        }
    }

    fn line_offset(&self, width: f32, line_width: f32) -> f32 {
        match self.text_align {
            TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                (width - line_width) / 2.0
            }
            TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => width - line_width,
            _ => 0.0,
        }
    }

    fn measure_graphemes(
        &self,
        ctx: &BuildContext,
        fragments: &[PreparedFragment],
    ) -> Vec<GraphemeBox> {
        let mut graphemes = Vec::new();
        for (fragment_index, fragment) in fragments.iter().enumerate() {
            let Some(source_range) = &fragment.source_range else {
                continue;
            };
            let style = self.spans[fragment.span_index].style;
            let font_size = style.font_size.max(1) as f32 * ctx.scale;
            let mut x = fragment.x;
            for (offset, grapheme) in fragment.text.grapheme_indices(true) {
                let width = ctx.canvas.measure_text_styled(
                    grapheme,
                    font_size,
                    style.font_family,
                    style.font_style,
                    style.font_weight.numeric(),
                );
                graphemes.push(GraphemeBox {
                    source_range: source_range.start + offset
                        ..source_range.start + offset + grapheme.len(),
                    fragment_index,
                    x,
                    width,
                });
                x += width;
            }
        }
        graphemes
    }

    /// Paints the inline backgrounds that lie inside the visible rectangle.
    pub fn draw_backgrounds(&self, ctx: &BuildContext, layout: &PreparedLayout) {
        for background in &layout.backgrounds {
            if !vertical_span_is_visible(background.y, background.height, ctx.visible_rect) {
                continue;
            }
            ctx.canvas.fill_color_rect(
                (background.x, background.y).into(),
                ResolvedSize {
                    width: background.width,
                    height: background.height,
                },
                background.color,
                [0.0; 4],
            );
        }
    }

    /// Paints every visible fragment with its decorations.
    ///
    /// `color_for` resolves the paint color of a span, letting the caller apply
    /// interaction state such as a hovered link. `visit` observes each painted
    /// fragment, which callers use to collect link regions.
    pub fn draw_spans(
        &self,
        ctx: &BuildContext,
        layout: &PreparedLayout,
        color_for: impl Fn(&ResolvedTextSpan) -> Color,
        mut visit: impl FnMut(&ResolvedTextSpan, &PreparedFragment),
    ) {
        for fragment in &layout.fragments {
            if !vertical_span_is_visible(
                fragment.baseline - fragment.ascent,
                fragment.height,
                ctx.visible_rect,
            ) {
                continue;
            }
            let span = &self.spans[fragment.span_index];
            let color = color_for(span);
            let font_size = span.style.font_size.max(1) as f32 * ctx.scale;
            let italic = span.style.font_style == FontStyle::Italic
                || span
                    .style
                    .text_decoration
                    .line
                    .contains(TextDecorationLine::ITALIC);
            if italic {
                ctx.canvas.set_italic(true);
            }
            ctx.canvas.draw_text_styled(
                &fragment.text,
                (fragment.x, fragment.baseline).into(),
                font_size,
                color,
                span.style.font_family,
                span.style.font_style,
                span.style.font_weight.numeric(),
            );
            if italic {
                ctx.canvas.set_italic(false);
            }

            self.draw_decorations(ctx, fragment, span, color, font_size);
            visit(span, fragment);
        }
    }

    fn draw_decorations(
        &self,
        ctx: &BuildContext,
        fragment: &PreparedFragment,
        span: &ResolvedTextSpan,
        color: Color,
        font_size: f32,
    ) {
        let decoration = span.style.text_decoration;
        let lines = decoration.line;
        if lines.is_none() {
            return;
        }
        let color = decoration.color.unwrap_or(color);
        let thickness = decoration
            .thickness
            .map(|value| value * ctx.scale)
            .unwrap_or((font_size * 0.06).max(1.0));
        let offset = decoration.offset * ctx.scale;
        let (band_height, period) = match decoration.style {
            aimer_style::TextDecorationStyle::Double => (thickness * 3.0, 1.0),
            aimer_style::TextDecorationStyle::Dotted => (thickness, (thickness * 2.0).max(2.0)),
            aimer_style::TextDecorationStyle::Dashed => (thickness, (thickness * 4.0).max(2.0)),
            aimer_style::TextDecorationStyle::Wavy => (thickness * 4.0, (thickness * 6.0).max(4.0)),
            aimer_style::TextDecorationStyle::Solid => (thickness, 1.0),
        };
        let draw_decoration = |center_y: f32| {
            ctx.canvas.draw_text_decoration(
                (fragment.x, center_y - band_height / 2.0).into(),
                ResolvedSize {
                    width: fragment.width,
                    height: band_height,
                },
                color,
                decoration.style.id(),
                thickness,
                period,
            );
        };
        if lines.contains(TextDecorationLine::UNDERLINE) {
            draw_decoration(fragment.baseline + fragment.descent.max(1.0) * 0.5 + offset);
        }
        if lines.contains(TextDecorationLine::LINE_THROUGH) {
            draw_decoration(fragment.baseline - fragment.ascent * 0.35 + offset);
        }
        if lines.contains(TextDecorationLine::OVERLINE) {
            draw_decoration(fragment.baseline - fragment.ascent + offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use aimer_style::TextStyle;
    use aimer_widget::base::Color;

    use super::display_color;
    use crate::text_span::ResolvedTextSpan;

    #[test]
    fn hovered_link_uses_the_configured_color_only_for_its_spans() {
        let hovered = Rc::<str>::from("https://aimer.dev");
        let hover_color = Color::Hex(0x388BFD);
        let linked = ResolvedTextSpan {
            text: Rc::from("Aimer"),
            style: TextStyle::new().color(Color::Hex(0x0969DA)),
            link: Some(hovered.clone()),
        };
        let plain = ResolvedTextSpan::plain(Rc::from(" docs"), TextStyle::default());

        assert_eq!(
            display_color(&linked, Some(&hovered), Some(hover_color)),
            hover_color
        );
        assert_eq!(
            display_color(&plain, Some(&hovered), Some(hover_color)),
            plain.style.color
        );
        assert_eq!(
            display_color(&plain, None, Some(hover_color)),
            plain.style.color
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cached_grapheme_boxes_reproduce_the_measured_widths() {
        use aimer_attribute::{ResolvedSize, Vec2d};
        use aimer_canvas::{Canvas, InnerCanvas};
        use aimer_style::{TextAlign, TextOverflow};
        use aimer_widget::base::{BuildContext, WindowHandle};

        use super::Paragraph;

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
        let paragraph = Paragraph::new(
            vec![ResolvedTextSpan::plain(
                Rc::from("héllo"),
                TextStyle::new().font_size(20),
            )],
            TextAlign::TopLeft,
            TextOverflow::Clip,
        );

        let layout = paragraph.prepare(&context);

        let fragment = &layout.fragments[0];
        assert_eq!(layout.graphemes.len(), 5);
        assert_eq!(layout.graphemes[0].source_range, 0..1);
        assert_eq!(layout.graphemes[1].source_range, 1..3);
        assert_eq!(layout.graphemes[0].x, fragment.x);
        for boxes in layout.graphemes.windows(2) {
            assert!((boxes[0].x + boxes[0].width - boxes[1].x).abs() < 0.01);
        }
        let measured = layout
            .graphemes
            .iter()
            .map(|grapheme| grapheme.width)
            .sum::<f32>();
        assert!((measured - fragment.width).abs() < 1.0);
    }
}
