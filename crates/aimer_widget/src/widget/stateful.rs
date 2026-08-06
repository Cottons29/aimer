use std::any::Any;
use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::{HashMap, VecDeque};
use std::panic::Location;
use std::rc::{Rc, Weak};

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_events::window::request_animation_frame;
use aimer_utils::error;

use crate::base::*;
use crate::components::element::identities_are_compatible;
use crate::widget::recovery::{BuildPhase, PanicDiagnostic, recover_operation};
use crate::{
    AnyElement, Drawable, Element, EventElement, EventResult, LayoutElement, Rebuildable,
    VisitorElement, Widget,
};

trait FetchAdd {
    fn fetch_add(&self, val: u64) -> u64;
}

impl FetchAdd for Cell<u64> {
    fn fetch_add(&self, val: u64) -> u64 {
        let previous = self.get();
        self.set(previous.wrapping_add(val));
        previous
    }
}

/// A `Send + Sync` wrapper around `UnsafeCell<AnyElement>`.
/// Safety: the rendering pipeline is single-threaded, so concurrent access does
/// not occur.
///
/// `pub(crate)` so `StatelessElement` can reuse the same swappable-child slot
/// (needed so `visit_children<'a>` can hand out `&'a` refs to a child that may
/// be replaced on rebuild).
pub(crate) struct SyncChild(pub(crate) UnsafeCell<AnyElement>);
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
/// concretely `Rc<SyncState<W::State>>`). Kept so a reconciling element can
/// hand its freshly-built state to the live element for a config refresh,
/// without `StatefulElement` being generic over `W`.
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

/// Type-erased state mutation applied during the next rebuild.
type StateMutation<S> = Box<dyn FnOnce(&mut S)>;

struct StateMutationQueue<S> {
    mutations: RefCell<VecDeque<StateMutation<S>>>,
}

impl<S> Default for StateMutationQueue<S> {
    fn default() -> Self {
        Self {
            mutations: RefCell::new(VecDeque::new()),
        }
    }
}

impl<S> StateMutationQueue<S> {
    #[inline]
    fn push(&self, mutation: StateMutation<S>) {
        self.mutations.borrow_mut().push_back(mutation);
    }

    #[inline]
    fn pop_front(&self) -> Option<StateMutation<S>> {
        self.mutations.borrow_mut().pop_front()
    }

    fn drain_into(&self, state: &mut S, mut on_applied: impl FnMut()) -> usize {
        let mut applied = 0;
        while let Some(mutation) = self.pop_front() {
            mutation(state);
            on_applied();
            applied += 1;
        }
        applied
    }
}

/// A handle that allows StatefulWidgets to trigger state mutations and
/// rebuilds. This is the Rust equivalent of Flutter's `setState`.
///
/// Mutations are queued as closures and applied on the render thread during
/// the next rebuild. The queue is local to the UI thread, so it needs neither
/// locking nor cross-thread channel storage.
///
/// `StateUpdater` is intentionally confined to the UI thread:
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// fn require_sync<T: Sync>() {}
/// require_send::<aimer_widget::StateUpdater<()>>();
/// require_sync::<aimer_widget::StateUpdater<()>>();
/// ```
pub struct StateUpdater<S> {
    inner: Option<StateUpdaterInner<S>>,
}

struct StateUpdaterInner<S> {
    /// Shared UI-thread queue for pending state mutations.
    tx: Rc<StateMutationQueue<S>>,
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
    /// Create a new `StateUpdater` from a channel sender, shared state, and a
    /// dirty flag.
    #[inline]
    fn with(tx: Rc<StateMutationQueue<S>>, state: Rc<SyncState<S>>, dirty: Rc<Cell<bool>>) -> Self {
        Self {
            inner: Some(StateUpdaterInner { tx, state, dirty }),
        }
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
    /// The mutation will be applied on the render thread during the next
    /// rebuild. This is deadlock-free: it never acquires a lock.
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
        inner.tx.push(Box::new(f));
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
    /// Called once after the state is created, providing a [`StateUpdater`]
    /// handle. Store the updater in your state struct to later call
    /// `set_state()` from event handlers or callbacks — similar to
    /// Flutter's `setState`.
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
pub type RebuildCallBack = dyn Fn(&BuildContext) -> AnyElement;

struct KeyedStateEntry {
    rebuild_fn: Weak<RebuildCallBack>,
    dirty: Weak<Cell<bool>>,
    state_revision: Weak<Cell<u64>>,
    state_any: Weak<dyn Any>,
    state_sender: Weak<dyn Any>,
    adopt_config_fn: Weak<AdoptConfigCallBack>,
}

struct LiveKeyedState {
    rebuild_fn: Rc<RebuildCallBack>,
    dirty: Rc<Cell<bool>>,
    state_revision: Rc<Cell<u64>>,
    state_any: Rc<dyn Any>,
    state_sender: Rc<dyn Any>,
    adopt_config_fn: Rc<AdoptConfigCallBack>,
}

thread_local! {
    static KEYED_STATE_REGISTRY: RefCell<HashMap<(crate::key::Key, &'static str), KeyedStateEntry>> =
        RefCell::new(HashMap::new());
    static KEYED_STATE_SCOPE_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct KeyedStateScope {
    owns_registry: bool,
}

impl KeyedStateScope {
    fn enter() -> Self {
        let owns_registry = KEYED_STATE_SCOPE_DEPTH.with(|depth| {
            let owns_registry = depth.get() == 0;
            if owns_registry {
                KEYED_STATE_REGISTRY.with(|registry| registry.borrow_mut().clear());
            }
            depth.set(depth.get() + 1);
            owns_registry
        });
        Self { owns_registry }
    }

    fn is_active() -> bool {
        KEYED_STATE_SCOPE_DEPTH.with(|depth| depth.get() > 0)
    }
}

impl Drop for KeyedStateScope {
    fn drop(&mut self) {
        KEYED_STATE_SCOPE_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

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
    /// so that multiple `set_state` calls between frames only trigger one
    /// rebuild.
    rebuild_generation: Cell<u64>,
    /// The generation at which the last rebuild was performed.
    last_rebuilt_generation: Cell<u64>,
    /// Shared revision incremented whenever a queued state mutation is applied.
    /// Used to disambiguate duplicate keyed copies retained by transitions.
    state_revision: RefCell<Rc<Cell<u64>>>,
    // #[cfg(debug_assertions)]
    debug_name: Cell<&'static str>,
    pub key: Option<crate::key::Key>,
    pub bounds: Cell<Option<(Vec2d, Vec2d)>>,
    /// This element's own state cell, type-erased, so a reconciling element can
    /// hand it to the live element's `adopt_config_fn` for a config refresh.
    state_any: SyncStateAny,
    state_sender: SyncStateAny,
    /// Copies widget configuration from another element's state (passed as
    /// `&dyn Any`) into this element's live state via
    /// `State::adopt_config_from`.
    adopt_config_fn: SyncAdoptConfigFn,
}

impl StatefulElement {
    pub fn boxed(self) -> AnyElement {
        Element::boxed(self)
    }
}

impl StatefulElement {
    /// Converts a stateful widget into an element while containing lifecycle
    /// panics to this widget subtree in unwind-enabled builds.
    #[doc(hidden)]
    pub fn from_widget<W: StatefulWidget + 'static>(
        widget: &W,
        ctx: &BuildContext,
        debug_name: &'static str,
        key: Option<crate::key::Key>,
    ) -> AnyElement
    where
        W::State: 'static,
    {
        match recover_operation(debug_name, BuildPhase::KeyedState, || {
            Self::try_new_with_identity(widget, ctx, debug_name, key)
        }) {
            Ok(Ok((element, _updater))) => element.boxed(),
            Ok(Err(diagnostic)) | Err(diagnostic) => diagnostic.into_error_element(),
        }
    }

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
        Self::try_new_with_identity(widget, ctx, debug_name, key)
            .unwrap_or_else(|diagnostic| panic!("{diagnostic}"))
    }

    pub fn new<W: StatefulWidget + 'static>(
        widget: &W,
        ctx: &BuildContext,
    ) -> (Self, StateUpdater<W::State>)
    where
        W::State: 'static,
    {
        Self::try_new_with_identity(widget, ctx, "Unknown", None)
            .unwrap_or_else(|diagnostic| panic!("{diagnostic}"))
    }

    fn try_new_with_identity<W: StatefulWidget + 'static>(
        widget: &W,
        ctx: &BuildContext,
        debug_name: &'static str,
        key: Option<crate::key::Key>,
    ) -> Result<(Self, StateUpdater<W::State>), PanicDiagnostic>
    where
        W::State: 'static,
    {
        let state = recover_operation(debug_name, BuildPhase::CreateState, || {
            widget.create_state()
        })?;

        if KeyedStateScope::is_active()
            && let Some(key_ref) = key.as_ref()
            && let Some(live) = lookup_keyed_state(key_ref, debug_name)
        {
            let fresh_state = Rc::new(SyncState(UnsafeCell::new(state)));
            let fresh_state_any: Rc<dyn Any> = fresh_state;
            recover_operation(debug_name, BuildPhase::AdoptConfig, || {
                (live.adopt_config_fn)(fresh_state_any.as_ref())
            })?;

            let child = (live.rebuild_fn)(ctx);
            return recover_operation(debug_name, BuildPhase::KeyedState, || {
                let element = StatefulElement {
                    child: SyncChild(UnsafeCell::new(child)),
                    dirty: RefCell::new(live.dirty),
                    rebuild_fn: SyncRebuildFn(UnsafeCell::new(live.rebuild_fn)),
                    rebuild_generation: Cell::new(0),
                    last_rebuilt_generation: Cell::new(0),
                    state_revision: RefCell::new(live.state_revision),
                    debug_name: Cell::new(debug_name),
                    key,
                    bounds: Cell::new(None),
                    state_any: SyncStateAny(UnsafeCell::new(live.state_any)),
                    state_sender: SyncStateAny(UnsafeCell::new(live.state_sender)),
                    adopt_config_fn: SyncAdoptConfigFn(UnsafeCell::new(live.adopt_config_fn)),
                };
                let updater = element
                    .state_updater()
                    .expect("keyed state registry contained a state of the wrong type");
                register_keyed_state(&element);
                (element, updater)
            });
        }

        let dirty = Rc::new(Cell::new(false));
        let build_consumer = BuildConsumer::new(dirty.clone());

        let tx = Rc::new(StateMutationQueue::default());

        let state_cell = Rc::new(SyncState(UnsafeCell::new(state)));
        let state_revision = Rc::new(Cell::new(0));

        // Create the updater and pass it into init_state.
        let init_updater = StateUpdater::with(tx.clone(), state_cell.clone(), dirty.clone());

        {
            // Safety: single-threaded — we are the only accessor during construction.
            let s = unsafe { &mut *state_cell.0.get() };
            recover_operation(debug_name, BuildPhase::InitState, || {
                s.init_state(init_updater.clone())
            })?;
        }

        let state_for_build = state_cell.clone();
        let revision_for_rebuild = state_revision.clone();
        let mutations_for_rebuild = tx.clone();
        let consumer_for_rebuild = build_consumer.clone();
        let rebuild_fn: Rc<RebuildCallBack> = Rc::new(move |ctx| {
            // Drain all pending mutations before rebuilding.
            let mutation_result =
                recover_operation(debug_name, BuildPhase::ApplyStateMutation, || {
                    let s = unsafe { &mut *state_for_build.0.get() };
                    mutations_for_rebuild.drain_into(s, || {
                        revision_for_rebuild.fetch_add(1);
                    });
                });
            if let Err(diagnostic) = mutation_result {
                return diagnostic.into_error_element();
            }
            ctx.with_build_consumer(consumer_for_rebuild.clone(), |ctx| {
                let s = unsafe { &*state_for_build.0.get() };
                let child_widget =
                    match recover_operation(debug_name, BuildPhase::Build, || s.build(ctx)) {
                        Ok(widget) => widget,
                        Err(diagnostic) => return diagnostic.into_error_element(),
                    };
                recover_operation(debug_name, BuildPhase::ToElement, || {
                    Widget::to_element(&child_widget, ctx)
                })
                .unwrap_or_else(|diagnostic| diagnostic.into_error_element())
            })
        });

        let child = {
            // Safety: single-threaded — initial build during construction.
            let s = unsafe { &*state_cell.0.get() };
            ctx.with_build_consumer(build_consumer, |ctx| {
                let child_widget =
                    recover_operation(debug_name, BuildPhase::Build, || s.build(ctx))?;
                recover_operation(debug_name, BuildPhase::ToElement, || {
                    Widget::to_element(&child_widget, ctx)
                })
            })?
        };

        // Type-erased handle to this element's state, plus a closure that can
        // pull configuration out of *another* element's state (of the same
        // `W::State` type) into this one. Together these let reconciliation
        // refresh a preserved live state's widget props without
        // `StatefulElement` being generic over `W`.
        let state_any: Rc<dyn Any> = state_cell.clone();
        let state_sender: Rc<dyn Any> = tx.clone();
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
            state_revision: RefCell::new(state_revision),
            debug_name: Cell::new(debug_name),
            key,
            bounds: Cell::new(None),
            state_any: SyncStateAny(UnsafeCell::new(state_any)),
            state_sender: SyncStateAny(UnsafeCell::new(state_sender)),
            adopt_config_fn: SyncAdoptConfigFn(UnsafeCell::new(adopt_config_fn)),
        };

        let element = if KeyedStateScope::is_active() {
            recover_operation(debug_name, BuildPhase::KeyedState, || {
                register_keyed_state(&element);
                element
            })?
        } else {
            element
        };

        Ok((element, updater))
    }

    /// Check if this element needs a rebuild and perform it if so.
    ///
    /// Uses element reconciliation: before replacing the child, tries to update
    /// it in-place via `try_update_element`. If the child's type and key
    /// match the new element's, the child is updated without replacement —
    /// preserving nested `StatefulElement` state, GPU resources, and
    /// reducing allocations.
    ///
    /// Before rebuilding itself, this method first walks the existing child
    /// tree to let any nested `StatefulElement`s rebuild independently.
    /// This avoids destroying and recreating the entire subtree when only a
    /// deeply-nested element's state has changed.
    pub fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        if !self.dirty.borrow().get() {
            // A clean ancestor does not participate in reconciliation, so avoid
            // scanning its entire subtree solely to populate the keyed-state
            // registry. Any dirty descendant enters its own scope when reached.
            let child = unsafe { &*self.child.0.get() };
            Self::propagate_rebuild(child.as_ref(), ctx);
            return;
        }

        let keyed_state_scope = KeyedStateScope::enter();
        if keyed_state_scope.owns_registry {
            register_keyed_state(self);
            let existing_child = unsafe { &*self.child.0.get() };
            register_keyed_subtree(existing_child.as_ref());
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
            crate::components::element::reconcile_generated_tree(
                old_child.as_ref(),
                new_child.as_ref(),
            );
        }

        // Install the newly-built child, replacing the old subtree.
        // Safety: single-threaded rendering pipeline; old_child is not used past this
        // point.
        unsafe {
            *self.child.0.get() = new_child;
        }

        self.dirty.borrow().set(false);
        self.rebuild_generation.fetch_add(1);
        self.last_rebuilt_generation
            .set(self.rebuild_generation.get());
    }

    /// Walk the element tree and rebuild any nested dirty `StatefulElement`s.
    /// This is called on the *existing* child tree so that inner stateful
    /// widgets can update in-place without the parent having to reconstruct
    /// the whole subtree.
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
    if !old.is_stateful_element() && !new.is_stateful_element() {
        return;
    }

    let Some(old_ele) = old
        .option_any()
        .and_then(|o| o.downcast_ref::<StatefulElement>())
    else {
        return;
    };

    let Some(new_ele) = new
        .option_any()
        .and_then(|o| o.downcast_ref::<StatefulElement>())
    else {
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
    let current = if element.is_stateful_element() {
        element
            .option_any()
            .and_then(|value| value.downcast_ref::<StatefulElement>())
            .filter(|stateful| {
                stateful.key.as_ref() == Some(key) && stateful.debug_name.get() == debug_name
            })
    } else {
        None
    };

    element_children(element)
        .into_iter()
        .filter_map(|child| find_keyed_stateful(child, key, debug_name))
        .fold(current, |freshest, candidate| match freshest {
            Some(existing)
                if existing.state_revision.borrow().get()
                    >= candidate.state_revision.borrow().get() =>
            {
                Some(existing)
            }
            _ => Some(candidate),
        })
}

fn element_children(element: &dyn Element) -> smallvec::SmallVec<[&dyn Element; 8]> {
    let mut children: smallvec::SmallVec<[&dyn Element; 8]> = smallvec::SmallVec::new();
    element.event_children(&mut |child| children.push(child));
    element.visit_children(&mut |child| {
        if !children
            .iter()
            .any(|existing| std::ptr::eq(*existing, child))
        {
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
    new.with_rebuild_context(ctx, &mut |ctx| {
        carry_keyed_child_state_in_context(old_root, new, ctx)
    });
}

fn carry_keyed_child_state_in_context(
    old_root: &dyn Element,
    new: &dyn Element,
    ctx: &BuildContext,
) {
    if new.is_stateful_element()
        && let Some(new_stateful) = new
            .option_any()
            .and_then(|value| value.downcast_ref::<StatefulElement>())
        && let Some(key) = new_stateful.key.as_ref()
    {
        if let Some(old_stateful) =
            find_keyed_stateful(old_root, key, new_stateful.debug_name.get())
            && old_stateful.state_revision.borrow().get()
                >= new_stateful.state_revision.borrow().get()
        {
            new_stateful.adopt_state_from(old_stateful, ctx);
            carry_state_below_keyed(old_stateful, new_stateful, ctx);
        }
        return;
    }

    for child in element_children(new) {
        carry_keyed_child_state(old_root, child, ctx);
    }
}

/// Hands the subtree below a keyed stateful element over to the element that
/// replaced it.
///
/// A keyed element keeps its own state wherever it reappears — it is looked up
/// by key, not by position — but the subtree it rebuilds from that state is
/// constructed anew, and everything below it is ordinary unkeyed state: a
/// request an `AsyncBuilder` already completed, a scroll offset, an animation
/// halfway through. Without this hand-over all of it starts over whenever an
/// ancestor rebuilds, which for a route transition wrapping a page means every
/// tick of an unrelated animation above it.
///
/// Unlike the walk below an unkeyed element, this one cannot assume the two
/// subtrees line up. A keyed element deliberately outlives changes to what it
/// contains — one route transition keeps its identity while the page inside it
/// is replaced — so a pair is only descended into once both sides agree on what
/// they are. Everything else keeps the state it was built with, which is how a
/// navigation still gets its new page.
///
/// Keys nested below this one are resolved first, and by key rather than by
/// position: the walk that follows deliberately steps over keyed elements, and
/// the pass that would otherwise claim them stopped at this element. A section
/// that names itself inside a page inside a keyed route transition — the shape
/// `website/src/screen/home_screen.rs` builds — would otherwise be the one
/// thing an ancestor's rebuild resets.
fn carry_state_below_keyed(old: &StatefulElement, new: &StatefulElement, ctx: &BuildContext) {
    if std::ptr::eq(old, new) {
        return;
    }

    for child in element_children(new) {
        carry_keyed_child_state(old, child, ctx);
    }

    carry_matching_child_state(old, new, ctx);
}

/// Pairs the children of `old` and `new` by position and hands over the state of
/// every pair that describes the same widget.
fn carry_matching_child_state(old: &dyn Element, new: &dyn Element, ctx: &BuildContext) {
    let old_children = element_children(old);
    if old_children.is_empty() {
        return;
    }

    let new_children = element_children(new);
    for (old_child, new_child) in old_children.iter().zip(new_children.iter()) {
        if !identities_are_compatible(*old_child, *new_child) {
            continue;
        }

        carry_matching_state(*old_child, *new_child, ctx);
    }
}

/// Hands one compatible pair over, then continues into their children.
fn carry_matching_state(old: &dyn Element, new: &dyn Element, ctx: &BuildContext) {
    new.with_rebuild_context(ctx, &mut |ctx| {
        adopt_runtime_state(old, new);

        if old.is_stateful_element()
            && new.is_stateful_element()
            && let Some(new_stateful) = new
                .option_any()
                .and_then(|value| value.downcast_ref::<StatefulElement>())
        {
            // A keyed element is the keyed pass's business, not this walk's.
            if new_stateful.key.is_some() {
                return;
            }

            carry_stateful(old, new, ctx);
        }

        carry_matching_child_state(old, new, ctx);
    });
}

/// Lets `new` take over the runtime state `old` was holding beside its children.
///
/// The positional walk below reaches every child an element owns, which covers
/// almost everything. What it cannot reach is a container that materializes its
/// children on demand — a freshly built one has none, so there is no pair to walk
/// — or state a container keeps beside them, such as a measurement of a list too
/// long to measure again. Both are handed over here instead; see
/// [`Rebuildable::adopt_runtime_state_from`].
///
/// Restricted to elements of the same concrete type, so an element is never
/// offered state it cannot interpret.
fn adopt_runtime_state(old: &dyn Element, new: &dyn Element) {
    if old.element_type_id() != new.element_type_id() || old.debug_name() != new.debug_name() {
        return;
    }

    new.adopt_runtime_state_from(old);
}

fn carry_unkeyed_child_state(old: &dyn Element, new: &dyn Element, ctx: &BuildContext) {
    new.with_rebuild_context(ctx, &mut |ctx| {
        carry_unkeyed_child_state_in_context(old, new, ctx)
    });
}

fn carry_unkeyed_child_state_in_context(old: &dyn Element, new: &dyn Element, ctx: &BuildContext) {
    adopt_runtime_state(old, new);

    if old.is_stateful_element() && new.is_stateful_element() {
        if new
            .option_any()
            .and_then(|value| value.downcast_ref::<StatefulElement>())
            .is_some_and(|stateful| stateful.key.is_some())
        {
            return;
        }

        carry_stateful(old, new, ctx);
    }

    let old_children = element_children(old);
    if old_children.is_empty() {
        return;
    }

    let new_children = element_children(new);

    for (old_child, new_child) in old_children.iter().zip(new_children.iter()) {
        carry_unkeyed_child_state(*old_child, *new_child, ctx);
    }
}

impl StatefulElement {
    fn state_updater<S: 'static>(&self) -> Option<StateUpdater<S>> {
        let state_any = unsafe { (&*self.state_any.0.get()).clone() };
        let state = state_any.downcast::<SyncState<S>>().ok()?;
        let sender_any = unsafe { (&*self.state_sender.0.get()).clone() };
        let sender = sender_any.downcast::<StateMutationQueue<S>>().ok()?;
        Some(StateUpdater::with(
            sender,
            state,
            self.dirty.borrow().clone(),
        ))
    }

    /// Returns true if this element is marked dirty.
    pub fn is_dirty(&self) -> bool {
        self.dirty.borrow().get()
    }

    /// Adopt the live state from another `StatefulElement` of the same widget
    /// type.
    ///
    /// Transfers the `rebuild_fn` (which captures the state cell and mutation
    /// channel), inherits the `debug_name`, and marks this element dirty so
    /// `rebuild_if_dirty` re-generates the child tree from the preserved state
    /// on the next frame.
    ///
    /// Called by `update_from_widget` when a parent's reconciliation replaces
    /// an entire subtree — without this, a freshly-constructed
    /// `StatefulElement` (with `current_index: 0`) would shadow the live
    /// one (with `current_index: 2`).
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
        *self.state_revision.borrow_mut() = old.state_revision.borrow().clone();

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
            *self.state_sender.0.get() = (*old.state_sender.0.get()).clone();
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
            crate::components::element::reconcile_generated_tree(
                old_child.as_ref(),
                new_child.as_ref(),
            );
        }
        // Install the newly-built child, replacing the old subtree.
        // Safety: single-threaded reconciliation; old child is not used past this
        // point.
        unsafe {
            *self.child.0.get() = new_child;
        }

        // The adopted state has already been materialized with the current
        // context. Leaving it dirty would rebuild the replacement again while
        // its ancestors are still reconciling, producing fresh nested state
        // that can overwrite the live subtree.
        self.dirty.borrow().set(false);
        let cur_gen = self.rebuild_generation.get();
        self.last_rebuilt_generation.set(cur_gen);
    }
}

fn lookup_keyed_state(key: &crate::key::Key, debug_name: &'static str) -> Option<LiveKeyedState> {
    KEYED_STATE_REGISTRY.with(|registry| {
        let registry = registry.borrow();
        let entry = registry.get(&(key.clone(), debug_name))?;
        Some(LiveKeyedState {
            rebuild_fn: entry.rebuild_fn.upgrade()?,
            dirty: entry.dirty.upgrade()?,
            state_revision: entry.state_revision.upgrade()?,
            state_any: entry.state_any.upgrade()?,
            state_sender: entry.state_sender.upgrade()?,
            adopt_config_fn: entry.adopt_config_fn.upgrade()?,
        })
    })
}

fn register_keyed_state(element: &StatefulElement) {
    let Some(key) = element.key.clone() else {
        return;
    };
    let rebuild_fn = unsafe { (&*element.rebuild_fn.0.get()).clone() };
    let state_any = unsafe { (&*element.state_any.0.get()).clone() };
    let state_sender = unsafe { (&*element.state_sender.0.get()).clone() };
    let adopt_config_fn = unsafe { (&*element.adopt_config_fn.0.get()).clone() };
    KEYED_STATE_REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        if registry
            .get(&(key.clone(), element.debug_name.get()))
            .and_then(|entry| entry.state_revision.upgrade())
            .is_some_and(|revision| revision.get() >= element.state_revision.borrow().get())
        {
            return;
        }
        registry.insert(
            (key, element.debug_name.get()),
            KeyedStateEntry {
                rebuild_fn: Rc::downgrade(&rebuild_fn),
                dirty: Rc::downgrade(&element.dirty.borrow()),
                state_revision: Rc::downgrade(&element.state_revision.borrow()),
                state_any: Rc::downgrade(&state_any),
                state_sender: Rc::downgrade(&state_sender),
                adopt_config_fn: Rc::downgrade(&adopt_config_fn),
            },
        );
    });
}

fn register_keyed_subtree(element: &dyn Element) {
    let mut pending = vec![element];

    while let Some(current) = pending.pop() {
        if current.is_stateful_element()
            && let Some(stateful) = current
                .option_any()
                .and_then(|value| value.downcast_ref::<StatefulElement>())
        {
            register_keyed_state(stateful);
        }

        pending.extend(element_children(current).into_iter().rev());
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
                let l_start = Vec2d {
                    x: start_x / scale,
                    y: start_y / scale,
                };
                let l_end = Vec2d {
                    x: end_x / scale,
                    y: end_y / scale,
                };
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

    fn reconciliation_key(&self) -> Option<&crate::Key> {
        self.key.as_ref()
    }
}

impl EventElement for StatefulElement {
    fn on_event(&self, _event: &ElementEvent) -> EventResult {
        EventResult::ignored()
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

    fn is_stateful_element(&self) -> bool {
        true
    }

    fn mark_needs_rebuild(&self) {
        // Safety: single-threaded rendering pipeline.
        let child = unsafe { &*self.child.0.get() };
        if !child.is_carry_state() {
            self.dirty.borrow().set(true);
        }
        child.mark_needs_rebuild();
    }
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;

    use aimer_events::pointer::{PointerButton, PointerInfo};

    use super::*;
    use crate::{EventDispatcher, StatelessElement};

    #[cfg(not(target_arch = "wasm32"))]
    fn dummy_async_handle() -> tokio::runtime::Handle {
        use std::sync::OnceLock;

        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        });
        let _guard = runtime.enter();
        tokio::runtime::Handle::current()
    }

    fn dummy_build_context() -> BuildContext<'static> {
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        BuildContext {
            parent_size: Default::default(),
            canvas,
            scale: 1.0,
            parent_pos: Default::default(),
            cursor_pos: Default::default(),
            box_constraint: Default::default(),
            visible_rect: None,
            window: WindowHandle::headless(Default::default(), 1.0),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: dummy_async_handle(),
            inherited_states: Default::default(),
        }
    }

    struct TestLeaf;
    impl VisitorElement for TestLeaf {
        fn debug_name(&self) -> &'static str {
            "TestLeaf"
        }
    }
    impl Drawable for TestLeaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl LayoutElement for TestLeaf {}
    impl EventElement for TestLeaf {}
    impl Rebuildable for TestLeaf {}

    /// An unkeyed stateful widget that records which of its states built the
    /// subtree that is live.
    ///
    /// Each state takes the next identity from `next_id`, so a state created by
    /// a rebuild is distinguishable from the one it was meant to replace.
    #[derive(Clone)]
    struct InnerProbe {
        next_id: Rc<Cell<usize>>,
        built_by: Rc<Cell<usize>>,
    }

    struct InnerProbeState {
        id: usize,
        built_by: Rc<Cell<usize>>,
    }

    impl StatefulWidget for InnerProbe {
        type State = InnerProbeState;

        fn create_state(&self) -> Self::State {
            let id = self.next_id.get();
            self.next_id.set(id + 1);
            InnerProbeState {
                id,
                built_by: self.built_by.clone(),
            }
        }
    }

    impl State<InnerProbe> for InnerProbeState {
        fn init_state(&mut self, _updater: StateUpdater<Self>) {}

        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            self.built_by.set(self.id);
            LeafProbe
        }
    }

    impl Widget for InnerProbe {
        fn to_element(&self, ctx: &BuildContext) -> AnyElement {
            StatefulElement::from_widget(self, ctx, "InnerProbe", None)
        }
    }

    struct LeafProbe;

    impl Widget for LeafProbe {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            TestLeaf.boxed()
        }
    }

    /// One of two interchangeable "pages" below the keyed element, distinguished
    /// only by the name it builds under — the shape a route's pages have.
    #[derive(Clone)]
    struct ProbePage {
        name: &'static str,
        inner: InnerProbe,
    }

    impl Widget for ProbePage {
        fn to_element(&self, ctx: &BuildContext) -> AnyElement {
            let inner = self.inner.clone();
            StatelessElement::from_builder(
                ctx,
                move |ctx| inner.to_element(ctx),
                None,
                self.name,
            )
            .boxed()
        }

        fn debug_name(&self) -> &'static str {
            self.name
        }
    }

    /// A keyed stateful widget standing where a route transition stands: it
    /// keeps its own state by key, and everything below it does not.
    struct KeyedOuter {
        page: ProbePage,
    }

    struct KeyedOuterState {
        page: ProbePage,
    }

    impl StatefulWidget for KeyedOuter {
        type State = KeyedOuterState;

        fn create_state(&self) -> Self::State {
            KeyedOuterState {
                page: self.page.clone(),
            }
        }
    }

    impl State<KeyedOuter> for KeyedOuterState {
        fn init_state(&mut self, _updater: StateUpdater<Self>) {}

        fn adopt_config_from(&mut self, new: &Self) {
            self.page = new.page.clone();
        }

        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            self.page.clone()
        }
    }

    impl Widget for KeyedOuter {
        fn to_element(&self, ctx: &BuildContext) -> AnyElement {
            StatefulElement::from_widget(
                self,
                ctx,
                "KeyedOuter",
                Some(crate::key::Key::from("keyed-outer")),
            )
        }
    }

    struct EventProbeWidget {
        events: Rc<Cell<usize>>,
    }

    struct EventProbeState {
        events: Rc<Cell<usize>>,
    }

    struct EventProbeChild {
        events: Rc<Cell<usize>>,
    }

    struct EventProbeElement {
        events: Rc<Cell<usize>>,
    }

    impl StatefulWidget for EventProbeWidget {
        type State = EventProbeState;

        fn create_state(&self) -> Self::State {
            EventProbeState {
                events: self.events.clone(),
            }
        }
    }

    impl State<EventProbeWidget> for EventProbeState {
        fn init_state(&mut self, _updater: StateUpdater<Self>) {}

        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            EventProbeChild {
                events: self.events.clone(),
            }
        }
    }

    impl Widget for EventProbeChild {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            EventProbeElement {
                events: self.events.clone(),
            }
            .boxed()
        }
    }

    impl VisitorElement for EventProbeElement {
        fn debug_name(&self) -> &'static str {
            "EventProbeElement"
        }
    }

    impl EventElement for EventProbeElement {
        fn on_event(&self, _event: &ElementEvent) -> EventResult {
            self.events.set(self.events.get() + 1);
            EventResult::ignored()
        }
    }

    impl LayoutElement for EventProbeElement {}
    impl Drawable for EventProbeElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for EventProbeElement {}

    #[test]
    fn stateful_generated_child_receives_each_routed_event_once() {
        let events = Rc::new(Cell::new(0));
        let element = StatefulElement::from_widget(
            &EventProbeWidget {
                events: events.clone(),
            },
            &dummy_build_context(),
            "EventProbeWidget",
            None,
        );

        let _ = EventDispatcher::new().dispatch(
            &element,
            Vec2d::default(),
            &ElementEvent::PointerMove(PointerInfo::mouse(
                Vec2d::default(),
                PointerButton::Primary,
            )),
        );

        assert_eq!(events.get(), 1);
    }

    struct TraversalElement {
        id: usize,
        children: Vec<AnyElement>,
        visits: Rc<RefCell<Vec<usize>>>,
    }

    impl VisitorElement for TraversalElement {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "TraversalElement"
        }
    }

    impl Drawable for TraversalElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl LayoutElement for TraversalElement {}
    impl EventElement for TraversalElement {}

    impl Rebuildable for TraversalElement {
        fn rebuild_if_dirty(&self, ctx: &BuildContext) {
            for child in &self.children {
                child.rebuild_if_dirty(ctx);
            }
        }

        fn is_stateful_element(&self) -> bool {
            self.visits.borrow_mut().push(self.id);
            false
        }
    }

    fn traversal_element(
        id: usize,
        children: Vec<AnyElement>,
        visits: &Rc<RefCell<Vec<usize>>>,
    ) -> AnyElement {
        TraversalElement {
            id,
            children,
            visits: visits.clone(),
        }
        .boxed()
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum PanicPhase {
        None,
        CreateState,
        InitState,
        Build,
        ToElement,
        AdoptConfig,
    }

    struct LifecycleWidget {
        phase: PanicPhase,
        updater: Rc<RefCell<Option<StateUpdater<LifecycleState>>>>,
        builds: Rc<Cell<usize>>,
    }

    struct LifecycleState {
        phase: PanicPhase,
        updater: Rc<RefCell<Option<StateUpdater<LifecycleState>>>>,
        builds: Rc<Cell<usize>>,
    }

    struct LifecycleChild {
        panic_in_to_element: bool,
    }

    impl Widget for LifecycleChild {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            if self.panic_in_to_element {
                panic!("child conversion failed");
            }
            TestLeaf.boxed()
        }
    }

    impl StatefulWidget for LifecycleWidget {
        type State = LifecycleState;

        fn create_state(&self) -> Self::State {
            if self.phase == PanicPhase::CreateState {
                panic!("state creation failed");
            }
            LifecycleState {
                phase: self.phase,
                updater: self.updater.clone(),
                builds: self.builds.clone(),
            }
        }
    }

    impl State<LifecycleWidget> for LifecycleState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            if self.phase == PanicPhase::InitState {
                panic!("state initialization failed");
            }
            self.updater.replace(Some(updater));
        }

        fn adopt_config_from(&mut self, new: &Self) {
            if new.phase == PanicPhase::AdoptConfig {
                panic!("keyed configuration failed");
            }
            self.phase = new.phase;
        }

        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            self.builds.set(self.builds.get() + 1);
            if self.phase == PanicPhase::Build {
                panic!("state build failed");
            }
            LifecycleChild {
                panic_in_to_element: self.phase == PanicPhase::ToElement,
            }
        }
    }

    fn lifecycle_widget(phase: PanicPhase) -> LifecycleWidget {
        LifecycleWidget {
            phase,
            updater: Rc::new(RefCell::new(None)),
            builds: Rc::new(Cell::new(0)),
        }
    }

    fn assert_initial_phase_recovers(phase: PanicPhase) {
        let context = dummy_build_context();
        let element = StatefulElement::from_widget(
            &lifecycle_widget(phase),
            &context,
            "LifecycleWidget",
            None,
        );
        assert_eq!(element.debug_name(), "ErrorWidget");
    }

    fn has_error_child(element: &dyn Element) -> bool {
        let mut found = false;
        element.visit_children(&mut |child| found |= child.debug_name() == "ErrorWidget");
        found
    }

    #[test]
    fn create_state_panic_becomes_error_element() {
        assert_initial_phase_recovers(PanicPhase::CreateState);
    }

    #[test]
    fn init_state_panic_becomes_error_element() {
        assert_initial_phase_recovers(PanicPhase::InitState);
    }

    #[test]
    fn initial_state_build_panic_becomes_error_element() {
        assert_initial_phase_recovers(PanicPhase::Build);
    }

    #[test]
    fn initial_child_conversion_panic_becomes_error_element() {
        assert_initial_phase_recovers(PanicPhase::ToElement);
    }

    #[test]
    fn queued_mutation_panic_installs_stable_error_child() {
        let context = dummy_build_context();
        let widget = lifecycle_widget(PanicPhase::None);
        let updater_slot = widget.updater.clone();
        let mutation_attempts = Rc::new(Cell::new(0));
        let observed_attempts = mutation_attempts.clone();
        let element = StatefulElement::from_widget(&widget, &context, "LifecycleWidget", None);
        let updater = updater_slot.borrow().as_ref().unwrap().clone();
        updater.set_state(move |_| {
            observed_attempts.set(observed_attempts.get() + 1);
            panic!("queued mutation failed");
        });

        element.rebuild_if_dirty(&context);
        assert!(has_error_child(element.as_ref()));
        element.rebuild_if_dirty(&context);
        assert_eq!(
            mutation_attempts.get(),
            1,
            "recovered mutation must not be retried"
        );
    }

    #[test]
    fn dirty_state_build_panic_installs_stable_error_child() {
        let context = dummy_build_context();
        let widget = lifecycle_widget(PanicPhase::None);
        let updater_slot = widget.updater.clone();
        let builds = widget.builds.clone();
        let element = StatefulElement::from_widget(&widget, &context, "LifecycleWidget", None);
        updater_slot
            .borrow()
            .as_ref()
            .unwrap()
            .set_state(|state| state.phase = PanicPhase::Build);

        element.rebuild_if_dirty(&context);
        assert!(has_error_child(element.as_ref()));
        element.rebuild_if_dirty(&context);
        assert_eq!(
            builds.get(),
            2,
            "recovered build must not be retried while clean"
        );
    }

    #[test]
    fn keyed_config_panic_becomes_error_element() {
        let context = dummy_build_context();
        let _scope = KeyedStateScope::enter();
        let key = crate::Key::Value("panic-recovery-key".to_owned());
        let live = StatefulElement::from_widget(
            &lifecycle_widget(PanicPhase::None),
            &context,
            "LifecycleWidget",
            Some(key.clone()),
        );
        assert_eq!(live.debug_name(), "LifecycleWidget");

        let recovered = StatefulElement::from_widget(
            &lifecycle_widget(PanicPhase::AdoptConfig),
            &context,
            "LifecycleWidget",
            Some(key),
        );
        assert_eq!(recovered.debug_name(), "ErrorWidget");
    }

    #[test]
    fn keyed_subtree_registration_visits_depth_first_in_child_order() {
        let visits = Rc::new(RefCell::new(Vec::new()));
        let first = traversal_element(
            1,
            vec![
                traversal_element(3, vec![], &visits),
                traversal_element(4, vec![], &visits),
            ],
            &visits,
        );
        let second = traversal_element(2, vec![traversal_element(5, vec![], &visits)], &visits);
        let root = traversal_element(0, vec![first, second], &visits);

        register_keyed_subtree(root.as_ref());

        assert_eq!(*visits.borrow(), vec![0, 1, 3, 4, 2, 5]);
    }

    #[test]
    fn nested_rebuilds_share_one_keyed_subtree_scan() {
        let context = dummy_build_context();
        let visits = Rc::new(RefCell::new(Vec::new()));
        let nested = StatefulElement::from_widget(
            &lifecycle_widget(PanicPhase::None),
            &context,
            "NestedLifecycleWidget",
            None,
        );
        let nested_stateful = nested
            .option_any()
            .and_then(|element| element.downcast_ref::<StatefulElement>())
            .unwrap();
        unsafe {
            *nested_stateful.child.0.get() = traversal_element(2, vec![], &visits);
        }

        let outer = StatefulElement::from_widget(
            &lifecycle_widget(PanicPhase::None),
            &context,
            "OuterLifecycleWidget",
            None,
        );
        let outer_stateful = outer
            .option_any()
            .and_then(|element| element.downcast_ref::<StatefulElement>())
            .unwrap();
        unsafe {
            *outer_stateful.child.0.get() = traversal_element(1, vec![nested], &visits);
        }
        outer_stateful.dirty.borrow().set(true);

        outer_stateful.rebuild_if_dirty(&context);

        assert_eq!(*visits.borrow(), vec![1, 2, 1]);
    }

    #[test]
    fn clean_stateful_rebuild_skips_keyed_subtree_registration() {
        let context = dummy_build_context();
        let visits = Rc::new(RefCell::new(Vec::new()));
        let outer = StatefulElement::from_widget(
            &lifecycle_widget(PanicPhase::None),
            &context,
            "OuterLifecycleWidget",
            None,
        );
        let outer_stateful = outer
            .option_any()
            .and_then(|element| element.downcast_ref::<StatefulElement>())
            .unwrap();
        unsafe {
            *outer_stateful.child.0.get() = traversal_element(1, vec![], &visits);
        }

        outer_stateful.rebuild_if_dirty(&context);

        assert!(visits.borrow().is_empty());
    }

    #[test]
    fn local_mutation_queue_preserves_order_and_accepts_reentrant_updates() {
        let queue: Rc<StateMutationQueue<Vec<i32>>> = Rc::new(StateMutationQueue::default());
        let nested_queue = queue.clone();
        let mut state = Vec::new();

        queue.push(Box::new(move |state| {
            state.push(1);
            nested_queue.push(Box::new(|state| state.push(3)));
        }));
        queue.push(Box::new(|state| state.push(2)));

        assert_eq!(queue.drain_into(&mut state, || {}), 3);
        assert_eq!(state, vec![1, 2, 3]);
        assert_eq!(queue.drain_into(&mut state, || {}), 0);
    }

    /// A keyed element is found by key, so it keeps its state wherever it
    /// reappears — but the subtree it rebuilds from that state is built anew,
    /// and what lives below it is unkeyed.
    ///
    /// This is the route transition an application wraps every page in: a theme
    /// change rebuilds the shell above it, and the request the page completed —
    /// an `AsyncBuilder`'s snapshot — must not start over because the switcher
    /// between them was rebuilt.
    #[test]
    fn state_below_a_keyed_element_outlives_a_rebuild_of_its_ancestor() {
        let context = dummy_build_context();
        let built_by = Rc::new(Cell::new(usize::MAX));
        let page = ProbePage {
            name: "ProbePageOne",
            inner: InnerProbe {
                next_id: Rc::new(Cell::new(0)),
                built_by: built_by.clone(),
            },
        };

        let old = KeyedOuter { page: page.clone() }.to_element(&context);
        assert_eq!(built_by.get(), 0, "the first subtree builds from its state");

        let new = KeyedOuter { page: page.clone() }.to_element(&context);
        assert_eq!(built_by.get(), 1, "a fresh subtree builds from fresh state");

        carry_child_state(old.as_ref(), new.as_ref(), &context);

        assert_eq!(
            built_by.get(),
            0,
            "the state below the keyed element was replaced by a fresh one"
        );
    }

    /// The other half of the same rule: a keyed element outlives changes to what
    /// it contains, so what it contains must be free to change.
    ///
    /// One route transition keeps its identity across every route. When the page
    /// inside it is replaced, the outgoing page's state must stay with the
    /// outgoing page — handing it to the page that replaced it would render the
    /// route that was navigated away from.
    #[test]
    fn a_replaced_page_below_a_keyed_element_keeps_its_own_state() {
        let context = dummy_build_context();
        let built_by = Rc::new(Cell::new(usize::MAX));
        let next_id = Rc::new(Cell::new(0));

        let old = KeyedOuter {
            page: ProbePage {
                name: "ProbePageOne",
                inner: InnerProbe {
                    next_id: next_id.clone(),
                    built_by: built_by.clone(),
                },
            },
        }
        .to_element(&context);
        assert_eq!(built_by.get(), 0);

        let new = KeyedOuter {
            page: ProbePage {
                name: "ProbePageTwo",
                inner: InnerProbe {
                    next_id: next_id.clone(),
                    built_by: built_by.clone(),
                },
            },
        }
        .to_element(&context);
        let page_two_state = built_by.get();
        assert_ne!(page_two_state, 0, "the second page builds from its own state");

        carry_child_state(old.as_ref(), new.as_ref(), &context);

        assert_ne!(
            built_by.get(),
            0,
            "the replaced page's state was handed to the page that replaced it"
        );
    }

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
