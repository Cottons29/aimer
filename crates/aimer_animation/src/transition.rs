use crate::controller::AnimationController;
use crate::time::AnimInstant;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement, Widget,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// FadeTransition
// ---------------------------------------------------------------------------

/// Animates the opacity of its child based on the controller's value.
///
/// At value 0.0 the child is fully transparent; at 1.0 fully opaque.
pub struct FadeTransition<T: Widget + 'static> {
    pub opacity: AnimationController,
    pub child: T,
}

impl<T: Widget> FadeTransition<T> {
    pub fn new(opacity: AnimationController, child: T) -> Self {
        Self { opacity, child }
    }
}

impl<T: Widget + 'static> Widget for FadeTransition<T> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);
        let controller = Arc::new(Mutex::new(self.opacity.clone()));
        let animating = Arc::new(AtomicBool::new(self.opacity.is_animating()));
        let window: &'static winit::window::Window = ctx.window;
        Box::new(FadeTransitionElement { child, controller, animating, window })
    }
}

macro_rules! impl_transition_element {
    ($name:ident, $debug:expr, $apply:expr) => {
        struct $name {
            child: Box<dyn Element>,
            controller: Arc<Mutex<AnimationController>>,
            animating: Arc<AtomicBool>,
            window: &'static winit::window::Window,
        }

        unsafe impl Send for $name {}
        unsafe impl Sync for $name {}

        impl Drawable for $name {
            fn draw(&self, ctx: &BuildContext) {
                let now = AnimInstant::now();
                let curved_value = {
                    let ctrl = self.controller.lock().unwrap();
                    let v = ctrl.tick(now);
                    self.animating.store(ctrl.is_animating(), Ordering::Relaxed);
                    v
                };

                ctx.canvas.save();
                $apply(ctx, curved_value);
                self.child.draw(ctx);
                ctx.canvas.restore();

                if self.animating.load(Ordering::Relaxed) {
                    self.window.request_redraw();
                }
            }
        }

        impl VisitorElement for $name {
            fn debug_name(&self) -> &'static str {
                $debug
            }
            fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
                visitor(self.child.as_ref());
            }
        }

        impl EventElement for $name {
            fn on_event(&self, event: &ElementEvent) -> bool {
                self.child.on_event(event)
            }
            fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
                visitor(self.child.as_ref());
            }
        }

        impl Rebuildable for $name {
            fn rebuild_if_dirty(&self, ctx: &BuildContext) {
                self.child.rebuild_if_dirty(ctx);
            }
        }

        impl LayoutElement for $name {
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
    };
}

impl_transition_element!(
    FadeTransitionElement,
    "FadeTransitionElement",
    |ctx: &BuildContext, v: f32| {
        ctx.canvas.set_alpha(v.clamp(0.0, 1.0));
    }
);

// ---------------------------------------------------------------------------
// SlideTransition
// ---------------------------------------------------------------------------

/// Animates a slide offset for its child.
///
/// The child is translated by `offset * controller_value` pixels.
/// At value 0.0 the child is at the offset position; at 1.0 it's at its natural position.
pub struct SlideTransition<T: Widget + 'static> {
    pub position: AnimationController,
    /// The offset direction in pixels at value 0.0. At value 1.0 the child is at (0,0).
    pub offset: (f32, f32),
    pub child: T,
}

impl<T: Widget> SlideTransition<T> {
    pub fn new(position: AnimationController, offset: (f32, f32), child: T) -> Self {
        Self { position, offset, child }
    }
}

impl<T: Widget + 'static> Widget for SlideTransition<T> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);
        let controller = Arc::new(Mutex::new(self.position.clone()));
        let animating = Arc::new(AtomicBool::new(self.position.is_animating()));
        let window: &'static winit::window::Window = ctx.window;
        let offset = self.offset;
        Box::new(SlideTransitionElement { child, controller, animating, window, offset })
    }
}

struct SlideTransitionElement {
    child: Box<dyn Element>,
    controller: Arc<Mutex<AnimationController>>,
    animating: Arc<AtomicBool>,
    window: &'static winit::window::Window,
    offset: (f32, f32),
}

unsafe impl Send for SlideTransitionElement {}
unsafe impl Sync for SlideTransitionElement {}

impl Drawable for SlideTransitionElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        let curved_value = {
            let ctrl = self.controller.lock().unwrap();
            let v = ctrl.tick(now);
            self.animating.store(ctrl.is_animating(), Ordering::Relaxed);
            v
        };

        // At value 0.0: child is fully offset. At value 1.0: child is at natural position.
        let remaining = 1.0 - curved_value;
        let dx = self.offset.0 * remaining;
        let dy = self.offset.1 * remaining;

        ctx.canvas.save();
        ctx.canvas.translate((dx, dy).into());
        self.child.draw(ctx);
        ctx.canvas.restore();

        if self.animating.load(Ordering::Relaxed) {
            self.window.request_redraw();
        }
    }
}

impl VisitorElement for SlideTransitionElement {
    fn debug_name(&self) -> &'static str {
        "SlideTransitionElement"
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl EventElement for SlideTransitionElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.child.on_event(event)
    }
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl Rebuildable for SlideTransitionElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for SlideTransitionElement {
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

// ---------------------------------------------------------------------------
// ScaleTransition
// ---------------------------------------------------------------------------

/// Animates uniform scale for its child based on the controller's value.
pub struct ScaleTransition<T: Widget + 'static> {
    pub scale: AnimationController,
    pub child: T,
}

impl<T: Widget> ScaleTransition<T> {
    pub fn new(scale: AnimationController, child: T) -> Self {
        Self { scale, child }
    }
}

impl<T: Widget + 'static> Widget for ScaleTransition<T> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);
        let controller = Arc::new(Mutex::new(self.scale.clone()));
        let animating = Arc::new(AtomicBool::new(self.scale.is_animating()));
        let window: &'static winit::window::Window = ctx.window;
        Box::new(ScaleTransitionElement { child, controller, animating, window })
    }
}

struct ScaleTransitionElement {
    child: Box<dyn Element>,
    controller: Arc<Mutex<AnimationController>>,
    animating: Arc<AtomicBool>,
    window: &'static winit::window::Window,
}

unsafe impl Send for ScaleTransitionElement {}
unsafe impl Sync for ScaleTransitionElement {}

impl Drawable for ScaleTransitionElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        let curved_value = {
            let ctrl = self.controller.lock().unwrap();
            let v = ctrl.tick(now);
            self.animating.store(ctrl.is_animating(), Ordering::Relaxed);
            v
        };

        let cx = ctx.box_constraint.max_width / 2.0;
        let cy = ctx.box_constraint.max_height / 2.0;

        ctx.canvas.save();
        ctx.canvas.translate((cx, cy).into());
        ctx.canvas.scale(curved_value, curved_value);
        ctx.canvas.translate((-cx, -cy).into());
        self.child.draw(ctx);
        ctx.canvas.restore();

        if self.animating.load(Ordering::Relaxed) {
            self.window.request_redraw();
        }
    }
}

impl VisitorElement for ScaleTransitionElement {
    fn debug_name(&self) -> &'static str {
        "ScaleTransitionElement"
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl EventElement for ScaleTransitionElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.child.on_event(event)
    }
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl Rebuildable for ScaleTransitionElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for ScaleTransitionElement {
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

// ---------------------------------------------------------------------------
// RotationTransition
// ---------------------------------------------------------------------------

/// Animates rotation (in full turns) for its child based on the controller's value.
///
/// At value 0.0 the child is at 0 rotation; at 1.0 it has completed one full turn (2π radians).
pub struct RotationTransition<T: Widget + 'static> {
    pub turns: AnimationController,
    pub child: T,
}

impl<T: Widget> RotationTransition<T> {
    pub fn new(turns: AnimationController, child: T) -> Self {
        Self { turns, child }
    }
}

impl<T: Widget + 'static> Widget for RotationTransition<T> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);
        let controller = Arc::new(Mutex::new(self.turns.clone()));
        let animating = Arc::new(AtomicBool::new(self.turns.is_animating()));
        let window: &'static winit::window::Window = ctx.window;
        Box::new(RotationTransitionElement { child, controller, animating, window })
    }
}

struct RotationTransitionElement {
    child: Box<dyn Element>,
    controller: Arc<Mutex<AnimationController>>,
    animating: Arc<AtomicBool>,
    window: &'static winit::window::Window,
}

unsafe impl Send for RotationTransitionElement {}
unsafe impl Sync for RotationTransitionElement {}

impl Drawable for RotationTransitionElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        let curved_value = {
            let ctrl = self.controller.lock().unwrap();
            let v = ctrl.tick(now);
            self.animating.store(ctrl.is_animating(), Ordering::Relaxed);
            v
        };

        // Convert turns to radians: 1.0 turn = 2π radians
        let angle = curved_value * std::f32::consts::TAU;
        let cx = ctx.box_constraint.max_width / 2.0;
        let cy = ctx.box_constraint.max_height / 2.0;

        ctx.canvas.save();
        ctx.canvas.translate((cx, cy).into());
        ctx.canvas.rotate(angle);
        ctx.canvas.translate((-cx, -cy).into());
        self.child.draw(ctx);
        ctx.canvas.restore();

        if self.animating.load(Ordering::Relaxed) {
            self.window.request_redraw();
        }
    }
}

impl VisitorElement for RotationTransitionElement {
    fn debug_name(&self) -> &'static str {
        "RotationTransitionElement"
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl EventElement for RotationTransitionElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.child.on_event(event)
    }
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl Rebuildable for RotationTransitionElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for RotationTransitionElement {
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
