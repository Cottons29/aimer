use aimer_events::element::ElementEvent;
use crate::components::element::VisitorElement;
use crate::Element;

// Event capabilities
pub trait EventElement: VisitorElement {


    /// Called when a pointer event hits this element.
    /// Return `true` if the event was consumed.
    fn on_event(&self, _event: &ElementEvent) -> bool {
        false
    }



    /// Visit children for event dispatch. By default delegates to `visit_children`.
    /// Override this when `visit_children` is not implemented (e.g. because the element
    /// handles its own child rendering) but events still need to reach the children.
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.visit_children(visitor);
    }
}