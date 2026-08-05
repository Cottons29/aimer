//! The widget layer over the recognizer.
//!
//! [`GestureDetector`] is deliberately thin: it translates
//! [`ElementEvent`] into [`PointerEvent`], calls
//! [`recognize`](crate::gesture::recognize::recognize), and hands the result to
//! [`GestureHandlers`]. All the interesting behaviour lives in
//! [`crate::gesture::recognize`], where it can be tested without a window.

use std::cell::RefCell;
use std::rc::Rc;

use aimer_attribute::CacheBounds;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerEvent;
use aimer_utils::AnimInstant;
use aimer_widget::base::{BuildContext, WindowHandle};
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, EventResult, LayoutElement, PointerKey,
    Rebuildable, RequiredChild, VisitorElement, Widget,
};

use crate::callback::VoidCallback;
use crate::gesture::handlers::{AsyncSpawner, GestureHandlers};
use crate::gesture::recognize::{poll, recognize};
use crate::gesture::state::GestureState;
use crate::gesture::{
    DragCallback, DragUpdateCallback, GestureStreamCallback, ScaleCallback, ScrollCallback,
    SwipeCallback,
};

/// A transparent widget that recognizes pointer gestures over its child.
///
/// The detector paints nothing and adopts its child's layout; finish
/// construction with [`GestureDetector::child`] or
/// [`GestureDetector::box_child`]. Scroll events are consumed only when
/// [`GestureDetector::on_scroll`] is configured, so a detector that does not
/// handle scrolling lets it reach whatever is below.
///
/// Every handler lives behind one [`Rc`], so rebuilding the widget costs a single
/// refcount bump however many gestures are configured — which matters when a
/// thousand-row list rebuilds its detectors every frame.
///
/// # Examples
///
/// The common gestures have one-line setters:
///
/// ```
/// use aimer_input::gesture::gesture_detector::GestureDetector;
/// use aimer_text::Text;
///
/// let detector = GestureDetector::new()
///     .on_tap(|| println!("tap"))
///     .child(Text::new("Tap me"));
/// ```
///
/// Anything the setters do not cover — press feedback, the long-press
/// lifecycle, drag cancellation, a middle click — is reached through
/// [`GestureDetector::on_gesture`], which sees the whole stream:
///
/// ```
/// use aimer_events::pointer::PointerButton;
/// use aimer_input::gesture::GestureEvent;
/// use aimer_input::gesture::gesture_detector::GestureDetector;
/// use aimer_text::Text;
///
/// let detector = GestureDetector::new()
///     .on_gesture(|event: GestureEvent| match event {
///         GestureEvent::TapDown { .. } => println!("pressed"),
///         GestureEvent::TapCancel => println!("released without activating"),
///         GestureEvent::Tap { pointer } if pointer.button == PointerButton::Middle => {
///             println!("middle click")
///         }
///         _ => {}
///     })
///     .child(Text::new("Save"));
/// ```
pub struct GestureDetector<W = RequiredChild> {
    handlers: Rc<GestureHandlers>,
    child: W,
}

impl Default for GestureDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureDetector {
    /// Creates a detector with no handlers and a required-child placeholder.
    #[inline]
    pub fn new() -> Self {
        Self {
            handlers: Rc::new(GestureHandlers::new()),
            child: RequiredChild,
        }
    }
}

/// Generates the builder setters, each one delegating to the matching
/// mask-maintaining setter on [`GestureHandlers`].
macro_rules! builder_setters {
    ($($setter:ident => $install:ident : $callback:ty, $doc:expr;)*) => {
        impl<W> GestureDetector<W> {
            $(
                #[doc = $doc]
                #[inline]
                pub fn $setter(mut self, callback: impl Into<$callback>) -> Self {
                    self.handlers_mut().$install(callback);
                    self
                }
            )*
        }
    };
}

builder_setters! {
    on_tap => set_on_tap: VoidCallback,
        "Sets the handler for a primary tap completed within the pointer's slop.\n\nA secondary click goes to [`Self::on_right_tap`] instead, and a middle or extra button reaches only [`Self::on_gesture`].";
    on_double_press => set_on_double_press: VoidCallback,
        "Sets the handler fired after two qualifying taps within the double-tap timeout.";
    on_long_press => set_on_long_press: VoidCallback,
        "Sets the handler fired once a held pointer reaches the long-press duration.\n\nFires while the pointer is still down, provided something redraws the frame while it is held; see [`Self::on_gesture`] for the surrounding start/move/end events.";
    on_drag_start => set_on_drag_start: DragCallback,
        "Sets the handler fired when movement first exceeds the pointer's slop.\n\nThe handler receives the position the press *started* at, so a dragged widget does not jump by the slop distance as it picks up.";
    on_drag_update => set_on_drag_update: DragUpdateCallback,
        "Sets the handler fired for movement while a drag is active.\n\n[`crate::gesture::DragUpdateData`] reports the current position and the delta since the previous update.";
    on_drag_end => set_on_drag_end: VoidCallback,
        "Sets the handler fired when an active drag ends.\n\nA flick fires this *and* [`Self::on_swipe`], in that order.";
    on_right_tap => set_on_right_tap: VoidCallback,
        "Sets the handler for a completed secondary-button tap.";
    on_swipe => set_on_swipe: SwipeCallback,
        "Sets the handler for a fast directional drag recognized as a swipe.\n\nThe handler receives the resulting [`crate::gesture::SwipeDirection`].";
    on_scroll => set_on_scroll: ScrollCallback,
        "Sets the handler for mouse-wheel or trackpad scrolling over the child.\n\nInstalling this causes the detector to consume matching scroll events; without it, those events fall through to lower layers.";
    on_scale => set_on_scale: ScaleCallback,
        "Sets the handler for a two-pointer pinch.\n\n[`crate::gesture::ScaleData`] reports the scale relative to the initial pointer distance.";
    on_gesture => set_on_gesture: GestureStreamCallback,
        "Sets the handler that receives every recognized gesture.\n\nThe escape hatch: a gesture with no setter of its own — the press lifecycle, the long-press start/move/end triple, drag cancellation, the pinch boundaries, a middle click — is read here. A new gesture is therefore a new [`crate::gesture::GestureEvent`] variant, not a new field on this struct.";
}

impl<W> GestureDetector<W> {
    /// The handler set, uniquely owned while the builder is still being
    /// configured.
    ///
    /// Mutating through the `Rc` in place is what keeps configuration free: the
    /// shared handle only ever gains a second owner in
    /// [`Widget::to_element`], which happens after the builder is finished.
    #[inline]
    fn handlers_mut(&mut self) -> &mut GestureHandlers {
        Rc::get_mut(&mut self.handlers)
            .expect("gesture handlers are uniquely owned while the detector is being built")
    }

    /// Supplies the terminal child and returns a statically typed detector.
    ///
    /// Existing handlers are preserved. A detector without a child is only an
    /// intermediate builder and does not implement [`Widget`].
    #[inline]
    pub fn child<C: Widget>(self, child: C) -> GestureDetector<C> {
        GestureDetector {
            handlers: self.handlers,
            child,
        }
    }

    /// Supplies the terminal child and erases the completed detector's concrete
    /// type.
    ///
    /// Exactly equivalent to `self.child(child).boxed()`. Use it when branching
    /// APIs need one [`AnyWidget`] return type despite using different concrete
    /// child types.
    #[inline]
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl<W: Widget + 'static> Widget for GestureDetector<W> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        RawGestureDetector {
            child: self.child.to_element(ctx),
            cached_bounds: CacheBounds::new(),
            window: ctx.window.clone(),
            // One refcount bump, whatever the detector was configured with.
            handlers: self.handlers.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            spawner: Some(ctx.async_handle.clone()),
            #[cfg(target_arch = "wasm32")]
            spawner: (),
            state: RefCell::new(GestureState::default()),
        }
        .boxed()
    }
}

/// The element a [`GestureDetector`] builds: a pure recognizer wrapped around a
/// child.
///
/// It renders nothing of its own — pressed overlays and hover effects belong to
/// higher-level widgets such as [`crate::button::Button`] — and mirrors Flutter's
/// `GestureDetector` in that respect.
pub struct RawGestureDetector<E: Element> {
    pub child: E,
    pub(crate) cached_bounds: CacheBounds,
    pub(crate) window: WindowHandle,
    pub(crate) handlers: Rc<GestureHandlers>,
    pub(crate) spawner: AsyncSpawner,
    /// Interior mutability because `on_event` takes `&self`.
    pub(crate) state: RefCell<GestureState>,
}

impl<E: Element> RawGestureDetector<E> {
    /// Runs the recognizer over one pointer event and delivers what it found.
    fn process(&self, event: &PointerEvent) {
        let output = {
            let mut state = self.state.borrow_mut();
            recognize(
                &mut state,
                event,
                AnimInstant::now(),
                self.handlers.mask(),
            )
        };
        self.handlers.dispatch_all(output, &self.spawner);
    }

    /// Gives the recognizer a chance to report a long press while the pointer is
    /// still held.
    ///
    /// Called from [`Drawable::draw`], because a frame is the only regular tick
    /// available: the windowing layer can be asked to redraw but not to redraw
    /// *later*. A press held in an otherwise completely static frame therefore
    /// reports its long press on release instead, which the recognizer handles as
    /// a late long press rather than mistaking it for a slow tap.
    fn poll_held_gestures(&self) {
        let output = {
            let mut state = self.state.borrow_mut();
            if state.press.is_none() {
                return;
            }
            poll(&mut state, AnimInstant::now(), self.handlers.mask())
        };

        if !output.is_empty() {
            self.handlers.dispatch_all(output, &self.spawner);
            self.window.request_redraw();
        }
    }
}

/// The pointer event an [`ElementEvent`] corresponds to, if any.
fn to_pointer_event(event: &ElementEvent) -> Option<PointerEvent> {
    match event {
        ElementEvent::PointerDown(pointer) => Some(PointerEvent::Down(*pointer)),
        ElementEvent::PointerUp(pointer) => Some(PointerEvent::Up(*pointer)),
        ElementEvent::PointerMove(pointer) => Some(PointerEvent::Move(*pointer)),
        ElementEvent::Scroll { delta, .. } => Some(PointerEvent::Scroll {
            delta_x: delta.x,
            delta_y: delta.y,
        }),
        ElementEvent::Cancel => Some(PointerEvent::Cancel),
        _ => None,
    }
}

/// Whether a pointer event is this detector's business.
///
/// Inside the bounds, always. Outside them, only a release for a pointer this
/// detector is already tracking: the finger that pressed inside owns the gesture
/// until it lifts, wherever it lifts.
fn should_accept_pointer_event(
    cached_bounds: &CacheBounds,
    state: &GestureState,
    event: &ElementEvent,
    pos: Vec2d,
) -> bool {
    if cached_bounds.is_inside(pos.x, pos.y) {
        return true;
    }

    match event {
        ElementEvent::PointerUp(pointer) => state.has_active_touch(pointer.source, pointer.id),
        _ => false,
    }
}

/// Claims the pointer on press and gives it back on release, so the gesture
/// keeps receiving events even after it leaves the detector's bounds.
fn pointer_capture_effect(result: EventResult, event: &ElementEvent) -> EventResult {
    match event {
        ElementEvent::PointerDown(pointer) => {
            result.with_pointer_capture(PointerKey::new(pointer.source, pointer.id))
        }
        ElementEvent::PointerUp(pointer) => {
            result.with_pointer_release(PointerKey::new(pointer.source, pointer.id))
        }
        _ => result,
    }
}

// ── Element trait impls ─────────────────────────────────────────────────

impl<E: Element> VisitorElement for RawGestureDetector<E> {
    fn debug_name(&self) -> &'static str {
        "GestureDetector"
    }
}

impl<E: Element> EventElement for RawGestureDetector<E> {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        if matches!(event, ElementEvent::Cancel) {
            self.process(&PointerEvent::Cancel);
            self.window.request_redraw();
            return EventResult::consumed().with_redraw();
        }

        if let ElementEvent::Scroll { delta, .. } = event {
            // A scroll carries no position, and a `MouseRegion` wrapper (which
            // has no bounds of its own) forwards every event here regardless of
            // where the cursor is. Consuming one unconditionally meant a
            // handler-less detector on a top `Stack` layer swallowed every wheel
            // and trackpad scroll before a `Scrollable` on a lower layer could
            // see it, and scrolling appeared completely dead.
            if !self.handlers.consumes_scroll() {
                return EventResult::ignored();
            }
            self.process(&PointerEvent::Scroll {
                delta_x: delta.x,
                delta_y: delta.y,
            });
            self.window.request_redraw();
            return EventResult::consumed().with_redraw();
        }

        let Some(pointer) = event.pointer() else {
            return EventResult::ignored();
        };

        if !should_accept_pointer_event(
            &self.cached_bounds,
            &self.state.borrow(),
            event,
            pointer.pos,
        ) {
            return EventResult::ignored();
        }

        let Some(pointer_event) = to_pointer_event(event) else {
            return EventResult::ignored();
        };

        self.process(&pointer_event);
        self.window.request_redraw();
        pointer_capture_effect(EventResult::consumed().with_redraw(), event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }
}

impl<E: Element> LayoutElement for RawGestureDetector<E> {
    #[inline]
    fn size(&self) -> Option<Size> {
        None
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        let size = self.child.layout(ctx);
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        self.cached_bounds
            .save(ctx.scale, abs_x, abs_y, size.width, size.height);
        size
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.cached_bounds.pos_start_end()
    }
}

impl<E: Element> Drawable for RawGestureDetector<E> {
    fn draw(&self, ctx: &BuildContext<'_>) {
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        let child_size = self.child.computed_size(ctx);
        self.cached_bounds
            .save(ctx.scale, abs_x, abs_y, child_size.width, child_size.height);

        self.poll_held_gestures();
        self.child.draw(ctx);
    }
}

impl<E: Element + 'static> Rebuildable for RawGestureDetector<E> {
    #[inline]
    fn option_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// Claims the gesture the element being replaced was in the middle of.
    ///
    /// A gesture outlives the element that started it. `TapDown` fires while the
    /// pointer is still down, so anything that reacts to a press — a
    /// [`crate::button::Button`] darkening itself, a row highlighting — calls
    /// `set_state`, and that rebuild replaces every element below it, this one
    /// included. A replacement starting from nothing would find no press when
    /// the release arrived and report no tap at all: the button would light up
    /// under the finger and then do nothing.
    ///
    /// So the recognizer's state is *moved* out of `old`, which reconciliation
    /// drops immediately afterwards, leaving exactly one element tracking the
    /// gesture.
    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        let Some(old) = old
            .option_any()
            .and_then(|value| value.downcast_ref::<Self>())
        else {
            return;
        };

        *self.state.borrow_mut() = std::mem::take(&mut *old.state.borrow_mut());
    }
}

#[cfg(test)]
mod tests {
    use aimer_events::pointer::{PointerButton, PointerInfo, PointerSource};
    use aimer_widget::CaptureRequest;

    use super::*;
    use crate::gesture::state::Press;
    use crate::gesture::{GestureMask, ScaleData, ScrollData, SwipeDirection};

    struct TestWidget;

    impl Widget for TestWidget {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            panic!("not needed for builder tests")
        }
    }

    struct TestElement;

    impl VisitorElement for TestElement {
        fn debug_name(&self) -> &'static str {
            "TestElement"
        }
    }

    impl EventElement for TestElement {}
    impl LayoutElement for TestElement {}
    impl Drawable for TestElement {
        fn draw(&self, _ctx: &BuildContext<'_>) {}
    }
    impl Rebuildable for TestElement {}

    /// A laid-out detector counting the taps it recognizes, so two of them can
    /// share a counter across a simulated rebuild.
    fn counting_detector(taps: Rc<std::cell::Cell<usize>>) -> RawGestureDetector<TestElement> {
        let mut handlers = GestureHandlers::new();
        handlers.set_on_tap(move || taps.set(taps.get() + 1));

        let cached_bounds = CacheBounds::new();
        cached_bounds.save(1.0, 0.0, 0.0, 100.0, 100.0);

        RawGestureDetector {
            child: TestElement,
            cached_bounds,
            window: WindowHandle::headless(winit::dpi::PhysicalSize::new(100, 100), 1.0),
            handlers: Rc::new(handlers),
            #[cfg(not(target_arch = "wasm32"))]
            spawner: None,
            #[cfg(target_arch = "wasm32")]
            spawner: (),
            state: RefCell::new(GestureState::default()),
        }
    }

    fn touch(x: f32, y: f32, id: u64) -> PointerInfo {
        PointerInfo::touch(Vec2d { x, y }, id)
    }

    fn pressing(pointer: PointerInfo) -> GestureState {
        let mut state = GestureState::default();
        state.touches.insert(pointer);
        state.press = Some(Press {
            pointer,
            down_at: AnimInstant::now(),
            long_pressed: false,
            long_press_last: None,
        });
        state
    }

    #[test]
    fn the_builder_configures_every_gesture_before_a_child_is_added() {
        let detector = GestureDetector::new()
            .on_tap(|| {})
            .on_double_press(|| {})
            .on_long_press(|| {})
            .on_drag_start(|_: PointerInfo| {})
            .on_drag_update(|_: crate::gesture::DragUpdateData| {})
            .on_drag_end(|| {})
            .on_right_tap(|| {})
            .on_swipe(|_: SwipeDirection| {})
            .on_scroll(|_: ScrollData| {})
            .on_scale(|_: ScaleData| {})
            .child(TestWidget);

        let mask = detector.handlers.mask();

        assert!(mask.contains(GestureMask::TAP));
        assert!(mask.contains(GestureMask::DOUBLE_TAP));
        assert!(mask.contains(GestureMask::LONG_PRESS));
        assert!(mask.contains(GestureMask::DRAG));
        assert!(mask.contains(GestureMask::SWIPE));
        assert!(mask.contains(GestureMask::SCALE));
        assert!(detector.handlers.consumes_scroll());
    }

    // Regression for "the Scroll is not able to scroll with mouse wheel or
    // trackpad": a detector with no `on_scroll` handler — a header `TextButton`
    // is a MouseRegion plus a GestureDetector — must not consume a scroll,
    // otherwise, sitting on a top `Stack` layer and dispatched first, it
    // swallows every wheel and trackpad scroll before a `Scrollable` on a lower
    // layer can see it, and nothing scrolls.
    #[test]
    fn a_detector_without_a_scroll_handler_lets_the_scroll_fall_through() {
        let detector = GestureDetector::new().on_tap(|| {}).child(TestWidget);

        assert!(
            !detector.handlers.consumes_scroll(),
            "a handler-less detector must let the scroll propagate to lower layers"
        );
    }

    #[test]
    fn a_detector_with_a_scroll_handler_consumes_the_scroll() {
        let detector = GestureDetector::new()
            .on_scroll(|_: ScrollData| {})
            .child(TestWidget);

        assert!(detector.handlers.consumes_scroll());
    }

    // The raw handler is the escape hatch for gestures with no setter, so it has
    // to ask the recognizer for everything.
    #[test]
    fn a_raw_gesture_handler_asks_for_every_gesture() {
        let detector = GestureDetector::new()
            .on_gesture(|_: crate::gesture::GestureEvent| {})
            .child(TestWidget);

        assert!(detector.handlers.mask().contains(GestureMask::SCALE));
        assert!(detector.handlers.mask().contains(GestureMask::LONG_PRESS));
    }

    #[test]
    fn adding_a_child_keeps_the_handlers_that_were_already_configured() {
        let detector = GestureDetector::new().on_scale(|_: ScaleData| {});
        let mask_before = detector.handlers.mask();

        let with_child = detector.child(TestWidget);

        assert_eq!(with_child.handlers.mask(), mask_before);
    }

    #[test]
    fn every_pointer_element_event_maps_to_a_pointer_event() {
        let pointer = touch(1.0, 2.0, 3);

        assert!(matches!(
            to_pointer_event(&ElementEvent::PointerDown(pointer)),
            Some(PointerEvent::Down(_))
        ));
        assert!(matches!(
            to_pointer_event(&ElementEvent::PointerUp(pointer)),
            Some(PointerEvent::Up(_))
        ));
        assert!(matches!(
            to_pointer_event(&ElementEvent::PointerMove(pointer)),
            Some(PointerEvent::Move(_))
        ));
        assert!(matches!(
            to_pointer_event(&ElementEvent::Cancel),
            Some(PointerEvent::Cancel)
        ));
        assert!(
            to_pointer_event(&ElementEvent::PointerExited(PointerSource::Mouse, 0)).is_none(),
            "leaving the window is not a gesture"
        );
    }

    // The translated pointer must keep its button, or the recognizer is back to
    // treating every press as primary.
    #[test]
    fn translation_preserves_the_button_the_press_was_made_with() {
        let secondary = PointerInfo::mouse(Vec2d { x: 5.0, y: 5.0 }, PointerButton::Secondary);

        let translated = to_pointer_event(&ElementEvent::PointerDown(secondary));

        assert!(matches!(
            translated,
            Some(PointerEvent::Down(pointer)) if pointer.button == PointerButton::Secondary
        ));
    }

    #[test]
    fn a_press_inside_the_cached_bounds_is_accepted() {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 10.0, 20.0, 100.0, 50.0);
        let pointer = touch(25.0, 35.0, 7);

        assert!(should_accept_pointer_event(
            &bounds,
            &GestureState::default(),
            &ElementEvent::PointerDown(pointer),
            pointer.pos
        ));
    }

    #[test]
    fn a_press_outside_the_cached_bounds_is_rejected() {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 10.0, 20.0, 100.0, 50.0);
        let pointer = touch(200.0, 35.0, 7);

        assert!(!should_accept_pointer_event(
            &bounds,
            &GestureState::default(),
            &ElementEvent::PointerDown(pointer),
            pointer.pos
        ));
    }

    #[test]
    fn a_release_outside_the_bounds_is_accepted_for_a_pointer_being_tracked() {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 10.0, 20.0, 100.0, 50.0);
        let state = pressing(touch(25.0, 35.0, 7));
        let outside = touch(115.0, 35.0, 7);

        assert!(should_accept_pointer_event(
            &bounds,
            &state,
            &ElementEvent::PointerUp(outside),
            outside.pos
        ));

        let other_device = PointerInfo::mouse(outside.pos, PointerButton::Primary);

        assert!(
            !should_accept_pointer_event(
                &bounds,
                &state,
                &ElementEvent::PointerUp(other_device),
                other_device.pos
            ),
            "a mouse release is not the tracked finger, however matching the id"
        );
    }

    #[test]
    fn an_accepted_pointer_sequence_captures_then_releases() {
        let pointer = touch(5.0, 5.0, 7);
        let key = PointerKey::new(pointer.source, pointer.id);

        let down = pointer_capture_effect(
            EventResult::consumed(),
            &ElementEvent::PointerDown(pointer),
        );
        let up =
            pointer_capture_effect(EventResult::consumed(), &ElementEvent::PointerUp(pointer));

        assert_eq!(down.capture_request(), CaptureRequest::Capture(key));
        assert_eq!(up.capture_request(), CaptureRequest::Release(key));
    }

    // Regression for "the theme toggle and the platform buttons stopped
    // working": `Button` reacts to `TapDown` by setting its pressed state, and
    // that rebuild replaces this element while the finger is still down. Unless
    // the replacement takes the press over, the release finds no gesture in
    // progress and no tap is ever reported — the button darkens and then does
    // nothing at all.
    #[test]
    fn a_rebuild_between_the_press_and_the_release_still_reports_the_tap() {
        let taps = Rc::new(std::cell::Cell::new(0));
        let pressed = counting_detector(taps.clone());
        let pointer = PointerInfo::mouse(Vec2d { x: 10.0, y: 10.0 }, PointerButton::Primary);

        let _ = pressed.on_event(&ElementEvent::PointerDown(pointer));

        let rebuilt = counting_detector(taps.clone());
        rebuilt.adopt_runtime_state_from(&pressed as &dyn Element);

        let _ = rebuilt.on_event(&ElementEvent::PointerUp(pointer));

        assert_eq!(
            taps.get(),
            1,
            "a rebuild triggered by the press must not swallow the tap"
        );
    }

    // The state is moved, not copied: two elements both believing they own the
    // press would report the tap twice.
    #[test]
    fn the_element_being_replaced_gives_the_press_up_rather_than_sharing_it() {
        let taps = Rc::new(std::cell::Cell::new(0));
        let pressed = counting_detector(taps.clone());
        let pointer = PointerInfo::mouse(Vec2d { x: 10.0, y: 10.0 }, PointerButton::Primary);

        let _ = pressed.on_event(&ElementEvent::PointerDown(pointer));

        let rebuilt = counting_detector(taps.clone());
        rebuilt.adopt_runtime_state_from(&pressed as &dyn Element);

        let _ = pressed.on_event(&ElementEvent::PointerUp(pointer));

        assert_eq!(taps.get(), 0, "the old element no longer owns the gesture");
    }

    #[test]
    fn a_move_neither_captures_nor_releases_the_pointer() {
        let pointer = touch(5.0, 5.0, 7);

        let moved = pointer_capture_effect(
            EventResult::consumed(),
            &ElementEvent::PointerMove(pointer),
        );

        assert_eq!(moved.capture_request(), CaptureRequest::None);
    }
}
