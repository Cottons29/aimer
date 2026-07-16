use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use aimer_attribute::{Bounds, CacheBounds, ResolvedSize};
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_macro::Rebuildable;
use aimer_style::{TextAlign, TextDecorationLine, TextOverflow, TextStyle};
use aimer_utils::callback::{Callback, CallbackExecutor, RawInnerCallback};
use aimer_widget::base::BuildContext;
use aimer_widget::{Drawable, Element, EventElement, LayoutElement, VisitorElement, Widget};
use unicode_segmentation::UnicodeSegmentation;

use crate::selection::{SelectionState, TextHitRegion, text_offset_at};
use crate::text_span::{ResolvedTextSpan, TextSpan, ellipsize_first_line, layout_resolved_spans};

pub type LinkCallback = Callback<Rc<str>, ()>;

pub struct RichText {
    span: TextSpan,
    text_style: TextStyle,
    text_align: TextAlign,
    on_link: LinkCallback,
    selectable: bool,
}

impl RichText {
    pub fn new(span: TextSpan) -> Self {
        Self {
            span,
            text_style: TextStyle::default(),
            text_align: TextAlign::default(),
            on_link: LinkCallback::default(),
            selectable: false,
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

    pub const fn selectable(mut self) -> Self {
        self.selectable = true;
        self
    }
}

impl Widget for RichText {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        let spans = self.span.flatten(&self.text_style);
        let plain_text: Rc<str> = spans
            .iter()
            .map(|span| span.text.as_ref())
            .collect::<String>()
            .into();
        RawRichText {
            spans,
            plain_text,
            text_align: self.text_align,
            overflow: self.text_style.text_overflow,
            on_link: self.on_link.clone(),
            selectable: self.selectable,
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
        }
        .boxed()
    }
}

struct PreparedFragment {
    span_index: usize,
    text: String,
    source_range: Option<Range<usize>>,
    line: usize,
    x: f32,
    baseline: f32,
    width: f32,
    height: f32,
    ascent: f32,
    descent: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PreparedBackground {
    line: usize,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: aimer_widget::base::Color,
}

struct PreparedLayout {
    fragments: Vec<PreparedFragment>,
    backgrounds: Vec<PreparedBackground>,
    size: ResolvedSize,
}

fn prepare_background_runs(
    fragments: &[PreparedFragment],
    spans: &[ResolvedTextSpan],
) -> Vec<PreparedBackground> {
    const TOUCH_EPSILON: f32 = 0.01;

    let mut runs: Vec<PreparedBackground> = Vec::new();
    for fragment in fragments {
        let Some(color) = spans[fragment.span_index]
            .style
            .background_color
        else {
            continue;
        };
        if color.as_u32() >> 24 == 0 || fragment.width <= 0.0 || fragment.height <= 0.0 {
            continue;
        }

        let y = fragment.baseline - fragment.ascent;
        if let Some(previous) = runs.last_mut()
            && previous.line == fragment.line
            && previous.color == color
            && (previous.y - y).abs() <= TOUCH_EPSILON
            && (previous.height - fragment.height).abs() <= TOUCH_EPSILON
            && (previous.x + previous.width - fragment.x).abs() <= TOUCH_EPSILON
        {
            previous.width = fragment.x + fragment.width - previous.x;
            continue;
        }

        runs.push(PreparedBackground {
            line: fragment.line,
            x: fragment.x,
            y,
            width: fragment.width,
            height: fragment.height,
            color,
        });
    }
    runs
}

#[derive(Clone)]
struct LinkRegion {
    target: Rc<str>,
    bounds: Bounds,
}

#[derive(Rebuildable)]
pub struct RawRichText {
    spans: Vec<ResolvedTextSpan>,
    plain_text: Rc<str>,
    text_align: TextAlign,
    overflow: TextOverflow,
    on_link: LinkCallback,
    selectable: bool,
    bounds: CacheBounds,
    link_regions: RefCell<Vec<LinkRegion>>,
    text_regions: RefCell<Vec<TextHitRegion>>,
    selection: RefCell<SelectionState>,
    focused: Cell<bool>,
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
            let metrics = ctx
                .canvas
                .measure_text_metrics_styled(
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
        let backgrounds = prepare_background_runs(&fragments, &self.spans);

        PreparedLayout { fragments, backgrounds, size: ResolvedSize { width, height } }
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
    fn captures_pointer(&self, pointer: u64) -> bool {
        self.selectable
            && self
                .selection
                .borrow()
                .active_pointer()
                == Some(pointer)
    }

    fn on_event(&self, event: &ElementEvent) -> bool {
        match event {
            ElementEvent::PointerDown(pos, _, pointer) => {
                let target = self.link_at(pos.x, pos.y);
                *self.pressed_link.borrow_mut() = target;
                if self.selectable
                    && let Some(offset) = text_offset_at(&self.text_regions.borrow(), pos.x, pos.y)
                {
                    self.focused.set(true);
                    self.selection
                        .borrow_mut()
                        .begin(offset, *pointer);
                    return true;
                }
                self.pressed_link
                    .borrow()
                    .is_some()
            }
            ElementEvent::PointerMove(pos, _, pointer) if self.selectable => {
                let mut selection = self.selection.borrow_mut();
                if !selection.is_active() {
                    return false;
                }
                if let Some(offset) = text_offset_at(&self.text_regions.borrow(), pos.x, pos.y) {
                    selection.update(offset, *pointer);
                    if selection.was_dragged() {
                        self.pressed_link
                            .borrow_mut()
                            .take();
                    }
                }
                true
            }
            ElementEvent::PointerUp(pos, _, pointer) => {
                let dragged = if self.selectable {
                    let mut selection = self.selection.borrow_mut();
                    if selection.is_active() {
                        if let Some(offset) =
                            text_offset_at(&self.text_regions.borrow(), pos.x, pos.y)
                        {
                            selection.update(offset, *pointer);
                        }
                        let dragged = selection.was_dragged();
                        selection.end(*pointer);
                        dragged
                    } else {
                        false
                    }
                } else {
                    false
                };
                if dragged {
                    self.pressed_link
                        .borrow_mut()
                        .take();
                    return true;
                }
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
                if matches!(event, ElementEvent::Cancel) {
                    self.selection
                        .borrow_mut()
                        .cancel();
                }
                false
            }
            ElementEvent::KeyInput { key: NamedKey::Other(key), action, modifiers }
                if self.selectable
                    && self.focused.get()
                    && matches!(action, KeyAction::Pressed | KeyAction::Repeat)
                    && (modifiers.ctrl || modifiers.meta) =>
            {
                match key.as_str() {
                    "a" => {
                        self.selection
                            .borrow_mut()
                            .select_all(self.plain_text.len());
                        true
                    }
                    "c" => {
                        let selection = self.selection.borrow().selection();
                        let Some(text) = selection.selected_text(&self.plain_text) else {
                            return false;
                        };
                        if text.is_empty() {
                            return false;
                        }
                        let _ = aimer_widget::clipboard::set_text(text);
                        true
                    }
                    _ => false,
                }
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
        self.text_regions
            .borrow_mut()
            .clear();

        if matches!(self.overflow, TextOverflow::Clip | TextOverflow::Ellipsis) {
            ctx.canvas.save();
            ctx.canvas.set_clip(
                (0.0, 0.0).into(),
                ResolvedSize { width: self.available_width(ctx), height: ctx.parent_size.height },
            );
        }

        for background in &layout.backgrounds {
            ctx.canvas.fill_color_rect(
                (background.x, background.y).into(),
                ResolvedSize { width: background.width, height: background.height },
                background.color,
                [0.0; 4],
            );
        }

        if self.selectable {
            let selection = self
                .selection
                .borrow()
                .selection()
                .range();
            for fragment in &layout.fragments {
                let Some(source_range) = &fragment.source_range else {
                    continue;
                };
                let span = &self.spans[fragment.span_index];
                let font_size = span.style.font_size.max(1) as f32 * ctx.scale;
                let mut x = fragment.x;
                let mut selected_start: Option<f32> = None;
                let mut selected_end = x;
                for (offset, grapheme) in fragment
                    .text
                    .grapheme_indices(true)
                {
                    let width = ctx.canvas.measure_text_styled(
                        grapheme,
                        font_size,
                        span.style.font_family,
                        span.style.font_style,
                        span.style.font_weight.numeric(),
                    );
                    let grapheme_range =
                        source_range.start + offset..source_range.start + offset + grapheme.len();
                    self.text_regions
                        .borrow_mut()
                        .push(TextHitRegion::new(
                            grapheme_range.clone(),
                            Bounds::new(
                                (abs_x + x) / ctx.scale,
                                (abs_y + fragment.baseline - fragment.height) / ctx.scale,
                                width / ctx.scale,
                                fragment.height / ctx.scale,
                            ),
                        ));
                    if grapheme_range.start < selection.end && selection.start < grapheme_range.end
                    {
                        selected_start.get_or_insert(x);
                        selected_end = x + width;
                    }
                    x += width;
                }
                if let Some(selected_start) = selected_start {
                    ctx.canvas.fill_color_rect(
                        (selected_start, fragment.baseline - fragment.height).into(),
                        ResolvedSize {
                            width: selected_end - selected_start,
                            height: fragment.height,
                        },
                        aimer_widget::base::Color::Rgba(51, 153, 255, 96),
                        [0.0; 4],
                    );
                }
            }
        }

        for fragment in &layout.fragments {
            let span = &self.spans[fragment.span_index];
            let font_size = span.style.font_size.max(1) as f32 * ctx.scale;
            let italic = span
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
                span.style.color,
                span.style.font_family,
                span.style.font_style,
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
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use aimer_attribute::{Bounds, CacheBounds, Vec2d};
    use aimer_events::element::{ElementEvent, KeyAction, Modifiers, NamedKey};
    use aimer_events::pointer::PointerSource;
    use aimer_style::{TextAlign, TextOverflow, TextStyle};
    use aimer_widget::EventElement;

    use super::{LinkCallback, LinkRegion, PreparedFragment, RawRichText, prepare_background_runs};
    use crate::selection::{SelectionState, TextHitRegion, TextSelection};
    use crate::text_span::{ResolvedTextSpan, layout_resolved_spans};

    fn selectable_raw_text(on_link: LinkCallback) -> RawRichText {
        RawRichText {
            spans: vec![ResolvedTextSpan::plain(Rc::from("élink"), TextStyle::default())],
            plain_text: Rc::from("élink"),
            text_align: TextAlign::TopLeft,
            overflow: TextOverflow::Clip,
            on_link,
            selectable: true,
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(vec![LinkRegion {
                target: Rc::from("https://aimer.dev"),
                bounds: Bounds::new(0.0, 0.0, 20.0, 10.0),
            }]),
            text_regions: RefCell::new(vec![TextHitRegion::new(
                0..6,
                Bounds::new(0.0, 0.0, 20.0, 10.0),
            )]),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
        }
    }

    #[test]
    fn rich_text_selection_is_opt_in() {
        let plain = super::RichText::new(crate::TextSpan::new("plain"));
        let selectable = super::RichText::new(crate::TextSpan::new("selectable")).selectable();

        assert!(!plain.selectable);
        assert!(selectable.selectable);
    }

    #[test]
    fn select_all_shortcut_selects_the_visible_text_after_focus() {
        let text = selectable_raw_text(LinkCallback::default());
        text.on_event(&ElementEvent::PointerDown(
            Vec2d { x: 1.0, y: 5.0 },
            PointerSource::Mouse,
            0,
        ));

        let handled = text.on_event(&ElementEvent::KeyInput {
            key: NamedKey::Other("a".into()),
            action: KeyAction::Pressed,
            modifiers: Modifiers { ctrl: true, ..Modifiers::default() },
        });

        assert!(handled);
        assert_eq!(text.selection.borrow().selection(), TextSelection::new(0, 6));
    }

    #[test]
    fn dragging_a_link_selects_text_without_activating_the_link() {
        let activations = Rc::new(Cell::new(0));
        let text = selectable_raw_text(LinkCallback::from({
            let activations = activations.clone();
            move |_| activations.set(activations.get() + 1)
        }));

        text.on_event(&ElementEvent::PointerDown(
            Vec2d { x: 1.0, y: 5.0 },
            PointerSource::Mouse,
            0,
        ));
        text.on_event(&ElementEvent::PointerMove(
            Vec2d { x: 19.0, y: 5.0 },
            PointerSource::Mouse,
            0,
        ));
        text.on_event(&ElementEvent::PointerUp(Vec2d { x: 19.0, y: 5.0 }, PointerSource::Mouse, 0));

        assert_eq!(text.selection.borrow().selection(), TextSelection::new(0, 6));
        assert_eq!(activations.get(), 0);
    }

    #[test]
    fn backgrounds_merge_on_one_line_but_not_across_lines_or_colors() {
        let spans = vec![
            ResolvedTextSpan::plain(
                Rc::from("ab"),
                TextStyle::new().background_color(aimer_widget::base::Color::RED),
            ),
            ResolvedTextSpan::plain(
                Rc::from("c"),
                TextStyle::new().background_color(aimer_widget::base::Color::RED),
            ),
            ResolvedTextSpan::plain(
                Rc::from("d"),
                TextStyle::new().background_color(aimer_widget::base::Color::BLUE),
            ),
        ];
        let fragments = vec![
            PreparedFragment {
                span_index: 0,
                text: "ab".into(),
                source_range: None,
                line: 0,
                x: 10.0,
                baseline: 18.0,
                width: 20.0,
                height: 12.0,
                ascent: 8.0,
                descent: 4.0,
            },
            PreparedFragment {
                span_index: 1,
                text: "c".into(),
                source_range: None,
                line: 0,
                x: 30.0,
                baseline: 18.0,
                width: 10.0,
                height: 12.0,
                ascent: 8.0,
                descent: 4.0,
            },
            PreparedFragment {
                span_index: 2,
                text: "d".into(),
                source_range: None,
                line: 0,
                x: 40.0,
                baseline: 18.0,
                width: 10.0,
                height: 12.0,
                ascent: 8.0,
                descent: 4.0,
            },
            PreparedFragment {
                span_index: 0,
                text: "a".into(),
                source_range: None,
                line: 1,
                x: 0.0,
                baseline: 34.0,
                width: 10.0,
                height: 16.0,
                ascent: 12.0,
                descent: 4.0,
            },
        ];

        let runs = prepare_background_runs(&fragments, &spans);

        assert_eq!(runs.len(), 3);
        assert_eq!((runs[0].x, runs[0].y, runs[0].width, runs[0].height), (10.0, 10.0, 30.0, 12.0));
        assert_eq!(runs[0].color, aimer_widget::base::Color::RED);
        assert_eq!((runs[1].x, runs[1].width), (40.0, 10.0));
        assert_eq!(runs[1].color, aimer_widget::base::Color::BLUE);
        assert_eq!((runs[2].x, runs[2].y, runs[2].height), (0.0, 22.0, 16.0));
    }

    #[test]
    fn transparent_backgrounds_do_not_create_runs() {
        let spans = vec![ResolvedTextSpan::plain(
            Rc::from("hidden"),
            TextStyle::new().background_color(aimer_widget::base::Color::Transparent),
        )];
        let fragments = vec![PreparedFragment {
            span_index: 0,
            text: "hidden".into(),
            source_range: None,
            line: 0,
            x: 0.0,
            baseline: 10.0,
            width: 30.0,
            height: 10.0,
            ascent: 8.0,
            descent: 2.0,
        }];

        assert!(prepare_background_runs(&fragments, &spans).is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn backgrounds_draw_before_text_without_changing_size_or_link_regions() {
        use std::cell::{Cell, RefCell};

        use aimer_attribute::{CacheBounds, ResolvedSize, Vec2d};
        use aimer_canvas::{Canvas, InnerCanvas};
        use aimer_cupid::draw_cmd::DrawCommand;
        use aimer_style::{TextAlign, TextOverflow};
        use aimer_widget::Drawable;
        use aimer_widget::base::{BuildContext, WindowHandle};

        let inner = InnerCanvas::new();
        let canvas = Canvas::new(&inner);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let context = BuildContext::new(
            canvas,
            ResolvedSize { width: 200.0, height: 100.0 },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 100), 1.0),
            runtime.handle().clone(),
        );
        let highlighted_span = ResolvedTextSpan {
            text: Rc::from("linked"),
            style: TextStyle::new().background_color(aimer_widget::base::Color::RED),
            link: Some(Rc::from("https://aimer.dev")),
        };
        let highlighted = RawRichText {
            spans: vec![highlighted_span.clone()],
            plain_text: Rc::from("linked"),
            text_align: TextAlign::TopLeft,
            overflow: TextOverflow::Clip,
            on_link: LinkCallback::default(),
            selectable: false,
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
        };
        let plain = RawRichText {
            spans: vec![ResolvedTextSpan {
                style: TextStyle { background_color: None, ..highlighted_span.style },
                ..highlighted_span
            }],
            plain_text: Rc::from("linked"),
            text_align: TextAlign::TopLeft,
            overflow: TextOverflow::Clip,
            on_link: LinkCallback::default(),
            selectable: false,
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
        };

        assert_eq!(
            highlighted
                .prepare_layout(&context)
                .size,
            plain.prepare_layout(&context).size
        );
        highlighted.draw(&context);

        let commands = inner.draw_list();
        let background_index = commands
            .commands()
            .iter()
            .position(|command| matches!(command, DrawCommand::FillRect { .. }))
            .unwrap();
        let text_index = commands
            .commands()
            .iter()
            .position(|command| matches!(command, DrawCommand::DrawText { .. }))
            .unwrap();
        assert!(background_index < text_index);
        assert_eq!(
            highlighted
                .link_regions
                .borrow()
                .len(),
            1
        );
    }

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
