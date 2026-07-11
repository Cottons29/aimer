use crate::callback::{CallbackExecutor, RawInnerCallback, VoidCallback};
use aimer_attribute::CacheBounds;
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;
use aimer_macro::Rebuildable;
use aimer_widget::{Drawable, Element, EventElement, LayoutElement, Reconcilable, VisitorElement, Widget, base::*};
use std::cell::Cell;
use std::rc::Rc;

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

pub struct MouseRegion<W: Widget + 'static> {
    pub on_hover_enter: VoidCallback,
    pub on_hover_exit: VoidCallback,
    pub cursor: Option<winit::window::CursorIcon>,
    pub current_state: SharedPointerState,
    pub cached_bounds: CacheBounds,
    pub child: W,
}

impl<W: Widget + 'static> Widget for MouseRegion<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);
        RawMouseRegion {
            on_hover_enter: self.on_hover_enter.clone(),
            on_hover_exit: self.on_hover_exit.clone(),
            cursor: self.cursor,
            current_state: self.current_state.clone(),
            cached_bounds: self.cached_bounds.clone(),
            window: ctx.window,
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
pub struct RawMouseRegion<'a, E: Element> {
    pub(crate) on_hover_enter: VoidCallback,
    pub(crate) on_hover_exit: VoidCallback,
    pub(crate) cursor: Option<winit::window::CursorIcon>,
    pub(crate) current_state: Rc<Cell<PointerState>>,
    pub(crate) cached_bounds: CacheBounds,
    pub(crate) child: E,
    pub(crate) window: &'a winit::window::Window,
}

impl<'a, E: Element> RawMouseRegion<'a, E> {
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
            if matches!(self.current_state.get(), PointerState::Outside) {
                Self::execute_void_callback(&self.on_hover_enter);
                self.current_state.set(PointerState::Inside);
                
            }
        } else if matches!(self.current_state.get(), PointerState::Inside) {
            Self::execute_void_callback(&self.on_hover_exit);
            self.current_state.set(PointerState::Outside);
            self.window.request_redraw();
        }
    }
}

impl<'a, E: Element> VisitorElement for RawMouseRegion<'a, E> {
    fn visit_children<'b>(&'b self, visitor: &mut dyn FnMut(&'b dyn Element)) {
        visitor(&self.child);
    }

    fn debug_name(&self) -> &'static str {
        "MouseRegion"
    }
}

impl<'a, E: Element> EventElement for RawMouseRegion<'a, E> {
    fn on_event(&self, event: &ElementEvent) -> bool {



        // println!("Event received: {:?}", event);

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
            _ => return self.child.on_event(event),
        };

        // println!("Event received: {:?}", event);

        let is_inside = self.cached_bounds.is_inside(pos.x, pos.y);

        // Update the cursor icon on every mouse event while over the region.
        if is_inside {
            if let Some(icon) = self.cursor {
                self.window.set_cursor(icon);
            }
        } else if self.cursor.is_some() {
            self.window.set_cursor(winit::window::CursorIcon::Default);
        }

        // Only fire callbacks on a state change between Enter <-> Exit,
        // not on every mouse event.
        self.sync_hover(is_inside);
        self.child.on_event(event)
    }

    // Return empty — we manually forward to the child in on_event
    // so that MouseRegion always sees the event first.
    fn event_children<'b>(&'b self, _visitor: &mut dyn FnMut(&'b dyn Element)) {}
}

impl<'a, E: Element> LayoutElement for RawMouseRegion<'a, E> {
    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        let size = self.child.layout(ctx);
        // Cache our own bounds from the canvas transform for hit-testing
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        self.cached_bounds.save(ctx.scale, abs_x, abs_y, size.width, size.height);
        size
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }
}

impl<'a, E: Element> Drawable for RawMouseRegion<'a, E> {
    fn draw(&self, ctx: &BuildContext<'_>) {
        // Update cached bounds from the current canvas position
        let child_size = self.child.computed_size(ctx);
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        self.cached_bounds
            .save(ctx.scale, abs_x, abs_y, child_size.width, child_size.height);

        // Re-evaluate hover against the actual (last-known) cursor position so
        // the hover state survives rebuilds/replacements. After a click the
        // subtree is rebuilt with a fresh `Outside` state and no pointer event
        // follows, so without this the button would lose its hover feedback
        // until the mouse moved again.
        let cursor = ctx.cursor_pos;
        let is_inside = self.cached_bounds.is_inside(cursor.x, cursor.y);
        self.sync_hover(is_inside);

        self.child.draw(ctx);
    }
}

impl<'a: 'static, E: Element + 'static> Reconcilable for RawMouseRegion<'a, E> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_from_widget(&self, new_element: &dyn Element, ctx: &BuildContext) -> bool {
        let new = match new_element.as_any().downcast_ref::<RawMouseRegion<E>>() {
            Some(n) => n,
            None => return false,
        };
        // Copy our cached bounds to the replacement element so hover
        // detection works immediately — the new element starts with empty
        // bounds and `is_inside` would return false until the next draw pass.
        if let Some(bounds) = self.cached_bounds.get_bounds() {
            new.cached_bounds.set_bounds(bounds);
        }

        // Give the child a chance to copy active runtime state into the
        // replacement child before this wrapper is replaced. This preserves
        // an in-flight touch gesture when touch hover feedback rebuilds a
        // Button between PointerDown and PointerUp.
        let _ = self.child.update_from_widget(&new.child, ctx);
        false // Let the element be replaced so child gets new decoration
    }
}
