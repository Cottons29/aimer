use std::cell::{Cell, RefCell};
use std::rc::Rc;

use aimer_attribute::CacheBounds;
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;
use aimer_events::window::request_animation_frame;
use aimer_macro::Rebuildable;
use aimer_widget::base::*;
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventDispatcher, EventElement, EventResult,
    LayoutElement, PointerKey, RequiredChild, VisitorElement, Widget,
};

use crate::callback::{CallbackExecutor, RawInnerCallback, VoidCallback};

/// Whether a mouse pointer is currently inside a [`MouseRegion`].
#[derive(Debug, Copy, Clone, Default)]
pub enum PointerState {
    /// The pointer is within the region's laid-out bounds.
    Inside,
    /// The pointer is outside the region's laid-out bounds.
    #[default]
    Outside,
}

/// A shared pointer state.
///
/// This is a type alias of `Rc<Cell<PointerState>>` and can be passed to
/// [`MouseRegion::current_state`] so other code can observe transitions.
pub type SharedPointerState = Rc<Cell<PointerState>>;

/// A transparent widget that tracks mouse hover over its child.
///
/// Touch events do not change hover state. Enter and exit callbacks run only on
/// actual state transitions; asynchronous callbacks are currently ignored. The
/// default cursor is unchanged and the initial state is
/// [`PointerState::Outside`].
///
/// # Example
///
/// ```
/// use aimer_input::mouse_region::MouseRegion;
/// use aimer_text::Text;
///
/// let region = MouseRegion::new().cursor(winit::window::CursorIcon::Pointer)
///                                .on_hover_enter(|| println!("entered"))
///                                .child(Text::new("Hover me"));
/// ```
pub struct MouseRegion<W = RequiredChild> {
    pub on_hover_enter: VoidCallback,
    pub on_hover_exit: VoidCallback,
    pub cursor: Option<winit::window::CursorIcon>,
    pub current_state: SharedPointerState,
    pub cached_bounds: CacheBounds,
    pub child: W,
}

impl Default for MouseRegion {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseRegion {
    /// Creates a region with no-op callbacks, no cursor override, and outside
    /// pointer state.
    pub fn new() -> Self {
        Self {
            on_hover_enter: VoidCallback::default(),
            on_hover_exit: VoidCallback::default(),
            cursor: None,
            current_state: Rc::new(Cell::new(PointerState::Outside)),
            cached_bounds: CacheBounds::new(),
            child: RequiredChild,
        }
    }
}

impl<W> MouseRegion<W> {
    /// Sets the callback fired when the mouse transitions from outside to
    /// inside the child bounds.
    pub fn on_hover_enter(mut self, on_hover_enter: impl Into<VoidCallback>) -> Self {
        self.on_hover_enter = on_hover_enter.into();
        self
    }

    /// Sets the callback fired when the mouse transitions from inside to
    /// outside the child bounds.
    pub fn on_hover_exit(mut self, on_hover_exit: impl Into<VoidCallback>) -> Self {
        self.on_hover_exit = on_hover_exit.into();
        self
    }

    /// Sets the cursor shown while the mouse is inside the region.
    ///
    /// Pass `None` to leave the cursor unchanged. On exit, a configured cursor
    /// resets to the platform default.
    pub fn cursor(mut self, cursor: impl Into<Option<winit::window::CursorIcon>>) -> Self {
        self.cursor = cursor.into();
        self
    }

    /// Replaces the shared cell in which hover transitions are recorded.
    pub fn current_state(mut self, current_state: SharedPointerState) -> Self {
        self.current_state = current_state;
        self
    }

    /// Supplies the terminal child and returns a statically typed region.
    ///
    /// Existing callbacks, cursor, and shared state are preserved. A region
    /// without a child is only an intermediate builder and does not
    /// implement [`Widget`].
    pub fn child<C: Widget>(self, child: C) -> MouseRegion<C> {
        MouseRegion {
            on_hover_enter: self.on_hover_enter,
            on_hover_exit: self.on_hover_exit,
            cursor: self.cursor,
            current_state: self.current_state,
            cached_bounds: self.cached_bounds,
            child,
        }
    }

    /// Supplies the terminal child and erases the completed region's concrete
    /// type.
    ///
    /// This is exactly equivalent to `self.child(child).boxed()`, combining
    /// [`MouseRegion::child`] with [`Widget::boxed`]. Use it when branching
    /// APIs need one [`AnyWidget`] return type despite using different
    /// concrete child types.
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl<W: Widget + 'static> Widget for MouseRegion<W> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        RawMouseRegion {
            on_hover_enter: self.on_hover_enter.clone(),
            on_hover_exit: self.on_hover_exit.clone(),
            cursor: self.cursor,
            current_state: self.current_state.clone(),
            cached_bounds: self.cached_bounds.clone(),
            window: ctx.window.clone(),
            child,
            event_dispatcher: RefCell::new(EventDispatcher::new()),
        }
        .boxed()
    }
}

/// ##### A transparent wrapper that tracks the mouse hover state.
///
/// `MouseRegion` only responds to mouse-originated pointer events — touch
/// input is ignored for hover purposes. It writes to a shared
/// `Rc<Cell<PointerState>>` so that a child element (e.g. `GestureDetector`)
/// can read the hover state for decoration switching without knowing about
/// `MouseRegion` at all.
///
/// Event dispatch is handled manually: `event_children` returns empty so that
/// `on_event` is called first, then events are forwarded to the child.
#[derive(Rebuildable)]
pub struct RawMouseRegion<E: Element> {
    pub(crate) on_hover_enter: VoidCallback,
    pub(crate) on_hover_exit: VoidCallback,
    pub(crate) cursor: Option<winit::window::CursorIcon>,
    pub(crate) current_state: Rc<Cell<PointerState>>,
    pub(crate) cached_bounds: CacheBounds,
    pub(crate) child: E,
    pub(crate) window: WindowHandle,
    pub(crate) event_dispatcher: RefCell<EventDispatcher>,
}

impl<E: Element> RawMouseRegion<E> {
    #[inline ]
    fn execute_void_callback(cb: &VoidCallback) {
        if let Some(callback) = (*cb.get()).as_ref() {
            match callback {
                RawInnerCallback::Empty => (),
                RawInnerCallback::Sync(f) => f(()),
                RawInnerCallback::Async(_) => {
                    // MouseRegion doesn't own a runtime handle.
                    // Async hover callbacks are not supported.
                }
            }
        }
    }

    /// Reconcile the stored hover state with `is_inside`, firing the
    /// enter/exit callbacks only on an actual transition and requesting a
    /// redraw so the decoration can update.
    ///
    /// This is shared by `on_event` (driven by pointer events) and `draw`
    /// (driven by the last-known cursor position). Evaluating it in `draw`
    /// is what keeps the hover state alive across rebuilds — e.g. after a
    /// click triggers a parent `set_state`, the region is rebuilt with a
    /// fresh `Outside` state and, without a new pointer event, would
    /// otherwise stay un-hovered until the mouse moved again.
    #[inline ]
    fn sync_hover(&self, is_inside: bool) {
        if is_inside {
            if matches!(self.current_state.get(), PointerState::Outside) {
                Self::execute_void_callback(&self.on_hover_enter);
                self.current_state.set(PointerState::Inside);
            }
        } else if matches!(self.current_state.get(), PointerState::Inside) {
            Self::execute_void_callback(&self.on_hover_exit);
            self.current_state.set(PointerState::Outside);
            request_animation_frame()
        }
    }
}

impl<E: Element> VisitorElement for RawMouseRegion<E> {
    fn visit_children<'b>(&'b self, visitor: &mut dyn FnMut(&'b dyn Element)) {
        visitor(&self.child);
    }

    fn debug_name(&self) -> &'static str {
        "MouseRegion"
    }
}

impl<E: Element> EventElement for RawMouseRegion<E> {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        let pointer = match event {
            ElementEvent::PointerDown(_, source, id)
            | ElementEvent::PointerUp(_, source, id)
            | ElementEvent::PointerMove(_, source, id)
            | ElementEvent::PointerExited(source, id) => Some(PointerKey::new(*source, *id)),
            _ => None,
        };
        let was_captured = pointer
            .is_some_and(|pointer| self.event_dispatcher.borrow().is_captured(pointer));

        if matches!(event, ElementEvent::PointerExited(PointerSource::Mouse, _)) {
            if self.cursor.is_some() {
                self.window.set_cursor(winit::window::CursorIcon::Default);
            }
            self.sync_hover(false);
        }

        let pos = match event {
            ElementEvent::PointerDown(p, _, _)
            | ElementEvent::PointerUp(p, _, _)
            | ElementEvent::PointerMove(p, _, _) => *p,
            ElementEvent::PointerExited(_, _) | ElementEvent::Cancel => Vec2d::default(),
            _ => return EventResult::ignored(),
        };

        // println!("Event received: {:?}", event);

        let is_inside = self.cached_bounds.is_inside(pos.x, pos.y);

        // Update the cursor icon on every mouse event while over the region.
        let is_mouse = matches!(pointer, Some(PointerKey { source: PointerSource::Mouse, .. }));
        if is_inside && is_mouse {
            if let Some(icon) = self.cursor {
                self.window.set_cursor(icon);
            }
        } else if is_mouse && self.cursor.is_some() {
            self.window.set_cursor(winit::window::CursorIcon::Default);
        }

        if is_mouse {
            self.sync_hover(is_inside);
        }
        if !is_inside && !was_captured && !matches!(event, ElementEvent::Cancel) {
            return EventResult::ignored();
        }
        let result = self
            .event_dispatcher
            .borrow_mut()
            .dispatch(&self.child, pos, event);
        let is_captured = pointer
            .is_some_and(|pointer| self.event_dispatcher.borrow().is_captured(pointer));
        let result = if result.is_consumed() {
            result.with_redraw()
        } else {
            result
        };
        match (pointer, was_captured, is_captured) {
            (Some(pointer), false, true) => result.with_pointer_capture(pointer),
            (Some(pointer), true, false) => result.with_pointer_release(pointer),
            _ => result,
        }
    }
    fn event_children<'b>(&'b self, _visitor: &mut dyn FnMut(&'b dyn Element)) {}
}

impl<E: Element> LayoutElement for RawMouseRegion<E> {
    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        let size = self.child.layout(ctx);
        // Cache our own bounds from the canvas transform for hit-testing
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        self.cached_bounds
            .save(ctx.scale, abs_x, abs_y, size.width, size.height);
        size
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }
}

impl<E: Element> Drawable for RawMouseRegion<E> {
    fn draw(&self, ctx: &BuildContext<'_>) {
        // Update cached bounds from the current canvas position
        let child_size = self.child.computed_size(ctx);
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        self.cached_bounds
            .save(ctx.scale, abs_x, abs_y, child_size.width, child_size.height);

        let cursor = ctx.cursor_pos;
        let is_inside = self.cached_bounds.is_inside(cursor.x, cursor.y);
        self.sync_hover(is_inside);

        self.child.draw(ctx);
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::cell::RefCell;

    use aimer_widget::{CaptureRequest, EventResult, PointerKey, Rebuildable};
    use aimer_widget::base::WindowHandle;
    use winit::dpi::PhysicalSize;

    use super::*;

    struct TestElement;

    struct TestWidget;

    impl Widget for TestWidget {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            panic!("not needed for builder tests")
        }
    }

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
    impl Rebuildable for TestElement {
        fn option_any(&self) -> Option<&dyn Any> {
            Some(self)
        }
    }

    struct ResultElement;

    impl VisitorElement for ResultElement {
        fn debug_name(&self) -> &'static str {
            "ResultElement"
        }
    }

    impl EventElement for ResultElement {
        fn on_event(&self, _event: &ElementEvent) -> EventResult {
            EventResult::consumed()
        }
    }

    impl LayoutElement for ResultElement {}
    impl Drawable for ResultElement {
        fn draw(&self, _ctx: &BuildContext<'_>) {}
    }
    impl Rebuildable for ResultElement {}

    struct CapturingElement {
        events: Rc<Cell<usize>>,
    }

    impl VisitorElement for CapturingElement {
        fn debug_name(&self) -> &'static str {
            "CapturingElement"
        }
    }

    impl EventElement for CapturingElement {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            self.events.set(self.events.get() + 1);
            match event {
                ElementEvent::PointerDown(_, source, id) => EventResult::consumed()
                    .with_pointer_capture(PointerKey::new(*source, *id)),
                ElementEvent::PointerUp(_, source, id) => EventResult::consumed()
                    .with_pointer_release(PointerKey::new(*source, *id)),
                _ => EventResult::consumed(),
            }
        }
    }

    impl LayoutElement for CapturingElement {}
    impl Drawable for CapturingElement {
        fn draw(&self, _ctx: &BuildContext<'_>) {}
    }
    impl Rebuildable for CapturingElement {}

    #[test]
    fn builder_configures_mouse_region_before_child_is_added() {
        let current_state = Rc::new(Cell::new(PointerState::Inside));

        let region = MouseRegion::new()
            .on_hover_enter(|| {})
            .on_hover_exit(|| {})
            .cursor(winit::window::CursorIcon::Pointer)
            .current_state(current_state.clone())
            .child(TestWidget);

        assert_eq!(region.cursor, Some(winit::window::CursorIcon::Pointer));
        assert!(Rc::ptr_eq(&region.current_state, &current_state));
    }

    #[test]
    fn pointer_exit_transitions_hover_state_without_a_synthetic_move() {
        let current_state = Rc::new(Cell::new(PointerState::Inside));
        let region = RawMouseRegion {
            on_hover_enter: VoidCallback::default(),
            on_hover_exit: VoidCallback::default(),
            cursor: None,
            current_state: current_state.clone(),
            cached_bounds: CacheBounds::new(),
            child: TestElement,
            window: WindowHandle::headless(PhysicalSize::new(100, 100), 1.0),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
        };

        let _ = region.on_event(&ElementEvent::PointerExited(PointerSource::Mouse, 0));

        assert!(matches!(current_state.get(), PointerState::Outside));
    }

    #[test]
    fn forwards_the_child_event_result_without_losing_redraw() {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 0.0, 0.0, 100.0, 100.0);
        let region = RawMouseRegion {
            on_hover_enter: VoidCallback::default(),
            on_hover_exit: VoidCallback::default(),
            cursor: None,
            current_state: Rc::new(Cell::new(PointerState::Outside)),
            cached_bounds: bounds,
            child: ResultElement,
            window: WindowHandle::headless(PhysicalSize::new(100, 100), 1.0),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
        };

        let result = region.on_event(&ElementEvent::PointerMove(
            aimer_attribute::position::Vec2d { x: 10.0, y: 10.0 },
            PointerSource::Mouse,
            7,
        ));

        assert!(result.is_consumed());
        assert!(result.needs_redraw());
    }

    #[test]
    fn captured_child_receives_move_and_up_outside_region() {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 0.0, 0.0, 100.0, 100.0);
        let events = Rc::new(Cell::new(0));
        let region = RawMouseRegion {
            on_hover_enter: VoidCallback::default(),
            on_hover_exit: VoidCallback::default(),
            cursor: None,
            current_state: Rc::new(Cell::new(PointerState::Outside)),
            cached_bounds: bounds,
            child: CapturingElement {
                events: events.clone(),
            }
            .boxed(),
            window: WindowHandle::headless(PhysicalSize::new(100, 100), 1.0),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
        };
        let pointer = PointerKey::new(PointerSource::Touch, 2);

        let down = region.on_event(&ElementEvent::PointerDown(
            Vec2d { x: 10.0, y: 10.0 },
            pointer.source,
            pointer.id,
        ));
        assert_eq!(down.capture_request(), CaptureRequest::Capture(pointer));

        let _ = region.on_event(&ElementEvent::PointerMove(
            Vec2d { x: 200.0, y: 200.0 },
            pointer.source,
            pointer.id,
        ));
        let up = region.on_event(&ElementEvent::PointerUp(
            Vec2d { x: 200.0, y: 200.0 },
            pointer.source,
            pointer.id,
        ));

        assert_eq!(events.get(), 3);
        assert_eq!(up.capture_request(), CaptureRequest::Release(pointer));
    }
}
