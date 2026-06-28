use crate::base::*;
use crate::components::event_element::EventElement;
use crate::components::layout_element::LayoutElement;
use crate::components::rebuildable::Rebuildable;
use crate::components::reconcilable::Reconcilable;
pub(crate) use crate::components::visitor_element::VisitorElement;
use crate::Drawable;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;

impl<T> Element for T where T: VisitorElement + EventElement + LayoutElement + Rebuildable + Drawable + Reconcilable {}

pub trait Element: VisitorElement + EventElement + LayoutElement + Rebuildable + Drawable + Reconcilable {}

impl VisitorElement for Box<dyn Element> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().visit_children(visitor)
    }

    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }
}

impl LayoutElement for Box<dyn Element> {
    fn pos(&self) -> Option<Vec2d> {
        self.as_ref().pos()
    }
    fn size(&self) -> Option<Size> {
        self.as_ref().size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().content_size(ctx)
    }
    fn layer(&self) -> u32 {
        self.as_ref().layer()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref().get_size_from_child()
    }
    fn invalidate_layout(&self) {
        self.as_ref().invalidate_layout()
    }
}

impl Rebuildable for Box<dyn Element> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.as_ref().rebuild_if_dirty(ctx)
    }
}

impl EventElement for Box<dyn Element> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.as_ref().on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().event_children(visitor)
    }
}

impl Drawable for Box<dyn Element> {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx)
    }
}

impl Reconcilable for Box<dyn Element> {
    fn as_any(&self) -> &dyn std::any::Any { self }

    fn key(&self) -> Option<crate::key::Key> {
        self.as_ref().key()
    }
    fn update_from_widget(&self, new_element: &dyn Element, ctx: &BuildContext) -> bool {
        self.as_ref().update_from_widget(new_element, ctx)
    }
}

/// Perform a hit-test on the element tree and dispatch the event to the deepest hit element.
/// Returns `true` if any element consumed the event.
pub fn dispatch_event(root: &dyn Element, pos: Vec2d, event: &ElementEvent) -> bool {
    use smallvec::SmallVec;

    let mut children: SmallVec<[&dyn Element; 8]> = SmallVec::new();
    root.event_children(&mut |child| children.push(child));

    for child in children.into_iter().rev() {
        if dispatch_event(child, pos, event) {
            return true;
        }
    }

    // Check if pos is inside this element's bounds
    if let Some((start, end)) = root.pos_start_end() {
        let inside = pos.x >= start.x && pos.x <= end.x && pos.y >= start.y && pos.y <= end.y;
        if inside {
            return root.on_event(event);
        }
    }

    // If the element has no position info, still try to dispatch the event.
    // This allows elements like Button (which don't track absolute position)
    // to receive events when reached through the tree traversal.
    if root.pos_start_end().is_none() {
        return root.on_event(event);
    }

    false
}

/// Broadcast an event to every element in the tree, regardless of hit-testing.
/// Returns `true` if any element consumed the event.
pub fn broadcast_event(root: &dyn Element, event: &ElementEvent) -> bool {
    use smallvec::SmallVec;

    let mut consumed = false;

    let mut children: SmallVec<[&dyn Element; 8]> = SmallVec::new();
    root.event_children(&mut |child| children.push(child));

    for child in children.into_iter().rev() {
        if broadcast_event(child, event) {
            consumed = true;
        }
    }

    if root.on_event(event) {
        consumed = true;
    }

    consumed
}
