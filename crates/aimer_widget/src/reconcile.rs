use crate::base::BuildContext;
use crate::components::element::Element;

/// Try to reconcile an existing element with a new element of the same type.
///
/// If the existing element's `debug_name` and `key` match the new element's,
/// the existing element is updated in-place via `update_from_widget`, preserving
/// its internal state, child elements, and any GPU resources.
///
/// Returns `true` if the existing element was updated in-place (caller keeps using it).
/// Returns `false` if the types/keys don't match — caller must replace.
pub fn try_update_element(
    existing: &dyn Element,
    new_element: &dyn Element,
    ctx: &BuildContext,
) -> bool {
    // Type check: same widget type produces same element debug_name
    if existing.debug_name() != new_element.debug_name() {
        return false;
    }

    // Key check: both must have matching keys (or both None)
    if existing.key() != new_element.key() {
        return false;
    }

    // Delegate to the element's own update logic.
    let updated = existing.update_from_widget(new_element, ctx);

    if !updated {
        // `existing` is about to be replaced wholesale by `new_element`. Before the
        // caller swaps it in, walk the matched child subtrees so nested elements can
        // carry live runtime state (e.g. a `Scrollable`'s scroll offset) from the old
        // element into the freshly built one. Without this, any rebuild at an ancestor
        // — such as a window resize re-running a `MediaQuery`-driven `build()` — throws
        // the whole subtree away and snaps the viewport back to the top, even though
        // the element itself knows how to adopt the previous state in its
        // `update_from_widget`.
        carry_child_state(existing, new_element, ctx);
    }

    updated
}

/// Recurse into the matched children of an element that is being replaced, letting
/// each nested element carry its runtime state (via `update_from_widget`) from the
/// old subtree into the new one.
///
/// Children are enumerated through `event_children` (the same accessor used for
/// event dispatch, which single-child wrappers like `Container` already expose) and
/// paired positionally. Elements whose `event_children` is empty (e.g. `Scrollable`
/// itself) stop the recursion, so it never walks below a state-owning leaf.
///
/// ponytail: pairing is positional; a keyed sibling list that reorders won't match
/// up and simply carries nothing across — safe, just no transfer for that node.
fn carry_child_state(existing: &dyn Element, new_element: &dyn Element, ctx: &BuildContext) {
    let old_children = event_children_of(existing);
    if old_children.is_empty() {
        return;
    }

    let new_children = event_children_of(new_element);

    for (old_child, new_child) in old_children.iter().zip(new_children.iter()) {
        try_update_element(*old_child, *new_child, ctx);
    }
}

/// Collect an element's children as exposed for event dispatch. This is the
/// traversal `carry_child_state` walks to reach nested state-owning elements
/// (e.g. a `Scrollable` inside a `Container`), so single-child wrappers must
/// surface their child here for scroll state to survive an ancestor rebuild.
fn event_children_of<'a>(element: &'a dyn Element) -> Vec<&'a dyn Element> {
    let mut children: Vec<&dyn Element> = Vec::new();
    element.event_children(&mut |c| children.push(c));
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::event_element::EventElement;
    use crate::components::layout_element::LayoutElement;
    use crate::components::rebuildable::Rebuildable;
    use crate::components::reconcilable::Reconcilable;
    use crate::components::visitor_element::VisitorElement;
    use crate::Drawable;
    use std::any::Any;

    // Minimal fake elements: all trait methods except the listed ones use defaults.
    struct Leaf(&'static str);
    impl VisitorElement for Leaf {
        fn debug_name(&self) -> &'static str {
            self.0
        }
    }
    impl Drawable for Leaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl EventElement for Leaf {}
    impl LayoutElement for Leaf {}
    impl Rebuildable for Leaf {}
    impl Reconcilable for Leaf {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // A single-child wrapper (like `Container`) that surfaces its child only
    // through `visit_children` — exactly the shape that previously hid the
    // nested scrollable from reconciliation.
    struct Wrapper(Box<dyn Element>);
    impl VisitorElement for Wrapper {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.0.as_ref());
        }
        fn debug_name(&self) -> &'static str {
            "Wrapper"
        }
    }
    impl Drawable for Wrapper {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl EventElement for Wrapper {}
    impl LayoutElement for Wrapper {}
    impl Rebuildable for Wrapper {}
    impl Reconcilable for Wrapper {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // Contract the carry-over relies on: a wrapper that only implements
    // `visit_children` still surfaces its child through `event_children` (via the
    // default delegation), so `carry_child_state` can descend to a nested
    // state-owning element and hand its scroll offset to the rebuilt one.
    #[test]
    fn event_children_reach_wrapper_child() {
        let wrapper = Wrapper(Box::new(Leaf("Scrollable")));
        let children = event_children_of(&wrapper);
        assert_eq!(children.len(), 1, "wrapper must expose its single child");
        assert_eq!(children[0].debug_name(), "Scrollable");
    }
}
