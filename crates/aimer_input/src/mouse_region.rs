use crate::callback::{CallbackExecutor, RawInnerCallback, VoidCallback};
use aimer_attribute::CacheBounds;
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;
use aimer_macro::Rebuildable;
use aimer_widget::{Drawable, Element, EventElement, LayoutElement, Reconcilable, VisitorElement, base::*};
use std::cell::Cell;
use std::rc::Rc;

/// A transparent wrapper that tracks the mouse hover state.
///
/// `MouseRegion` only responds to mouse-originated pointer events — touch
/// input is ignored for hover purposes. It writes to a shared `Rc<Cell<bool>>`
/// so that a child element (e.g. `GestureDetector`) can read the hover state
/// for decoration switching without knowing about `MouseRegion` at all.
///
/// Event dispatch is handled manually: `event_children` returns empty so that
/// `on_event` is called first, then events are forwarded to the child.
#[derive(Rebuildable)]
pub struct MouseRegion<'a, E: Element> {
    pub(crate) on_hover_enter: VoidCallback,
    pub(crate) on_hover_exit: VoidCallback,
    pub(crate) cursor: Option<winit::window::CursorIcon>,
    pub(crate) is_hovered: Rc<Cell<bool>>,
    pub(crate) cached_bounds: CacheBounds,
    pub(crate) child: E,
    pub(crate) window: &'a winit::window::Window,
}

impl<'a, E: Element> MouseRegion<'a, E> {
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
}

impl<'a, E: Element> VisitorElement for MouseRegion<'a, E> {
    fn visit_children<'b>(&'b self, visitor: &mut dyn FnMut(&'b dyn Element)) {
        visitor(&self.child);
    }

    fn debug_name(&self) -> &'static str {
        "MouseRegion"
    }
}

impl<'a, E: Element> EventElement for MouseRegion<'a, E> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        // Forward to child — since event_children returns empty, the dispatch
        // system calls us directly. We forward manually so we always see the
        // event regardless of whether the child consumes it.
        let child_consumed = self.child.on_event(event);

        // Only process hover for mouse-originated events
        let pos = match event {
            ElementEvent::PointerDown(p, src, _) if *src == PointerSource::Mouse => *p,
            ElementEvent::PointerUp(p, src, _) if *src == PointerSource::Mouse => *p,
            ElementEvent::PointerMove(p, src, _) if *src == PointerSource::Mouse => *p,
            _ => return child_consumed,
        };

        let is_inside = self.cached_bounds.is_inside(pos.x, pos.y);
        let was_hovered = self.is_hovered.get();

        if is_inside == was_hovered {
            return child_consumed;
        }

        self.is_hovered.set(is_inside);
        self.window.request_redraw();

        if is_inside {
            if let Some(icon) = self.cursor {
                self.window.set_cursor(icon);
            }
            Self::execute_void_callback(&self.on_hover_enter);

        } else {
            if self.cursor.is_some() {
                self.window.set_cursor(winit::window::CursorIcon::Default);
            }
            Self::execute_void_callback(&self.on_hover_exit);
        }

        child_consumed
    }

    // Return empty — we manually forward to the child in on_event
    // so that MouseRegion always sees the event first.
    fn event_children<'b>(&'b self, _visitor: &mut dyn FnMut(&'b dyn Element)) {}
}

impl<'a, E: Element> LayoutElement for MouseRegion<'a, E> {
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

impl<'a, E: Element> Drawable for MouseRegion<'a, E> {
    fn draw(&self, ctx: &BuildContext<'_>) {
        // Update cached bounds from the current canvas position
        let child_size = self.child.computed_size(ctx);
        let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
        self.cached_bounds.save(ctx.scale, abs_x, abs_y, child_size.width, child_size.height);

        self.child.draw(ctx);
    }
}

impl<'a: 'static, E: Element + 'static> Reconcilable for MouseRegion<'a, E> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_from_widget(&self, new_element: &dyn Element, _ctx: &BuildContext) -> bool {
        let new = match new_element.as_any().downcast_ref::<MouseRegion<E>>() {
            Some(n) => n,
            None => return false,
        };
        // Copy our cached bounds to the replacement element so hover
        // detection works immediately — the new element starts with empty
        // bounds and `is_inside` would return false until the next draw pass.
        if let Some(bounds) = self.cached_bounds.get_bounds() {
            new.cached_bounds.set_bounds(bounds);
        }
        false // Let the element be replaced so child gets new decoration
    }
}
