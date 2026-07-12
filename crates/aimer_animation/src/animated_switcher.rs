use crate::controller::AnimationController;
use crate::curve::Curve;
use crate::time::AnimInstant;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_macro::WidgetConstructor;
use aimer_widget::base::*;
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement,
    Widget,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A widget that cross-fades between its old and new child when the child changes.
///
/// When the `child` field is updated (via rebuild), the switcher fades out the
/// old child and fades in the new one over the specified `duration`.
///
/// # Example
/// ```ignore
/// AnimatedSwitcher::new(
///     Duration::from_millis(300),
///     Curve::FastOutSlowIn,
///     if show_first { text_widget("First") } else { text_widget("Second") },
/// )
/// ```
#[derive(WidgetConstructor)]
pub struct AnimatedSwitcher<T: Widget + 'static> {
    pub child: T,
    pub duration: Duration,
    pub curve: Curve,
    /// Optional separate curve for the outgoing child. Defaults to `curve`.
    #[constructor(default)]
    pub switch_out_curve: Option<Curve>,
}

impl<T: Widget> AnimatedSwitcher<T> {
    pub fn new(duration: Duration, curve: Curve, child: T) -> Self {
        Self {
            child,
            duration,
            curve,
            switch_out_curve: None,
        }
    }

    pub fn with_switch_out_curve(mut self, curve: Curve) -> Self {
        self.switch_out_curve = Some(curve);
        self
    }
}

impl<T: Widget + 'static> Widget for AnimatedSwitcher<T> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);
        let switch_out_curve = self.switch_out_curve.unwrap_or(self.curve);

        let in_controller = Arc::new(Mutex::new(AnimationController::new(self.duration, self.curve)));
        let out_controller = Arc::new(Mutex::new(AnimationController::new(self.duration, switch_out_curve)));
        let animating = Arc::new(AtomicBool::new(false));
        let window: &'static winit::window::Window = ctx.window;

        // Start the "in" animation immediately for the initial child
        {
            let mut ctrl = in_controller.lock().unwrap();
            ctrl.forward();
        }

        Box::new(AnimatedSwitcherElement {
            current_child: child,
            old_child: None,
            in_controller,
            out_controller,
            animating,
            window,
        })
    }
}

struct AnimatedSwitcherElement {
    current_child: Box<dyn Element>,
    old_child: Option<Box<dyn Element>>,
    in_controller: Arc<Mutex<AnimationController>>,
    out_controller: Arc<Mutex<AnimationController>>,
    animating: Arc<AtomicBool>,
    window: &'static winit::window::Window,
}

unsafe impl Send for AnimatedSwitcherElement {}
unsafe impl Sync for AnimatedSwitcherElement {}

impl Drawable for AnimatedSwitcherElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();

        // Tick both controllers
        let (in_value, out_value) = {
            let mut in_ctrl = self.in_controller.lock().unwrap();
            let mut out_ctrl = self.out_controller.lock().unwrap();
            let in_v = in_ctrl.tick(now);
            let out_v = out_ctrl.tick(now);
            let any_animating = in_ctrl.is_animating() || out_ctrl.is_animating();
            self.animating.store(any_animating, Ordering::Relaxed);
            (in_v, out_v)
        };

        // Draw old child (fading out)
        if let Some(ref old) = self.old_child
            && out_value < 1.0
        {
            ctx.canvas.save();
            ctx.canvas.set_alpha(1.0 - out_value);
            old.draw(ctx);
            ctx.canvas.restore();
        }

        // Draw new child (fading in)
        ctx.canvas.save();
        ctx.canvas.set_alpha(in_value);
        self.current_child.draw(ctx);
        ctx.canvas.restore();

        if self.animating.load(Ordering::Relaxed) {
            self.window.request_redraw();
        }
    }
}

impl VisitorElement for AnimatedSwitcherElement {
    fn debug_name(&self) -> &'static str {
        "AnimatedSwitcherElement"
    }

    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.current_child.as_ref());
        if let Some(ref old) = self.old_child {
            visitor(old.as_ref());
        }
    }
}

impl EventElement for AnimatedSwitcherElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.current_child.on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.current_child.as_ref());
        if let Some(ref old) = self.old_child {
            visitor(old.as_ref());
        }
    }
}

impl Rebuildable for AnimatedSwitcherElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.current_child.rebuild_if_dirty(ctx);
        if let Some(ref old) = self.old_child {
            old.rebuild_if_dirty(ctx);
        }
    }
}

impl LayoutElement for AnimatedSwitcherElement {
    fn pos(&self) -> Option<Vec2d> {
        self.current_child.pos()
    }

    fn size(&self) -> Option<Size> {
        self.current_child.size()
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.current_child.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.current_child.content_size(ctx)
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.current_child.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.current_child.invalidate_layout();
    }
}
