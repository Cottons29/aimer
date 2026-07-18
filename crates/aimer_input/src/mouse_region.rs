use std::cell::Cell;
use std::rc::Rc;

use aimer_attribute::CacheBounds;
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;
use aimer_events::window::request_animation_frame;
use aimer_macro::Rebuildable;
use aimer_widget::base::*;
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, RequiredChild, VisitorElement, Widget,
};

use crate::callback::{CallbackExecutor, RawInnerCallback, VoidCallback};

#[derive(Debug, Copy, Clone, Default)]
pub enum PointerState {
    Inside,
    #[default]
    Outside,
}

/// A shared pointer state.
///
/// This is a type alias of Rc<Cell<PointerState>>
pub type SharedPointerState = Rc<Cell<PointerState>>;

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
    pub fn on_hover_enter(mut self, on_hover_enter: impl Into<VoidCallback>) -> Self {
        self.on_hover_enter = on_hover_enter.into();
        self
    }

    pub fn on_hover_exit(mut self, on_hover_exit: impl Into<VoidCallback>) -> Self {
        self.on_hover_exit = on_hover_exit.into();
        self
    }

    pub fn cursor(mut self, cursor: impl Into<Option<winit::window::CursorIcon>>) -> Self {
        self.cursor = cursor.into();
        self
    }

    pub fn current_state(mut self, current_state: SharedPointerState) -> Self {
        self.current_state = current_state;
        self
    }

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
}

impl<W: Widget + 'static> Widget for MouseRegion<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self
            .child
            .to_element(ctx);
        RawMouseRegion {
            on_hover_enter: self
                .on_hover_enter
                .clone(),
            on_hover_exit: self
                .on_hover_exit
                .clone(),
            cursor: self.cursor,
            current_state: self
                .current_state
                .clone(),
            cached_bounds: self
                .cached_bounds
                .clone(),
            window: ctx
                .window
                .clone(),
            child,
        }
        .boxed()
    }
}

/// ##### A transparent wrapper that tracks the mouse hover state.
///
/// `MouseRegion` only responds to mouse-originated pointer events — touch
/// input is ignored for hover purposes. It writes to a shared `Rc<Cell<bool>>`
/// so that a child element (e.g. `GestureDetector`) can read the hover state
/// for decoration switching without knowing about `MouseRegion` at all.
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
}

impl<E: Element> RawMouseRegion<E> {
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
    fn sync_hover(&self, is_inside: bool) {
        if is_inside {
            if matches!(
                self.current_state
                    .get(),
                PointerState::Outside
            ) {
                Self::execute_void_callback(&self.on_hover_enter);
                self.current_state
                    .set(PointerState::Inside);
            }
        } else if matches!(
            self.current_state
                .get(),
            PointerState::Inside
        ) {
            Self::execute_void_callback(&self.on_hover_exit);
            self.current_state
                .set(PointerState::Outside);
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
    fn on_event(&self, event: &ElementEvent) -> bool {
        // println!("Event received: {:?}", event);

        if matches!(event, ElementEvent::PointerExited(PointerSource::Mouse, _)) {
            if self
                .cursor
                .is_some()
            {
                self.window
                    .set_cursor(winit::window::CursorIcon::Default);
            }
            self.sync_hover(false);
            return self
                .child
                .on_event(event);
        }

        // Hover tracking is a mouse-only concept. Touch input must NOT drive
        // `sync_hover`: firing `on_hover_enter` on a touch `PointerDown` calls
        // the Button's `set_state`, which marks the subtree dirty and rebuilds
        // (replacing) the child `GestureDetector` mid-gesture — between the
        // touch `Down` and `Up`. The replacement loses the recorded
        // `down_position`, so the tap (`on_tap`/`on_press`) never fires.
        // For touch we simply forward the event to the child untouched.
        let pos = match event {
            ElementEvent::PointerDown(p, src, _) if *src == PointerSource::Mouse => *p,
            ElementEvent::PointerUp(p, src, _) if *src == PointerSource::Mouse => *p,
            ElementEvent::PointerMove(p, src, _) if *src == PointerSource::Mouse => *p,
            _ => {
                return self
                    .child
                    .on_event(event);
            }
        };

        // println!("Event received: {:?}", event);

        let is_inside = self
            .cached_bounds
            .is_inside(pos.x, pos.y);

        // Update the cursor icon on every mouse event while over the region.
        if is_inside {
            if let Some(icon) = self.cursor {
                self.window
                    .set_cursor(icon);
            }
        } else if self
            .cursor
            .is_some()
        {
            self.window
                .set_cursor(winit::window::CursorIcon::Default);
        }

        // Only fire callbacks on a state change between Enter <-> Exit,
        // not on every mouse event.
        self.sync_hover(is_inside);
        self.child
            .on_event(event)
    }

    // Return empty — we manually forward to the child in on_event
    // so that MouseRegion always sees the event first.
    fn event_children<'b>(&'b self, _visitor: &mut dyn FnMut(&'b dyn Element)) {}
}

impl<E: Element> LayoutElement for RawMouseRegion<E> {
    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        let size = self
            .child
            .layout(ctx);
        // Cache our own bounds from the canvas transform for hit-testing
        let (abs_x, abs_y) = ctx
            .canvas
            .get_transform_translation();
        self.cached_bounds
            .save(ctx.scale, abs_x, abs_y, size.width, size.height);
        size
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child
            .computed_size(ctx)
    }
}

impl<E: Element> Drawable for RawMouseRegion<E> {
    fn draw(&self, ctx: &BuildContext<'_>) {
        // Update cached bounds from the current canvas position
        let child_size = self
            .child
            .computed_size(ctx);
        let (abs_x, abs_y) = ctx
            .canvas
            .get_transform_translation();
        self.cached_bounds
            .save(ctx.scale, abs_x, abs_y, child_size.width, child_size.height);

        // Re-evaluate hover against the actual (last-known) cursor position so
        // the hover state survives rebuilds/replacements. After a click the
        // subtree is rebuilt with a fresh `Outside` state and no pointer event
        // follows, so without this the button would lose its hover feedback
        // until the mouse moved again.
        let cursor = ctx.cursor_pos;
        let is_inside = self
            .cached_bounds
            .is_inside(cursor.x, cursor.y);
        self.sync_hover(is_inside);

        self.child
            .draw(ctx);
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use aimer_widget::Rebuildable;
    use aimer_widget::base::WindowHandle;
    use winit::dpi::PhysicalSize;

    use super::*;

    struct TestElement;

    struct TestWidget;

    impl Widget for TestWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
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

        region.on_event(&ElementEvent::PointerExited(PointerSource::Mouse, 0));

        assert!(matches!(current_state.get(), PointerState::Outside));
    }
}
