use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use crate::time::AnimInstant;

use attribute::position::Vec2d;
use attribute::size::{ResolvedSize, Size};
use constructor::{Constructor, WidgetConstructor};
use events::element::ElementEvent;
use widget::base::*;
use widget::{Drawable, Element, Widget};

use crate::controller::AnimationController;

#[cfg(target_arch = "wasm32")]
type FLOAT = f64;
#[cfg(not(target_arch = "wasm32"))]
type FLOAT = f32;

/// Describes what visual property the `Animated` widget should animate.
#[derive(Debug, Clone, Copy)]
pub enum AnimationEffect {
    /// Animate opacity from `from` to `to` (0.0 = invisible, 1.0 = fully opaque).
    Opacity { from: f64, to: f64 },
    /// Animate uniform scale from `from` to `to` (1.0 = normal size).
    Scale { from: f64, to: f64 },
    /// Animate translation offset in pixels.
    Translate { from_x: f64, from_y: f64, to_x: f64, to_y: f64 },
    /// Animate rotation in radians.
    Rotate { from: f64, to: f64 },
    /// Animate a slide-in from a direction (0.0 = off-screen, 1.0 = in place).
    SlideX { from: f64, to: f64 },
    /// Animate a slide-in vertically.
    SlideY { from: f64, to: f64 },
}

impl AnimationEffect {
    /// Interpolate between `from` and `to` using progress `t` (0.0–1.0).
    fn lerp(from: f64, to: f64, t: f64) -> f64 {
        from + (to - from) * t
    }
}

/// A widget that wraps a child and animates it using an [`AnimationController`].
///
/// The `Animated` widget applies a canvas transform (opacity, scale, translate, or rotate)
/// to its child based on the current animation progress. It internally manages a
/// `StatefulWidget` that ticks the controller each frame and requests redraws while
/// the animation is running.
///
/// # Example
/// ```ignore
/// Animated::new(
///     controller,
///     AnimationEffect::Opacity { from: 0.0, to: 1.0 },
///     my_child_widget,
/// )
/// ```
#[derive(WidgetConstructor)]
pub struct Animated<T: Widget + 'static> {
    pub controller: AnimationController,
    pub effect: AnimationEffect,
    pub child: T,
}

impl<T: Widget> Animated<T> {
    pub fn new(controller: AnimationController, effect: AnimationEffect, child: T) -> Self {
        Self { controller, effect, child }
    }
}

impl<T: Widget + 'static> Widget for Animated<T> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child_element = self.child.to_element(ctx);

        let controller = Arc::new(Mutex::new(self.controller.clone()));
        let animating = Arc::new(AtomicBool::new(self.controller.is_animating()));

        // Create a StateUpdater-like mechanism: we use a shared dirty flag + window ref
        // to request redraws while the animation is running.
        let window: &'static winit::window::Window = ctx.window;
        Box::new(AnimatedElement {
            child: child_element,
            controller,
            effect: self.effect,
            animating,
            window,
        })
    }
}

/// The element produced by [`Animated`]. On each `draw`, it ticks the controller,
/// applies the canvas transform, draws the child, then requests another redraw
/// if the animation is still running.
struct AnimatedElement {
    child: Box<dyn Element>,
    controller: Arc<Mutex<AnimationController>>,
    effect: AnimationEffect,
    animating: Arc<AtomicBool>,
    window: &'static winit::window::Window,
}

// Safety: rendering pipeline is single-threaded
unsafe impl Send for AnimatedElement {}
unsafe impl Sync for AnimatedElement {}

impl Drawable for AnimatedElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        let curved_value = {
            let mut ctrl = self.controller.lock().unwrap();

            let v = ctrl.tick(now);
            self.animating.store(ctrl.is_animating(), Ordering::Relaxed);
            v
        };

        #[cfg(not(target_arch = "wasm32"))]
        ctx.canvas.save();
        #[cfg(target_arch = "wasm32")]
        ctx.canvas.save();

        // Clip to the widget's bounds so content outside (e.g. sliding in) is hidden
        self.clip_to_bounds(ctx);

        self.apply_effect(ctx, curved_value);
        self.child.draw(ctx);

        #[cfg(not(target_arch = "wasm32"))]
        ctx.canvas.restore();
        #[cfg(target_arch = "wasm32")]
        ctx.canvas.restore();

        // Request another frame if still animating
        if self.animating.load(Ordering::Relaxed) {
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
        #[cfg(not(target_arch = "wasm32"))]
        {
            ctx.canvas.clip_rect(
                skia_safe::Rect::from_xywh(0.0, 0.0, w, h),
                skia_safe::ClipOp::Intersect,
                true,
            );
        }
        #[cfg(target_arch = "wasm32")]
        {
            ctx.canvas.begin_path();
            ctx.canvas.rect(0.0, 0.0, w as f64, h as f64);
            ctx.canvas.clip();
        }
    }

    fn apply_effect(&self, ctx: &BuildContext, t: f64) {
        match self.effect {
            AnimationEffect::Opacity { from, to } => {
                let alpha = AnimationEffect::lerp(from, to, t);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ctx.canvas.save_layer_alpha(None, (alpha * 255.0) as u32);
                }
                #[cfg(target_arch = "wasm32")]
                {
                    ctx.canvas.set_global_alpha(alpha);
                }
            }
            AnimationEffect::Scale { from, to } => {
                let scale = AnimationEffect::lerp(from, to, t) as FLOAT;
                // Scale around the center of the parent area
                let cx = ctx.box_constraint.max_width / 2.0;
                let cy = ctx.box_constraint.max_height / 2.0;
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ctx.canvas.translate((cx, cy));
                    ctx.canvas.scale((scale, scale));
                    ctx.canvas.translate((-cx, -cy));
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = ctx.canvas.translate(cx, cy);
                    let _ = ctx.canvas.scale(scale, scale);
                    let _ = ctx.canvas.translate(-cx, -cy);
                }
            }
            AnimationEffect::Translate { from_x, from_y, to_x, to_y } => {
                let dx = AnimationEffect::lerp(from_x, to_x, t) as FLOAT;
                let dy = AnimationEffect::lerp(from_y, to_y, t) as FLOAT;
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ctx.canvas.translate((dx, dy));
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = ctx.canvas.translate(dx, dy);
                }
            }
            AnimationEffect::Rotate { from, to } => {
                let angle = AnimationEffect::lerp(from, to, t);
                let cx = ctx.box_constraint.max_width / 2.0;
                let cy = ctx.box_constraint.max_height / 2.0;
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ctx.canvas.translate((cx, cy));
                    ctx.canvas.rotate(angle as f32 * 180.0 / std::f32::consts::PI, None);
                    ctx.canvas.translate((-cx, -cy));
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = ctx.canvas.translate(cx, cy);
                    ctx.canvas.rotate(angle);
                    let _ = ctx.canvas.translate(-cx, -cy);
                }
            }
            AnimationEffect::SlideX { from, to } => {
                let offset = AnimationEffect::lerp(from, to, t) as FLOAT;
                let dx = ctx.box_constraint.max_width * offset;
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ctx.canvas.translate((dx, 0.0));
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = ctx.canvas.translate(dx, 0.0);
                }
            }
            AnimationEffect::SlideY { from, to } => {
                let offset = AnimationEffect::lerp(from, to, t) as FLOAT;
                let dy = ctx.box_constraint.max_height * offset;
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ctx.canvas.translate((0.0, dy));
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let _ = ctx.canvas.translate(0.0, dy);
                }
            }
        }
    }
}

impl Element for AnimatedElement {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }

    fn size(&self) -> Option<Size> {
        self.child.size()
    }

    fn on_event(&self, event: &ElementEvent) -> bool {
        self.child.on_event(event)
    }

    fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Animated handles its own child rendering in draw() with proper transforms,
        // so we don't expose children here to avoid double-rendering.
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
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

    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}
