use aimer_attribute::{BoxConstraint, ResolvedSize, Vec2d};
use aimer_color::prelude::Color;

use crate::base::BuildContext;
use crate::{Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement, Widget};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OverflowEdges {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl OverflowEdges {
    pub fn has_overflow(self) -> bool {
        self.left > 0.0 || self.top > 0.0 || self.right > 0.0 || self.bottom > 0.0
    }

    #[cfg(debug_assertions)]
    fn maximum(self) -> f32 {
        self.left
            .max(self.top)
            .max(self.right)
            .max(self.bottom)
    }
}

pub fn detect_overflow(child: ResolvedSize, bounds: ResolvedSize, offset: Vec2d) -> OverflowEdges {
    OverflowEdges {
        left: (-offset.x).max(0.0),
        top: (-offset.y).max(0.0),
        right: (offset.x + child.width - bounds.width).max(0.0),
        bottom: (offset.y + child.height - bounds.height).max(0.0),
    }
}

#[derive(Clone, Debug)]
/// A diagnostic widget that fills its available bounds with an error message.
///
/// Debug builds display the supplied message. Release builds log that message
/// and render a generic description so internal diagnostic details are not
/// exposed to users.
pub struct ErrorWidget {
    message: String,
}

impl ErrorWidget {
    /// Creates an error diagnostic with the message used for logging and debug
    /// rendering.
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl Widget for ErrorWidget {
    fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
        #[cfg(not(debug_assertions))]
        aimer_utils::log::error(&self.message);
        Box::new(ErrorElement { message: self.message.clone() })
    }

    fn debug_name(&self) -> &'static str {
        "ErrorWidget"
    }
}

#[derive(Clone, Debug)]
pub struct ErrorElement {
    message: String,
}

impl ErrorElement {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    pub fn draw_message(ctx: &BuildContext, message: &str) {
        let size = diagnostic_bounds(ctx);
        ctx.canvas
            .fill_color_rect(Vec2d::default(), size, Color::RED, [0.0; 4]);

        #[cfg(debug_assertions)]
        let text = message;
        #[cfg(not(debug_assertions))]
        let text = "A layout error occurred";
        #[cfg(not(debug_assertions))]
        let _ = message;


        let (pos_y, font_size) = if cfg!(target_os = "ios") || cfg!(target_os = "android") {
            (400f32, 40.0)
        }else {
            (200f32, 34f32)
        };

        ctx.canvas
            .draw_text_wrapped(
                text,
                Vec2d { x: 24.0, y: pos_y },
                font_size,
                Color::YELLOW,
                (size.width - 24.0).max(0.0),
                600,
            );
    }
}

impl Drawable for ErrorElement {
    fn draw(&self, ctx: &BuildContext) {
        Self::draw_message(ctx, &self.message);
    }
}

impl EventElement for ErrorElement {}
impl Rebuildable for ErrorElement {}

impl VisitorElement for ErrorElement {
    fn debug_name(&self) -> &'static str {
        "ErrorWidget"
    }
}

impl LayoutElement for ErrorElement {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        diagnostic_bounds(ctx)
    }
}

/// Wraps a child and paints a debug overflow warning around constrained edges.
///
/// The child is clipped to the available bounds by default. In debug builds,
/// overflowing edges receive striped markers and a label; release builds keep
/// only the optional clipping behavior.
pub struct OverflowIndicator<W> {
    child: W,
    label: Option<String>,
    clip: bool,
}

impl<W: Widget + 'static> OverflowIndicator<W> {
    /// Creates an indicator for `child`, with clipping enabled and a label
    /// derived from [`Widget::debug_name`].
    pub fn new(child: W) -> Self {
        Self { child, label: None, clip: true }
    }

    /// Overrides the label displayed in the debug overflow message.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Controls whether child drawing is clipped to the constrained bounds.
    ///
    /// The default is `true`. Overflow detection and debug markers are still
    /// computed when clipping is disabled.
    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }
}

impl<W: Widget + 'static> Widget for OverflowIndicator<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self
            .child
            .to_element(ctx);
        let label = self
            .label
            .clone()
            .unwrap_or_else(|| {
                self.child
                    .debug_name()
                    .to_string()
            });
        Box::new(RawOverflowIndicator { child, label, clip: self.clip })
    }

    fn debug_name(&self) -> &'static str {
        "OverflowIndicator"
    }
}

struct RawOverflowIndicator {
    child: Box<dyn Element>,
    label: String,
    clip: bool,
}

impl Drawable for RawOverflowIndicator {
    fn draw(&self, ctx: &BuildContext) {
        let bounds = self.computed_size(ctx);
        let child_size = self
            .child
            .computed_size(ctx);
        let overflow = detect_overflow(child_size, bounds, Vec2d::default());

        ctx.canvas.save();
        if self.clip {
            ctx.canvas
                .set_clip(Vec2d::default(), bounds);
        }
        self.child.draw(ctx);
        ctx.canvas.restore();

        paint_overflow_indicator(ctx, bounds, overflow, &self.label);
    }
}

impl EventElement for RawOverflowIndicator {
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl Rebuildable for RawOverflowIndicator {}

impl VisitorElement for RawOverflowIndicator {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "OverflowIndicator"
    }
}

impl LayoutElement for RawOverflowIndicator {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let child = self
            .child
            .computed_size(ctx);
        ResolvedSize {
            width: constrain(
                child.width,
                ctx.box_constraint
                    .min_width,
                ctx.box_constraint
                    .max_width,
            ),
            height: constrain(
                child.height,
                ctx.box_constraint
                    .min_height,
                ctx.box_constraint
                    .max_height,
            ),
        }
    }

    fn invalidate_layout(&self) {
        self.child
            .invalidate_layout();
    }
}

fn constrain(value: f32, min: f32, max: f32) -> f32 {
    if max == f32::MAX { value.max(min) } else { value.clamp(min, max.max(min)) }
}

fn diagnostic_bounds(ctx: &BuildContext) -> ResolvedSize {
    let BoxConstraint { min_width, min_height, max_width, max_height } = ctx.box_constraint;
    ResolvedSize {
        width: if max_width == f32::MAX {
            ctx.parent_size
                .width
                .max(min_width)
        } else {
            max_width.max(min_width)
        },
        height: if max_height == f32::MAX {
            ctx.parent_size
                .height
                .max(min_height)
        } else {
            max_height.max(min_height)
        },
    }
}

#[cfg(debug_assertions)]
pub fn paint_overflow_indicator(
    ctx: &BuildContext,
    bounds: ResolvedSize,
    overflow: OverflowEdges,
    label: &str,
) {
    if !overflow.has_overflow() || bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }

    const THICKNESS: f32 = 8.0;
    const STRIPE: f32 = 6.0;
    let paint_horizontal = |y: f32| {
        let mut x = 0.0;
        let mut yellow = true;
        while x < bounds.width {
            ctx.canvas
                .fill_color_rect(
                    Vec2d { x, y },
                    ResolvedSize {
                        width: STRIPE.min(bounds.width - x),
                        height: THICKNESS.min(bounds.height),
                    },
                    if yellow { Color::YELLOW } else { Color::BLACK },
                    [0.0; 4],
                );
            yellow = !yellow;
            x += STRIPE;
        }
    };
    let paint_vertical = |x: f32| {
        let mut y = 0.0;
        let mut yellow = true;
        while y < bounds.height {
            ctx.canvas
                .fill_color_rect(
                    Vec2d { x, y },
                    ResolvedSize {
                        width: THICKNESS.min(bounds.width),
                        height: STRIPE.min(bounds.height - y),
                    },
                    if yellow { Color::YELLOW } else { Color::BLACK },
                    [0.0; 4],
                );
            yellow = !yellow;
            y += STRIPE;
        }
    };

    if overflow.top > 0.0 {
        paint_horizontal(0.0);
    }
    if overflow.bottom > 0.0 {
        paint_horizontal((bounds.height - THICKNESS).max(0.0));
    }
    if overflow.left > 0.0 {
        paint_vertical(0.0);
    }
    if overflow.right > 0.0 {
        paint_vertical((bounds.width - THICKNESS).max(0.0));
    }

    let text = format!("{label} overflowed by {:.1}px", overflow.maximum());
    let width = ((text.len() as f32 * 6.0) + 8.0).min(bounds.width);
    ctx.canvas
        .fill_color_rect(
            Vec2d { x: 0.0, y: 0.0 },
            ResolvedSize { width, height: 18.0_f32.min(bounds.height) },
            Color::BLACK,
            [0.0; 4],
        );
    ctx.canvas
        .draw_text(&text, Vec2d { x: 4.0, y: 13.0 }, 10.0, Color::YELLOW, 600);
}

#[cfg(not(debug_assertions))]
pub fn paint_overflow_indicator(
    _ctx: &BuildContext,
    _bounds: ResolvedSize,
    _overflow: OverflowEdges,
    _label: &str,
) {
}

#[cfg(test)]
mod tests {
    use aimer_attribute::ResolvedSize;

    use super::{OverflowEdges, detect_overflow};

    #[test]
    fn detects_each_overflowing_edge() {
        let overflow = detect_overflow(
            ResolvedSize { width: 120.0, height: 80.0 },
            ResolvedSize { width: 100.0, height: 60.0 },
            (-4.0, -3.0).into(),
        );

        assert_eq!(overflow, OverflowEdges { left: 4.0, top: 3.0, right: 16.0, bottom: 17.0 });
    }

    #[test]
    fn fitting_child_has_no_overflow() {
        let overflow = detect_overflow(
            ResolvedSize { width: 40.0, height: 30.0 },
            ResolvedSize { width: 100.0, height: 60.0 },
            (10.0, 10.0).into(),
        );

        assert!(!overflow.has_overflow());
    }
}
