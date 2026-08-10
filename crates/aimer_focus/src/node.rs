use std::cell::Cell;
use std::rc::Rc;

thread_local! {
    static NEXT_FOCUS_REQUEST: Cell<u64> = const { Cell::new(0) };
}

/// An imperative focus change recorded on a [`FocusNode`], tagged with the
/// order in which it was made.
///
/// Requests are resolved in issue order, so the newest request of a frame wins
/// regardless of where its node sits in the tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusRequest {
    /// The node asked to become the focus owner.
    Focus(u64),
    /// The node asked to relinquish focus.
    Unfocus(u64),
}

impl FocusRequest {
    /// Returns the issue order of this request.
    #[inline]
    pub const fn order(self) -> u64 {
        match self {
            Self::Focus(order) | Self::Unfocus(order) => order,
        }
    }
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
///
/// ```
/// use aimer_focus::FocusNode;
///
/// let node = FocusNode::new();
/// assert!(!node.has_focus());
///
/// node.request_focus();
/// assert!(node.request().is_some());
/// ```
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

    /// Returns the pending request recorded on this node, if any.
    ///
    /// This is framework plumbing: the focus manager reads pending requests
    /// while resolving the focus owner for a frame.
    #[inline]
    pub fn request(&self) -> Option<FocusRequest> {
        self.state.request.get()
    }

    /// Drops the pending request recorded on this node.
    ///
    /// This is framework plumbing, called once a request has been taken into
    /// account so it is never applied twice.
    #[inline]
    pub fn clear_request(&self) {
        self.state.request.set(None);
    }

    /// Records whether this node owns keyboard focus.
    ///
    /// This is framework plumbing: ownership is decided by the focus manager,
    /// which keeps the flag on every node it hands focus to or takes it from.
    #[inline]
    pub fn set_focused(&self, focused: bool) {
        self.state.focused.set(focused);
    }

    /// Returns whether both handles refer to the same focus target.
    ///
    /// Identity is by shared state, not by value: cloning a node yields a
    /// handle that compares equal here, while a separately constructed node
    /// never does.
    #[inline]
    pub fn ptr_eq(&self, other: &Self) -> bool {
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

/// Returns the number of focus requests made on this thread so far.
///
/// The counter only ever grows, so comparing it against a previously observed
/// value tells the framework whether any node asked for a focus change since
/// then. An idle frame therefore costs a single integer comparison instead of a
/// tree walk.
#[inline]
pub fn focus_request_generation() -> u64 {
    NEXT_FOCUS_REQUEST.with(Cell::get)
}
