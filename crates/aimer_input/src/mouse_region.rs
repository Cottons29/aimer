use std::cell::Cell;
use std::rc::Rc;

use aimer_attribute::CacheBounds;
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;
use aimer_events::window::request_animation_frame;
use aimer_macro::PortableWidget;
use aimer_widget::base::*;
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventDispatchContext, EventElement, EventResult,
    LayoutElement, PointerKey, Rebuildable, RequiredChild, VisitorElement, Widget, dispatch_event,
};

use crate::callback::{CallbackExecutor, VoidCallback};

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
#[derive(PortableWidget)]
#[portable_widget(
    id = "aimer_input::MouseRegion",
    schema_only
)]
pub struct MouseRegion<W = RequiredChild> {
    #[portable_callback(async)]
    pub on_hover_enter: VoidCallback,
    #[portable_callback(async)]
    pub on_hover_exit: VoidCallback,
    #[portable_skip]
    pub cursor: Option<winit::window::CursorIcon>,
    #[portable_skip]
    pub current_state: SharedPointerState,
    #[portable_skip]
    pub cached_bounds: CacheBounds,
    #[portable_child(discriminator = 0)]
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
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        RawMouseRegion {
            on_hover_enter: self.on_hover_enter.clone(),
            on_hover_exit: self.on_hover_exit.clone(),
            cursor: self.cursor,
            current_state: self.current_state.clone(),
            cached_bounds: self.cached_bounds.clone(),
            window: ctx.window.clone(),
            child,
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
/// the region's handler runs first, then the child is forwarded through the
/// parent [`aimer_widget::EventDispatchContext`]. This keeps capture and path
/// state shared with the enclosing dispatcher.
pub struct RawMouseRegion<E: Element> {
    pub(crate) on_hover_enter: VoidCallback,
    pub(crate) on_hover_exit: VoidCallback,
    pub(crate) cursor: Option<winit::window::CursorIcon>,
    pub(crate) current_state: Rc<Cell<PointerState>>,
    pub(crate) cached_bounds: CacheBounds,
    pub(crate) child: E,
    pub(crate) window: WindowHandle,
}

impl<E: Element> RawMouseRegion<E> {
    /// Fires one hover callback.
    ///
    /// The region holds no runtime handle of its own, so an async callback goes
    /// to whichever runtime the frame is being built on — which is how it
    /// reaches the application's. Before the spawning policy moved into
    /// [`CallbackExecutor::execute`] this arm did nothing at all, silently.
    #[inline]
    fn execute_void_callback(cb: &VoidCallback) {
        cb.execute(());
    }

    /// Reconcile the stored hover state with `is_inside`, firing the
    /// enter/exit callbacks, updating the cursor icon this region configures,
    /// and requesting a redraw so the decoration can update — all only on an
    /// actual transition, so the moves between two crossings cost nothing.
    ///
    /// Edge-triggering is what the frame budget rests on: pointer moves no
    /// longer buy a repaint each, so the one frame a hover decoration needs is
    /// scheduled here, by the move that crossed the edge. The same goes for
    /// the platform cursor — setting it per move was a syscall per mouse
    /// event; setting it per crossing is two per visit.
    ///
    /// This is shared by `on_event` (driven by pointer events) and `draw`
    /// (driven by the last-known cursor position). Evaluating it in `draw`
    /// is what keeps the hover state alive across rebuilds — e.g. after a
    /// click triggers a parent `set_state`, the region is rebuilt with a
    /// fresh `Outside` state and, without a new pointer event, would
    /// otherwise stay un-hovered until the mouse moved again.
    #[inline]
    fn sync_hover(&self, is_inside: bool) {
        if is_inside {
            if matches!(self.current_state.get(), PointerState::Outside) {
                if let Some(icon) = self.cursor {
                    self.window.set_cursor(icon);
                }
                Self::execute_void_callback(&self.on_hover_enter);
                self.current_state.set(PointerState::Inside);
                request_animation_frame()
            }
        } else if matches!(self.current_state.get(), PointerState::Inside) {
            if self.cursor.is_some() {
                self.window.set_cursor(winit::window::CursorIcon::Default);
            }
            Self::execute_void_callback(&self.on_hover_exit);
            self.current_state.set(PointerState::Outside);
            request_animation_frame()
        }
    }
}

impl<E: Element> RawMouseRegion<E> {
    fn handle_event<'dispatcher, 'tree>(
        &self,
        event: &ElementEvent,
        mut context: Option<&mut EventDispatchContext<'dispatcher, 'tree>>,
    ) -> EventResult {
        let pointer = match event {
            ElementEvent::PointerDown(info)
            | ElementEvent::PointerUp(info)
            | ElementEvent::PointerMove(info) => Some(PointerKey::new(info.source, info.id)),
            ElementEvent::PointerExited(source, id) => Some(PointerKey::new(*source, *id)),
            _ => None,
        };
        let was_captured = pointer.is_some_and(|pointer| {
            context
                .as_ref()
                .is_some_and(|context| context.is_captured(pointer))
        });

        if matches!(event, ElementEvent::PointerExited(PointerSource::Mouse, _)) {
            if self.cursor.is_some() {
                self.window.set_cursor(winit::window::CursorIcon::Default);
            }
            self.sync_hover(false);
        }

        let pos = match event {
            ElementEvent::PointerDown(info)
            | ElementEvent::PointerUp(info)
            | ElementEvent::PointerMove(info) => info.pos,
            ElementEvent::PointerExited(_, _) | ElementEvent::Cancel => Vec2d::default(),
            _ => return EventResult::ignored(),
        };

        let is_inside = self.cached_bounds.is_inside(pos.x, pos.y);

        let is_mouse = matches!(
            pointer,
            Some(PointerKey {
                source: PointerSource::Mouse,
                ..
            })
        );
        if is_mouse {
            self.sync_hover(is_inside);
        }
        if !is_inside && !was_captured && !matches!(event, ElementEvent::Cancel) {
            return EventResult::ignored();
        }
        let result = if let Some(context) = context.as_mut() {
            context.dispatch_child(&self.child, pos, event)
        } else {
            dispatch_event(&self.child, pos, event)
        };
        let is_captured = pointer.is_some_and(|pointer| {
            context
                .as_ref()
                .is_some_and(|context| context.is_captured(pointer))
        });
        let result = match event {
            // A child claiming a hover move is not a child changing pixels:
            // upgrading every consumed move to a redraw repainted the window
            // at input rate while the cursor merely crossed the UI. Whoever
            // changes state on a move schedules its own frame. A region with
            // an icon claims the move itself, because the application
            // restores the default cursor whenever a move goes unconsumed.
            ElementEvent::PointerMove(_) => {
                if is_mouse && is_inside && self.cursor.is_some() {
                    result.merge(EventResult::consumed())
                } else {
                    result
                }
            }
            _ if result.is_consumed() => result.with_redraw(),
            _ => result,
        };
        match (pointer, was_captured, is_captured) {
            (Some(pointer), false, true) => result.with_pointer_capture(pointer),
            (Some(pointer), true, false) => result.with_pointer_release(pointer),
            _ => result,
        }
    }
}

impl<E: Element + 'static> Rebuildable for RawMouseRegion<E> {
    #[inline]
    fn option_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
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
        self.handle_event(event, None)
    }

    fn on_event_with_context(
        &self,
        event: &ElementEvent,
        context: &mut EventDispatchContext<'_, '_>,
    ) -> EventResult {
        self.handle_event(event, Some(context))
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

    use aimer_events::pointer::{PointerButton, PointerInfo};
    use aimer_widget::base::WindowHandle;
    use aimer_widget::{EventDispatcher, EventResult, PointerKey, Rebuildable};
    use winit::dpi::PhysicalSize;

    use super::*;

    struct TestElement;

    struct TestWidget;

    impl Widget for TestWidget {
        fn to_element(self, _ctx: &BuildContext) -> AnyElement {
            panic!("not needed for builder tests")
        }
    }

    impl aimer_widget::PortableWidget for TestWidget {}

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
                ElementEvent::PointerDown(info) => EventResult::consumed()
                    .with_pointer_capture(PointerKey::new(info.source, info.id)),
                ElementEvent::PointerUp(info) => EventResult::consumed()
                    .with_pointer_release(PointerKey::new(info.source, info.id)),
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
        };

        let result = region.on_event(&ElementEvent::PointerDown(PointerInfo::new(
            aimer_attribute::position::Vec2d { x: 10.0, y: 10.0 },
            PointerSource::Mouse,
            7,
            PointerButton::Primary,
        )));

        assert!(result.is_consumed());
        assert!(result.needs_redraw());
    }

    // Regression for "moving the cursor pins a core": a child claiming a hover
    // move is not a child changing pixels. Inventing a redraw for every
    // consumed move repainted the whole window at input rate while the cursor
    // merely crossed the UI — whoever changes state on a move asks for its own
    // frame.
    #[test]
    fn a_consumed_hover_move_is_forwarded_without_inventing_a_redraw() {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 0.0, 0.0, 100.0, 100.0);
        let region = RawMouseRegion {
            on_hover_enter: VoidCallback::default(),
            on_hover_exit: VoidCallback::default(),
            cursor: None,
            current_state: Rc::new(Cell::new(PointerState::Inside)),
            cached_bounds: bounds,
            child: ResultElement,
            window: WindowHandle::headless(PhysicalSize::new(100, 100), 1.0),
        };

        let result = region.on_event(&ElementEvent::PointerMove(PointerInfo::new(
            aimer_attribute::position::Vec2d { x: 10.0, y: 10.0 },
            PointerSource::Mouse,
            7,
            PointerButton::Primary,
        )));

        assert!(result.is_consumed());
        assert!(!result.needs_redraw(), "a hover claim is not a repaint");
    }

    // The icon a region sets survives only while the move is consumed: an
    // unconsumed move makes the application restore the platform default
    // cursor. So a region with an icon claims the hover move even when its
    // child ignores it — without asking for a frame, which is what used to
    // make the claim so expensive.
    #[test]
    fn a_cursor_region_claims_the_hover_move_its_icon_depends_on() {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 0.0, 0.0, 100.0, 100.0);
        let region = RawMouseRegion {
            on_hover_enter: VoidCallback::default(),
            on_hover_exit: VoidCallback::default(),
            cursor: Some(winit::window::CursorIcon::Pointer),
            current_state: Rc::new(Cell::new(PointerState::Inside)),
            cached_bounds: bounds,
            child: TestElement,
            window: WindowHandle::headless(PhysicalSize::new(100, 100), 1.0),
        };

        let result = region.on_event(&ElementEvent::PointerMove(PointerInfo::new(
            aimer_attribute::position::Vec2d { x: 10.0, y: 10.0 },
            PointerSource::Mouse,
            7,
            PointerButton::Primary,
        )));

        assert!(result.is_consumed(), "the icon lives only while the move is claimed");
        assert!(!result.needs_redraw(), "claiming the cursor repaints nothing");
    }

    // With per-move redraws gone, the frame that repaints a hover decoration
    // has to come from the transition itself — once per crossing, not once per
    // move.
    #[test]
    fn a_hover_transition_schedules_exactly_one_frame() {
        let frames = Rc::new(Cell::new(0usize));
        let counted = frames.clone();
        let previous = aimer_events::window::set_thread_redraw_requester(move || {
            counted.set(counted.get() + 1);
        });

        let bounds = CacheBounds::new();
        bounds.save(1.0, 0.0, 0.0, 100.0, 100.0);
        let region = RawMouseRegion {
            on_hover_enter: VoidCallback::default(),
            on_hover_exit: VoidCallback::default(),
            cursor: None,
            current_state: Rc::new(Cell::new(PointerState::Outside)),
            cached_bounds: bounds,
            child: TestElement,
            window: WindowHandle::headless(PhysicalSize::new(100, 100), 1.0),
        };
        let move_inside = |x: f32, y: f32| {
            ElementEvent::PointerMove(PointerInfo::new(
                aimer_attribute::position::Vec2d { x, y },
                PointerSource::Mouse,
                7,
                PointerButton::Primary,
            ))
        };

        let _ = region.on_event(&move_inside(10.0, 10.0));
        assert_eq!(frames.get(), 1, "entering schedules the repaint");

        let _ = region.on_event(&move_inside(20.0, 20.0));
        assert_eq!(frames.get(), 1, "moving within schedules nothing further");

        aimer_events::window::restore_thread_redraw_requester(previous);
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
        }
        .boxed();
        let pointer = PointerKey::new(PointerSource::Touch, 2);
        let mut dispatcher = EventDispatcher::new();

        let _ = dispatcher.dispatch(
            region.as_ref(),
            Vec2d { x: 10.0, y: 10.0 },
            &ElementEvent::PointerDown(PointerInfo::touch(
                Vec2d { x: 10.0, y: 10.0 },
                pointer.id,
            )),
        );
        assert!(dispatcher.is_captured(pointer));

        let _ = dispatcher.dispatch(
            region.as_ref(),
            Vec2d { x: 200.0, y: 200.0 },
            &ElementEvent::PointerMove(PointerInfo::touch(
                Vec2d { x: 200.0, y: 200.0 },
                pointer.id,
            )),
        );
        let _ = dispatcher.dispatch(
            region.as_ref(),
            Vec2d { x: 200.0, y: 200.0 },
            &ElementEvent::PointerUp(PointerInfo::touch(
                Vec2d { x: 200.0, y: 200.0 },
                pointer.id,
            )),
        );

        assert_eq!(events.get(), 3);
        assert!(!dispatcher.is_captured(pointer));
    }

    #[test]
    fn nested_regions_share_one_capture_state_across_the_boundary_chain() {
        let events = Rc::new(Cell::new(0));
        let inner = capturing_region(events.clone()).boxed();
        let bounds = CacheBounds::new();
        bounds.save(1.0, 0.0, 0.0, 100.0, 100.0);
        let root = RawMouseRegion {
            on_hover_enter: VoidCallback::default(),
            on_hover_exit: VoidCallback::default(),
            cursor: None,
            current_state: Rc::new(Cell::new(PointerState::Outside)),
            cached_bounds: bounds,
            child: inner,
            window: WindowHandle::headless(PhysicalSize::new(100, 100), 1.0),
        }
        .boxed();
        let pointer = PointerKey::new(PointerSource::Touch, 3);
        let mut dispatcher = EventDispatcher::new();

        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 10.0, y: 10.0 },
            &ElementEvent::PointerDown(PointerInfo::touch(
                Vec2d { x: 10.0, y: 10.0 },
                pointer.id,
            )),
        );
        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 200.0, y: 200.0 },
            &ElementEvent::PointerMove(PointerInfo::touch(
                Vec2d { x: 200.0, y: 200.0 },
                pointer.id,
            )),
        );
        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 200.0, y: 200.0 },
            &ElementEvent::PointerUp(PointerInfo::touch(
                Vec2d { x: 200.0, y: 200.0 },
                pointer.id,
            )),
        );

        assert_eq!(events.get(), 3);
        assert!(!dispatcher.is_captured(pointer));
    }

    #[test]
    fn cancelling_a_nested_region_delivers_once_and_clears_shared_capture_state() {
        let events = Rc::new(Cell::new(0));
        let root = capturing_region(events.clone()).boxed();
        let pointer = PointerKey::new(PointerSource::Touch, 4);
        let mut dispatcher = EventDispatcher::new();

        let _ = dispatcher.dispatch(
            root.as_ref(),
            Vec2d { x: 10.0, y: 10.0 },
            &ElementEvent::PointerDown(PointerInfo::touch(
                Vec2d { x: 10.0, y: 10.0 },
                pointer.id,
            )),
        );
        let _ = dispatcher.dispatch(root.as_ref(), Vec2d::default(), &ElementEvent::Cancel);

        assert_eq!(events.get(), 2);
        assert!(!dispatcher.is_captured(pointer));
    }

    /// A region laid out over the top-left 100x100 corner, wrapping a child that
    /// captures the pointer it is pressed with.
    fn capturing_region(events: Rc<Cell<usize>>) -> RawMouseRegion<AnyElement> {
        let cached_bounds = CacheBounds::new();
        cached_bounds.save(1.0, 0.0, 0.0, 100.0, 100.0);

        RawMouseRegion {
            on_hover_enter: VoidCallback::default(),
            on_hover_exit: VoidCallback::default(),
            cursor: None,
            current_state: Rc::new(Cell::new(PointerState::Outside)),
            cached_bounds,
            child: CapturingElement { events }.boxed(),
            window: WindowHandle::headless(PhysicalSize::new(100, 100), 1.0),
        }
    }

    // A rebuild triggered by the press itself — a `Button` darkening under the
    // finger — replaces this region. The shared dispatcher keeps both the
    // boundary capture and the nested child owner, so the replacement still
    // receives the release outside its bounds.
    #[test]
    fn a_rebuild_during_a_press_keeps_the_capture_so_a_release_outside_still_lands() {
        let events = Rc::new(Cell::new(0));
        let pressed_region = capturing_region(events.clone());
        let pressed_child_id = pressed_region.child.id();
        let pressed = pressed_region.boxed();
        let pointer = PointerKey::new(PointerSource::Touch, 2);
        let mut dispatcher = EventDispatcher::new();

        let _ = dispatcher.dispatch(
            pressed.as_ref(),
            Vec2d { x: 10.0, y: 10.0 },
            &ElementEvent::PointerDown(PointerInfo::touch(
                Vec2d { x: 10.0, y: 10.0 },
                pointer.id,
            )),
        );
        assert!(dispatcher.is_captured(pointer));

        let rebuilt_region = capturing_region(events.clone());
        rebuilt_region.child.set_element_id(pressed_child_id);
        let rebuilt = rebuilt_region.boxed();
        rebuilt.set_element_id(pressed.id());

        let _ = dispatcher.dispatch(
            rebuilt.as_ref(),
            Vec2d { x: 200.0, y: 200.0 },
            &ElementEvent::PointerUp(PointerInfo::touch(
                Vec2d { x: 200.0, y: 200.0 },
                pointer.id,
            )),
        );

        assert_eq!(events.get(), 2, "the child must hear the release it is owed");
        assert!(!dispatcher.is_captured(pointer));
    }
}
