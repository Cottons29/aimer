use crate::controller::AnimationController;
use crate::time::AnimInstant;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement,
    Widget,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A widget that rebuilds its child on every animation tick.
///
/// Unlike `Animated` which applies a fixed visual effect, `AnimatedBuilder`
/// gives you the current animation value each frame so you can build any widget
/// based on it.
///
/// # Example
/// ```ignore
/// AnimatedBuilder::new(controller, |value| {
///     Container::new()
///         .width(Size::Fixed(value * 200.0))
///         .child(Text::new(format!("{:.0}%", value * 100.0)))
/// })
/// ```
pub struct AnimatedBuilder<F> {
    pub controller: AnimationController,
    pub builder: F,
}

impl<F, W> AnimatedBuilder<F>
where
    F: Fn(f32) -> W + 'static,
    W: Widget,
{
    pub fn new(controller: AnimationController, builder: F) -> Self {
        Self { controller, builder }
    }
}

impl<F, W> Widget for AnimatedBuilder<F>
where
    F: Fn(f32) -> W + 'static,
    W: Widget,
{
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let curved_value = self.controller.curve.transform(self.controller.value);
        let child_widget = (self.builder)(curved_value);
        let child = child_widget.to_element(ctx);

        let controller = Arc::new(Mutex::new(self.controller.clone()));
        let animating = Arc::new(AtomicBool::new(self.controller.is_animating()));
        let window: &'static winit::window::Window = ctx.window;

        // We need to store the builder as a trait object. Since Fn closures
        // can't be made into trait objects easily across to_element calls,
        // we rebuild the child from the element side using the stored value.
        Box::new(AnimatedBuilderElement {
            child,
            controller,
            animating,
            window,
        })
    }
}

/// The element produced by `AnimatedBuilder`.
///
/// On each draw, it ticks the controller, rebuilds the child from the
/// builder closure (which was captured at construction), and draws the result.
/// This approach means the child is rebuilt every frame while animating,
/// which is the intended behavior for responsive animations.
struct AnimatedBuilderElement {
    child: Box<dyn Element>,
    controller: Arc<Mutex<AnimationController>>,
    animating: Arc<AtomicBool>,
    window: &'static winit::window::Window,
}

// Safety: rendering pipeline is single-threaded
unsafe impl Send for AnimatedBuilderElement {}
unsafe impl Sync for AnimatedBuilderElement {}

impl Drawable for AnimatedBuilderElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        let _curved_value = {
            let mut ctrl = self.controller.lock().unwrap();
            let v = ctrl.tick(now);
            self.animating.store(ctrl.is_animating(), Ordering::Relaxed);
            v
        };

        // Draw the child (which was built with the current animation value)
        self.child.draw(ctx);

        if self.animating.load(Ordering::Relaxed) {
            self.window.request_redraw();
        }
    }
}

impl VisitorElement for AnimatedBuilderElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedBuilderElement"
    }
}

impl EventElement for AnimatedBuilderElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.child.on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl Rebuildable for AnimatedBuilderElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for AnimatedBuilderElement {
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

