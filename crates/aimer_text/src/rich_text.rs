use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use aimer_attribute::{Bounds, CacheBounds, ResolvedSize};
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_events::pointer::PointerSource;
use aimer_macro::Rebuildable;
use aimer_style::{FontStyle, TextAlign, TextDecorationLine, TextOverflow, TextStyle};
use aimer_utils::callback::{Callback, CallbackExecutor, RawInnerCallback};
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{Drawable, Element, EventElement, LayoutElement, VisitorElement, Widget};
use unicode_segmentation::UnicodeSegmentation;

use crate::selection::{SelectionState, TextHitRegion, text_offset_at};
use crate::text_span::{ResolvedTextSpan, TextSpan, ellipsize_first_line, layout_resolved_spans};

pub type LinkCallback = Callback<Rc<str>, ()>;

const DEFAULT_SELECTION_COLOR: Color = Color::Rgba(51, 153, 255, 96);

pub struct RichText {
    span: TextSpan,
    text_style: TextStyle,
    overflow: Option<TextOverflow>,
    text_align: TextAlign,
    on_link: LinkCallback,
    link_hover_color: Option<Color>,
    selectable: bool,
    selection_color: Color,
}

impl RichText {
    pub fn new(span: TextSpan) -> Self {
        Self {
            span,
            text_style: TextStyle::default(),
            overflow: None,
            text_align: TextAlign::default(),
            on_link: LinkCallback::default(),
            link_hover_color: None,
            selectable: false,
            selection_color: DEFAULT_SELECTION_COLOR,
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
        self.overflow = Some(text_overflow);
        self
    }

    fn resolved_overflow(&self) -> TextOverflow {
        self.overflow
            .unwrap_or(
                self.text_style
                    .text_overflow,
            )
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

    /// Changes linked text to `color` while the mouse pointer is over it.
    pub const fn link_hover_color(mut self, color: Color) -> Self {
        self.link_hover_color = Some(color);
        self
    }

    pub const fn selectable(mut self) -> Self {
        self.selectable = true;
        self
    }

    pub const fn selection_color(mut self, color: Color) -> Self {
        self.selection_color = color;
        self
    }
}

impl Widget for RichText {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let spans = self
            .span
            .flatten(&self.text_style);
        let plain_text: Rc<str> = spans
            .iter()
            .map(|span| {
                span.text
                    .as_ref()
            })
            .collect::<String>()
            .into();
        RawRichText {
            spans,
            plain_text,
            text_align: self.text_align,
            overflow: self.resolved_overflow(),
            on_link: self
                .on_link
                .clone(),
            link_hover_color: self.link_hover_color,
            selectable: self.selectable,
            selection_color: self.selection_color,
            window: ctx
                .window
                .clone(),
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
            hovered_link: RefCell::new(None),
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
    line_heights: Vec<f32>,
    size: ResolvedSize,
}

#[derive(Clone, Copy)]
struct PreparedSelection {
    line: usize,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

fn push_selection_run(runs: &mut Vec<PreparedSelection>, run: PreparedSelection) {
    const TOUCH_EPSILON: f32 = 0.01;

    if let Some(previous) = runs.last_mut()
        && previous.line == run.line
        && (previous.y - run.y).abs() <= TOUCH_EPSILON
        && (previous.height - run.height).abs() <= TOUCH_EPSILON
        && (previous.x + previous.width - run.x).abs() <= TOUCH_EPSILON
    {
        previous.width = run.x + run.width - previous.x;
    } else {
        runs.push(run);
    }
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
    link_hover_color: Option<Color>,
    selectable: bool,
    selection_color: Color,
    window: aimer_widget::base::WindowHandle,
    bounds: CacheBounds,
    link_regions: RefCell<Vec<LinkRegion>>,
    text_regions: RefCell<Vec<TextHitRegion>>,
    selection: RefCell<SelectionState>,
    focused: Cell<bool>,
    pressed_link: RefCell<Option<Rc<str>>>,
    hovered_link: RefCell<Option<Rc<str>>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum SelectableCursor {
    Pointer,
    Text,
    Default,
}

fn interactive_cursor_for_event(
    selectable: bool,
    over_link: bool,
    event: &ElementEvent,
) -> Option<SelectableCursor> {
    match event {
        ElementEvent::PointerDown(_, PointerSource::Mouse, _)
        | ElementEvent::PointerUp(_, PointerSource::Mouse, _)
        | ElementEvent::PointerMove(_, PointerSource::Mouse, _)
            if over_link =>
        {
            Some(SelectableCursor::Pointer)
        }
        ElementEvent::PointerDown(_, PointerSource::Mouse, _)
        | ElementEvent::PointerUp(_, PointerSource::Mouse, _)
        | ElementEvent::PointerMove(_, PointerSource::Mouse, _)
            if selectable =>
        {
            Some(SelectableCursor::Text)
        }
        ElementEvent::PointerExited(PointerSource::Mouse, _) => Some(SelectableCursor::Default),
        _ => None,
    }
}

fn display_color(
    span: &ResolvedTextSpan,
    hovered_link: Option<&Rc<str>>,
    link_hover_color: Option<Color>,
) -> Color {
    if hovered_link.is_some()
        && span
            .link
            .as_ref()
            == hovered_link
    {
        link_hover_color.unwrap_or(
            span.style
                .color,
        )
    } else {
        span.style
            .color
    }
}

impl RawRichText {
    fn available_width(&self, ctx: &BuildContext) -> f32 {
        if ctx
            .box_constraint
            .max_width
            > 0.0
            && ctx
                .box_constraint
                .max_width
                < f32::MAX
        {
            ctx.box_constraint
                .max_width
        } else {
            ctx.parent_size
                .width
        }
    }

    fn prepare_layout(&self, ctx: &BuildContext) -> PreparedLayout {
        let wrap_width = if matches!(self.overflow, TextOverflow::Wrap | TextOverflow::Ellipsis) {
            self.available_width(ctx)
        } else {
            0.0
        };
        let mut layout = layout_resolved_spans(&self.spans, wrap_width, |text, style| {
            let font_size = style
                .font_size
                .max(1) as f32
                * ctx.scale;
            ctx.canvas
                .measure_text_styled(
                    text,
                    font_size,
                    style.font_family,
                    style.font_style,
                    style
                        .font_weight
                        .numeric(),
                )
        });
        if matches!(self.overflow, TextOverflow::Ellipsis) {
            ellipsize_first_line(&mut layout, &self.spans, wrap_width, |text, style| {
                ctx.canvas
                    .measure_text_styled(
                        text,
                        style
                            .font_size
                            .max(1) as f32
                            * ctx.scale,
                        style.font_family,
                        style.font_style,
                        style
                            .font_weight
                            .numeric(),
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
                    style
                        .font_size
                        .max(1) as f32
                        * ctx.scale,
                    0.0,
                    style.font_family,
                    style.font_style,
                    style
                        .font_weight
                        .numeric(),
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
        let line_heights = (0..layout.line_count)
            .map(|line| {
                if line + 1 < layout.line_count {
                    line_top[line + 1] - line_top[line]
                } else {
                    line_ascent[line] + line_descent[line]
                }
            })
            .collect();

        PreparedLayout {
            fragments,
            backgrounds,
            line_heights,
            size: ResolvedSize { width, height },
        }
    }

    fn link_at(&self, x: f32, y: f32) -> Option<Rc<str>> {
        self.link_regions
            .borrow()
            .iter()
            .find(|region| {
                let b = region.bounds;
                b.x <= x && x <= b.x + b.width && b.y <= y && y <= b.y + b.height
            })
            .map(|region| {
                region
                    .target
                    .clone()
            })
    }

    fn set_hovered_link(&self, hovered_link: Option<Rc<str>>) {
        if *self
            .hovered_link
            .borrow()
            != hovered_link
        {
            *self
                .hovered_link
                .borrow_mut() = hovered_link;
            self.window
                .request_redraw();
        }
    }

    fn execute_link(&self, target: Rc<str>) {
        if let Some(callback) = self
            .on_link
            .get()
            .as_ref()
        {
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
        let hovered_link = match event {
            ElementEvent::PointerDown(pos, PointerSource::Mouse, _)
            | ElementEvent::PointerUp(pos, PointerSource::Mouse, _)
            | ElementEvent::PointerMove(pos, PointerSource::Mouse, _) => self.link_at(pos.x, pos.y),
            ElementEvent::PointerExited(PointerSource::Mouse, _) | ElementEvent::Cancel => None,
            _ => self
                .hovered_link
                .borrow()
                .clone(),
        };
        self.set_hovered_link(hovered_link.clone());

        match interactive_cursor_for_event(self.selectable, hovered_link.is_some(), event) {
            Some(SelectableCursor::Pointer) => self
                .window
                .set_pointer_cursor(),
            Some(SelectableCursor::Text) => self
                .window
                .set_text_cursor(),
            Some(SelectableCursor::Default) => self
                .window
                .reset_cursor(),
            None => {}
        }

        match event {
            ElementEvent::PointerDown(pos, _, pointer) => {
                let target = self.link_at(pos.x, pos.y);
                *self
                    .pressed_link
                    .borrow_mut() = target;
                if self.selectable
                    && let Some(offset) = text_offset_at(
                        &self
                            .text_regions
                            .borrow(),
                        pos.x,
                        pos.y,
                    )
                {
                    self.focused
                        .set(true);
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
                let mut selection = self
                    .selection
                    .borrow_mut();
                if !selection.is_active() {
                    return false;
                }
                if let Some(offset) = text_offset_at(
                    &self
                        .text_regions
                        .borrow(),
                    pos.x,
                    pos.y,
                ) {
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
                    let mut selection = self
                        .selection
                        .borrow_mut();
                    if selection.is_active() {
                        if let Some(offset) = text_offset_at(
                            &self
                                .text_regions
                                .borrow(),
                            pos.x,
                            pos.y,
                        ) {
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
                    && self
                        .focused
                        .get()
                    && matches!(action, KeyAction::Pressed | KeyAction::Repeat)
                    && (modifiers.ctrl || modifiers.meta) =>
            {
                match key.as_str() {
                    "a" => {
                        self.selection
                            .borrow_mut()
                            .select_all(
                                self.plain_text
                                    .len(),
                            );
                        true
                    }
                    "c" => {
                        let selection = self
                            .selection
                            .borrow()
                            .selection();
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

    fn captures_pointer(&self, pointer: u64) -> bool {
        self.selectable
            && self
                .selection
                .borrow()
                .active_pointer()
                == Some(pointer)
    }
}

impl LayoutElement for RawRichText {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.prepare_layout(ctx)
            .size
    }

    fn pos_start_end(&self) -> Option<(aimer_attribute::Vec2d, aimer_attribute::Vec2d)> {
        self.bounds
            .pos_start_end()
    }
}

impl Drawable for RawRichText {
    fn draw(&self, ctx: &BuildContext) {
        let layout = self.prepare_layout(ctx);
        let (abs_x, abs_y) = ctx
            .canvas
            .get_transform_translation();
        self.bounds
            .save(
                ctx.scale,
                abs_x,
                abs_y,
                layout
                    .size
                    .width,
                layout
                    .size
                    .height,
            );
        self.link_regions
            .borrow_mut()
            .clear();
        self.text_regions
            .borrow_mut()
            .clear();

        if matches!(self.overflow, TextOverflow::Clip | TextOverflow::Ellipsis) {
            ctx.canvas
                .save();
            ctx.canvas
                .set_clip(
                    (0.0, 0.0).into(),
                    ResolvedSize {
                        width: self.available_width(ctx),
                        height: ctx
                            .parent_size
                            .height,
                    },
                );
        }

        for background in &layout.backgrounds {
            ctx.canvas
                .fill_color_rect(
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
            let mut selection_runs = Vec::new();
            for fragment in &layout.fragments {
                let Some(source_range) = &fragment.source_range else {
                    continue;
                };
                let span = &self.spans[fragment.span_index];
                let font_size = span
                    .style
                    .font_size
                    .max(1) as f32
                    * ctx.scale;
                let mut x = fragment.x;
                let mut selected_start: Option<f32> = None;
                let mut selected_end = x;
                for (offset, grapheme) in fragment
                    .text
                    .grapheme_indices(true)
                {
                    let width = ctx
                        .canvas
                        .measure_text_styled(
                            grapheme,
                            font_size,
                            span.style
                                .font_family,
                            span.style
                                .font_style,
                            span.style
                                .font_weight
                                .numeric(),
                        );
                    let grapheme_range =
                        source_range.start + offset..source_range.start + offset + grapheme.len();
                    self.text_regions
                        .borrow_mut()
                        .push(TextHitRegion::new(
                            grapheme_range.clone(),
                            Bounds::new(
                                (abs_x + x) / ctx.scale,
                                (abs_y + fragment.baseline - fragment.ascent) / ctx.scale,
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
                    push_selection_run(
                        &mut selection_runs,
                        PreparedSelection {
                            line: fragment.line,
                            x: selected_start,
                            y: fragment.baseline - fragment.ascent,
                            width: selected_end - selected_start,
                            height: layout.line_heights[fragment.line],
                        },
                    );
                }
            }
            for run in selection_runs {
                ctx.canvas
                    .fill_color_rect(
                        (run.x, run.y).into(),
                        ResolvedSize { width: run.width, height: run.height },
                        self.selection_color,
                        [0.0; 4],
                    );
            }
        }

        for fragment in &layout.fragments {
            let span = &self.spans[fragment.span_index];
            let hovered_link = self
                .hovered_link
                .borrow();
            let color = display_color(span, hovered_link.as_ref(), self.link_hover_color);
            let font_size = span
                .style
                .font_size
                .max(1) as f32
                * ctx.scale;
            let italic = span
                .style
                .font_style
                == FontStyle::Italic
                || span
                    .style
                    .text_decoration
                    .line
                    .contains(TextDecorationLine::ITALIC);
            if italic {
                ctx.canvas
                    .set_italic(true);
            }
            ctx.canvas
                .draw_text_styled(
                    &fragment.text,
                    (fragment.x, fragment.baseline).into(),
                    font_size,
                    color,
                    span.style
                        .font_family,
                    span.style
                        .font_style,
                    span.style
                        .font_weight
                        .numeric(),
                );
            if italic {
                ctx.canvas
                    .set_italic(false);
            }

            let decoration = span
                .style
                .text_decoration;
            let lines = decoration.line;
            if !lines.is_none() {
                let color = decoration
                    .color
                    .unwrap_or(color);
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
                    ctx.canvas
                        .draw_text_decoration(
                            (fragment.x, center_y - band_height / 2.0).into(),
                            ResolvedSize { width: fragment.width, height: band_height },
                            color,
                            decoration
                                .style
                                .id(),
                            thickness,
                            period,
                        );
                };
                if lines.contains(TextDecorationLine::UNDERLINE) {
                    draw_decoration(
                        fragment.baseline
                            + fragment
                                .descent
                                .max(1.0)
                                * 0.5
                            + offset,
                    );
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
                            (abs_y + fragment.baseline - fragment.ascent) / ctx.scale,
                            fragment.width / ctx.scale,
                            fragment.height / ctx.scale,
                        ),
                    });
            }
        }

        self.set_hovered_link(
            self.link_at(
                ctx.cursor_pos
                    .x,
                ctx.cursor_pos
                    .y,
            ),
        );

        if matches!(self.overflow, TextOverflow::Clip | TextOverflow::Ellipsis) {
            ctx.canvas
                .clear_clip();
            ctx.canvas
                .restore();
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
    use aimer_widget::base::{Color, WindowHandle};

    use super::{
        DEFAULT_SELECTION_COLOR, LinkCallback, LinkRegion, PreparedFragment, RawRichText,
        SelectableCursor, interactive_cursor_for_event, prepare_background_runs,
    };
    use crate::selection::{SelectionState, TextHitRegion, TextSelection};
    use crate::text_span::{ResolvedTextSpan, layout_resolved_spans};

    fn selectable_raw_text(on_link: LinkCallback) -> RawRichText {
        RawRichText {
            spans: vec![ResolvedTextSpan::plain(Rc::from("élink"), TextStyle::default())],
            plain_text: Rc::from("élink"),
            text_align: TextAlign::TopLeft,
            overflow: TextOverflow::Clip,
            on_link,
            link_hover_color: Some(Color::Hex(0x388BFD)),
            selectable: true,
            selection_color: DEFAULT_SELECTION_COLOR,
            window: WindowHandle::headless(winit::dpi::PhysicalSize::new(100, 100), 1.0),
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
            hovered_link: RefCell::new(None),
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
    fn rich_text_selection_color_is_customizable() {
        let color = Color::Rgba(255, 0, 128, 64);
        let text = super::RichText::new(crate::TextSpan::new("selectable"))
            .selectable()
            .selection_color(color);

        assert_eq!(text.selection_color, color);
    }

    #[test]
    fn explicit_overflow_override_is_independent_of_builder_order() {
        let before_style = super::RichText::new(crate::TextSpan::new("before"))
            .text_overflow(TextOverflow::Wrap)
            .text_style(TextStyle::new().font_size(20));
        let after_style = super::RichText::new(crate::TextSpan::new("after"))
            .text_style(TextStyle::new().font_size(20))
            .text_overflow(TextOverflow::Wrap);

        assert!(matches!(before_style.resolved_overflow(), TextOverflow::Wrap));
        assert!(matches!(after_style.resolved_overflow(), TextOverflow::Wrap));
    }

    #[test]
    fn links_use_pointer_cursor_before_selectable_text_cursor() {
        let hover = ElementEvent::PointerMove(Vec2d { x: 1.0, y: 1.0 }, PointerSource::Mouse, 0);
        let exit = ElementEvent::PointerExited(PointerSource::Mouse, 0);
        let touch = ElementEvent::PointerMove(Vec2d { x: 1.0, y: 1.0 }, PointerSource::Touch, 1);

        assert_eq!(
            interactive_cursor_for_event(true, true, &hover),
            Some(SelectableCursor::Pointer)
        );
        assert_eq!(interactive_cursor_for_event(true, false, &hover), Some(SelectableCursor::Text));
        assert_eq!(
            interactive_cursor_for_event(true, false, &exit),
            Some(SelectableCursor::Default)
        );
        assert_eq!(interactive_cursor_for_event(true, true, &touch), None);
        assert_eq!(interactive_cursor_for_event(false, false, &hover), None);
    }

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

        assert_eq!(super::display_color(&linked, Some(&hovered), Some(hover_color)), hover_color);
        assert_eq!(
            super::display_color(&plain, Some(&hovered), Some(hover_color)),
            plain
                .style
                .color
        );
        assert_eq!(
            super::display_color(&plain, None, Some(hover_color)),
            plain
                .style
                .color
        );
    }

    #[test]
    fn moving_into_and_out_of_a_link_updates_hover_and_requests_redraw() {
        let text = selectable_raw_text(LinkCallback::default());

        text.on_event(&ElementEvent::PointerMove(
            Vec2d { x: 1.0, y: 5.0 },
            PointerSource::Mouse,
            0,
        ));
        assert_eq!(
            text.hovered_link
                .borrow()
                .as_deref(),
            Some("https://aimer.dev")
        );
        assert!(
            text.window
                .take_redraw_request()
        );

        text.on_event(&ElementEvent::PointerMove(
            Vec2d { x: 50.0, y: 50.0 },
            PointerSource::Mouse,
            0,
        ));
        assert!(
            text.hovered_link
                .borrow()
                .is_none()
        );
        assert!(
            text.window
                .take_redraw_request()
        );
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
        assert_eq!(
            text.selection
                .borrow()
                .selection(),
            TextSelection::new(0, 6)
        );
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

        assert_eq!(
            text.selection
                .borrow()
                .selection(),
            TextSelection::new(0, 6)
        );
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
    fn selection_highlight_starts_at_the_text_line_top() {
        use aimer_attribute::{CacheBounds, ResolvedSize};
        use aimer_canvas::{Canvas, InnerCanvas};
        use aimer_cupid::draw_cmd::DrawCommand;
        use aimer_widget::Drawable;
        use aimer_widget::base::BuildContext;

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
            runtime
                .handle()
                .clone(),
        );
        let selection_color = Color::Rgba(255, 0, 128, 64);
        let text = RawRichText {
            spans: vec![ResolvedTextSpan::plain(
                Rc::from("selected"),
                TextStyle::new().font_size(24),
            )],
            plain_text: Rc::from("selected"),
            text_align: TextAlign::TopLeft,
            overflow: TextOverflow::Wrap,
            on_link: LinkCallback::default(),
            link_hover_color: None,
            selectable: true,
            selection_color,
            window: context
                .window
                .clone(),
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
            hovered_link: RefCell::new(None),
        };
        text.selection
            .borrow_mut()
            .select_all(
                text.plain_text
                    .len(),
            );
        let fragment = text
            .prepare_layout(&context)
            .fragments
            .remove(0);

        text.draw(&context);

        let (selection_top, rendered_color) = inner
            .draw_list()
            .commands()
            .iter()
            .find_map(|command| match command {
                DrawCommand::FillRect { rect, color, .. } => Some((rect.y, *color)),
                _ => None,
            })
            .unwrap();
        let expected_color: aimer_cupid::utilities::Color = selection_color.into();
        assert_eq!(selection_top, fragment.baseline - fragment.ascent);
        assert_eq!(rendered_color.to_array(), expected_color.to_array());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selection_highlight_connects_across_adjacent_spans() {
        use aimer_attribute::{CacheBounds, ResolvedSize};
        use aimer_canvas::{Canvas, InnerCanvas};
        use aimer_cupid::draw_cmd::DrawCommand;
        use aimer_widget::Drawable;
        use aimer_widget::base::BuildContext;

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
            runtime
                .handle()
                .clone(),
        );
        let text = RawRichText {
            spans: vec![
                ResolvedTextSpan::plain(Rc::from("normal "), TextStyle::new().font_size(20)),
                ResolvedTextSpan::plain(
                    Rc::from("italic"),
                    TextStyle::new()
                        .font_size(20)
                        .font_style(aimer_style::FontStyle::Italic),
                ),
            ],
            plain_text: Rc::from("normal italic"),
            text_align: TextAlign::TopLeft,
            overflow: TextOverflow::Wrap,
            on_link: LinkCallback::default(),
            link_hover_color: None,
            selectable: true,
            selection_color: DEFAULT_SELECTION_COLOR,
            window: context
                .window
                .clone(),
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
            hovered_link: RefCell::new(None),
        };
        text.selection
            .borrow_mut()
            .select_all(
                text.plain_text
                    .len(),
            );

        text.draw(&context);

        let highlight_count = inner
            .draw_list()
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::FillRect { .. }))
            .count();
        assert_eq!(highlight_count, 1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selection_highlights_touch_between_wrapped_lines() {
        use aimer_attribute::{BoxConstraint, CacheBounds, ResolvedSize};
        use aimer_canvas::{Canvas, InnerCanvas};
        use aimer_cupid::draw_cmd::DrawCommand;
        use aimer_widget::Drawable;
        use aimer_widget::base::BuildContext;

        let inner = InnerCanvas::new();
        let canvas = Canvas::new(&inner);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut context = BuildContext::new(
            canvas,
            ResolvedSize { width: 70.0, height: 200.0 },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(winit::dpi::PhysicalSize::new(70, 200), 1.0),
            runtime
                .handle()
                .clone(),
        );
        context.box_constraint =
            BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: 70.0, max_height: 200.0 };
        let text = RawRichText {
            spans: vec![ResolvedTextSpan::plain(
                Rc::from("first second third"),
                TextStyle::new().font_size(24),
            )],
            plain_text: Rc::from("first second third"),
            text_align: TextAlign::TopLeft,
            overflow: TextOverflow::Wrap,
            on_link: LinkCallback::default(),
            link_hover_color: None,
            selectable: true,
            selection_color: DEFAULT_SELECTION_COLOR,
            window: context
                .window
                .clone(),
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
            hovered_link: RefCell::new(None),
        };
        text.selection
            .borrow_mut()
            .select_all(
                text.plain_text
                    .len(),
            );

        text.draw(&context);

        let highlights = inner
            .draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::FillRect { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(highlights.len() > 1);
        for lines in highlights.windows(2) {
            assert!((lines[0].y + lines[0].height - lines[1].y).abs() < 0.01);
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn italic_span_enables_synthetic_italic_for_its_draw() {
        use aimer_attribute::{CacheBounds, ResolvedSize};
        use aimer_canvas::{Canvas, InnerCanvas};
        use aimer_cupid::draw_cmd::DrawCommand;
        use aimer_widget::Drawable;
        use aimer_widget::base::BuildContext;

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
            runtime
                .handle()
                .clone(),
        );
        let text = RawRichText {
            spans: vec![ResolvedTextSpan::plain(
                Rc::from("italic"),
                TextStyle::new()
                    .font_size(20)
                    .font_style(aimer_style::FontStyle::Italic),
            )],
            plain_text: Rc::from("italic"),
            text_align: TextAlign::TopLeft,
            overflow: TextOverflow::Clip,
            on_link: LinkCallback::default(),
            link_hover_color: None,
            selectable: false,
            selection_color: DEFAULT_SELECTION_COLOR,
            window: context
                .window
                .clone(),
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
            hovered_link: RefCell::new(None),
        };

        text.draw(&context);

        let commands = inner.draw_list();
        let commands = commands.commands();
        let draw_index = commands
            .iter()
            .position(|command| matches!(command, DrawCommand::DrawText { .. }))
            .unwrap();
        assert!(matches!(commands[draw_index - 1], DrawCommand::SetItalic { italic: true }));
        assert!(matches!(commands[draw_index + 1], DrawCommand::SetItalic { italic: false }));
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
            Vec2d { x: 1.0, y: 5.0 },
            Vec2d::default(),
            WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 100), 1.0),
            runtime
                .handle()
                .clone(),
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
            link_hover_color: None,
            selectable: false,
            selection_color: DEFAULT_SELECTION_COLOR,
            window: context
                .window
                .clone(),
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
            hovered_link: RefCell::new(None),
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
            link_hover_color: None,
            selectable: false,
            selection_color: DEFAULT_SELECTION_COLOR,
            window: context
                .window
                .clone(),
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
            hovered_link: RefCell::new(None),
        };

        assert_eq!(
            highlighted
                .prepare_layout(&context)
                .size,
            plain
                .prepare_layout(&context)
                .size
        );
        highlighted.draw(&context);
        assert_eq!(
            highlighted
                .hovered_link
                .borrow()
                .as_deref(),
            Some("https://aimer.dev")
        );

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

        let layout = layout_resolved_spans(&spans, 20.0, |text, _| {
            text.chars()
                .count() as f32
                * 5.0
        });

        assert_eq!(layout.line_count, 2);
        assert_eq!(layout.fragments[0].line, 0);
        assert_eq!(layout.fragments[1].line, 0);
        assert_eq!(layout.fragments[1].x, 15.0);
        assert_eq!(layout.fragments[2].line, 1);
        assert_eq!(layout.fragments[2].x, 0.0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn wrapping_uses_parent_width_when_constraint_is_unbounded() {
        use aimer_attribute::{BoxConstraint, CacheBounds, ResolvedSize, Vec2d};
        use aimer_canvas::{Canvas, InnerCanvas};
        use aimer_style::{TextAlign, TextOverflow};
        use aimer_widget::base::{BuildContext, WindowHandle};

        let inner = InnerCanvas::new();
        let canvas = Canvas::new(&inner);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut context = BuildContext::new(
            canvas,
            ResolvedSize { width: 20.0, height: 100.0 },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(winit::dpi::PhysicalSize::new(20, 100), 1.0),
            runtime
                .handle()
                .clone(),
        );
        context.box_constraint = BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: f32::MAX,
            max_height: f32::MAX,
        };
        let rich_text = RawRichText {
            spans: vec![ResolvedTextSpan::plain(
                Rc::from("abcdef"),
                TextStyle::new().font_size(10),
            )],
            plain_text: Rc::from("abcdef"),
            text_align: TextAlign::TopLeft,
            overflow: TextOverflow::Wrap,
            on_link: LinkCallback::default(),
            link_hover_color: None,
            selectable: false,
            selection_color: DEFAULT_SELECTION_COLOR,
            window: context
                .window
                .clone(),
            bounds: CacheBounds::new(),
            link_regions: RefCell::new(Vec::new()),
            text_regions: RefCell::new(Vec::new()),
            selection: RefCell::new(SelectionState::default()),
            focused: Cell::new(false),
            pressed_link: RefCell::new(None),
            hovered_link: RefCell::new(None),
        };

        assert_eq!(rich_text.available_width(&context), 20.0);
        assert_eq!(
            rich_text
                .prepare_layout(&context)
                .size
                .width,
            20.0
        );
    }
}
