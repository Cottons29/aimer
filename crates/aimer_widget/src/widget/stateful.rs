use crate::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement, Widget, base::*,
};
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_events::window::request_animation_frame;
use aimer_utils::error;
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::any::Any;
use std::cell::{Cell, RefCell, UnsafeCell};
use std::panic::Location;
use std::rc::Rc;

trait FetchAdd {
    fn fetch_add(&self, val: u64) -> u64;
}

impl FetchAdd for Cell<u64> {
    fn fetch_add(&self, val: u64) -> u64 {
        self.get().wrapping_add(val)
    }
}

/// A `Send + Sync` wrapper around `UnsafeCell<Box<dyn Element>>`.
/// Safety: the rendering pipeline is single-threaded, so concurrent access does not occur.
///
/// `pub(crate)` so `StatelessElement` can reuse the same swappable-child slot
/// (needed so `visit_children<'a>` can hand out `&'a` refs to a child that may
/// be replaced on rebuild).
pub(crate) struct SyncChild(pub(crate) UnsafeCell<Box<dyn Element>>);
unsafe impl Send for SyncChild {}
unsafe impl Sync for SyncChild {}

/// A `Send + Sync` wrapper around `UnsafeCell<S>` for state storage.
/// Safety: the rendering pipeline is single-threaded. Mutations are applied
/// exclusively during `rebuild_if_dirty` on the render thread, and reads
/// happen only on the render thread (event handlers, build).
struct SyncState<S>(UnsafeCell<S>);
unsafe impl<S: Send> Send for SyncState<S> {}
unsafe impl<S: Send> Sync for SyncState<S> {}

/// A `Sync` wrapper for the rebuild closure so `StatefulElement` can replace
/// it during `adopt_state_from` (reconciliation) without requiring `&mut self`.
/// Safety: the rendering pipeline is single-threaded; the closure is only
/// invoked from `rebuild_if_dirty` on the render thread.
struct SyncRebuildFn(UnsafeCell<Rc<RebuildCallBack>>);
unsafe impl Send for SyncRebuildFn {}
unsafe impl Sync for SyncRebuildFn {}

/// A `Send + Sync` wrapper around the type-erased state cell (`Rc<dyn Any>`,
/// concretely `Rc<SyncState<W::State>>`). Kept so a reconciling element can hand
/// its freshly-built state to the live element for a config refresh, without
/// `StatefulElement` being generic over `W`.
///
/// Wrapped in `UnsafeCell` so `adopt_state_from` can *repoint* it to the OLD
/// element's state cell alongside `rebuild_fn`: after adoption the live element
/// reads the OLD cell, so its config-refresh machinery (`state_any` +
/// `adopt_config_fn`) must reference that SAME cell — otherwise a later
/// reconcile that uses this element as the `old` side would refresh an
/// orphaned cell while the live `rebuild_fn` keeps reading a stale one.
/// Safety: the rendering pipeline is single-threaded.
struct SyncStateAny(UnsafeCell<Rc<dyn Any>>);
unsafe impl Send for SyncStateAny {}
unsafe impl Sync for SyncStateAny {}

/// Type-erased "copy the widget configuration from another element's state into
/// mine" hook. Captures this element's state cell (typed as `W::State`);
/// downcasts the supplied `&dyn Any` (another element's `SyncState<W::State>`)
/// and calls `State::adopt_config_from`. No-op when the concrete types differ.
type AdoptConfigCallBack = dyn Fn(&dyn Any);

/// A `Send + Sync` wrapper around the config-adoption closure.
///
/// Wrapped in `UnsafeCell` for the same reason as [`SyncStateAny`]: it must be
/// repointed to the OLD element's cell during `adopt_state_from` so it stays in
/// sync with the adopted `rebuild_fn`.
/// Safety: invoked only during single-threaded reconciliation.
struct SyncAdoptConfigFn(UnsafeCell<Rc<AdoptConfigCallBack>>);
unsafe impl Send for SyncAdoptConfigFn {}
unsafe impl Sync for SyncAdoptConfigFn {}

/// Type-erased mutation closure sent through the channel.
type StateMutation<S> = Box<dyn FnOnce(&mut S)>;

/// A handle that allows StatefulWidgets to trigger state mutations and rebuilds.
/// This is the Rust equivalent of Flutter's `setState`.
///
/// Instead of locking a `Mutex`, mutations are sent as closures through a
/// `crossbeam_channel` and applied on the render thread during the next
/// rebuild. This eliminates the possibility of deadlocks.
pub struct StateUpdater<S> {
    inner: Option<StateUpdaterInner<S>>,
}

unsafe impl<S> Send for StateUpdater<S> {}
unsafe impl<S> Sync for StateUpdater<S> {}

struct StateUpdaterInner<S> {
    /// Channel sender for queueing state mutations.
    tx: Sender<StateMutation<S>>,
    /// Shared state for synchronous reads on the render thread.
    state: Rc<SyncState<S>>,
    dirty: Rc<Cell<bool>>,
}

impl<S> Clone for StateUpdater<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.as_ref().map(|inner| StateUpdaterInner {
                tx: inner.tx.clone(),
                state: inner.state.clone(),
                dirty: inner.dirty.clone(),
            }),
        }
    }
}

impl<S: 'static> StateUpdater<S> {
    /// Create a new `StateUpdater` from a channel sender, shared state, and a dirty flag.
    #[inline]
    fn with(tx: Sender<StateMutation<S>>, state: Rc<SyncState<S>>, dirty: Rc<Cell<bool>>) -> Self {
        Self { inner: Some(StateUpdaterInner { tx, state, dirty }) }
    }

    #[track_caller]
    pub fn read_state(&self) -> &S {
        match self.inner.as_ref().map(|inner| inner.state.clone()) {
            Some(state) => unsafe { &*state.0.get() },
            None => {
                let loc = Location::caller();
                error!("Attempted to read state from an uninitialized StateUpdater");
                self.beautiful_error(loc);
                panic!("State is not initialized (see error above)")
            }
        }
    }

    /// Create an empty `StateUpdater` that is not yet initialized.
    /// Calling `set_state` or `read` on an empty updater will panic.
    ///
    /// It has the same functionality as `StateUpdater<S>::empty`
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::empty()
    }

    /// Create an empty `StateUpdater` that is not yet initialized.
    /// Calling `set_state` or `read` on an empty updater will panic.
    ///
    /// It has the same functionality as `StateUpdater<S>::new`
    #[inline]
    pub fn empty() -> Self {
        Self { inner: None }
    }

    /// Mutate the state using a value that is cloned once and moved into the
    /// mutation closure. This avoids the double-clone that would otherwise be
    /// needed when calling `set_state` from inside an `Fn` closure:
    ///
    /// ```ignore
    /// // Before (two clones):
    /// let id = item.id.clone();          // clone 1 – for the Fn capture
    /// move || {
    ///     let id = id.clone();           // clone 2 – for the 'static FnOnce
    ///     updater.set_state(move |s| { /* use id */ });
    /// }
    ///
    /// // After (one clone):
    /// let id = item.id.clone();          // clone 1 – for the Fn capture
    /// move || {
    ///     updater.set_state_with(id.clone(), |s, id| { /* use id */ });
    /// }
    /// ```
    ///
    /// Wait — that's still `id.clone()`. The real win is that `set_state_with`
    /// accepts a *reference* and clones internally, so from an `Fn` closure you
    /// can write:
    ///
    /// ```ignore
    /// let id = item.id.clone();          // clone 1 – captured by the Fn
    /// move || {
    ///     updater.set_state_with(&id, |s, id| { /* use owned id */ });
    /// }
    /// ```
    #[track_caller]
    pub fn set_state_with<V: Clone + Send + 'static>(
        &self,
        value: &V,
        f: impl FnOnce(&mut S, V) + Send + 'static,
    ) {
        let owned = value.clone();
        self.set_state(move |s| f(s, owned));
    }

    /// Mutate the state by sending a closure through the channel.
    /// The mutation will be applied on the render thread during the next rebuild.
    /// This is deadlock-free: it never acquires a lock.
    ///
    /// Multiple calls between frames are coalesced: the dirty flag is set once,
    /// and only a single rebuild happens during the next `draw`.
    #[track_caller]
    pub fn set_state(&self, f: impl FnOnce(&mut S) + 'static) {
        let inner = match self.inner.as_ref() {
            Some(inner) => inner,
            None => {
                let loc = Location::caller();
                self.beautiful_error(loc);
                panic!("State is not initialized (see error above)");
            }
        };
        // Send the mutation through the channel — never blocks, never deadlocks.
        let _ = inner.tx.send(Box::new(f));
        // Only request a redraw if this is the first set_state since the last rebuild.
        // This coalesces multiple set_state calls into a single redraw request.
        if !inner.dirty.replace(true) {
            request_animation_frame()
        }
    }

    /// Read the current state without marking dirty.
    ///
    /// Safety: this reads from the `UnsafeCell` directly. It is safe because
    /// reads only happen on the render thread (event handlers, build methods),
    /// and mutations are also applied exclusively on the render thread during
    /// `rebuild_if_dirty`.
    #[track_caller]
    pub fn read<R>(&self, f: impl FnOnce(&S) -> R) -> R {
        let inner = match self.inner.as_ref() {
            Some(inner) => inner,
            None => {
                let loc = Location::caller();
                #[cfg(not(target_os = "ios"))]
                self.beautiful_error(loc);
                panic!("State is not initialized (see error above)");
            }
        };
        // Safety: single-threaded rendering pipeline — no concurrent mutation.
        let state = unsafe { &*inner.state.0.get() };
        f(state)
    }

    #[inline]
    fn beautiful_error(&self, loc: &Location) {
        {
            const BRACE: &str = "{";
            error!(
                "State is not initialized and trying to read or update at {}:{}
   {}
   {} impl State<YourStatefulWidget> for YourWidgetState {BRACE}
   {}
   {}     fn init_state(&mut self, _updater: StateUpdater<Self>)
   {}         where
   {}             Self: Sized,
   {}         {{
   {}             self.updater = _updater;
   {}             {}
   {}         }}
   {}
   {}: call `self.updater = _updater` inside `init_state`
",
                loc.file(),
                loc.line(),
                "|",
                "|",
                "|",
                "|",
                "|",
                "|",
                "|",
                "|",
                "|",
                "^^^^^^^^^^^^^^^^^^^^^^^^^ add this line to prevent panic",
                "|",
                "|",
                "help",
            );
        }
    }
}

pub trait StatefulWidget: Sized {
    type State: State<Self>;

    fn widget(&self) -> &Self
    where
        Self: Sized,
    {
        self
    }

    fn create_state(&self) -> Self::State;
}

pub trait State<W: StatefulWidget> {
    /// Called once after the state is created, providing a [`StateUpdater`] handle.
    /// Store the updater in your state struct to later call `set_state()` from
    /// event handlers or callbacks — similar to Flutter's `setState`.
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized;

    /// Called during reconciliation when a parent rebuild produces a freshly
    /// built element for the *same* stateful widget (e.g. a window resize, or a
    /// parent `set_state` that re-emits this widget with new props).
    ///
    /// The framework preserves this (the *live*) state object — keeping runtime
    /// fields such as hover/focus/scroll/animation progress — but the freshly
    /// built `new` state carries the up-to-date widget *configuration* (the
    /// props passed down from the parent, e.g. a `TextButton`'s `style` /
    /// `hover_style` / `on_press`, or a selected/disabled flag). Copy those
    /// configuration fields out of `new` into `self` here so the widget renders
    /// with the current configuration while retaining its runtime state.
    fn adopt_config_from(&mut self, _new: &Self) {}

    /// Override this method to build the widget
    fn build(&self, ctx: &BuildContext) -> impl Widget;
}
pub type RebuildCallBack = dyn Fn(&BuildContext) -> Box<dyn Element>;
pub struct StatefulElement {
    child: SyncChild,
    /// Marked when this element (or its state's own `set_state`) requests a
    /// rebuild. Wrapped in `RefCell` so `adopt_state_from` can *repoint* it to
    /// the OLD element's flag during reconciliation — see `adopt_state_from`
    /// for why the live element must share the flag the preserved state's
    /// captured updater flips.
    pub dirty: RefCell<Rc<Cell<bool>>>,
    rebuild_fn: SyncRebuildFn,
    /// Monotonically increasing generation counter. Incremented on each rebuild
    /// so that multiple `set_state` calls between frames only trigger one rebuild.
    rebuild_generation: Cell<u64>,
    /// The generation at which the last rebuild was performed.
    last_rebuilt_generation: Cell<u64>,
    // #[cfg(debug_assertions)]
    debug_name: Cell<&'static str>,
    pub key: Option<crate::key::Key>,
    pub bounds: Cell<Option<(Vec2d, Vec2d)>>,
    /// This element's own state cell, type-erased, so a reconciling element can
    /// hand it to the live element's `adopt_config_fn` for a config refresh.
    state_any: SyncStateAny,
    /// Copies widget configuration from another element's state (passed as
    /// `&dyn Any`) into this element's live state via `State::adopt_config_from`.
    adopt_config_fn: SyncAdoptConfigFn,
}

impl StatefulElement {
    pub fn boxed(self) -> Box<dyn Element> {
        Box::new(self)
    }
}

impl StatefulElement {
    /// Create a new StatefulElement from a StatefulWidget.
    /// Returns the element and a StateUpdater that can be used in callbacks.
    pub fn new_with_name<W: StatefulWidget + 'static>(
        widget: &W,
        ctx: &BuildContext,
        debug_name: &'static str,
        key: Option<crate::key::Key>,
    ) -> (Self, StateUpdater<W::State>)
    where
        W::State: 'static,
    {
        let (mut element, updater) = Self::new(widget, ctx);
        element.debug_name.set(debug_name);
        element.key = key;
        (element, updater)
    }

    pub fn new<W: StatefulWidget + 'static>(
        widget: &W,
        ctx: &BuildContext,
    ) -> (Self, StateUpdater<W::State>)
    where
        W::State: 'static,
    {
        let state = widget.create_state();
        let dirty = Rc::new(Cell::new(false));

        // Create the channel for state mutations.
        #[allow(clippy::type_complexity)]
        let (tx, rx): (Sender<StateMutation<W::State>>, Receiver<StateMutation<W::State>>) =
            unbounded();

        let state_cell = Rc::new(SyncState(UnsafeCell::new(state)));

        // Create the updater and pass it into init_state.
        let init_updater = StateUpdater::with(tx.clone(), state_cell.clone(), dirty.clone());

        {
            // Safety: single-threaded — we are the only accessor during construction.
            let s = unsafe { &mut *state_cell.0.get() };
            s.init_state(init_updater.clone());
        }

        let state_for_build = state_cell.clone();
        let rx_for_rebuild = rx;
        let rebuild_fn: Rc<RebuildCallBack> = Rc::new(move |ctx| {
            // Drain all pending mutations from the channel before rebuilding.
            let s = unsafe { &mut *state_for_build.0.get() };
            while let Ok(mutation) = rx_for_rebuild.try_recv() {
                mutation(s);
            }
            let child_widget = s.build(ctx);
            Widget::to_element(&child_widget, ctx)
        });

        let child = {
            // Safety: single-threaded — initial build during construction.
            let s = unsafe { &*state_cell.0.get() };
            Widget::to_element(&s.build(ctx), ctx)
        };

        // Type-erased handle to this element's state, plus a closure that can
        // pull configuration out of *another* element's state (of the same
        // `W::State` type) into this one. Together these let reconciliation
        // refresh a preserved live state's widget props without
        // `StatefulElement` being generic over `W`.
        let state_any: Rc<dyn Any> = state_cell.clone();
        let state_for_config = state_cell.clone();
        let adopt_config_fn: Rc<AdoptConfigCallBack> = Rc::new(move |new_any: &dyn Any| {
            if let Some(new_cell) = new_any.downcast_ref::<SyncState<W::State>>() {
                // Safety: single-threaded reconciliation; the live state is not
                // otherwise borrowed while we copy the fresh config into it.
                let old_state = unsafe { &mut *state_for_config.0.get() };
                let new_state = unsafe { &*new_cell.0.get() };
                old_state.adopt_config_from(new_state);
            }
        });

        let updater = StateUpdater::with(tx, state_cell, dirty.clone());

        let element = StatefulElement {
            child: SyncChild(UnsafeCell::new(child)),
            dirty: RefCell::new(dirty),
            rebuild_fn: SyncRebuildFn(UnsafeCell::new(rebuild_fn)),
            rebuild_generation: Cell::new(0),
            last_rebuilt_generation: Cell::new(0),
            debug_name: Cell::new("Unknown"),
            key: None,
            bounds: Cell::new(None),
            state_any: SyncStateAny(UnsafeCell::new(state_any)),
            adopt_config_fn: SyncAdoptConfigFn(UnsafeCell::new(adopt_config_fn)),
        };

        (element, updater)
    }

    /// Check if this element needs a rebuild and perform it if so.
    ///
    /// Uses element reconciliation: before replacing the child, tries to update it
    /// in-place via `try_update_element`. If the child's type and key match the new
    /// element's, the child is updated without replacement — preserving nested
    /// `StatefulElement` state, GPU resources, and reducing allocations.
    ///
    /// Before rebuilding itself, this method first walks the existing child tree
    /// to let any nested `StatefulElement`s rebuild independently. This avoids
    /// destroying and recreating the entire subtree when only a deeply-nested
    /// element's state has changed.
    pub fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        if !self.dirty.borrow().get() {
            // Self is clean — but a nested StatefulElement might be dirty.
            // Propagate rebuild through the existing child tree.
            let child = unsafe { &*self.child.0.get() };
            Self::propagate_rebuild(child.as_ref(), ctx);
            return;
        }

        // Coalesce: only rebuild once per generation bump.
        let current_gen = self.rebuild_generation.get();
        let last = self.last_rebuilt_generation.get();
        if current_gen == last && !self.dirty.borrow().get() {
            return;
        }

        // Build the new child element FIRST. Running our own `build` before
        // propagating the rebuild downward ensures any inherited state this
        // element provides via `ctx.insert_state` (e.g. a `Navigator` inserting
        // its `NavigatorController`) is re-published into the *current* frame's
        // context before descendants rebuild and look it up.
        //
        // Otherwise a nested consumer rebuilt during `propagate_rebuild` — such
        // as a header calling `NavigatorController::of` on a window resize,
        // where `mark_needs_rebuild` dirties the whole tree and the frame's
        // `BuildContext` starts with an empty `inherited_states` map — would
        // look up state the provider has not re-inserted yet this frame and
        // panic ("No Navigator found in context").
        let new_child = {
            let rf = unsafe { &*self.rebuild_fn.0.get() };
            rf(ctx)
        };

        // Then let nested dirty StatefulElements in the existing subtree rebuild
        // in-place, now that the parent-provided context is populated.
        {
            let child = unsafe { &*self.child.0.get() };
            Self::propagate_rebuild(child.as_ref(), ctx);
        }

        // Carry live state from nested StatefulElements in the old tree into the
        // freshly-built new tree before replacing. This preserves runtime state
        // (e.g. selected tab index, scroll position) across the rebuild.
        {
            let old_child = unsafe { &*self.child.0.get() };
            carry_child_state(old_child.as_ref(), new_child.as_ref(), ctx);
        }

        // Install the newly-built child, replacing the old subtree.
        // Safety: single-threaded rendering pipeline; old_child is not used past this point.
        unsafe {
            *self.child.0.get() = new_child;
        }

        self.dirty.borrow().set(false);
        self.rebuild_generation.fetch_add(1);
        self.last_rebuilt_generation.set(self.rebuild_generation.get());
    }

    /// Walk the element tree and rebuild any nested dirty `StatefulElement`s.
    /// This is called on the *existing* child tree so that inner stateful widgets
    /// can update in-place without the parent having to reconstruct the whole subtree.
    fn propagate_rebuild(element: &dyn Element, ctx: &BuildContext) {
        element.rebuild_if_dirty(ctx);
    }
}

/// If both elements are `StatefulElement`s with the same `debug_name`, adopt
/// the live state from `old` into `new`. This is the rescue path that runs
/// even when the element tree shape changed — it ensures nested stateful
/// widgets (e.g. tab buttons, form inputs) keep their runtime state across
/// a parent rebuild.
///
/// Safe to call on any pair: when both sides aren't matching
/// `StatefulElement`s, it's a no-op.
pub(crate) fn carry_stateful(old: &dyn Element, new: &dyn Element, ctx: &BuildContext) {
    let Some(old_ele) = old.option_any().and_then(|o| o.downcast_ref::<StatefulElement>()) else {
        return;
    };

    let Some(new_ele) = new.option_any().and_then(|o| o.downcast_ref::<StatefulElement>()) else {
        return;
    };
    if old_ele.debug_name.get() != new_ele.debug_name.get() || old_ele.key != new_ele.key {
        return;
    }
    new_ele.adopt_state_from(old_ele, ctx);
}

fn find_keyed_stateful<'a>(
    element: &'a dyn Element,
    key: &crate::key::Key,
    debug_name: &'static str,
) -> Option<&'a StatefulElement> {
    if let Some(stateful) =
        element.option_any().and_then(|value| value.downcast_ref::<StatefulElement>())
        && stateful.key.as_ref() == Some(key)
        && stateful.debug_name.get() == debug_name
    {
        return Some(stateful);
    }

    element_children(element)
        .into_iter()
        .find_map(|child| find_keyed_stateful(child, key, debug_name))
}

fn element_children(element: &dyn Element) -> smallvec::SmallVec<[&dyn Element; 8]> {
    let mut children: smallvec::SmallVec<[&dyn Element; 8]> = smallvec::SmallVec::new();
    element.event_children(&mut |child| children.push(child));
    element.visit_children(&mut |child| {
        if !children.iter().any(|existing| std::ptr::eq(*existing, child)) {
            children.push(child);
        }
    });
    children
}

/// Recurse into the matched children of an old and new element tree, letting
/// each nested `StatefulElement` carry its runtime state from the old subtree
/// into the new one.
///
/// Children are enumerated through `event_children` first (the same accessor
/// used for event dispatch). If that yields nothing, falls back to
/// `visit_children` (used by the visitor/layout system) so that elements like
/// scrollable containers — which hide children from event dispatch but expose
/// them for layout — still get their nested stateful state carried across.
pub(crate) fn carry_child_state(old: &dyn Element, new: &dyn Element, ctx: &BuildContext) {
    carry_keyed_child_state(old, new, ctx);
    carry_unkeyed_child_state(old, new, ctx);
}

fn carry_keyed_child_state(old_root: &dyn Element, new: &dyn Element, ctx: &BuildContext) {
    if let Some(new_stateful) =
        new.option_any().and_then(|value| value.downcast_ref::<StatefulElement>())
        && let Some(key) = new_stateful.key.as_ref()
    {
        if let Some(old_stateful) =
            find_keyed_stateful(old_root, key, new_stateful.debug_name.get())
        {
            new_stateful.adopt_state_from(old_stateful, ctx);
        }
        return;
    }

    for child in element_children(new) {
        carry_keyed_child_state(old_root, child, ctx);
    }
}

fn carry_unkeyed_child_state(old: &dyn Element, new: &dyn Element, ctx: &BuildContext) {
    if new
        .option_any()
        .and_then(|value| value.downcast_ref::<StatefulElement>())
        .is_some_and(|stateful| stateful.key.is_some())
    {
        return;
    }

    // Adopt state at this level first.
    // println!("Before carry_stateful");
    carry_stateful(old, new, ctx);
    // println!(" -> carry_stateful : success");

    // Try event_children first (primary traversal for reconciliation).
    // println!("Step 1");
    let old_children = element_children(old);
    // println!("Step 4");
    if old_children.is_empty() {
        return;
    }
    // println!("Step 5");

    let new_children = element_children(new);
    // println!("Step 8");

    for (old_child, new_child) in old_children.iter().zip(new_children.iter()) {
        // println!("Before call carry_child_state");
        carry_unkeyed_child_state(*old_child, *new_child, ctx);
        // println!("After call carry_child_state");
    }
    // println!("Step 9");
}

impl StatefulElement {
    /// Returns true if this element is marked dirty.
    pub fn is_dirty(&self) -> bool {
        self.dirty.borrow().get()
    }

    /// Adopt the live state from another `StatefulElement` of the same widget type.
    ///
    /// Transfers the `rebuild_fn` (which captures the state cell and mutation
    /// channel), inherits the `debug_name`, and marks this element dirty so
    /// `rebuild_if_dirty` re-generates the child tree from the preserved state
    /// on the next frame.
    ///
    /// Called by `update_from_widget` when a parent's reconciliation replaces an
    /// entire subtree — without this, a freshly-constructed `StatefulElement`
    /// (with `current_index: 0`) would shadow the live one (with `current_index: 2`).
    pub(crate) fn adopt_state_from(&self, old: &StatefulElement, ctx: &BuildContext) {
        // Safety: called only from `update_from_widget` during single-threaded
        // reconciliation, before the new element is visible to any other code.
        unsafe {
            // The rebuild closure captures the state cell and mutation channel.
            // Replacing it makes this element's build() read from the live state.
            // println!("adopt_state_from casting raw ptr");
            *self.rebuild_fn.0.get() = (*old.rebuild_fn.0.get()).clone();
        }
        // Inherit name so inspector and future reconciliation still match.
        self.debug_name.set(old.debug_name.get());

        // Adopt the OLD element's dirty flag so the *live* element
        *self.dirty.borrow_mut() = old.dirty.borrow().clone();

        // Refresh the *configuration* stored in the preserved live state from
        // the freshly-built element. We keep `old`'s state cell (its runtime
        // state — hover, scroll offset, selected tab, animation progress, …),
        // but that same cell also holds whatever props the widget copied from
        // its parent at `create_state` time (e.g. a `TextButton`'s `style` /
        // `hover_style` / `on_press`, a selected/disabled flag). Without this
        // refresh a widget re-emitted with different props after a parent
        // rebuild (a window resize, a parent `set_state`) would keep rendering
        // its *stale* props — the classic symptom being a tab whose highlight
        // stays stuck on the initially-selected button even though the live
        // selection moved on. `self` is the fresh element and carries the
        // up-to-date config in its state; hand it to `old`'s config hook.
        //
        // NOTE: this MUST run before we repoint `self.state_any` below, because
        // it uses `self`'s own (freshly-built) state as the *source* of the new
        // config.
        {
            // Safety: single-threaded reconciliation.
            let fresh_state: &dyn Any = unsafe { &*self.state_any.0.get() }.as_ref();
            let old_adopt = unsafe { &*old.adopt_config_fn.0.get() };
            old_adopt(fresh_state);
        }

        // Repoint this element's config-refresh machinery at the OLD state cell,
        // matching the `rebuild_fn` we just adopted. `rebuild_fn` now reads
        // `old`'s cell, but `self` was constructed with `state_any` /
        // `adopt_config_fn` bound to its OWN (now-orphaned) fresh cell. If we
        // left them pointing there, a *subsequent* reconcile that uses `self` as
        // the `old` side — which a single window resize does trigger (the eager
        // rebuild below reconciles this subtree, and the follow-up
        // `carry_child_state` pass reconciles it again) — would refresh the
        // ORPHANED cell while the live `rebuild_fn` keeps reading `old`'s cell,
        // so the freshly-built config would never reach what actually renders
        // and the selected/highlight styling would freeze on a stale value.
        // Safety: single-threaded reconciliation; not otherwise borrowed here.
        unsafe {
            *self.state_any.0.get() = (*old.state_any.0.get()).clone();
            *self.adopt_config_fn.0.get() = (*old.adopt_config_fn.0.get()).clone();
        }

        // Materialize the adopted state *immediately*, during reconciliation —
        // do not defer to the next `draw`.
        //
        // The child we were constructed with was built from this widget's
        // *initial* state (e.g. `current_index: 0`). Merely flagging `dirty` and
        // waiting for `draw` → `rebuild_if_dirty` to regenerate it is not
        // enough: on a window resize the rebuilt element is frequently *culled*
        // by a scroll viewport (its `draw`, and hence `rebuild_if_dirty`, never
        // runs) or sits behind a wrapper whose rebuild cascade — which walks
        // `visit_children` — never reaches it (containers such as `Container`
        // and `Row`/`Column` expose their children only through
        // `event_children`). In those cases the adopted `rebuild_fn` would never
        // execute and the user's state would silently snap back to the initial
        // value. Regenerating the child here, against the current
        // `BuildContext`, guarantees the live state is reflected regardless of
        // whether this element is ever drawn.
        let new_child = {
            let rf = unsafe { &*self.rebuild_fn.0.get() };
            rf(ctx)
        };
        // Carry live state from nested StatefulElements into the new tree.
        {
            let old_child = unsafe { &*self.child.0.get() };
            carry_child_state(old_child.as_ref(), new_child.as_ref(), ctx);
        }
        // Install the newly-built child, replacing the old subtree.
        // Safety: single-threaded reconciliation; old child is not used past this point.
        unsafe {
            *self.child.0.get() = new_child;
        }

        // Keep `dirty` set so a later `draw` (e.g. after the element scrolls
        // back into view at a new size) still refreshes the subtree against the
        // then-current `BuildContext`. The eager rebuild above only guarantees
        // the live state is never lost; a redraw still picks up responsive
        // layout changes.
        self.dirty.borrow().set(true);
        let cur_gen = self.rebuild_generation.get();
        self.last_rebuilt_generation.set(cur_gen.wrapping_sub(1));
    }
}

impl Drawable for StatefulElement {
    fn draw(&self, ctx: &BuildContext) {
        #[cfg(debug_assertions)]
        {
            if crate::inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx.canvas.get_transform_translation();
                let size = self.content_size(ctx);
                let end_x = start_x + size.width;
                let end_y = start_y + size.height;

                let scale = ctx.scale;
                let l_start = Vec2d { x: start_x / scale, y: start_y / scale };
                let l_end = Vec2d { x: end_x / scale, y: end_y / scale };
                self.bounds.set(Some((l_start, l_end)));

                let cp = ctx.cursor_pos;
                if cp.x >= l_start.x
                    && cp.x <= l_end.x
                    && cp.y >= l_start.y
                    && cp.y <= l_end.y
                    && let Ok(mut hovered) = crate::inspector_overlay::HOVERED_WIDGET.write()
                {
                    *hovered = Some((self.debug_name.get(), l_start, l_end));
                }
            }
        }
        self.rebuild_if_dirty(ctx);
        // Safety: single-threaded rendering pipeline
        let child = unsafe { &*self.child.0.get() };
        child.draw(ctx);
    }
}

impl VisitorElement for StatefulElement {
    fn visit_children<'a>(&self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Safety: single-threaded rendering pipeline
        let child = unsafe { &*self.child.0.get() };
        visitor(child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        self.debug_name.get()
    }

    fn element_type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<StatefulElement>()
    }
}

impl EventElement for StatefulElement {
    fn on_event(&self, event: &ElementEvent) -> bool {
        let child = unsafe { &*self.child.0.get() };
        crate::components::element::dispatch_event(
            child.as_ref(),
            match event {
                ElementEvent::PointerDown(p, _, _) => *p,
                ElementEvent::PointerUp(p, _, _) => *p,
                ElementEvent::PointerMove(p, _, _) => *p,
                ElementEvent::Scroll { .. } => Vec2d::default(),
                ElementEvent::CharInput { .. } => Vec2d::default(),
                ElementEvent::KeyInput { .. } => Vec2d::default(),
                ElementEvent::ImePreedit { .. } => Vec2d::default(),
                ElementEvent::Cancel => Vec2d::default(),
            },
            event,
        )
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Safety: single-threaded rendering pipeline
        let child = unsafe { &*self.child.0.get() };
        visitor(child.as_ref());
    }
}

impl LayoutElement for StatefulElement {
    fn pos(&self) -> Option<Vec2d> {
        unsafe { &*self.child.0.get() }.pos()
    }

    fn size(&self) -> Option<Size> {
        unsafe { &*self.child.0.get() }.size()
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe { &*self.child.0.get() }.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe { &*self.child.0.get() }.content_size(ctx)
    }
    fn get_size_from_child(&self) -> Option<Size> {
        unsafe { &*self.child.0.get() }.get_size_from_child()
    }
    fn invalidate_layout(&self) {
        unsafe { &*self.child.0.get() }.invalidate_layout();
    }
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        if self.bounds.get().is_some() {
            return self.bounds.get();
        }
        unsafe { &*self.child.0.get() }.pos_start_end()
    }
}

impl Rebuildable for StatefulElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        StatefulElement::rebuild_if_dirty(self, ctx);
    }

    fn option_any(&self) -> Option<&dyn Any> {
        Some(self)
    }

    fn mark_needs_rebuild(&self) {
        self.dirty.borrow().set(true);
        // Safety: single-threaded rendering pipeline.
        let child = unsafe { &*self.child.0.get() };
        child.mark_needs_rebuild();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::AssertUnwindSafe;

    #[test]
    fn test_state_updater_empty_panic() {
        let updater: StateUpdater<i32> = StateUpdater::empty();
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            updater.set_state(|_s| {});
        }));
        assert!(result.is_err());

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            updater.read(|_s| {});
        }));
        assert!(result.is_err());
    }
}
