use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, RequiredChild, VisitorElement,
    Widget,
};
use std::cell::Cell;

use crate::control::controller::AnimationController;
use crate::primitives::time::AnimInstant;

/// Describes what visual property the `Animated` widget should animate.
#[derive(Debug, Clone, Copy)]
pub enum AnimationEffect {
    /// Animate opacity from `from` to `to` (0.0 = invisible, 1.0 = fully
    /// opaque).
    Opacity { from: f32, to: f32 },
    /// Animate uniform scale from `from` to `to` (1.0 = normal size).
    Scale { from: f32, to: f32 },
    /// Animate translation offset in pixels.
    Translate { from_x: f32, from_y: f32, to_x: f32, to_y: f32 },
    /// Animate rotation in radians.
    Rotate { from: f32, to: f32 },
    /// Animate a slide-in from a direction (0.0 = off-screen, 1.0 = in place).
    SlideX { from: f32, to: f32 },
    /// Animate a slide-in vertically.
    SlideY { from: f32, to: f32 },
}

impl AnimationEffect {
    /// Interpolate between `from` and `to` using progress `t` (0.0–1.0).
    fn lerp(from: f32, to: f32, t: f32) -> f32 {
        from + (to - from) * t
    }
}

/// A widget that wraps a child and animates it using an
/// [`AnimationController`].
///
/// The `Animated` widget applies a canvas transform (opacity, scale, translate,
/// or rotate) to its child based on the current animation progress. It
/// internally manages a `StatefulWidget` that ticks the controller each frame
/// and requests redraws while the animation is running.
///
/// # Example
/// ```rust ignore
/// Animated::new(
///     controller,
///     AnimationEffect::Opacity { from: 0.0, to: 1.0 },
///     my_child_widget,
/// )
/// ```
pub struct Animated<T = RequiredChild> {
    pub controller: AnimationController,
    pub effect: AnimationEffect,
    pub child: T,
}
//
// impl Animated {
//     pub fn new() -> Self {
//         Self {
//             controller: AnimationController::new(),
//             effect: AnimationEffect::Opacity { from: 0.0, to: 1.0 },
//             child: RequiredChild,
//         }
//     }
// }

impl<T: Widget> Animated<T> {
    pub fn new(controller: AnimationController, effect: AnimationEffect, child: T) -> Self {
        Self { controller, effect, child }
    }
}

impl<T: Widget + 'static> Widget for Animated<T> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child_element = self.child.to_element(ctx);

        let controller = self.controller.clone();
        let animating = Cell::new(self.controller.is_animating());

        // Create a StateUpdater-like mechanism: we use a shared dirty flag + window ref
        // to request redraws while the animation is running.
        let window = ctx.window.clone();
        AnimatedElement { child: child_element, controller, effect: self.effect, animating, window }
            .boxed()
    }
}

/// The element produced by [`Animated`]. On each `draw`, it ticks the
/// controller, applies the canvas transform, draws the child, then requests
/// another redraw if the animation is still running.
struct AnimatedElement {
    child: Box<dyn Element>,
    controller: AnimationController,
    effect: AnimationEffect,
    animating: Cell<bool>,
    window: WindowHandle,
}

// Safety: rendering pipeline is single-threaded
unsafe impl Send for AnimatedElement {}
unsafe impl Sync for AnimatedElement {}

impl Drawable for AnimatedElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        let curved_value = {
            let v = self.controller.tick(now);
            self.animating
                .set(self.controller.is_animating());
            v
        };

        ctx.canvas.save();

        // Clip to the widget's bounds so content outside (e.g. sliding in) is hidden
        self.clip_to_bounds(ctx);

        self.apply_effect(ctx, curved_value);
        self.child.draw(ctx);

        ctx.canvas.clear_clip();
        ctx.canvas.restore();

        // Request another frame if still animating
        if self.animating.get() {
            self.window.request_redraw();
        }
    }
}

impl AnimatedElement {
    /// Clip drawing to the child's computed size so that content
    /// outside (e.g. a child sliding in from off-screen) stays hidden.
    fn clip_to_bounds(&self, ctx: &BuildContext) {
        let child_size = self.child.computed_size(ctx);
        let w = child_size.width;
        let h = child_size.height;
        ctx.canvas
            .set_clip((0.0, 0.0).into(), ResolvedSize { width: w, height: h });
    }

    fn apply_effect(&self, ctx: &BuildContext, t: f32) {
        match self.effect {
            AnimationEffect::Opacity { from, to } => {
                let alpha = AnimationEffect::lerp(from, to, t);
                ctx.canvas.set_alpha(alpha);
            }
            AnimationEffect::Scale { from, to } => {
                let scale = AnimationEffect::lerp(from, to, t);
                let cx = ctx.box_constraint.max_width / 2.0;
                let cy = ctx.box_constraint.max_height / 2.0;
                ctx.canvas
                    .translate((cx, cy).into());
                ctx.canvas.scale(scale, scale);
                ctx.canvas
                    .translate((-cx, -cy).into());
            }
            AnimationEffect::Translate { from_x, from_y, to_x, to_y } => {
                let dx = AnimationEffect::lerp(from_x, to_x, t);
                let dy = AnimationEffect::lerp(from_y, to_y, t);
                ctx.canvas
                    .translate((dx, dy).into());
            }
            AnimationEffect::Rotate { from, to } => {
                let angle = AnimationEffect::lerp(from, to, t);
                let cx = ctx.box_constraint.max_width / 2.0;
                let cy = ctx.box_constraint.max_height / 2.0;
                ctx.canvas
                    .translate((cx, cy).into());
                ctx.canvas.rotate(angle);
                ctx.canvas
                    .translate((-cx, -cy).into());
            }
            AnimationEffect::SlideX { from, to } => {
                let offset = AnimationEffect::lerp(from, to, t);
                let dx = ctx.box_constraint.max_width * offset;
                ctx.canvas
                    .translate((dx, 0.0).into());
            }
            AnimationEffect::SlideY { from, to } => {
                let offset = AnimationEffect::lerp(from, to, t);
                let dy = ctx.box_constraint.max_height * offset;
                ctx.canvas
                    .translate((0.0, dy).into());
            }
        }
    }
}

impl VisitorElement for AnimatedElement {
    fn debug_name(&self) -> &'static str {
        "AnimatedElement"
    }
}

impl EventElement for AnimatedElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.child.on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl Rebuildable for AnimatedElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for AnimatedElement {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }

    fn size(&self) -> Option<Size> {
        self.child.size()
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.child.invalidate_layout();
    }
}
