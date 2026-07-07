use crate::reconcile::try_update_element;
use crate::{Drawable, Element, EventElement, LayoutElement, Rebuildable, Reconcilable, VisitorElement, Widget, base::*};
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_events::window::request_animation_frame;
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::cell::{Cell, UnsafeCell};
use std::panic::Location;
use std::process::exit;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use aimer_utils::error;

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

/// Type-erased mutation closure sent through the channel.
type StateMutation<S> = Box<dyn FnOnce(&mut S) + Send>;

/// A handle that allows StatefulWidgets to trigger state mutations and rebuilds.
/// This is the Rust equivalent of Flutter's `setState`.
///
/// Instead of locking a `Mutex`, mutations are sent as closures through a
/// `crossbeam_channel` and applied on the render thread during the next
/// rebuild. This eliminates the possibility of deadlocks.
pub struct StateUpdater<S> {
    inner: Option<StateUpdaterInner<S>>,
}

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
                exit(1)
            }
        }
    }

    /// Create an empty `StateUpdater` that is not yet initialized.
    /// Calling `set_state` or `read` on an empty updater will panic.
    ///
    /// It has the same functionality as `StateUpdater<S>::empty`
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
    pub fn set_state_with<V: Clone + Send + 'static>(&self, value: &V, f: impl FnOnce(&mut S, V) + Send + 'static) {
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
    pub fn set_state(&self, f: impl FnOnce(&mut S) + Send + 'static) {
        let inner = match self.inner.as_ref() {
            Some(inner) => inner,
            None => {
                let loc = Location::caller();
                self.beautiful_error(loc);
                exit(1);
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

    // pub fn state(&self) -> Option<&S>{
    //     let inner = match self.inner.as_ref() {
    //         Some(inner) => inner.state,
    //         None => {
    //             let loc = Location::caller();
    //             #[cfg(not(target_os = "ios"))]
    //             self.beautiful_error(loc);
    //             exit(1);
    //         }
    //     };
    //
    // }

    // pub fn state(&self) -> Option<&S> {
    //     let inner = match self.inner.as_ref() {
    //         Some(inner) => inner.state,
    //         None => {
    //             let loc = Location::caller();
    //             #[cfg(not(target_os = "ios"))]
    //             self.beautiful_error(loc);
    //             exit(1);
    //         }
    //     };
    // }

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
                exit(1);
            }
        };
        // Safety: single-threaded rendering pipeline — no concurrent mutation.
        let state = unsafe { &*inner.state.0.get() };
        f(state)
    }

    #[inline]
    fn beautiful_error(&self, loc: &Location) {
        {
            use colored::Colorize;
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
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "^^^^^^^^^^^^^^^^^^^^^^^^^ add this line to prevent panic".red().bold(),
                "|".blue(),
                "|".blue(),
                "help".yellow().bold(),
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

    fn build(&self, ctx: &BuildContext) -> impl Widget;
}
pub type RebuildCallBack = dyn Fn(&BuildContext) -> Box<dyn Element>;
pub struct StatefulElement {
    child: SyncChild,
    pub dirty: Rc<Cell<bool>>,
    rebuild_fn: SyncRebuildFn,
    /// Monotonically increasing generation counter. Incremented on each rebuild
    /// so that multiple `set_state` calls between frames only trigger one rebuild.
    rebuild_generation: AtomicU64,
    /// The generation at which the last rebuild was performed.
    last_rebuilt_generation: AtomicU64,
    // #[cfg(debug_assertions)]
    debug_name: Cell<&'static str>,
    pub key: Option<crate::key::Key>,
    pub bounds: std::cell::Cell<Option<(Vec2d, Vec2d)>>,
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

    pub fn new<W: StatefulWidget + 'static>(widget: &W, ctx: &BuildContext) -> (Self, StateUpdater<W::State>)
    where
        W::State: 'static,
    {
        let state = widget.create_state();
        let dirty = Rc::new(Cell::new(false));

        // Create the channel for state mutations.
        #[allow(clippy::type_complexity)]
        let (tx, rx): (Sender<StateMutation<W::State>>, Receiver<StateMutation<W::State>>) = unbounded();

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

        let updater = StateUpdater::with(tx, state_cell, dirty.clone());

        let element = StatefulElement {
            child: SyncChild(UnsafeCell::new(child)),
            dirty,
            rebuild_fn: SyncRebuildFn(UnsafeCell::new(rebuild_fn)),
            rebuild_generation: AtomicU64::new(0),
            last_rebuilt_generation: AtomicU64::new(0),
            debug_name: Cell::new("Unknown"),
            key: None,
            bounds: Cell::new(None),
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
        if !self.dirty.get() {
            // Self is clean — but a nested StatefulElement might be dirty.
            // Propagate rebuild through the existing child tree.
            let child = unsafe { &*self.child.0.get() };
            Self::propagate_rebuild(child.as_ref(), ctx);
            return;
        }

        // Coalesce: only rebuild once per generation bump.
        let current_gen = self.rebuild_generation.load(Ordering::Relaxed);
        let last = self.last_rebuilt_generation.load(Ordering::Relaxed);
        if current_gen == last && !self.dirty.get() {
            return;
        }

        // First, let nested dirty StatefulElements rebuild in-place.
        {
            let child = unsafe { &*self.child.0.get() };
            Self::propagate_rebuild(child.as_ref(), ctx);
        }

        // Build the new child element.
        let new_child = {
            let rf = unsafe { &*self.rebuild_fn.0.get() };
            rf(ctx)
        };

        // Reconciliation: try to update the existing child in-place.
        // If types/keys match, the child is updated without replacement,
        // preserving nested state, GPU resources, etc.
        let old_child = unsafe { &*self.child.0.get() };
        if !try_update_element(old_child.as_ref(), new_child.as_ref(), ctx) {
            // Types or keys don't match — replace the child entirely.
            unsafe {
                *self.child.0.get() = new_child;
            }
        }
        // If try_update_element returned true, the old child was updated in-place
        // and remains valid — no replacement needed.

        self.dirty.set(false);
        self.rebuild_generation.fetch_add(1, Ordering::Relaxed);
        self.last_rebuilt_generation
            .store(self.rebuild_generation.load(Ordering::Relaxed), Ordering::Relaxed);
    }

    /// Walk the element tree and rebuild any nested dirty `StatefulElement`s.
    /// This is called on the *existing* child tree so that inner stateful widgets
    /// can update in-place without the parent having to reconstruct the whole subtree.
    fn propagate_rebuild(element: &dyn Element, ctx: &BuildContext) {
        element.rebuild_if_dirty(ctx);
    }

    /// Returns true if this element is marked dirty.
    pub fn is_dirty(&self) -> bool {
        self.dirty.get()
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
    pub(crate) fn adopt_state_from(&self, old: &StatefulElement) {
        // Safety: called only from `update_from_widget` during single-threaded
        // reconciliation, before the new element is visible to any other code.
        unsafe {
            // The rebuild closure captures the state cell and mutation channel.
            // Replacing it makes this element's build() read from the live state.
            *self.rebuild_fn.0.get() = (*old.rebuild_fn.0.get()).clone();
        }
        // Inherit name so inspector and future reconciliation still match.
        self.debug_name.set(old.debug_name.get());
        // Force a rebuild on the next draw: the child tree we were constructed
        // with was built from the *initial* state; after adopting the old state
        // we must regenerate it so the visual tree reflects the live state.
        self.dirty.set(true);
        let cur_gen = self.rebuild_generation.load(Ordering::Relaxed);
        self.last_rebuilt_generation.store(cur_gen.wrapping_sub(1), Ordering::Relaxed);
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
                if !(cp.x >= l_start.x && cp.x <= l_end.x && cp.y >= l_start.y && cp.y <= l_end.y) {
                    return;
                }
                if let Ok(mut hovered) = crate::inspector_overlay::HOVERED_WIDGET.write() {
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

    fn mark_needs_rebuild(&self) {
        self.dirty.set(true);
        // Safety: single-threaded rendering pipeline.
        let child = unsafe { &*self.child.0.get() };
        child.mark_needs_rebuild();
    }
}

impl Reconcilable for StatefulElement {
    fn key(&self) -> Option<crate::key::Key> {
        self.key.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_from_widget(&self, new_element: &dyn Element, _ctx: &BuildContext) -> bool {
        // Transfer our live state into the freshly-built new element so that when
        // the parent replaces this subtree, the new element carries the old state
        // (e.g. a selected tab index, form input, etc.) forward.
        //
        // This mirrors how RawScrollableContainer adopts its scroll offset:
        // modify the new element, return false, and let the parent swap us out.
        if let Some(new) = new_element.as_any().downcast_ref::<StatefulElement>() {
            new.adopt_state_from(self);
        }
        false
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
