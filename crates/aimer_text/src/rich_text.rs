use std::cell::RefCell;
use std::rc::Rc;

use aimer_attribute::{Bounds, CacheBounds, ResolvedSize};
use aimer_events::element::ElementEvent;
use aimer_macro::Rebuildable;
use aimer_style::{FontStyle, TextAlign, TextDecorationLine, TextOverflow, TextStyle};
use aimer_utils::callback::{Callback, CallbackExecutor, RawInnerCallback};
use aimer_widget::base::BuildContext;
use aimer_widget::{Drawable, Element, EventElement, LayoutElement, VisitorElement, Widget};

use crate::text_span::{ResolvedTextSpan, TextSpan, ellipsize_first_line, layout_resolved_spans};

pub type LinkCallback = Callback<Rc<str>, ()>;

pub struct RichText {
    span: TextSpan,
    text_style: TextStyle,
    text_align: TextAlign,
    on_link: LinkCallback,
}

impl RichText {
    pub fn new(span: TextSpan) -> Self {
        Self {
            span,
            text_style: TextStyle::default(),
            text_align: TextAlign::default(),
            on_link: LinkCallback::default(),
        }
    }

    pub fn text_style(mut self, text_style: TextStyle) -> Self {
        self.text_style = text_style;
        self
    }

    pub fn text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = text_align;
        self
    }

    pub fn text_overflow(mut self, text_overflow: TextOverflow) -> Self {
        self.text_style.text_overflow = text_overflow;
        self
    }

    pub fn wrapped(self) -> Self {
        self.text_overflow(TextOverflow::Wrap)
    }

    pub fn ellipsis(self) -> Self {
        self.text_overflow(TextOverflow::Ellipsis)
    }

    pub fn on_link(mut self, on_link: impl Into<LinkCallback>) -> Self {
        self.on_link = on_link.into();
        self
    }
}

impl Widget for RichText {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        RawRichText {
            spans: self.span.flatten(&self.text_style),
            text_align: self.text_align,
            overflow: self.text_style.text_overflow,
            on_link: self.on_link.clone(),
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            pressed_link: RefCell::new(None),
        }
        .boxed()
    }
}

struct PreparedFragment {
    span_index: usize,
    text: String,
    x: f32,
    baseline: f32,
    width: f32,
    height: f32,
    ascent: f32,
    descent: f32,
}

struct PreparedLayout {
    fragments: Vec<PreparedFragment>,
    size: ResolvedSize,
}

#[derive(Clone)]
struct LinkRegion {
    target: Rc<str>,
    bounds: Bounds,
}

#[derive(Rebuildable)]
pub struct RawRichText {
    spans: Vec<ResolvedTextSpan>,
    text_align: TextAlign,
    overflow: TextOverflow,
    on_link: LinkCallback,
    bounds: CacheBounds,
    link_regions: RefCell<Vec<LinkRegion>>,
    pressed_link: RefCell<Option<Rc<str>>>,
}

impl RawRichText {
    fn available_width(&self, ctx: &BuildContext) -> f32 {
        if ctx.box_constraint.max_width > 0.0 {
            ctx.box_constraint.max_width
        } else {
            ctx.parent_size.width
        }
    }

    fn prepare_layout(&self, ctx: &BuildContext) -> PreparedLayout {
        let wrap_width = if matches!(self.overflow, TextOverflow::Wrap | TextOverflow::Ellipsis) {
            self.available_width(ctx)
        } else {
            0.0
        };
        let mut layout = layout_resolved_spans(&self.spans, wrap_width, |text, style| {
            let font_size = style.font_size.max(1) as f32 * ctx.scale;
            ctx.canvas
                .measure_text(text, font_size)
        });
        if matches!(self.overflow, TextOverflow::Ellipsis) {
            ellipsize_first_line(&mut layout, &self.spans, wrap_width, |text, style| {
                ctx.canvas
                    .measure_text(text, style.font_size.max(1) as f32 * ctx.scale)
            });
        }

        let mut line_ascent = vec![0.0_f32; layout.line_count];
        let mut line_descent = vec![0.0_f32; layout.line_count];
        let mut line_gap = vec![0.0_f32; layout.line_count];
        let mut line_width = vec![0.0_f32; layout.line_count];
        for fragment in &layout.fragments {
            let style = self.spans[fragment.span_index].style;
            let metrics = ctx.canvas.measure_text_metrics(
                &fragment.text,
                style.font_size.max(1) as f32 * ctx.scale,
                0.0,
            );
            line_ascent[fragment.line] = line_ascent[fragment.line].max(metrics.ascent);
            line_descent[fragment.line] = line_descent[fragment.line].max(-metrics.descent);
            line_gap[fragment.line] = line_gap[fragment.line].max(metrics.line_gap);
            line_width[fragment.line] = line_width[fragment.line].max(fragment.x + fragment.width);
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
        let natural_width = line_width
            .iter()
            .copied()
            .fold(0.0, f32::max);
        let width =
            if matches!(self.overflow, TextOverflow::Wrap) { wrap_width } else { natural_width };

        let fragments = layout
            .fragments
            .into_iter()
            .map(|fragment| {
                let line_offset = match self.text_align {
                    TextAlign::TopCenter | TextAlign::MidCenter | TextAlign::BotCenter => {
                        (width - line_width[fragment.line]) / 2.0
                    }
                    TextAlign::TopRight | TextAlign::MidRight | TextAlign::BotRight => {
                        width - line_width[fragment.line]
                    }
                    _ => 0.0,
                };
                PreparedFragment {
                    span_index: fragment.span_index,
                    text: fragment.text,
                    x: fragment.x + line_offset,
                    baseline: line_top[fragment.line] + line_ascent[fragment.line],
                    width: fragment.width,
                    height: line_ascent[fragment.line] + line_descent[fragment.line],
                    ascent: line_ascent[fragment.line],
                    descent: line_descent[fragment.line],
                }
            })
            .collect();

        PreparedLayout { fragments, size: ResolvedSize { width, height } }
    }

    fn link_at(&self, x: f32, y: f32) -> Option<Rc<str>> {
        self.link_regions
            .borrow()
            .iter()
            .find(|region| {
                let b = region.bounds;
                b.x <= x && x <= b.x + b.width && b.y <= y && y <= b.y + b.height
            })
            .map(|region| region.target.clone())
    }

    fn execute_link(&self, target: Rc<str>) {
        if let Some(callback) = self.on_link.get().as_ref() {
            match callback {
                RawInnerCallback::Empty => {}
                RawInnerCallback::Sync(function) => function(target),
                RawInnerCallback::Async(function) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        handle.spawn(function(target));
                    }
                    #[cfg(target_arch = "wasm32")]
                    wasm_bindgen_futures::spawn_local(function(target));
                }
            }
        }
    }
}

impl VisitorElement for RawRichText {
    fn debug_name(&self) -> &'static str {
        "RawRichText"
    }
}

impl EventElement for RawRichText {
    fn on_event(&self, event: &ElementEvent) -> bool {
        match event {
            ElementEvent::PointerDown(pos, _, _) => {
                let target = self.link_at(pos.x, pos.y);
                let consumed = target.is_some();
                *self.pressed_link.borrow_mut() = target;
                consumed
            }
            ElementEvent::PointerUp(pos, _, _) => {
                let pressed = self
                    .pressed_link
                    .borrow_mut()
                    .take();
                let released = self.link_at(pos.x, pos.y);
                if let (Some(pressed), Some(released)) = (pressed, released)
                    && pressed == released
                {
                    self.execute_link(released);
                    return true;
                }
                false
            }
            ElementEvent::PointerExited(_, _) | ElementEvent::Cancel => {
                self.pressed_link
                    .borrow_mut()
                    .take();
                false
            }
            _ => false,
        }
    }
}

impl LayoutElement for RawRichText {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.prepare_layout(ctx).size
    }

    fn pos_start_end(&self) -> Option<(aimer_attribute::Vec2d, aimer_attribute::Vec2d)> {
        self.bounds.pos_start_end()
    }
}

impl Drawable for RawRichText {
    fn draw(&self, ctx: &BuildContext) {
        let layout = self.prepare_layout(ctx);
        let (abs_x, abs_y) = ctx
            .canvas
            .get_transform_translation();
        self.bounds
            .save(ctx.scale, abs_x, abs_y, layout.size.width, layout.size.height);
        self.link_regions
            .borrow_mut()
            .clear();

        if matches!(self.overflow, TextOverflow::Clip | TextOverflow::Ellipsis) {
            ctx.canvas.save();
            ctx.canvas.set_clip(
                (0.0, 0.0).into(),
                ResolvedSize { width: self.available_width(ctx), height: ctx.parent_size.height },
            );
        }

        for fragment in &layout.fragments {
            let span = &self.spans[fragment.span_index];
            let font_size = span.style.font_size.max(1) as f32 * ctx.scale;
            let italic = !matches!(span.style.font_style, FontStyle::Normal)
                || span
                    .style
                    .text_decoration
                    .line
                    .contains(TextDecorationLine::ITALIC);
            if italic {
                ctx.canvas.set_italic(true);
            }
            ctx.canvas.draw_text(
                &fragment.text,
                (fragment.x, fragment.baseline).into(),
                font_size,
                span.style.color,
                span.style.font_weight.numeric(),
            );
            if italic {
                ctx.canvas.set_italic(false);
            }

            let decoration = span.style.text_decoration;
            let lines = decoration.line;
            if !lines.is_none() {
                let color = decoration
                    .color
                    .unwrap_or(span.style.color);
                let thickness = decoration
                    .thickness
                    .map(|value| value * ctx.scale)
                    .unwrap_or((font_size * 0.06).max(1.0));
                let offset = decoration.offset * ctx.scale;
                let (band_height, period) = match decoration.style {
                    aimer_style::TextDecorationStyle::Double => (thickness * 3.0, 1.0),
                    aimer_style::TextDecorationStyle::Dotted => {
                        (thickness, (thickness * 2.0).max(2.0))
                    }
                    aimer_style::TextDecorationStyle::Dashed => {
                        (thickness, (thickness * 4.0).max(2.0))
                    }
                    aimer_style::TextDecorationStyle::Wavy => {
                        (thickness * 4.0, (thickness * 6.0).max(4.0))
                    }
                    aimer_style::TextDecorationStyle::Solid => (thickness, 1.0),
                };
                let draw_decoration = |center_y: f32| {
                    ctx.canvas.draw_text_decoration(
                        (fragment.x, center_y - band_height / 2.0).into(),
                        ResolvedSize { width: fragment.width, height: band_height },
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

            if let Some(target) = &span.link {
                self.link_regions
                    .borrow_mut()
                    .push(LinkRegion {
                        target: target.clone(),
                        bounds: Bounds::new(
                            (abs_x + fragment.x) / ctx.scale,
                            (abs_y + fragment.baseline - fragment.height) / ctx.scale,
                            fragment.width / ctx.scale,
                            fragment.height / ctx.scale,
                        ),
                    });
            }
        }

        if matches!(self.overflow, TextOverflow::Clip | TextOverflow::Ellipsis) {
            ctx.canvas.clear_clip();
            ctx.canvas.restore();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use aimer_style::TextStyle;

    use crate::text_span::{ResolvedTextSpan, layout_resolved_spans};

    #[test]
    fn wrapping_uses_one_cursor_across_span_boundaries() {
        let style = TextStyle::new().font_size(10);
        let spans = vec![
            ResolvedTextSpan::plain(Rc::from("abc"), style),
            ResolvedTextSpan::plain(Rc::from("def"), style),
        ];

        let layout =
            layout_resolved_spans(&spans, 20.0, |text, _| text.chars().count() as f32 * 5.0);

        assert_eq!(layout.line_count, 2);
        assert_eq!(layout.fragments[0].line, 0);
        assert_eq!(layout.fragments[1].line, 0);
        assert_eq!(layout.fragments[1].x, 15.0);
        assert_eq!(layout.fragments[2].line, 1);
        assert_eq!(layout.fragments[2].x, 0.0);
    }
}
