use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;

use crate::Element;
use crate::components::element::VisitorElement;

/// Identifies one pointer independently of pointers from other input sources.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PointerKey {
    /// The device class that produced the pointer.
    pub source: PointerSource,
    /// The source-local pointer identifier.
    pub id: u64,
}

impl PointerKey {
    /// Creates a pointer key from its source and source-local identifier.
    #[inline]
    pub const fn new(source: PointerSource, id: u64) -> Self {
        Self { source, id }
    }
}

/// Requests a change to persistent pointer ownership after event handling.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CaptureRequest {
    /// Leaves pointer ownership unchanged.
    #[default]
    None,
    /// Routes subsequent events for this pointer to the requesting element.
    Capture(PointerKey),
    /// Releases this pointer if it is currently captured.
    Release(PointerKey),
}

/// The independent effects produced by an element event handler.
///
/// Consumption controls event propagation, while redraw indicates that visual
/// state changed. Keeping these effects independent allows a handler to report
/// either or both without allocating or borrowing the incoming event.
#[must_use = "event results may contain propagation or redraw requests"]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventResult {
    consumed: bool,
    needs_redraw: bool,
    capture_request: CaptureRequest,
}

impl EventResult {
    /// Creates a result that leaves propagation and application state unchanged.
    #[inline]
    pub const fn ignored() -> Self {
        Self {
            consumed: false,
            needs_redraw: false,
            capture_request: CaptureRequest::None,
        }
    }

    /// Creates a result that stops normal event propagation.
    #[inline]
    pub const fn consumed() -> Self {
        Self {
            consumed: true,
            ..Self::ignored()
        }
    }

    /// Creates a non-consuming result that requests a redraw.
    #[inline]
    pub const fn redraw() -> Self {
        Self {
            needs_redraw: true,
            ..Self::ignored()
        }
    }

    /// Returns whether normal event propagation should stop.
    #[inline]
    pub const fn is_consumed(self) -> bool {
        self.consumed
    }

    /// Returns whether handling changed visual state that must be redrawn.
    #[inline]
    pub const fn needs_redraw(self) -> bool {
        self.needs_redraw
    }

    /// Adds a redraw request while preserving all other effects.
    #[inline]
    pub const fn with_redraw(mut self) -> Self {
        self.needs_redraw = true;
        self
    }

    /// Requests persistent ownership of `pointer` by the producing element.
    #[inline]
    pub const fn with_pointer_capture(mut self, pointer: PointerKey) -> Self {
        self.capture_request = CaptureRequest::Capture(pointer);
        self
    }

    /// Requests release of `pointer` after this result is processed.
    #[inline]
    pub const fn with_pointer_release(mut self, pointer: PointerKey) -> Self {
        self.capture_request = CaptureRequest::Release(pointer);
        self
    }

    /// Returns the requested pointer-ownership change.
    #[inline]
    pub const fn capture_request(self) -> CaptureRequest {
        self.capture_request
    }

    /// Removes the ownership request after a dispatcher has applied it.
    ///
    /// Consumption and redraw are preserved so nested dispatchers can return
    /// ordinary effects without accidentally replaying ownership changes in a
    /// parent registry.
    #[inline]
    pub const fn without_capture_request(mut self) -> Self {
        self.capture_request = CaptureRequest::None;
        self
    }

    /// Combines effects produced along one dispatch path.
    ///
    /// Consumption and redraw are accumulated independently.
    #[inline]
    pub const fn merge(self, other: Self) -> Self {
        Self {
            consumed: self.consumed || other.consumed,
            needs_redraw: self.needs_redraw || other.needs_redraw,
            capture_request: match self.capture_request {
                CaptureRequest::None => other.capture_request,
                request => request,
            },
        }
    }
}

impl From<bool> for EventResult {
    #[inline]
    fn from(consumed: bool) -> Self {
        if consumed {
            Self::consumed()
        } else {
            Self::ignored()
        }
    }
}

// Event capabilities
pub trait EventElement: VisitorElement {
    /// Called when a pointer event hits this element.
    /// Return the independent effects produced while handling it.
    fn on_event(&self, _event: &ElementEvent) -> EventResult {
        EventResult::ignored()
    }


    /// Visit children for event dispatch. By default delegates to
    /// `visit_children`. Override this when `visit_children` is not
    /// implemented (e.g. because the element handles its own child
    /// rendering) but events still need to reach the children.
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.visit_children(visitor);
    }
}

#[cfg(test)]
mod tests {
    use aimer_events::pointer::PointerSource;

    use super::{CaptureRequest, EventResult, PointerKey};

    #[test]
    fn event_result_combines_consumption_and_redraw() {
        let result = EventResult::consumed().with_redraw();

        assert!(result.is_consumed());
        assert!(result.needs_redraw());
    }

    #[test]
    fn merge_preserves_independent_flags() {
        let consumed = EventResult::consumed();
        let redraw = EventResult::redraw();

        let result = consumed.merge(redraw);

        assert!(result.is_consumed());
        assert!(result.needs_redraw());
    }

    #[test]
    fn ignored_event_result_has_no_effect() {
        assert_eq!(EventResult::ignored(), EventResult::default());
        assert!(!EventResult::ignored().is_consumed());
        assert!(!EventResult::ignored().needs_redraw());
    }

    #[test]
    fn capture_effect_preserves_consumption_and_redraw() {
        let pointer = PointerKey::new(PointerSource::Touch, 4);
        let result = EventResult::consumed()
            .with_redraw()
            .with_pointer_capture(pointer);

        assert!(result.is_consumed());
        assert!(result.needs_redraw());
        assert_eq!(result.capture_request(), CaptureRequest::Capture(pointer));
    }

    #[test]
    fn first_capture_effect_wins_when_results_merge() {
        let deepest = PointerKey::new(PointerSource::Touch, 1);
        let ancestor = PointerKey::new(PointerSource::Mouse, 1);

        let result = EventResult::ignored()
            .with_pointer_capture(deepest)
            .merge(EventResult::ignored().with_pointer_release(ancestor));

        assert_eq!(result.capture_request(), CaptureRequest::Capture(deepest));
    }
}
