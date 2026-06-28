use crate::base::BuildContext;
use crate::Element;

/// Trait for elements that can be updated in-place from a new widget description.
///
/// When a parent element rebuilds, instead of destroying and recreating child elements,
/// the reconciliation system checks if the existing element can be updated in-place
/// (same type + same key). This preserves nested state, GPU resources, and reduces allocations.
pub trait Reconcilable {
    /// Returns the key of this element, if any.
    /// Keys are used for identity matching during reconciliation.
    fn key(&self) -> Option<crate::key::Key> {
        None
    }

    /// Returns this element as `&dyn std::any::Any` for downcasting.
    /// Used by `update_from_widget` implementations to access concrete element fields.
    ///
    /// Each element type should implement this as `fn as_any(&self) -> &dyn Any { self }`.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Try to update this element in-place from a new element of the same type.
    ///
    /// Called when the reconciliation system determines that the old and new elements
    /// were created by the same widget type (via `debug_name` and `key` matching).
    ///
    /// Returns `true` if the element was successfully updated in-place.
    /// Returns `false` if the update failed — the caller should replace the element entirely.
    ///
    /// The type check is performed by the caller before invoking this method,
    /// so implementations can assume the new element is of a compatible type.
    ///
    /// Default returns `false` (always replace — safe for leaf elements with no reconcilable state).
    fn update_from_widget(&self, _new_element: &dyn Element, _ctx: &BuildContext) -> bool {
        false
    }
}
