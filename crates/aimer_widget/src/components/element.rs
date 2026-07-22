use crate::base::*;
use crate::components::event_element::EventElement;
use crate::components::layout_element::LayoutElement;
use crate::components::rebuildable::Rebuildable;
pub(crate) use crate::components::visitor_element::VisitorElement;
use crate::{AnyElement, Drawable};
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;

impl<T> Element for T where T: VisitorElement + EventElement + LayoutElement + Rebuildable + Drawable
{}

pub trait Element: VisitorElement + EventElement + LayoutElement + Rebuildable + Drawable {
    /// Erases this element into an inline-or-heap [`AnyElement`].
    ///
    /// Elements fitting `Rubick`'s configured size and alignment are embedded
    /// directly in the returned owner. Larger or over-aligned elements use one
    /// heap allocation. The historical method name is retained for source
    /// familiarity and does not imply that allocation occurred.
    ///
    /// Borrowing the owner provides a `dyn Element` view. Moving an inline owner
    /// also moves its concrete element, so callers must not rely on a stable
    /// payload address without pinning.
    ///
    /// This method requires a sized, `'static` concrete element because stable
    /// Rust does not support general implicit unsizing for custom smart
    /// pointers.
    fn boxed(self) -> AnyElement
    where
        Self: Sized + 'static,
    {
        AnyElement::new_projected(self, project_element, project_element_mut)
    }
}

fn project_element<E: Element + 'static>(value: &E) -> &(dyn Element + 'static) {
    value
}

fn project_element_mut<E: Element + 'static>(value: &mut E) -> &mut (dyn Element + 'static) {
    value
}

impl VisitorElement for AnyElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref()
            .visit_children(visitor)
    }

    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }

    fn element_type_id(&self) -> std::any::TypeId {
        self.as_ref()
            .element_type_id()
    }
}

impl LayoutElement for AnyElement {
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
        self.as_ref()
            .computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref()
            .content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.as_ref().layer()
    }

    fn flex(&self) -> Option<f32> {
        self.as_ref().flex()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref()
            .get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.as_ref()
            .invalidate_layout()
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().pos_start_end()
    }
}

impl Rebuildable for AnyElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.as_ref()
            .rebuild_if_dirty(ctx)
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        self.as_ref().option_any()
    }

    fn is_carry_state(&self) -> bool {
        self.as_ref().is_carry_state()
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.as_ref()
            .with_rebuild_context(ctx, callback)
    }

    fn mark_needs_rebuild(&self) {
        self.as_ref()
            .mark_needs_rebuild()
    }
}

impl EventElement for AnyElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.as_ref().on_event(event)
    }

    fn captures_pointer(&self, pointer: u64) -> bool {
        self.as_ref()
            .captures_pointer(pointer)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref()
            .event_children(visitor)
    }
}

impl Drawable for AnyElement {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx)
    }
}

impl VisitorElement for Box<dyn Element> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref()
            .visit_children(visitor)
    }
    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }
    fn element_type_id(&self) -> std::any::TypeId {
        self.as_ref()
            .element_type_id()
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
        self.as_ref()
            .computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref()
            .content_size(ctx)
    }
    fn layer(&self) -> u32 {
        self.as_ref().layer()
    }

    fn flex(&self) -> Option<f32> {
        self.as_ref().flex()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref()
            .get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.as_ref()
            .invalidate_layout()
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().pos_start_end()
    }
}

impl Rebuildable for Box<dyn Element> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.as_ref()
            .rebuild_if_dirty(ctx)
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        self.as_ref().option_any()
    }

    fn is_carry_state(&self) -> bool {
        self.as_ref().is_carry_state()
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.as_ref()
            .with_rebuild_context(ctx, callback)
    }

    fn mark_needs_rebuild(&self) {
        self.as_ref()
            .mark_needs_rebuild()
    }
}

impl EventElement for Box<dyn Element> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        self.as_ref().on_event(event)
    }

    fn captures_pointer(&self, pointer: u64) -> bool {
        self.as_ref()
            .captures_pointer(pointer)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref()
            .event_children(visitor)
    }
}

impl Drawable for Box<dyn Element> {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx)
    }
}

/// Perform a hit-test on the element tree and dispatch the event to the deepest
/// hit element. Returns `true` if any element consumed the event.
pub fn dispatch_event(root: &dyn Element, pos: Vec2d, event: &ElementEvent) -> bool {
    let mut children = Vec::new();
    let captured_pointer = match event {
        ElementEvent::PointerMove(_, _, pointer)
        | ElementEvent::PointerUp(_, _, pointer)
        | ElementEvent::PointerExited(_, pointer) => Some(*pointer),
        _ => None,
    };
    if let Some(pointer) = captured_pointer
        && let Some(handled) = dispatch_captured_event_inner(root, pointer, event, &mut children)
    {
        return handled;
    }

    dispatch_event_inner(root, pos, event, &mut children)
}

fn dispatch_event_inner<'a>(
    root: &'a dyn Element,
    pos: Vec2d,
    event: &ElementEvent,
    children: &mut Vec<&'a dyn Element>,
) -> bool {
    let start = children.len();
    root.event_children(&mut |child| children.push(child));
    let end = children.len();

    for index in (start..end).rev() {
        let child = children[index];
        if dispatch_event_inner(child, pos, event, children) {
            children.truncate(start);
            return true;
        }
    }
    children.truncate(start);

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

fn dispatch_captured_event_inner<'a>(
    root: &'a dyn Element,
    pointer: u64,
    event: &ElementEvent,
    children: &mut Vec<&'a dyn Element>,
) -> Option<bool> {
    let start = children.len();
    root.event_children(&mut |child| children.push(child));
    let end = children.len();

    for index in (start..end).rev() {
        let child = children[index];
        if let Some(handled) = dispatch_captured_event_inner(child, pointer, event, children) {
            children.truncate(start);
            return Some(handled);
        }
    }
    children.truncate(start);

    root.captures_pointer(pointer)
        .then(|| root.on_event(event))
}

/// Cancels the element that captured `pointer`, even when `pos` is outside its bounds.
/// Falls back to normal hit-tested dispatch when no element owns the pointer.
pub fn cancel_pointer(root: &dyn Element, pointer: u64, pos: Vec2d) -> bool {
    let event = ElementEvent::Cancel;
    let mut children = Vec::new();
    dispatch_captured_event_inner(root, pointer, &event, &mut children)
        .unwrap_or_else(|| dispatch_event_inner(root, pos, &event, &mut children))
}

/// Broadcast an event to every element in the tree, regardless of hit-testing.
/// Returns `true` if any element consumed the event.
pub fn broadcast_event(root: &dyn Element, event: &ElementEvent) -> bool {
    let mut children = Vec::new();
    broadcast_event_inner(root, event, &mut children)
}

fn broadcast_event_inner<'a>(
    root: &'a dyn Element,
    event: &ElementEvent,
    children: &mut Vec<&'a dyn Element>,
) -> bool {
    let mut consumed = false;
    let start = children.len();
    root.event_children(&mut |child| children.push(child));
    let end = children.len();

    for index in (start..end).rev() {
        let child = children[index];
        if broadcast_event_inner(child, event, children) {
            consumed = true;
        }
    }
    children.truncate(start);

    if root.on_event(event) {
        consumed = true;
    }

    consumed
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::cell::Cell;

    use aimer_rubick::INLINE_CAPACITY;

    use super::*;

    struct DowncastableElement;

    impl VisitorElement for DowncastableElement {
        fn debug_name(&self) -> &'static str {
            "DowncastableElement"
        }
    }

    impl EventElement for DowncastableElement {}
    impl LayoutElement for DowncastableElement {}
    impl Drawable for DowncastableElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for DowncastableElement {
        fn option_any(&self) -> Option<&dyn Any> {
            Some(self)
        }
    }

    #[test]
    fn boxed_element_delegates_runtime_downcasting() {
        let element: Box<dyn Element> = Box::new(DowncastableElement);

        assert!(
            element
                .option_any()
                .is_some_and(|value| value.is::<DowncastableElement>())
        );
    }

    struct StorageElement<const N: usize>([u8; N]);

    impl<const N: usize> VisitorElement for StorageElement<N> {
        fn debug_name(&self) -> &'static str {
            "StorageElement"
        }
    }

    impl<const N: usize> EventElement for StorageElement<N> {}
    impl<const N: usize> LayoutElement for StorageElement<N> {}
    impl<const N: usize> Drawable for StorageElement<N> {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl<const N: usize> Rebuildable for StorageElement<N> {
        fn option_any(&self) -> Option<&dyn Any> {
            Some(self)
        }
    }

    #[test]
    fn erased_elements_select_inline_or_heap_storage_and_dispatch_after_moves() {
        let inline = StorageElement([]).boxed();
        let heap = StorageElement([0; INLINE_CAPACITY + 1]).boxed();

        assert!(inline.is_inline());
        assert!(heap.is_heap());

        let owners = std::hint::black_box([inline, heap]);
        assert_eq!(owners[0].debug_name(), "StorageElement");
        assert!(
            owners[1]
                .option_any()
                .is_some_and(|value| value.is::<StorageElement<{ INLINE_CAPACITY + 1 }>>())
        );
    }

    struct CapturingElement {
        events: Cell<usize>,
    }

    impl VisitorElement for CapturingElement {
        fn debug_name(&self) -> &'static str {
            "CapturingElement"
        }
    }

    impl EventElement for CapturingElement {
        fn on_event(&self, _event: &ElementEvent) -> bool {
            self.events
                .set(self.events.get() + 1);
            true
        }

        fn captures_pointer(&self, pointer: u64) -> bool {
            pointer == 7
        }
    }

    impl LayoutElement for CapturingElement {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some((Vec2d { x: 0.0, y: 0.0 }, Vec2d { x: 10.0, y: 10.0 }))
        }
    }

    impl Drawable for CapturingElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for CapturingElement {}

    struct TreeElement {
        children: Vec<TreeElement>,
        events: Cell<usize>,
        captures_pointer: bool,
    }

    impl VisitorElement for TreeElement {
        fn debug_name(&self) -> &'static str {
            "TreeElement"
        }
    }

    impl EventElement for TreeElement {
        fn on_event(&self, _event: &ElementEvent) -> bool {
            self.events
                .set(self.events.get() + 1);
            true
        }

        fn captures_pointer(&self, _pointer: u64) -> bool {
            self.captures_pointer
        }

        fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child);
            }
        }
    }

    impl LayoutElement for TreeElement {}

    impl Drawable for TreeElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for TreeElement {}

    #[test]
    fn captured_pointer_move_is_delivered_outside_element_bounds() {
        use aimer_events::pointer::PointerSource;

        let element = CapturingElement {
            events: Cell::new(0),
        };
        let event = ElementEvent::PointerMove(Vec2d { x: 50.0, y: 50.0 }, PointerSource::Touch, 7);

        assert!(dispatch_event(&element, Vec2d { x: 50.0, y: 50.0 }, &event));
        assert_eq!(element.events.get(), 1);
    }

    #[test]
    fn cancel_pointer_reaches_captured_element_outside_bounds() {
        let element = CapturingElement {
            events: Cell::new(0),
        };

        assert!(cancel_pointer(&element, 7, Vec2d { x: 50.0, y: 50.0 }));
        assert_eq!(element.events.get(), 1);
    }

    #[test]
    fn cancel_pointer_falls_back_to_hit_testing_without_capture() {
        let element = CapturingElement {
            events: Cell::new(0),
        };

        assert!(cancel_pointer(&element, 8, Vec2d { x: 5.0, y: 5.0 }));
        assert_eq!(element.events.get(), 1);
    }

    #[test]
    fn cancel_pointer_is_not_delivered_twice_to_captured_target() {
        let element = CapturingElement {
            events: Cell::new(0),
        };

        assert!(cancel_pointer(&element, 7, Vec2d { x: 5.0, y: 5.0 }));
        assert_eq!(element.events.get(), 1);
    }

    #[test]
    fn recursive_dispatch_helpers_reuse_and_clear_the_scratch_vector() {
        let element = TreeElement {
            children: vec![TreeElement {
                children: vec![TreeElement {
                    children: Vec::new(),
                    events: Cell::new(0),
                    captures_pointer: true,
                }],
                events: Cell::new(0),
                captures_pointer: false,
            }],
            events: Cell::new(0),
            captures_pointer: false,
        };
        let event = ElementEvent::Cancel;
        let mut children = Vec::with_capacity(8);
        let allocation = children.as_ptr();

        assert_eq!(
            dispatch_captured_event_inner(&element, 7, &event, &mut children),
            Some(true)
        );
        assert!(children.is_empty());
        assert_eq!(children.as_ptr(), allocation);

        assert!(dispatch_event_inner(
            &element,
            Vec2d { x: 5.0, y: 5.0 },
            &event,
            &mut children
        ));
        assert!(children.is_empty());
        assert_eq!(children.as_ptr(), allocation);

        assert!(broadcast_event_inner(&element, &event, &mut children));
        assert!(children.is_empty());
        assert_eq!(children.as_ptr(), allocation);
    }
}
