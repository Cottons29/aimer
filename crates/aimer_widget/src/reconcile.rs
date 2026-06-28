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
    existing.update_from_widget(new_element, ctx)
}
