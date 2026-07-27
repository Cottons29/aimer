use crate::{Element, ElementId, Key};

pub trait VisitorElement {
    #[allow(unused_variables)]
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {}
    fn debug_name(&self) -> &'static str;

    /// Returns the `TypeId` of the concrete element type, enabling runtime
    /// type checks without the `Reconcilable` trait. Used by the state-carrying
    /// logic to identify `StatefulElement`s in the element tree.
    fn element_type_id(&self) -> std::any::TypeId {
        // Default returns a dummy that never matches any real element type.
        std::any::TypeId::of::<()>()
    }

    /// Returns the key used to preserve logical identity during reconciliation.
    fn reconciliation_key(&self) -> Option<&Key> {
        None
    }

    #[doc(hidden)]
    fn element_id(&self) -> Option<ElementId> {
        None
    }

    #[doc(hidden)]
    fn set_element_id(&self, _id: ElementId) {}
}
