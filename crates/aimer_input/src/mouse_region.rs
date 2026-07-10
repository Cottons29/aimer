use crate::callback::{CallbackExecutor, RawInnerCallback, VoidCallback};
use aimer_attribute::CacheBounds;
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;
use aimer_macro::Rebuildable;
use aimer_widget::{Drawable, Element, EventElement, LayoutElement, Reconcilable, VisitorElement, Widget, base::*};
use std::cell::Cell;
use std::rc::Rc;
use aimer_events::window::request_animation_frame;

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
            pressed: Cell::new(false),
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
    /// Whether a mouse button is currently pressed inside this region — i.e.
    /// a `PointerDown` has been seen and its matching `PointerUp`/`Cancel`
    /// has not. While this is `true`, hover feedback is suppressed so it can
    /// never rebuild the subtree mid-gesture (see [`should_reconcile_hover`]).
    pub(crate) pressed: Cell<bool>,
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
                request_animation_frame()
            }
        } else if matches!(self.current_state.get(), PointerState::Inside) {
            Self::execute_void_callback(&self.on_hover_exit);
            self.current_state.set(PointerState::Outside);
            request_animation_frame()
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

        // A `Cancel` ends any in-flight press (e.g. the app was backgrounded
        // or the gesture was interrupted). Clear the pressed flag so hover
        // feedback can resume, then forward it to the child untouched.
        if matches!(event, ElementEvent::Cancel) {
            self.pressed.set(false);
            return self.child.on_event(event);
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
            _ => return self.child.on_event(event),
        };

        let is_inside = self.cached_bounds.is_inside(pos.x, pos.y);

        // Update the cursor icon on every mouse event while over the region.
        if is_inside {
            if let Some(icon) = self.cursor {
                self.window.set_cursor(icon);
            }
        } else if self.cursor.is_some() {
            self.window.set_cursor(winit::window::CursorIcon::Default);
        }

        // Track the press so hover feedback can't rebuild the subtree between a
        // `PointerDown` and its `PointerUp`. A tap is a Down→Up handshake owned
        // by the child `GestureDetector`; firing `on_hover_enter` in between
        // calls the Button's `set_state`, which replaces the child and drops
        // the recorded `down_position`, so the tap never fires and the user
        // has to click again (the intermittent "button freezes" bug). The
        // mouse variant of the touch guard already documented above.
        self.pressed.set(next_pressed_state(self.pressed.get(), event, is_inside));

        // Only reconcile hover (Enter <-> Exit) when no press is in flight,
        // and only on an actual state change — not on every mouse event.
        if should_reconcile_hover(self.pressed.get()) {
            self.sync_hover(is_inside);
        }
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
        // Never let a redraw fire hover feedback while a press is in flight:
        // the cursor sits inside the button during a click, so a draw-driven
        // `on_hover_enter` would rebuild (replace) the child `GestureDetector`
        // between `PointerDown` and `PointerUp` and swallow the tap.
        if should_reconcile_hover(self.pressed.get()) {
            self.sync_hover(is_inside);
        }

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

        // Carry the in-flight press across the replacement. If some *other*
        // `set_state` rebuilds this region while a mouse button is held down,
        // the fresh element would start un-pressed and immediately re-enable
        // hover feedback — reintroducing the very mid-gesture rebuild we guard
        // against. Preserving `pressed` keeps hover suppressed until the
        // matching `PointerUp` arrives.
        new.pressed.set(self.pressed.get());

        // Give the child a chance to copy active runtime state into the
        // replacement child before this wrapper is replaced. This preserves
        // an in-flight touch gesture when touch hover feedback rebuilds a
        // Button between PointerDown and PointerUp.
        let _ = self.child.update_from_widget(&new.child, ctx);
        false // Let the element be replaced so child gets new decoration
    }
}

/// Whether hover enter/exit should be reconciled given a press is in flight.
///
/// While a mouse button is held down (`pressed == true`) the [`MouseRegion`]
/// must NOT fire its hover callbacks: they call the Button's `set_state`,
/// which rebuilds (replaces) the child `GestureDetector` between the
/// `PointerDown` and the `PointerUp`. The replacement loses the recorded
/// `down_position`, so the tap never fires and the user has to click again
/// (the intermittent "button freezes" bug). Hover only reconciles once the
/// press is released.
fn should_reconcile_hover(pressed: bool) -> bool {
    !pressed
}

/// Compute the next pressed state for a mouse event.
///
/// A press begins on a mouse `PointerDown` inside the region and ends on any
/// mouse `PointerUp` (or a `Cancel`, handled separately in `on_event`).
/// Touch events never affect this flag — hover is a mouse-only concept and
/// touch is forwarded to the child untouched.
fn next_pressed_state(current: bool, event: &ElementEvent, is_inside: bool) -> bool {
    match event {
        ElementEvent::PointerDown(_, PointerSource::Mouse, _) if is_inside => true,
        ElementEvent::PointerUp(_, PointerSource::Mouse, _) => false,
        _ => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aimer_attribute::position::Vec2d;

    fn mouse_down(x: f32, y: f32) -> ElementEvent {
        ElementEvent::PointerDown(Vec2d { x, y }, PointerSource::Mouse, 0)
    }

    fn mouse_up(x: f32, y: f32) -> ElementEvent {
        ElementEvent::PointerUp(Vec2d { x, y }, PointerSource::Mouse, 0)
    }

    fn touch_down(x: f32, y: f32) -> ElementEvent {
        ElementEvent::PointerDown(Vec2d { x, y }, PointerSource::Touch, 1)
    }

    // Regression for "the buttons sometimes freeze so I need to click again to
    // trigger the button": while a mouse press is in flight the region must not
    // reconcile hover, otherwise `on_hover_enter` -> Button `set_state` rebuilds
    // and replaces the child `GestureDetector` between the `PointerDown` and the
    // `PointerUp`, dropping the recorded press so the tap is silently swallowed.
    #[test]
    fn hover_is_suppressed_between_press_and_release() {
        // Before any press, hover is free to update.
        let mut pressed = false;
        assert!(should_reconcile_hover(pressed), "hover should reconcile before a press");

        // A mouse PointerDown inside the region starts the press.
        pressed = next_pressed_state(pressed, &mouse_down(25.0, 35.0), true);
        assert!(pressed, "a PointerDown inside must start a press");
        assert!(!should_reconcile_hover(pressed), "hover must be suppressed while pressed");

        // A move while still pressed must keep the press — and keep hover off.
        pressed = next_pressed_state(pressed, &ElementEvent::PointerMove(Vec2d { x: 26.0, y: 36.0 }, PointerSource::Mouse, 0), true);
        assert!(pressed, "a move must not end the press");
        assert!(!should_reconcile_hover(pressed));

        // The matching PointerUp releases the press and hover resumes.
        pressed = next_pressed_state(pressed, &mouse_up(25.0, 35.0), true);
        assert!(!pressed, "a PointerUp must end the press");
        assert!(should_reconcile_hover(pressed), "hover should reconcile again after release");
    }

    // A press must only start when the button is actually pressed *inside* the
    // region, so hovering (move) or clicking elsewhere never blocks hover.
    #[test]
    fn press_only_starts_on_pointer_down_inside() {
        assert!(!next_pressed_state(false, &mouse_down(500.0, 500.0), false), "a PointerDown outside must not start a press");
        assert!(
            !next_pressed_state(false, &ElementEvent::PointerMove(Vec2d { x: 25.0, y: 35.0 }, PointerSource::Mouse, 0), true),
            "a move over the region must not start a press"
        );
    }

    // Touch input is handled by a separate (touch) path and must never toggle
    // the mouse press flag.
    #[test]
    fn touch_events_do_not_affect_mouse_press() {
        assert!(!next_pressed_state(false, &touch_down(25.0, 35.0), true), "a touch down must not start a mouse press");
        assert!(next_pressed_state(true, &touch_down(25.0, 35.0), true), "a touch down must not clear a mouse press");
    }
}
