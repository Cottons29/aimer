use std::cell::UnsafeCell;
use std::rc::Rc;
use std::time::Duration;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    Drawable, Element, EventElement, Key, LayoutElement, Rebuildable, State, StateUpdater,
    StatefulElement, StatefulWidget, VisitorElement, Widget,
};

use crate::control::controller::AnimationController;
use crate::primitives::curve::Curve;
use crate::primitives::time::AnimInstant;

fn request_next_frame() {
    aimer_events::window::request_animation_frame();
}

/// A widget that cross-fades between its old and new child when the child
/// changes.
///
/// When the `child` field is updated (via rebuild), the switcher fades out the
/// old child and fades in the new one over the specified `duration`.
/// Child widgets should provide distinct keys; use
/// [`AnimatedSwitcher::child_key`] when the child type does not expose one
/// itself.
///
/// # Example
/// ```ignore
/// AnimatedSwitcher::new(
///     Duration::from_millis(300),
///     Curve::FastOutSlowIn,
///     if show_first { text_widget("First") } else { text_widget("Second") },
/// )
/// ```
pub struct AnimatedSwitcher<T: Widget + 'static> {
    pub child: Rc<T>,
    pub duration: Duration,
    pub curve: Curve,
    /// Optional separate curve for the outgoing child. Defaults to `curve`.
    pub switch_out_curve: Option<Curve>,
    transition_key: Option<Key>,
    widget_key: Option<Key>,
}

impl<T: Widget> AnimatedSwitcher<T> {
    pub fn new(duration: Duration, curve: Curve, child: T) -> Self {
        Self {
            child: Rc::new(child),
            duration,
            curve,
            switch_out_curve: None,
            transition_key: None,
            widget_key: None,
        }
    }

    pub fn with_switch_out_curve(mut self, curve: Curve) -> Self {
        self.switch_out_curve = Some(curve);
        self
    }

    /// Set the child identity used to decide whether a transition is needed.
    /// This is useful when the child widget itself does not expose a key.
    pub fn child_key(mut self, key: impl Into<Key>) -> Self {
        self.transition_key = Some(key.into());
        self
    }

    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.widget_key = Some(key.into());
        self
    }
}

impl<T: Widget + 'static> StatefulWidget for AnimatedSwitcher<T> {
    type State = AnimatedSwitcherState<T>;

    fn create_state(&self) -> Self::State {
        let in_controller = AnimationController::new(self.duration, self.curve);
        in_controller.set_value(1.0);
        AnimatedSwitcherState {
            current_child: self.child.clone(),
            old_child: None,
            child_key: self
                .transition_key
                .clone()
                .or_else(|| self.child.key()),
            duration: self.duration,
            curve: self.curve,
            switch_out_curve: self
                .switch_out_curve
                .unwrap_or(self.curve),
            in_controller,
            out_controller: AnimationController::new(
                self.duration,
                self.switch_out_curve
                    .unwrap_or(self.curve),
            ),
            updater: StateUpdater::empty(),
        }
    }
}

impl<T: Widget + 'static> Widget for AnimatedSwitcher<T> {
    fn key(&self) -> Option<Key> {
        self.widget_key.clone()
    }

    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        StatefulElement::new_with_name(self, ctx, "AnimatedSwitcher", self.key())
            .0
            .boxed()
    }
}

#[doc(hidden)]
pub struct AnimatedSwitcherState<T: Widget + 'static> {
    current_child: Rc<T>,
    old_child: Option<Rc<T>>,
    child_key: Option<Key>,
    duration: Duration,
    curve: Curve,
    switch_out_curve: Curve,
    in_controller: AnimationController,
    out_controller: AnimationController,
    updater: StateUpdater<Self>,
}

impl<T: Widget + 'static> State<AnimatedSwitcher<T>> for AnimatedSwitcherState<T> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: &Self) {
        self.duration = new.duration;
        self.curve = new.curve;
        self.switch_out_curve = new.switch_out_curve;
        self.in_controller
            .set_duration(new.duration);
        self.in_controller
            .set_curve(new.curve);
        self.out_controller
            .set_duration(new.duration);
        self.out_controller
            .set_curve(new.switch_out_curve);

        if self.child_key != new.child_key {
            self.old_child = Some(self.current_child.clone());
            self.current_child = new.current_child.clone();
            self.child_key = new.child_key.clone();
            self.in_controller.reset();
            self.out_controller.reset();
            self.in_controller
                .forward_from_first_tick();
            self.out_controller
                .forward_from_first_tick();
            request_next_frame();
        } else {
            self.current_child = new.current_child.clone();
        }
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        AnimatedSwitcherFrame {
            current_child: self.current_child.clone(),
            old_child: if self.out_controller.is_animating() {
                self.old_child.clone()
            } else {
                None
            },
            in_controller: self.in_controller.clone(),
            out_controller: self.out_controller.clone(),
        }
    }
}

struct AnimatedSwitcherFrame<T: Widget + 'static> {
    current_child: Rc<T>,
    old_child: Option<Rc<T>>,
    in_controller: AnimationController,
    out_controller: AnimationController,
}

impl<T: Widget + 'static> Widget for AnimatedSwitcherFrame<T> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(AnimatedSwitcherElement {
            current_child: self.current_child.to_element(ctx),
            old_child: UnsafeCell::new(
                self.old_child
                    .as_ref()
                    .map(|child| child.to_element(ctx)),
            ),
            in_controller: self.in_controller.clone(),
            out_controller: self.out_controller.clone(),
        })
    }
}

struct AnimatedSwitcherElement {
    current_child: Box<dyn Element>,
    old_child: UnsafeCell<Option<Box<dyn Element>>>,
    in_controller: AnimationController,
    out_controller: AnimationController,
}

unsafe impl Send for AnimatedSwitcherElement {}
unsafe impl Sync for AnimatedSwitcherElement {}

impl Drawable for AnimatedSwitcherElement {
    fn draw(&self, ctx: &BuildContext) {
        let now = AnimInstant::now();

        // Tick both controllers
        let in_value = self.in_controller.tick(now);
        let out_value = self.out_controller.tick(now);

        // Draw old child (fading out)
        if let Some(old) = unsafe { &*self.old_child.get() }
            && out_value < 1.0
        {
            ctx.canvas.save();
            ctx.canvas
                .set_alpha(1.0 - out_value);
            old.draw(ctx);
            ctx.canvas.restore();
        }

        // Draw new child (fading in)
        ctx.canvas.save();
        ctx.canvas.set_alpha(in_value);
        self.current_child.draw(ctx);
        ctx.canvas.restore();

        if self.in_controller.is_animating() || self.out_controller.is_animating() {
            request_next_frame();
        } else if out_value >= 1.0 {
            unsafe { *self.old_child.get() = None };
        }
    }
}

impl VisitorElement for AnimatedSwitcherElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.current_child.as_ref());
        if let Some(old) = unsafe { &*self.old_child.get() } {
            visitor(old.as_ref());
        }
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedSwitcherElement"
    }
}

impl EventElement for AnimatedSwitcherElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.current_child.on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.current_child.as_ref());
    }
}

impl Rebuildable for AnimatedSwitcherElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.current_child
            .rebuild_if_dirty(ctx);
        if let Some(old) = unsafe { &*self.old_child.get() } {
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
        self.current_child
            .computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.current_child
            .content_size(ctx)
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.current_child
            .get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.current_child
            .invalidate_layout();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct TestWidget(&'static str);

    impl Widget for TestWidget {
        fn key(&self) -> Option<Key> {
            Some(Key::Value(self.0.to_owned()))
        }

        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            panic!("not needed for state lifecycle tests")
        }
    }

    fn state(key: &'static str) -> AnimatedSwitcherState<TestWidget> {
        AnimatedSwitcher::new(Duration::from_millis(100), Curve::Linear, TestWidget(key))
            .create_state()
    }

    #[test]
    fn initial_child_is_shown_without_starting_a_transition() {
        let current = state("initial");

        assert_eq!(current.in_controller.value(), 1.0);
        assert!(
            !current
                .in_controller
                .is_animating()
        );
        assert!(current.old_child.is_none());
    }

    #[test]
    fn changed_key_preserves_outgoing_child_and_starts_both_transitions() {
        let requests = Arc::new(AtomicUsize::new(0));
        let observed_requests = requests.clone();
        aimer_events::window::set_redraw_requester(move || {
            observed_requests.fetch_add(1, Ordering::Relaxed);
        });
        let mut current = state("first");

        current.adopt_config_from(&state("second"));

        assert!(current.old_child.is_some());
        assert_eq!(current.child_key, Some(Key::Value("second".to_owned())));
        assert!(
            current
                .in_controller
                .is_animating()
        );
        assert!(
            current
                .out_controller
                .is_animating()
        );
        assert_eq!(
            requests.load(Ordering::Relaxed),
            1,
            "transition startup must schedule its first frame"
        );
        std::thread::sleep(Duration::from_millis(10));

        assert_eq!(
            current
                .in_controller
                .tick(AnimInstant::now()),
            0.0
        );
        assert_eq!(
            current
                .out_controller
                .tick(AnimInstant::now()),
            0.0
        );
        assert!(
            current
                .in_controller
                .is_animating()
        );
        assert!(
            current
                .out_controller
                .is_animating()
        );
    }

    #[test]
    fn unchanged_key_updates_without_a_transition() {
        let mut current = state("same");

        current.adopt_config_from(&state("same"));

        assert!(current.old_child.is_none());
        assert!(
            !current
                .out_controller
                .is_animating()
        );
    }

    // ─── End-to-end draw test: a keyed switcher across a "route" change ────
    //
    // Reproduces the website router: a top-level stateful widget rebuilds a
    // keyed `AnimatedSwitcher` whose child changes with the active route. On a
    // route change the switcher's live state must be carried across the parent
    // rebuild (via keyed reconciliation) so `adopt_config_from` observes the
    // new child key and starts a cross-fade. During that fade BOTH the outgoing
    // and incoming children must be painted; if only the new child is painted,
    // the switch is instant (the reported bug).
    mod draw_transition {
        use std::cell::RefCell;
        use std::collections::HashMap;
        use std::rc::Rc;
        use std::sync::RwLock;

        use aimer_widget::StatefulElement;
        use aimer_widget::base::BuildContext;

        use super::*;

        /// A leaf that records the label it renders each time it is drawn, so a
        /// test can see exactly which children reached the screen this frame.
        struct RecordingLeaf {
            label: &'static str,
            drawn: Rc<RefCell<Vec<&'static str>>>,
        }
        impl VisitorElement for RecordingLeaf {
            fn debug_name(&self) -> &'static str {
                "RecordingLeaf"
            }
        }
        impl Drawable for RecordingLeaf {
            fn draw(&self, _ctx: &BuildContext) {
                self.drawn
                    .borrow_mut()
                    .push(self.label);
            }
        }
        impl EventElement for RecordingLeaf {}
        impl LayoutElement for RecordingLeaf {}
        impl Rebuildable for RecordingLeaf {}

        /// Widget wrapper around `RecordingLeaf` so it can be the switcher
        /// child.
        struct RecordingPage {
            label: &'static str,
            drawn: Rc<RefCell<Vec<&'static str>>>,
        }
        impl Widget for RecordingPage {
            fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
                Box::new(RecordingLeaf { label: self.label, drawn: self.drawn.clone() })
            }
            fn debug_name(&self) -> &'static str {
                "RecordingPage"
            }
        }

        fn route_label(route: usize) -> &'static str {
            if route == 0 { "home" } else { "docs" }
        }

        /// A "router": rebuilding it with a new `route` swaps the switcher
        /// child.
        struct RouterMock {
            drawn: Rc<RefCell<Vec<&'static str>>>,
        }
        struct RouterMockState {
            route: usize,
            drawn: Rc<RefCell<Vec<&'static str>>>,
            updater: StateUpdater<Self>,
        }
        impl StatefulWidget for RouterMock {
            type State = RouterMockState;
            fn create_state(&self) -> Self::State {
                RouterMockState {
                    route: 0,
                    drawn: self.drawn.clone(),
                    updater: StateUpdater::new(),
                }
            }
        }
        impl State<RouterMock> for RouterMockState {
            fn init_state(&mut self, updater: StateUpdater<Self>) {
                self.updater = updater;
            }
            fn build(&self, _ctx: &BuildContext) -> impl Widget {
                let label = route_label(self.route);
                AnimatedSwitcher::new(
                    Duration::from_millis(50),
                    Curve::Linear,
                    RecordingPage { label, drawn: self.drawn.clone() }.boxed(),
                )
                .child_key(label)
                .key("route-switcher")
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
        fn dummy_async_handle() -> tokio::runtime::Handle {
            static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> =
                std::sync::OnceLock::new();
            let runtime = RUNTIME.get_or_init(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
            });
            let _guard = runtime.enter();
            tokio::runtime::Handle::current()
        }

        fn dummy_build_context() -> BuildContext<'static> {
            let canvas = {
                let leaked: &'static aimer_canvas::InnerCanvas =
                    Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
                aimer_canvas::Canvas::new(leaked)
            };
            BuildContext {
                parent_size: Default::default(),
                canvas,
                scale: 1.0,
                parent_pos: Default::default(),
                cursor_pos: Default::default(),
                box_constraint: Default::default(),
                visible_rect: None,
                window: WindowHandle::headless(Default::default(), 1.0),
                #[cfg(not(target_arch = "wasm32"))]
                async_handle: dummy_async_handle(),
                inherited_states: Rc::new(RwLock::new(HashMap::new())),
            }
        }

        #[test]
        fn switching_route_cross_fades_instead_of_switching_instantly() {
            let ctx = dummy_build_context();
            let drawn: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));

            let (router, updater) = StatefulElement::new_with_name(
                &RouterMock { drawn: drawn.clone() },
                &ctx,
                "Router",
                None,
            );

            // Initial frame: only the "home" page is painted.
            drawn.borrow_mut().clear();
            router.draw(&ctx);
            assert_eq!(*drawn.borrow(), ["home"], "initial frame should paint only the home page");

            // Navigate home -> docs, then draw the very next frame.
            updater.set_state(|s| s.route = 1);
            router.rebuild_if_dirty(&ctx);
            drawn.borrow_mut().clear();
            router.draw(&ctx);

            let painted = drawn.borrow().clone();
            assert!(
                painted.contains(&"home") && painted.contains(&"docs"),
                "a cross-fade must paint BOTH the outgoing (home) and incoming (docs) pages on the \
                 first frame after navigation; instead only {painted:?} was painted (instant switch)"
            );
        }
    }
}
