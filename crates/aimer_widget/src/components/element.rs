use crate::Drawable;
use crate::base::*;
use crate::components::event_element::EventElement;
use crate::components::layout_element::LayoutElement;
use crate::components::rebuildable::Rebuildable;
pub(crate) use crate::components::visitor_element::VisitorElement;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;

impl<T> Element for T where T: VisitorElement + EventElement + LayoutElement + Rebuildable + Drawable
{}

pub trait Element: VisitorElement + EventElement + LayoutElement + Rebuildable + Drawable {
    /// Converts the implementing instance into a `Box` containing a dynamic trait object of type `Element`.
    ///
    /// This method is useful when you want to box a type that implements the `Element` trait to enable
    /// dynamic dispatch at runtime. It requires the size of the type to be known at compile time (`Self: Sized`)
    /// and the type to have a `'static` lifetime.
    ///
    /// # Returns
    ///
    /// A `Box` containing the implementing instance as a dynamic `Element` trait object.
    ///
    /// # Example
    ///
    /// ```rust ignore
    /// struct MyElement;
    ///
    /// impl Element for MyElement {
    ///     // implementation details
    /// }
    ///
    /// let element = MyElement;
    /// let boxed_element: Box<dyn Element> = element.boxed();
    /// ```
    ///
    /// # Constraints
    ///
    /// - The type must implement the `Element` trait.
    /// - The type must be `Sized` and `'static`.
    fn boxed(self) -> Box<dyn Element>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }
}

impl VisitorElement for Box<dyn Element> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().visit_children(visitor)
    }
    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }
    fn element_type_id(&self) -> std::any::TypeId {
        self.as_ref().element_type_id()
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

    fn flex(&self) -> Option<f32> {
        self.as_ref().flex()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref().get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.as_ref().invalidate_layout()
    }

    // Delegate to the inner concrete element instead of falling back to the
    // trait default. The default `pos_start_end` is derived from `pos()` +
    // `size()`, but many elements (containers, stateful/stateless elements)
    // never report an absolute `pos()` and instead override `pos_start_end`
    // directly to return the on-screen bounds they record while drawing. Since
    // children are almost always held as `Box<dyn Element>`, forgetting to
    // delegate here made every boxed element report `None`, so `dispatch_event`
    // fell into its "no position" branch and invoked `on_event` regardless of
    // the pointer location — e.g. an opaque header on a top `Stack` layer
    // swallowed scrolls across the entire window instead of only over itself.
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.as_ref().pos_start_end()
    }
}

impl Rebuildable for Box<dyn Element> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.as_ref().rebuild_if_dirty(ctx)
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        self.as_ref().option_any()
    }

    fn mark_needs_rebuild(&self) {
        self.as_ref().mark_needs_rebuild()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;

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

        assert!(element.option_any().is_some_and(|value| value.is::<DowncastableElement>()));
    }
}
