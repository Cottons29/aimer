use crate::controller::AnimationController;
use crate::time::AnimInstant;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement, Widget,
};
use std::cell::{Cell, UnsafeCell};
use std::sync::Arc;

type AnimatedElementBuilder = dyn Fn(f32, &BuildContext) -> Box<dyn Element>;

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
pub struct AnimatedBuilder {
    pub controller: AnimationController,
    builder: Arc<AnimatedElementBuilder>,
}

impl AnimatedBuilder {
    pub fn new<F, W>(controller: AnimationController, builder: F) -> Self
    where
        F: Fn(f32) -> W + 'static,
        W: Widget,
    {
        let builder =
            Arc::new(move |value: f32, ctx: &BuildContext| builder(value).to_element(ctx));
        Self { controller, builder }
    }
}

impl Widget for AnimatedBuilder {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let curved_value = self.controller.curve().transform(self.controller.value());
        let child = (self.builder)(curved_value, ctx);
        let window: &'static winit::window::Window = ctx.window;

        Box::new(AnimatedBuilderElement {
            child: UnsafeCell::new(child),
            controller: self.controller.clone(),
            builder: self.builder.clone(),
            last_value: Cell::new(curved_value),
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
    child: UnsafeCell<Box<dyn Element>>,
    controller: AnimationController,
    builder: Arc<AnimatedElementBuilder>,
    last_value: Cell<f32>,
    window: &'static winit::window::Window,
}

// Safety: rendering pipeline is single-threaded
unsafe impl Send for AnimatedBuilderElement {}
unsafe impl Sync for AnimatedBuilderElement {}

impl Drawable for AnimatedBuilderElement {
    fn draw(&self, ctx: &BuildContext) {
        let curved_value = self.controller.tick(AnimInstant::now());
        if curved_value != self.last_value.get() {
            let child = (self.builder)(curved_value, ctx);
            unsafe { *self.child.get() = child };
            self.last_value.set(curved_value);
        }

        unsafe { &*self.child.get() }.draw(ctx);

        if self.controller.is_animating() {
            self.window.request_redraw();
        }
    }
}

impl VisitorElement for AnimatedBuilderElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(unsafe { &*self.child.get() }.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedBuilderElement"
    }
}

impl EventElement for AnimatedBuilderElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        unsafe { &*self.child.get() }.on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(unsafe { &*self.child.get() }.as_ref());
    }
}

impl Rebuildable for AnimatedBuilderElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        unsafe { &*self.child.get() }.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for AnimatedBuilderElement {
    fn pos(&self) -> Option<Vec2d> {
        unsafe { &*self.child.get() }.pos()
    }

    fn size(&self) -> Option<Size> {
        unsafe { &*self.child.get() }.size()
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe { &*self.child.get() }.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe { &*self.child.get() }.content_size(ctx)
    }

    fn get_size_from_child(&self) -> Option<Size> {
        unsafe { &*self.child.get() }.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        unsafe { &*self.child.get() }.invalidate_layout();
    }
}
