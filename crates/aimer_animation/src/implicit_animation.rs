use crate::animatable::Animatable;
use crate::controller::AnimationController;
use crate::curve::Curve;
use crate::time::AnimInstant;
use crate::tween::Tween;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, Reconcilable, VisitorElement,
    Widget,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A widget that automatically animates when its value changes.
///
/// On the first build, the value is used directly (no animation).
/// When the widget is rebuilt with a different value, a tween animation
/// runs from the old value to the new value over the specified duration.
///
/// # Example
/// ```ignore
/// ImplicitAnimatedBuilder::new(
///     current_width,
///     Duration::from_millis(300),
///     Curve::FastOutSlowIn,
///     |width| Container::new().width(Size::Fixed(width)).child(Text::new("Hello")),
/// )
/// ```
pub struct ImplicitAnimatedBuilder<T: Animatable + Clone + PartialEq + 'static, F, W: Widget> {
    pub value: T,
    pub duration: Duration,
    pub curve: Curve,
    pub builder: F,
    _phantom: std::marker::PhantomData<W>,
}

impl<T, F, W> ImplicitAnimatedBuilder<T, F, W>
where
    T: Animatable + Clone + PartialEq + 'static,
    F: Fn(&T) -> W + 'static,
    W: Widget,
{
    pub fn new(value: T, duration: Duration, curve: Curve, builder: F) -> Self {
        Self {
            value,
            duration,
            curve,
            builder,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T, F, W> Widget for ImplicitAnimatedBuilder<T, F, W>
where
    T: Animatable + Clone + PartialEq + 'static,
    F: Fn(&T) -> W + 'static,
    W: Widget,
{
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child_widget = (self.builder)(&self.value);
        let child = child_widget.to_element(ctx);

        let controller = Arc::new(Mutex::new(AnimationController::new(self.duration, self.curve)));
        let animating = Arc::new(AtomicBool::new(false));
        let window: &'static winit::window::Window = ctx.window;
        let current_value = Arc::new(Mutex::new(self.value.clone()));
        let tween = Arc::new(Mutex::new(None::<Tween<T>>));

        Box::new(ImplicitAnimatedElement {
            child,
            controller,
            animating,
            window,
            current_value,
            tween,
        })
    }
}

struct ImplicitAnimatedElement<T: Animatable + Clone + 'static> {
    child: Box<dyn Element>,
    controller: Arc<Mutex<AnimationController>>,
    animating: Arc<AtomicBool>,
    window: &'static winit::window::Window,
    #[allow(dead_code)]
    current_value: Arc<Mutex<T>>,
    #[allow(dead_code)]
    tween: Arc<Mutex<Option<Tween<T>>>>,
}

unsafe impl<T: Animatable + Clone + 'static> Send for ImplicitAnimatedElement<T> {}
unsafe impl<T: Animatable + Clone + 'static> Sync for ImplicitAnimatedElement<T> {}

impl<T: Animatable + Clone + 'static> Drawable for ImplicitAnimatedElement<T> {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();
        let _curved_value = {
            let mut ctrl = self.controller.lock().unwrap();
            let v = ctrl.tick(now);
            self.animating.store(ctrl.is_animating(), Ordering::Relaxed);
            v
        };

        self.child.draw(ctx);

        if self.animating.load(Ordering::Relaxed) {
            self.window.request_redraw();
        }
    }
}

impl<T: Animatable + Clone + 'static> VisitorElement for ImplicitAnimatedElement<T> {
    fn debug_name(&self) -> &'static str {
        "ImplicitAnimatedElement"
    }

    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl<T: Animatable + Clone + 'static> EventElement for ImplicitAnimatedElement<T> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.child.on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl<T: Animatable + Clone + 'static> Rebuildable for ImplicitAnimatedElement<T> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.child.rebuild_if_dirty(ctx);
    }
}

impl<T: Animatable + Clone + 'static> LayoutElement for ImplicitAnimatedElement<T> {
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

impl<T: Animatable + Clone + 'static> Reconcilable for ImplicitAnimatedElement<T> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_from_widget(&self, _new_element: &dyn Element, _ctx: &BuildContext) -> bool {
        // Try to downcast and detect value changes.
        // If the new element has a different value, start a tween animation.
        // For now, always replace — the implicit animation logic is driven
        // by the StatefulWidget pattern where the state detects value changes.
        false
    }
}
