use std::cell::Cell;
use std::rc::Rc;

thread_local! {
    static NEXT_FOCUS_REQUEST: Cell<u64> = const { Cell::new(0) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FocusRequest {
    Focus(u64),
    Unfocus(u64),
}

struct FocusNodeState {
    focused: Cell<bool>,
    request: Cell<Option<FocusRequest>>,
}

/// A handle for requesting and observing keyboard focus.
///
/// A focus node is attached to a focusable element and may be retained by its
/// owner across widget rebuilds. Calling [`request_focus`](Self::request_focus)
/// or [`unfocus`](Self::unfocus) records an imperative request. The framework
/// applies that request when it next synchronizes the element tree, ensuring
/// that at most one attached node owns keyboard and input-method events.
///
/// Focus nodes are intentionally UI-thread-local. Cloning a node creates
/// another handle to the same focus state; it does not create another focus
/// target.
#[derive(Clone)]
pub struct FocusNode {
    state: Rc<FocusNodeState>,
}

impl FocusNode {
    /// Creates a node that does not currently own focus.
    #[inline]
    pub fn new() -> Self {
        Self {
            state: Rc::new(FocusNodeState {
                focused: Cell::new(false),
                request: Cell::new(None),
            }),
        }
    }

    /// Requests keyboard focus for the element attached to this node.
    ///
    /// The request is resolved against the current element tree during the
    /// next event dispatch. If another node requests focus first, the most
    /// recent request wins.
    #[inline]
    pub fn request_focus(&self) {
        self.state
            .request
            .set(Some(FocusRequest::Focus(next_focus_request())));
    }

    /// Requests that this node relinquish keyboard focus.
    #[inline]
    pub fn unfocus(&self) {
        self.state
            .request
            .set(Some(FocusRequest::Unfocus(next_focus_request())));
    }

    /// Returns whether this node currently owns keyboard focus.
    #[inline]
    pub fn has_focus(&self) -> bool {
        self.state.focused.get()
    }

    #[inline]
    pub(crate) fn request(&self) -> Option<FocusRequest> {
        self.state.request.get()
    }

    #[inline]
    pub(crate) fn clear_request(&self) {
        self.state.request.set(None);
    }

    #[inline]
    pub(crate) fn set_focused(&self, focused: bool) {
        self.state.focused.set(focused);
    }

    #[inline]
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.state, &other.state)
    }
}

impl Default for FocusNode {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

fn next_focus_request() -> u64 {
    NEXT_FOCUS_REQUEST.with(|next| {
        let request = next
            .get()
            .checked_add(1)
            .expect("exhausted all focus request identities");
        next.set(request);
        request
    })
}

#[inline]
pub(crate) fn focus_request_generation() -> u64 {
    NEXT_FOCUS_REQUEST.with(Cell::get)
}