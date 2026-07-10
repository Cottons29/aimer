use crate::base::BuildContext;
use crate::components::element::Element;
use crate::components::visitor_element::VisitorElement;
use crate::widget::stateful::StatefulElement;

/// Try to reconcile an existing element with a new element of the same type.
///
/// If the existing element's `debug_name` and `key` match the new element's,
/// the existing element is updated in-place via `update_from_widget`, preserving
/// its internal state, child elements, and any GPU resources.
///
/// Returns `true` if the existing element was updated in-place (caller keeps using it).
/// Returns `false` if the types/keys don't match — caller must replace.
pub fn try_update_element(existing: &dyn Element, new_element: &dyn Element, ctx: &BuildContext) -> bool {
    // Type check: same widget type produces same element debug_name
    if existing.debug_name() != new_element.debug_name() {
        // Different widget types — can't reconcile, but if both happen to be
        // the same StatefulElement widget type, carry the state forward so a
        // resize that swaps an outer wrapper doesn't reset the user.
        carry_stateful(existing, new_element, ctx);
        return false;
    }

    // Key check: both must have matching keys (or both None)
    if existing.key() != new_element.key() {
        // Key mismatch — the wrapper identity changed but the underlying
        // stateful widget may still be the same. Carry state if so.
        carry_stateful(existing, new_element, ctx);
        return false;
    }

    // Delegate to the element's own update logic.
    let updated = existing.update_from_widget(new_element, ctx);

    if !updated {
        // `existing` is about to be replaced wholesale by `new_element`. Before the
        // caller swaps it in, walk the matched child subtrees so nested elements can
        // carry live runtime state (e.g. a `Scrollable`'s scroll offset) from the old
        // element into the freshly built one. Without this, any rebuild at an ancestor
        // — such as a window resize re-running a `MediaQuery`-driven `build()` — throws
        // the whole subtree away and snaps the viewport back to the top, even though
        // the element itself knows how to adopt the previous state in its
        // `update_from_widget`.
        carry_child_state(existing, new_element, ctx);
    }

    updated
}

/// If both sides are `StatefulElement`s of the same widget type, adopt the live
/// state from `old` into `new`. This is the rescue path: it runs even when the
/// `debug_name` or `key` check in `try_update_element` would otherwise
/// short-circuit and silently reset the user's state — the common case being a
/// window resize that rebuilds the parent tree with a slightly different
/// shape, where an outer wrapper's identity changes but the inner
/// `StatefulElement` is the same widget instance.
///
/// Safe to call on every reconcile: when both sides aren't matching
/// `StatefulElement`s of the same widget type, it's a no-op.
fn carry_stateful(old: &dyn Element, new: &dyn Element, ctx: &BuildContext) {
    let Some(old_s) = old.as_any().downcast_ref::<StatefulElement>() else {
        return;
    };
    let Some(new_s) = new.as_any().downcast_ref::<StatefulElement>() else {
        return;
    };
    // Don't adopt if the widget types differ — that would transfer state from
    // a different widget into the new one (e.g. a `TabView`'s state into a
    // `SettingsPanel`).
    if old_s.debug_name() != new_s.debug_name() {
        return;
    }
    new_s.adopt_state_from(old_s, ctx);
}

/// Recurse into the matched children of an element that is being replaced, letting
/// each nested element carry its runtime state (via `update_from_widget`) from the
/// old subtree into the new one.
///
/// Children are enumerated through `event_children` (the same accessor used for
/// event dispatch, which single-child wrappers like `Container` already expose) and
/// paired positionally. Elements whose `event_children` is empty (e.g. `Scrollable`
/// itself) stop the recursion, so it never walks below a state-owning leaf.
///
/// ponytail: pairing is positional; a keyed sibling list that reorders won't match
/// up and simply carries nothing across — safe, just no transfer for that node.
fn carry_child_state(existing: &dyn Element, new_element: &dyn Element, ctx: &BuildContext) {
    // Adopt state at this level BEFORE walking children. The caller's
    // `update_from_widget` has already returned false (subtree is being
    // replaced wholesale), so this is the only shot at preserving the live
    // state before the parent swaps it out.
    carry_stateful(existing, new_element, ctx);

    let old_children = event_children_of(existing);
    if old_children.is_empty() {
        return;
    }

    let new_children = event_children_of(new_element);

    for (old_child, new_child) in old_children.iter().zip(new_children.iter()) {
        try_update_element(*old_child, *new_child, ctx);
    }
}

/// Collect an element's children as exposed for event dispatch. This is the
/// traversal `carry_child_state` walks to reach nested state-owning elements
/// (e.g. a `Scrollable` inside a `Container`), so single-child wrappers must
/// surface their child here for scroll state to survive an ancestor rebuild.
#[allow(clippy::needless_lifetimes)]
fn event_children_of<'a>(element: &'a dyn Element) -> Vec<&'a dyn Element> {
    let mut children: Vec<&dyn Element> = Vec::new();
    element.event_children(&mut |c| children.push(c));
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Drawable;
    use crate::components::event_element::EventElement;
    use crate::components::layout_element::LayoutElement;
    use crate::components::rebuildable::Rebuildable;
    use crate::components::reconcilable::Reconcilable;
    use crate::components::visitor_element::VisitorElement;
    use std::any::Any;

    // Minimal fake elements: all trait methods except the listed ones use defaults.
    struct Leaf(&'static str);
    impl VisitorElement for Leaf {
        fn debug_name(&self) -> &'static str {
            self.0
        }
    }
    impl Drawable for Leaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl EventElement for Leaf {}
    impl LayoutElement for Leaf {}
    impl Rebuildable for Leaf {}
    impl Reconcilable for Leaf {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // A single-child wrapper (like `Container`) that surfaces its child only
    // through `visit_children` — exactly the shape that previously hid the
    // nested scrollable from reconciliation.
    struct Wrapper(Box<dyn Element>);
    impl VisitorElement for Wrapper {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.0.as_ref());
        }
        fn debug_name(&self) -> &'static str {
            "Wrapper"
        }
    }
    impl Drawable for Wrapper {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl EventElement for Wrapper {}
    impl LayoutElement for Wrapper {}
    impl Rebuildable for Wrapper {}
    impl Reconcilable for Wrapper {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // Contract the carry-over relies on: a wrapper that only implements
    // `visit_children` still surfaces its child through `event_children` (via the
    // default delegation), so `carry_child_state` can descend to a nested
    // state-owning element and hand its scroll offset to the rebuilt one.
    #[test]
    fn event_children_reach_wrapper_child() {
        let wrapper = Wrapper(Box::new(Leaf("Scrollable")));
        let children = event_children_of(&wrapper);
        assert_eq!(children.len(), 1, "wrapper must expose its single child");
        assert_eq!(children[0].debug_name(), "Scrollable");
    }

    // ─── StatefulElement state carry on reconcile ─────────────────────────
    //
    // The reconcile chain `StatelessElement::rebuild_if_dirty → try_update_element`
    // is the only thing that preserves a `StatefulElement`'s live state cell
    // across a window-resize rebuild. The current `try_update_element` early-
    // returns on key mismatch (`reconcile.rs:23`) BEFORE calling
    // `update_from_widget` → `adopt_state_from`, so a resize that produces a
    // freshly-built `StatefulElement` with a different key silently drops the
    // user's state.

    use crate::Widget;
    use crate::key::Key;
    use crate::widget::stateful::{State, StateUpdater, StatefulElement, StatefulWidget};
    use std::any::TypeId;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::{OnceLock, RwLock};

    /// A no-op widget used as the inner child of a `StatefulElement` for tests.
    /// Its `to_element` does not touch `ctx` fields beyond borrowing them, so
    /// the dummy `BuildContext` is safe.
    struct EmptyWidget;
    impl Widget for EmptyWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            Box::new(EmptyLeaf)
        }
        fn debug_name(&self) -> &'static str {
            "EmptyWidget"
        }
    }

    struct EmptyLeaf;
    impl VisitorElement for EmptyLeaf {
        fn debug_name(&self) -> &'static str {
            "EmptyLeaf"
        }
    }
    impl Drawable for EmptyLeaf {
        fn draw(&self, _: &BuildContext) {}
    }
    impl EventElement for EmptyLeaf {}
    impl LayoutElement for EmptyLeaf {}
    impl Rebuildable for EmptyLeaf {}
    impl Reconcilable for EmptyLeaf {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// Adapts an already-constructed `Box<dyn Element>` into a `Widget` so a
    /// `State::build` (which must return `impl Widget`) can hand back a subtree
    /// that was assembled directly from elements. `to_element` is called once
    /// per build; the element is taken out on that call.
    struct ElementWidget {
        element: RefCell<Option<Box<dyn Element>>>,
    }
    impl ElementWidget {
        fn new(element: Box<dyn Element>) -> Self {
            Self { element: RefCell::new(Some(element)) }
        }
    }
    impl Widget for ElementWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            self.element
                .borrow_mut()
                .take()
                .expect("ElementWidget::to_element called more than once")
        }
        fn debug_name(&self) -> &'static str {
            "ElementWidget"
        }
    }

    /// A minimal `StatefulWidget` for reconcile tests. The state struct
    /// holds a counter and a shared observer that records the most recent
    /// counter value `build()` saw — this is the only way to observe what
    /// `rebuild_fn` actually reads from the state cell after `adopt_state_from`
    /// swaps it.
    struct CounterWidget {
        observer: Rc<Cell<usize>>,
    }
    struct CounterState {
        counter: usize,
        observer: Rc<Cell<usize>>,
        #[allow(dead_code)]
        updater: StateUpdater<Self>,
    }
    impl StatefulWidget for CounterWidget {
        type State = CounterState;
        fn create_state(&self) -> Self::State {
            CounterState { counter: 1, observer: self.observer.clone(), updater: StateUpdater::new() }
        }
    }
    impl State<CounterWidget> for CounterState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            self.updater = updater;
        }
        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            // Record the counter value the rebuild closure actually saw. This
            // is the only observable proof of which state cell a `StatefulElement`
            // is reading from after `adopt_state_from` rewires its `rebuild_fn`.
            self.observer.set(self.counter);
            EmptyWidget
        }
    }

    struct ConfigWidget {
        label: usize,
        observed_label: Rc<Cell<usize>>,
        observed_runtime: Rc<Cell<usize>>,
        live_updater: Rc<RefCell<Option<StateUpdater<ConfigState>>>>,
    }

    struct ConfigState {
        config_label: usize,
        runtime: usize,
        observed_label: Rc<Cell<usize>>,
        observed_runtime: Rc<Cell<usize>>,
        live_updater: Rc<RefCell<Option<StateUpdater<Self>>>>,
        updater: StateUpdater<Self>,
    }

    impl StatefulWidget for ConfigWidget {
        type State = ConfigState;

        fn create_state(&self) -> Self::State {
            ConfigState {
                config_label: self.label,
                runtime: 0,
                observed_label: self.observed_label.clone(),
                observed_runtime: self.observed_runtime.clone(),
                live_updater: self.live_updater.clone(),
                updater: StateUpdater::new(),
            }
        }
    }

    impl State<ConfigWidget> for ConfigState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            self.updater = updater;
        }

        // Config = `config_label`; runtime = `runtime`. On reconcile the
        // framework preserves this live state but must refresh the config from
        // the freshly-built widget.
        fn adopt_config_from(&mut self, new: &Self) {
            self.config_label = new.config_label;
        }

        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            self.observed_label.set(self.config_label);
            self.observed_runtime.set(self.runtime);
            *self.live_updater.borrow_mut() = Some(self.updater.clone());
            EmptyWidget
        }
    }

    fn current_config_updater(live_updater: &Rc<RefCell<Option<StateUpdater<ConfigState>>>>) -> StateUpdater<ConfigState> {
        live_updater
            .borrow()
            .as_ref()
            .cloned()
            .expect("live updater should be published from build()")
    }

    /// Leak a zeroed buffer large enough to hold any `winit::window::Window`
    /// on every supported target and cast it to `&Window`. The pointer is
    /// never dereferenced — the reconcile code paths
    /// (`try_update_element`, `StatefulElement::update_from_widget`) never
    /// read through `ctx.window`, and the test widget's `build()` ignores it.
    /// ponytail: known-unsafe test-only phantom; the alternative is a full
    /// `winit::EventLoop` setup per test, which is heavier than the contract
    /// we're testing.
    fn dummy_window() -> &'static winit::window::Window {
        const SIZE: usize = 16384;
        static SLOT: OnceLock<usize> = OnceLock::new();
        let addr = *SLOT.get_or_init(|| {
            let leaked: &'static mut [u8; SIZE] = Box::leak(Box::new([0u8; SIZE]));
            leaked.as_mut_ptr() as usize
        });
        // SAFETY: pointer is never dereferenced; see fn doc.
        unsafe { &*(addr as *const winit::window::Window) }
    }

    /// A tokio runtime handle for the non-wasm `async_handle` field. Built
    /// once via `OnceLock` so the leaked runtime outlives every test that
    /// constructs a `BuildContext`.
    #[cfg(not(target_arch = "wasm32"))]
    fn dummy_async_handle() -> tokio::runtime::Handle {
        use std::sync::OnceLock;
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap());
        let _guard = runtime.enter();
        tokio::runtime::Handle::current()
    }

    fn dummy_build_context() -> BuildContext<'static> {
        // SAFETY: see `dummy_window`. `InnerCanvas::new()` requires no GPU;
        // leaked so the canvas reference has the `'static` lifetime
        // `BuildContext` demands.
        let canvas = {
            let leaked: &'static aimer_canvas::InnerCanvas = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(leaked)
        };
        BuildContext {
            parent_size: Default::default(),
            canvas,
            scale: 1.0,
            parent_pos: Default::default(),
            cursor_pos: Default::default(),
            box_constraint: Default::default(),
            visible_rect: None,
            window: dummy_window(),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: dummy_async_handle(),
            inherited_states: Rc::new(RwLock::new(HashMap::<TypeId, Rc<dyn Any>>::new())),
        }
    }

    /// Regression: when `old` and `new` are `StatefulElement`s of the same
    /// widget type but their keys differ, `try_update_element` currently
    /// early-returns on the key check (`reconcile.rs:23`) without ever
    /// calling `adopt_state_from`. The live state cell is dropped, and a
    /// window resize that produces a freshly-built `StatefulElement` (e.g.
    /// via a parent `StatelessElement` rebuild) silently resets the user's
    /// state to the widget's initial value.
    ///
    /// `adopt_state_from` flips `new.dirty = true` so the next draw rebuilds
    /// the new element from the old state cell. That flag is the observable
    /// contract this test pins.
    #[test]
    fn stateful_state_survives_key_mismatch() {
        let ctx = dummy_build_context();
        let observer = Rc::new(Cell::new(0usize));
        let widget = CounterWidget { observer: observer.clone() };

        let (old, _old_updater) = StatefulElement::new_with_name(&widget, &ctx, "Counter", Some(Key::Value("tab1".into())));
        let (new, _new_updater) = StatefulElement::new_with_name(&widget, &ctx, "Counter", None);

        // Baseline: a freshly-built `StatefulElement` is not dirty.
        assert!(!new.dirty.borrow().get(), "new element must start clean");
        assert!(old.key().is_some(), "old has an explicit key");
        assert!(new.key().is_none(), "new has no key");

        let old_dyn: &dyn Element = &old;
        let new_dyn: &dyn Element = &new;
        try_update_element(old_dyn, new_dyn, &ctx);

        // After the fix: `carry_stateful` reaches `adopt_state_from` on the
        // inner `StatefulElement`, which flips `new.dirty = true`. Without
        // the fix, `try_update_element` early-returns on the key check and
        // the flag stays false — exactly the bug the user sees.
        assert!(new.dirty.borrow().get(), "key mismatch must not silently drop the StatefulElement's live state");
    }

    /// End-to-end version of the bug: a `StatefulElement` whose state is
    /// mutated to a non-initial value must keep reading that value through
    /// any future rebuild that creates a fresh `StatefulElement`. The user's
    /// actual scenario is a window resize rebuilding the parent tree; this
    /// test simulates it by constructing a fresh `StatefulElement` and
    /// calling `try_update_element` (which is what the reconcile chain does
    /// under the hood).
    #[test]
    fn stateful_state_value_survives_rebuild() {
        let ctx = dummy_build_context();
        let observer = Rc::new(Cell::new(0usize));
        let widget = CounterWidget { observer: observer.clone() };

        // Build the OLD element. Initial state has counter=1; build() records
        // `1` in the observer.
        let (old, old_updater) = StatefulElement::new_with_name(&widget, &ctx, "Counter", None);
        assert_eq!(observer.get(), 1, "initial build should record counter=1");

        // Mutate state to 2 and drain the channel via rebuild. After this,
        // the OLD element is fully caught up: observer records 2.
        old_updater.set_state(|s| s.counter = 2);
        old.rebuild_if_dirty(&ctx);
        assert_eq!(observer.get(), 2, "after set_state + rebuild, observer=2");

        // Build the NEW element the way `Scrollable::to_element` does on
        // resize — a brand-new `StatefulElement` of the same widget type.
        // Its own create_state() runs with counter=1; observer records 1.
        let (new, _new_updater) = StatefulElement::new_with_name(&widget, &ctx, "Counter", None);
        assert_eq!(observer.get(), 1, "new element's initial build records 1");

        // Reconcile, the same way `try_update_element` is called from
        // `RawScrollableContainer::update_from_widget` (and from every other
        // wrapper that returns false from `update_from_widget`).
        let old_dyn: &dyn Element = &old;
        let new_dyn: &dyn Element = &new;
        try_update_element(old_dyn, new_dyn, &ctx);

        // Trigger the new element's rebuild, which now uses the ADOPTED
        // `rebuild_fn` (pointing at the OLD state cell). The build() call
        // inside that closure must observe counter=2, not 1.
        new.rebuild_if_dirty(&ctx);
        assert_eq!(observer.get(), 2, "after reconcile + rebuild, new element must read OLD state (counter=2), not initial (1)");
    }

    /// A marker inherited state a provider inserts into the `BuildContext`
    /// during its own `build` — the test analogue of the `Navigator` inserting
    /// its `NavigatorController`.
    #[derive(Clone, Copy)]
    struct ProvidedValue;

    /// A `StatefulWidget` that provides an inherited value during `build`
    /// (mirroring `NavigatorState::build` inserting its controller) and renders
    /// a nested `ConsumerWidget` below it.
    struct ProviderWidget;
    struct ProviderState {
        #[allow(dead_code)]
        updater: StateUpdater<Self>,
    }
    impl StatefulWidget for ProviderWidget {
        type State = ProviderState;
        fn create_state(&self) -> Self::State {
            ProviderState { updater: StateUpdater::new() }
        }
    }
    impl State<ProviderWidget> for ProviderState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            self.updater = updater;
        }
        fn build(&self, ctx: &BuildContext) -> impl Widget {
            // Provide the inherited value *before* building the child subtree —
            // exactly what `NavigatorState::build` does with its controller.
            ctx.insert_state(ProvidedValue);
            let (consumer, _u) = StatefulElement::new(&ConsumerWidget, ctx);
            ElementWidget::new(consumer.boxed())
        }
    }

    /// A nested `StatefulWidget` that reads the ancestor-provided inherited
    /// value during `build`, panicking if it is absent — mirroring
    /// `NavigatorController::of`, which panics with "No Navigator found in
    /// context" when the controller has not been re-provided this frame.
    struct ConsumerWidget;
    struct ConsumerState {
        #[allow(dead_code)]
        updater: StateUpdater<Self>,
    }
    impl StatefulWidget for ConsumerWidget {
        type State = ConsumerState;
        fn create_state(&self) -> Self::State {
            ConsumerState { updater: StateUpdater::new() }
        }
    }
    impl State<ConsumerWidget> for ConsumerState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            self.updater = updater;
        }
        fn build(&self, ctx: &BuildContext) -> impl Widget {
            ctx.get_state::<ProvidedValue>()
                .expect("No provided value in context. Ancestor provider must build before descendant.");
            EmptyWidget
        }
    }

    /// Regression for the reported "app panics when resizing the window: No
    /// Navigator found in context" bug.
    ///
    /// On a window resize the whole element tree is marked dirty
    /// (`mark_needs_rebuild`) and rebuilt in a single draw pass against a FRESH
    /// `BuildContext` whose `inherited_states` map starts empty. A dirty
    /// `StatefulElement` that provides inherited state in its own `build` (e.g.
    /// `Navigator` inserting its `NavigatorController`) must re-provide that
    /// state BEFORE its descendants rebuild.
    ///
    /// The bug: `rebuild_if_dirty` propagated the rebuild down to the existing
    /// child subtree *before* running its own `build`, so a nested consumer (a
    /// header calling `NavigatorController::of`) rebuilt against the empty fresh
    /// context and panicked. This test drives that exact ordering and must not
    /// panic.
    #[test]
    fn provider_reprovides_inherited_state_before_children_rebuild_on_resize() {
        // Initial build: provider inserts the value, the consumer reads it.
        let ctx = dummy_build_context();
        let (provider, _u) = StatefulElement::new(&ProviderWidget, &ctx);

        // Simulate a resize: dirty the whole subtree, then rebuild it against a
        // brand-new context whose inherited_states map is empty (a new frame).
        Rebuildable::mark_needs_rebuild(&provider);
        let fresh_ctx = dummy_build_context();

        // Must not panic: the provider's own build (which re-inserts the value)
        // must run before the consumer subtree rebuilds and looks it up.
        provider.rebuild_if_dirty(&fresh_ctx);
    }

    /// Regression for the reported "selected tab highlight is stuck after a
    /// resize even though the content updated" bug.
    ///
    /// A `StatefulWidget` whose `State` mirrors parent-provided props (here
    /// `config_label`) alongside a runtime field (`runtime`) is reconciled: the
    /// old element carries runtime=5 and config=0; a freshly-built element
    /// arrives with config=99. After reconciliation the live state MUST keep its
    /// runtime (5) yet REFRESH its config to the new value (99). Before the fix,
    /// `adopt_state_from` copied the old state wholesale and the config stayed
    /// stale at 0 — exactly why a `TextButton`'s selected/hover styling stopped
    /// tracking the live selection after a window resize.
    #[test]
    fn stateful_reconcile_refreshes_config_but_keeps_runtime() {
        let ctx = dummy_build_context();

        let observed_label = Rc::new(Cell::new(usize::MAX));
        let observed_runtime = Rc::new(Cell::new(usize::MAX));
        let live_updater = Rc::new(RefCell::new(None));

        // OLD element: initial config=0, then runtime mutated to 5.
        let old_widget = ConfigWidget {
            label: 0,
            observed_label: observed_label.clone(),
            observed_runtime: observed_runtime.clone(),
            live_updater: live_updater.clone(),
        };
        let (old, _old_ctor_updater) = StatefulElement::new_with_name(&old_widget, &ctx, "ConfigWidget", None);
        old.draw(&ctx);
        current_config_updater(&live_updater).set_state(|state| state.runtime = 5);
        old.draw(&ctx);
        assert_eq!(observed_label.get(), 0, "old element config=0 before reconcile");
        assert_eq!(observed_runtime.get(), 5, "old element runtime=5 before reconcile");

        // NEW element: freshly built with config=99 (as a parent rebuild would
        // emit for a now-selected tab), runtime resets to 0.
        let new_widget = ConfigWidget {
            label: 99,
            observed_label: observed_label.clone(),
            observed_runtime: observed_runtime.clone(),
            live_updater: live_updater.clone(),
        };
        let (new, _new_ctor_updater) = StatefulElement::new_with_name(&new_widget, &ctx, "ConfigWidget", None);

        // Reconcile old against the freshly built new (what a resize triggers).
        let updated = try_update_element(&old, &new, &ctx);
        assert!(!updated, "StatefulElement reconcile replaces with the new element");

        // `adopt_state_from` refreshes config eagerly during reconciliation, so
        // the observers already reflect the merged state: NEW config, OLD runtime.
        assert_eq!(observed_label.get(), 99, "reconcile MUST refresh config to the freshly-built value (99), not keep the stale 0");
        assert_eq!(observed_runtime.get(), 5, "reconcile MUST preserve the live runtime state (5)");

        // A subsequent draw of the preserved live element keeps the merge.
        new.draw(&ctx);
        assert_eq!(observed_label.get(), 99, "config stays refreshed after draw");
        assert_eq!(observed_runtime.get(), 5, "runtime stays preserved after draw");
    }

    /// Regression for the reported "after a window resize the selected tab's
    /// highlight is stuck on the initial tab even though the content updated"
    /// bug — the true root cause behind repeated reports.
    ///
    /// A single window resize reconciles a preserved `StatefulElement` MORE THAN
    /// ONCE (the eager rebuild inside `adopt_state_from` and the follow-up
    /// `carry_child_state` pass both reconcile the same subtree). So an element
    /// that has already ADOPTED another's live state must itself remain a
    /// correct config-refresh source when it is later used as the `old` side of
    /// another reconcile.
    ///
    /// The trap: `adopt_state_from` rewires `self.rebuild_fn` to read the OLD
    /// state cell, but historically left `self.state_any` / `self.adopt_config_fn`
    /// pointing at `self`'s own (now-orphaned) cell. On the second reconcile the
    /// config refresh then updated the ORPHANED cell while the live `rebuild_fn`
    /// kept reading the OLD cell — so the freshly-built config never reached the
    /// element that actually renders, and the highlight froze on a stale value.
    ///
    /// This chains three reconciles with distinct configs (1 → 2 → 3) and
    /// asserts the live element renders the newest config after each step.
    #[test]
    fn config_refresh_survives_chained_reconciles() {
        let ctx = dummy_build_context();

        let observed_label = Rc::new(Cell::new(usize::MAX));
        let observed_runtime = Rc::new(Cell::new(usize::MAX));
        let live_updater = Rc::new(RefCell::new(None));

        let mk = |label: usize| ConfigWidget {
            label,
            observed_label: observed_label.clone(),
            observed_runtime: observed_runtime.clone(),
            live_updater: live_updater.clone(),
        };

        // A: config 1.
        let (a, _a) = StatefulElement::new_with_name(&mk(1), &ctx, "ConfigWidget", None);
        a.rebuild_if_dirty(&ctx);
        assert_eq!(observed_label.get(), 1, "A renders config 1");

        // Reconcile A -> B (config 2). B becomes the live element; it adopts A's
        // state cell (via the copied rebuild_fn) and must render config 2.
        let (b, _b) = StatefulElement::new_with_name(&mk(2), &ctx, "ConfigWidget", None);
        try_update_element(&a, &b, &ctx);
        b.rebuild_if_dirty(&ctx);
        assert_eq!(observed_label.get(), 2, "after A->B the live element renders config 2");

        // Reconcile B -> C (config 3). C becomes live. Because B already adopted
        // A's cell, B's config-refresh machinery must still target the cell its
        // rebuild_fn actually reads. Before the fix, B's adopt_config_fn pointed
        // at B's orphaned cell, so the refresh missed the live cell and C
        // rendered the STALE config 2 instead of 3.
        let (c, _c) = StatefulElement::new_with_name(&mk(3), &ctx, "ConfigWidget", None);
        try_update_element(&b, &c, &ctx);
        c.rebuild_if_dirty(&ctx);
        assert_eq!(observed_label.get(), 3, "after chained A->B->C reconciles the live element MUST render the newest config 3");
    }

    /// Regression for the resize-rebuild case in `website/src/same_looking.rs`:
    /// a `StatefulElement` is wrapped inside a parent element (similar to
    /// `RawScrollableContainer`) whose own `update_from_widget` recurses into
    /// the child via `try_update_element` and whose `event_children` is
    /// intentionally empty (events are dispatched from `on_event` directly).
    ///
    /// The user reported this exact symptom when `SameLookingSection`
    /// lives inside a `Scrollable!`. This test pins the contract: a wrapper
    /// that recurses manually must carry the inner state forward.
    #[test]
    fn stateful_state_survives_when_wrapped_scrollablelike() {
        let ctx = dummy_build_context();
        let observer = Rc::new(Cell::new(0usize));
        let widget = CounterWidget { observer: observer.clone() };

        let (old_stateful, old_updater) = StatefulElement::new_with_name(&widget, &ctx, "Counter", None);
        old_updater.set_state(|s| s.counter = 2);
        old_stateful.rebuild_if_dirty(&ctx);
        assert_eq!(observer.get(), 2, "old stateful carries counter=2");

        // Build a fresh inner stateful — what `Scrollable::to_element` does on
        // resize: brand-new widget instance, fresh state, fresh inner tree.
        let (new_stateful, _new_updater) = StatefulElement::new_with_name(&widget, &ctx, "Counter", None);
        assert_eq!(observer.get(), 1, "new stateful's initial build records 1");

        // Wrap each in a parent that mimics `RawScrollableContainer`:
        //   * `event_children` is empty (events handled via `on_event`)
        //   * `update_from_widget` recurses manually via `try_update_element`
        //   * `key()` is stable across the rebuild
        let old_parent = ScrollableLikeWrapper { child: Box::new(old_stateful), key: Some(crate::Key::Static("scrollable-default")) };
        let new_parent = ScrollableLikeWrapper { child: Box::new(new_stateful), key: Some(crate::Key::Static("scrollable-default")) };

        let old_dyn: &dyn crate::Element = &old_parent;
        let new_dyn: &dyn crate::Element = &new_parent;
        try_update_element(old_dyn, new_dyn, &ctx);

        // Trigger the inner stateful's rebuild — adopted `rebuild_fn` must
        // read from the OLD state cell, so the build closure records 2.
        new_parent.child.rebuild_if_dirty(&ctx);
        assert_eq!(observer.get(), 2, "stateful inside a Scrollable-like wrapper must keep its state across resize rebuild");
    }

    mod resize_repro {
        use super::*;
        use crate::widget::stateless::StatelessElement;
        use aimer_attribute::size::ResolvedSize;
        use std::cell::{Cell, RefCell};

        struct ResizeCounterWidget {
            observer: Rc<Cell<usize>>,
            live_updater: Rc<RefCell<Option<StateUpdater<ResizeCounterState>>>>,
        }

        struct ResizeCounterState {
            counter: usize,
            observer: Rc<Cell<usize>>,
            live_updater: Rc<RefCell<Option<StateUpdater<Self>>>>,
            updater: StateUpdater<Self>,
        }

        impl StatefulWidget for ResizeCounterWidget {
            type State = ResizeCounterState;

            fn create_state(&self) -> Self::State {
                ResizeCounterState {
                    counter: 1,
                    observer: self.observer.clone(),
                    live_updater: self.live_updater.clone(),
                    updater: StateUpdater::new(),
                }
            }
        }

        impl State<ResizeCounterWidget> for ResizeCounterState {
            fn init_state(&mut self, updater: StateUpdater<Self>) {
                self.updater = updater;
            }

            fn build(&self, _ctx: &BuildContext) -> impl Widget {
                self.observer.set(self.counter);
                *self.live_updater.borrow_mut() = Some(self.updater.clone());
                EmptyWidget
            }
        }

        struct FakeLeaf {
            name: &'static str,
            size: ResolvedSize,
        }

        impl FakeLeaf {
            fn new(name: &'static str, width: f32, height: f32) -> Self {
                Self { name, size: ResolvedSize { width, height } }
            }
        }

        impl VisitorElement for FakeLeaf {
            fn debug_name(&self) -> &'static str {
                self.name
            }
        }

        impl Drawable for FakeLeaf {
            fn draw(&self, _ctx: &BuildContext) {}
        }

        impl EventElement for FakeLeaf {}

        impl LayoutElement for FakeLeaf {
            fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
                self.size
            }
        }

        impl Rebuildable for FakeLeaf {}

        impl Reconcilable for FakeLeaf {
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        struct FakeContainer {
            child: Box<dyn Element>,
            size: ResolvedSize,
        }

        impl FakeContainer {
            fn new(child: Box<dyn Element>, width: f32, height: f32) -> Self {
                Self { child, size: ResolvedSize { width, height } }
            }
        }

        impl VisitorElement for FakeContainer {
            fn debug_name(&self) -> &'static str {
                "FakeContainer"
            }
        }

        impl Drawable for FakeContainer {
            fn draw(&self, ctx: &BuildContext) {
                self.child.draw(ctx);
            }
        }

        impl EventElement for FakeContainer {
            fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
                visitor(self.child.as_ref());
            }
        }

        impl LayoutElement for FakeContainer {
            fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
                self.size
            }
        }

        impl Rebuildable for FakeContainer {}

        impl Reconcilable for FakeContainer {
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        struct FakeFlex {
            children: Vec<Box<dyn Element>>,
        }

        impl FakeFlex {
            fn new(children: Vec<Box<dyn Element>>) -> Self {
                Self { children }
            }
        }

        impl VisitorElement for FakeFlex {
            fn debug_name(&self) -> &'static str {
                "FakeFlex"
            }
        }

        impl Drawable for FakeFlex {
            fn draw(&self, ctx: &BuildContext) {
                let mut current_y = 0.0;
                for child in &self.children {
                    let child_size = child.computed_size(ctx);
                    let c_w = child_size.width;
                    let c_h = child_size.height;

                    let mut is_visible = true;
                    if let Some((vx, vy, vw, vh)) = ctx.visible_rect {
                        if c_w < vx || 0.0 > vx + vw || current_y + c_h < vy || current_y > vy + vh {
                            is_visible = false;
                        }
                    }

                    if is_visible {
                        let mut child_ctx = ctx.clone();
                        child_ctx.parent_size = child_size;
                        child_ctx.visible_rect = ctx.visible_rect.map(|(vx, vy, vw, vh)| (vx, vy - current_y, vw, vh));
                        child.draw(&child_ctx);
                    }

                    current_y += c_h;
                }
            }
        }

        impl EventElement for FakeFlex {
            fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
                for child in &self.children {
                    visitor(child.as_ref());
                }
            }
        }

        impl LayoutElement for FakeFlex {
            fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
                let mut width: f32 = 0.0;
                let mut height: f32 = 0.0;
                for child in &self.children {
                    let child_size = child.computed_size(ctx);
                    width = width.max(child_size.width);
                    height += child_size.height;
                }
                ResolvedSize { width, height }
            }
        }

        impl Rebuildable for FakeFlex {}

        impl Reconcilable for FakeFlex {
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        struct FakeStack {
            children: Vec<Box<dyn Element>>,
        }

        impl FakeStack {
            fn new(children: Vec<Box<dyn Element>>) -> Self {
                Self { children }
            }
        }

        impl VisitorElement for FakeStack {
            fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
                for child in &self.children {
                    visitor(child.as_ref());
                }
            }

            fn debug_name(&self) -> &'static str {
                "FakeStack"
            }
        }

        impl Drawable for FakeStack {
            fn draw(&self, ctx: &BuildContext) {
                for child in &self.children {
                    child.draw(ctx);
                }
            }
        }

        impl EventElement for FakeStack {}

        impl LayoutElement for FakeStack {
            fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
                let mut width: f32 = 0.0;
                let mut height: f32 = 0.0;
                for child in &self.children {
                    let child_size = child.computed_size(ctx);
                    width = width.max(child_size.width);
                    height = height.max(child_size.height);
                }
                ResolvedSize { width, height }
            }
        }

        impl Rebuildable for FakeStack {}

        impl Reconcilable for FakeStack {
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        struct FakePositioned {
            child: Box<dyn Element>,
        }

        impl FakePositioned {
            fn new(child: Box<dyn Element>) -> Self {
                Self { child }
            }
        }

        impl VisitorElement for FakePositioned {
            fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}

            fn debug_name(&self) -> &'static str {
                "FakePositioned"
            }
        }

        impl Drawable for FakePositioned {
            fn draw(&self, ctx: &BuildContext) {
                self.child.draw(ctx);
            }
        }

        impl EventElement for FakePositioned {
            fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
                visitor(self.child.as_ref());
            }
        }

        impl LayoutElement for FakePositioned {
            fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
                self.child.computed_size(ctx)
            }
        }

        impl Rebuildable for FakePositioned {
            fn rebuild_if_dirty(&self, ctx: &BuildContext) {
                self.child.rebuild_if_dirty(ctx);
            }

            fn mark_needs_rebuild(&self) {
                self.child.mark_needs_rebuild();
            }
        }

        impl Reconcilable for FakePositioned {
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        struct FakeScrollable {
            child: Box<dyn Element>,
            key: Option<crate::Key>,
        }

        impl FakeScrollable {
            fn new(child: Box<dyn Element>) -> Self {
                Self { child, key: Some(crate::Key::Static("scrollable-default")) }
            }
        }

        impl VisitorElement for FakeScrollable {
            fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
                visitor(self.child.as_ref());
            }

            fn debug_name(&self) -> &'static str {
                "FakeScrollable"
            }
        }

        impl Drawable for FakeScrollable {
            fn draw(&self, ctx: &BuildContext) {
                self.child.draw(ctx);
            }
        }

        impl EventElement for FakeScrollable {
            fn event_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}
        }

        impl LayoutElement for FakeScrollable {
            fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
                self.child.computed_size(ctx)
            }
        }

        impl Rebuildable for FakeScrollable {}

        impl Reconcilable for FakeScrollable {
            fn key(&self) -> Option<crate::Key> {
                self.key.clone()
            }

            fn as_any(&self) -> &dyn Any {
                self
            }

            fn update_from_widget(&self, new_element: &dyn Element, ctx: &BuildContext) -> bool {
                let new = new_element
                    .as_any()
                    .downcast_ref::<FakeScrollable>()
                    .expect("new element is a FakeScrollable");
                try_update_element(self.child.as_ref(), new.child.as_ref(), ctx);
                false
            }
        }

        #[derive(Debug)]
        struct VariantResult {
            label: &'static str,
            observer_after_resize: usize,
            live_counter_after_resize: usize,
        }

        fn build_home_page(
            ctx: &BuildContext,
            observer: Rc<Cell<usize>>,
            live_updater: Rc<RefCell<Option<StateUpdater<ResizeCounterState>>>>,
        ) -> Box<dyn Element> {
            let counter_widget = ResizeCounterWidget { observer, live_updater };
            let (stateful, _updater) = StatefulElement::new_with_name(&counter_widget, ctx, "Counter", None);

            FakeContainer::new(
                FakeStack::new(vec![
                    FakePositioned::new(FakeLeaf::new("HeaderLeaf", 200.0, 40.0).boxed()).boxed(),
                    FakePositioned::new(
                        FakeScrollable::new(
                            FakeFlex::new(vec![
                                FakeContainer::new(FakeLeaf::new("LeafA", 200.0, 100.0).boxed(), 200.0, 100.0).boxed(),
                                FakeContainer::new(FakeLeaf::new("LeafB", 200.0, 100.0).boxed(), 200.0, 100.0).boxed(),
                                FakeContainer::new(FakeLeaf::new("LeafC", 200.0, 100.0).boxed(), 200.0, 100.0).boxed(),
                                stateful.boxed(),
                            ])
                            .boxed(),
                        )
                        .boxed(),
                    )
                    .boxed(),
                ])
                .boxed(),
                200.0,
                400.0,
            )
            .boxed()
        }

        fn current_live_updater(live_updater: &Rc<RefCell<Option<StateUpdater<ResizeCounterState>>>>) -> StateUpdater<ResizeCounterState> {
            live_updater
                .borrow()
                .as_ref()
                .cloned()
                .expect("current live updater should be published from build()")
        }

        fn run_variant(culled: bool, resize_count: usize) -> VariantResult {
            let initial_ctx = dummy_build_context();
            let observer = Rc::new(Cell::new(0usize));
            let live_updater = Rc::new(RefCell::new(None));

            let initial_child = build_home_page(&initial_ctx, observer.clone(), live_updater.clone());
            let rebuild_observer = observer.clone();
            let rebuild_live_updater = live_updater.clone();
            let driver = StatelessElement::new(
                initial_child,
                move |ctx| build_home_page(ctx, rebuild_observer.clone(), rebuild_live_updater.clone()),
                None,
                "HomePage",
            );

            driver.draw(&initial_ctx);
            let current = current_live_updater(&live_updater);
            current.set_state(|state| state.counter = 2);
            driver.draw(&initial_ctx);

            assert_eq!(observer.get(), 2, "setup failed: stateful draw should observe counter=2 before resize");

            let mut resize_ctx = initial_ctx.clone();
            resize_ctx.visible_rect = if culled { Some((0.0, 0.0, 500.0, 250.0)) } else { None };

            for _ in 0..resize_count {
                driver.mark_needs_rebuild();
                driver.draw(&resize_ctx);
            }

            VariantResult {
                label: match (culled, resize_count) {
                    (false, 1) => "on-screen / one resize",
                    (false, 2) => "on-screen / two resizes",
                    (true, 1) => "culled / one resize",
                    (true, 2) => "culled / two resizes",
                    _ => "unexpected",
                },
                observer_after_resize: observer.get(),
                live_counter_after_resize: current_live_updater(&live_updater).read(|state| state.counter),
            }
        }

        fn diagnosis(result: &VariantResult) -> &'static str {
            if result.observer_after_resize == 2 && result.live_counter_after_resize == 2 {
                "PASS: state survived; `StatefulElement::update_from_widget` adopted state at crates/aimer_widget/src/widget/stateful.rs:597-606 and `StatefulElement::draw` consumed the dirty flag at crates/aimer_widget/src/widget/stateful.rs:500-503."
            } else if result.observer_after_resize == 1 && result.live_counter_after_resize == 1 {
                "FAIL: `StatefulElement::update_from_widget` can adopt only by copying the old `rebuild_fn` and marking the new element dirty at crates/aimer_widget/src/widget/stateful.rs:457-472 and 597-606; it does NOT swap the freshly-created state cell or updater from `StatefulElement::new` at crates/aimer_widget/src/widget/stateful.rs:321-361. When the resize pre-pass in crates/aimer_widget/src/widget/stateless.rs:125-129 cannot reach the nested stateful and the later draw is skipped by flex-style culling mirroring crates/aimer_container/src/flex/raw_flex.rs:285-319, the adopted rebuild never runs at crates/aimer_widget/src/widget/stateful.rs:500-503, so both the observer and the latest updater remain on the fresh state value 1."
            } else if result.observer_after_resize == 1 && result.live_counter_after_resize == 2 {
                "FAIL: adoption happened, but the rebuilt state was never rendered; the fresh element records 1 during construction, then stays dirty until draw. The resize pre-pass in crates/aimer_widget/src/widget/stateless.rs:125-129 cannot reach through container-like wrappers that only expose `event_children`, and the adopted state is only materialized when `StatefulElement::draw` runs at crates/aimer_widget/src/widget/stateful.rs:500-503. In this reproduction that draw is skipped by flex-style culling mirroring crates/aimer_container/src/flex/raw_flex.rs:285-319."
            } else if result.live_counter_after_resize != 2 {
                "FAIL: state adoption itself appears to have been lost before draw; inspect crates/aimer_widget/src/widget/stateful.rs:597-606 and crates/aimer_widget/src/reconcile.rs:91-107."
            } else {
                "FAIL: unexpected mixed result; inspect the printed counters."
            }
        }

        #[test]
        fn resize_repro_state_survival_across_window_resize() {
            let results = [run_variant(false, 1), run_variant(false, 2), run_variant(true, 1), run_variant(true, 2)];

            let mut failures = Vec::new();
            for result in &results {
                println!(
                    "[resize-repro] {} => observer={}, live_counter={} :: {}",
                    result.label,
                    result.observer_after_resize,
                    result.live_counter_after_resize,
                    diagnosis(result)
                );
                if result.observer_after_resize != 2 || result.live_counter_after_resize != 2 {
                    failures.push(format!(
                        "{} produced observer={} and live_counter={} (expected both 2)",
                        result.label, result.observer_after_resize, result.live_counter_after_resize
                    ));
                }
            }

            assert!(failures.is_empty(), "window-resize state reproduction failed:\n{}", failures.join("\n"));
        }

        // ─── Multi-button selection reproduction ──────────────────────────
        //
        // Reproduces `website/src/same_looking.rs`: a Row of platform
        // `TextButton`s where only the selected index is highlighted. Tapping a
        // button triggers the section's `set_state`, which rebuilds the whole
        // row of freshly-built buttons; those reconcile positionally against the
        // live ones. Each button is a `StatefulWidget` whose `State` mirrors the
        // parent-provided `selected` prop (exactly like `ButtonState` mirrors
        // the `TextButton` config), so it relies on `adopt_config_from` to
        // refresh that prop on reconcile.
        //
        // The reported regression: after picking "Android" (index 3) the other
        // buttons ("iOS"/"Web") ALSO render as active. This test asserts that
        // after switching the selection exactly ONE button (the newly selected
        // one) reports `selected == true`.

        struct TabButtonWidget {
            index: usize,
            selected: bool,
            observer: Rc<Cell<i32>>,
        }

        struct TabButtonState {
            index: usize,
            selected: bool,
            // Present to mirror `ButtonState`'s runtime field that
            // `adopt_config_from` must NOT clobber; not read by this test.
            #[allow(dead_code)]
            hovered: bool,
            observer: Rc<Cell<i32>>,
            updater: StateUpdater<Self>,
        }

        impl StatefulWidget for TabButtonWidget {
            type State = TabButtonState;

            fn create_state(&self) -> Self::State {
                TabButtonState {
                    index: self.index,
                    selected: self.selected,
                    hovered: false,
                    observer: self.observer.clone(),
                    updater: StateUpdater::new(),
                }
            }
        }

        impl State<TabButtonWidget> for TabButtonState {
            fn init_state(&mut self, updater: StateUpdater<Self>) {
                self.updater = updater;
            }

            // Mirrors `ButtonState::adopt_config_from`: refresh the parent-
            // provided config (`index`, `selected`) while keeping runtime
            // (`hovered`).
            fn adopt_config_from(&mut self, new: &Self) {
                self.index = new.index;
                self.selected = new.selected;
            }

            fn build(&self, _ctx: &BuildContext) -> impl Widget {
                // Record what this button believes its selection state is —
                // this is what drives its visible highlight in the real app.
                self.observer.set(if self.selected { 1 } else { 0 });
                EmptyWidget
            }
        }

        const TAB_COUNT: usize = 4;

        fn build_tab_row(ctx: &BuildContext, selected_index: Rc<Cell<usize>>, observers: Rc<Vec<Rc<Cell<i32>>>>) -> Box<dyn Element> {
            let selected = selected_index.get();
            let mut children: Vec<Box<dyn Element>> = Vec::with_capacity(TAB_COUNT);
            for index in 0..TAB_COUNT {
                let widget = TabButtonWidget { index, selected: index == selected, observer: observers[index].clone() };
                // Mirror the real app: `TextButton::to_element` yields a
                // `StatefulElement` whose `debug_name` is "Unknown", and the
                // `#[derive(WidgetConstructor)]`-generated `NamedWidget` then
                // wraps it in a `StatelessElement` named after the widget
                // ("TextButton") because the names differ. Reproduce that exact
                // wrapper so reconciliation takes the same path.
                let (stateful, _updater) = StatefulElement::new_with_name(&widget, ctx, "Unknown", None);
                let wrapped = StatelessElement::wrapper(stateful.boxed(), None, "TextButton");
                children.push(Box::new(wrapped));
            }
            FakeFlex::new(children).boxed()
        }

        #[test]
        fn switching_selected_tab_highlights_only_the_new_tab() {
            let ctx = dummy_build_context();
            let selected_index = Rc::new(Cell::new(0usize));
            let observers: Rc<Vec<Rc<Cell<i32>>>> = Rc::new((0..TAB_COUNT).map(|_| Rc::new(Cell::new(-1))).collect());

            let initial_child = build_tab_row(&ctx, selected_index.clone(), observers.clone());
            let rebuild_selected = selected_index.clone();
            let rebuild_observers = observers.clone();
            let driver = StatelessElement::new(
                initial_child,
                move |ctx| build_tab_row(ctx, rebuild_selected.clone(), rebuild_observers.clone()),
                None,
                "TabRow",
            );

            // Initial draw: index 0 selected.
            driver.draw(&ctx);
            let initial: Vec<i32> = observers.iter().map(|o| o.get()).collect();
            assert_eq!(initial, vec![1, 0, 0, 0], "initial render must highlight only tab 0");

            // User taps "Android" (index 3): the section rebuilds the row.
            for o in observers.iter() {
                o.set(-1);
            }
            selected_index.set(3);
            driver.mark_needs_rebuild();
            driver.draw(&ctx);

            let after: Vec<i32> = observers.iter().map(|o| o.get()).collect();
            assert_eq!(after, vec![0, 0, 0, 1], "after switching to tab 3, ONLY tab 3 must be highlighted (got {:?})", after);
        }

        /// Root-cause regression for the "hover highlight stuck after a parent
        /// rebuild" bug (the `TextButton` symptom: after picking a tab the other
        /// buttons the mouse passed over stay highlighted).
        ///
        /// A `StatefulWidget`'s own callbacks capture ITS OWN `StateUpdater`
        /// (like `TextButton`'s hover-enter/exit capturing the button's
        /// updater). On reconcile the framework preserves the live state and
        /// copies the old `rebuild_fn` (old state cell + old mutation channel)
        /// into the fresh live element. If the live element keeps its own,
        /// disconnected `dirty` flag, then a `set_state` made through that
        /// preserved (old) updater flips a flag the live element never checks —
        /// its queued mutation is never drained/applied and the widget freezes
        /// on its pre-reconcile runtime value.
        ///
        /// This asserts that after a reconcile, a `set_state` through the
        /// state's own (published) updater DOES drive the live element on its
        /// next rebuild.
        #[test]
        fn preserved_state_own_setstate_drives_live_element_after_reconcile() {
            let ctx = dummy_build_context();
            let observer = Rc::new(Cell::new(0usize));
            let live_updater = Rc::new(RefCell::new(None));

            let widget = ResizeCounterWidget { observer: observer.clone(), live_updater: live_updater.clone() };

            // OLD live element (counter starts at 1, publishes its own updater).
            let (old, _old_ctor) = StatefulElement::new_with_name(&widget, &ctx, "Counter", None);
            old.rebuild_if_dirty(&ctx);
            assert_eq!(observer.get(), 1, "old element initial build records 1");

            // Freshly-built element arriving from a parent rebuild.
            let (new, _new_ctor) = StatefulElement::new_with_name(&widget, &ctx, "Counter", None);

            // Reconcile: `new` becomes the live element, adopting `old`'s state
            // cell + channel + dirty flag; the eager rebuild re-publishes the
            // (old) updater into the shared slot.
            try_update_element(&old, &new, &ctx);

            // Consume the dirty flag `adopt_state_from` leaves set, so the next
            // `set_state` is the only thing that can re-dirty the live element.
            new.rebuild_if_dirty(&ctx);

            // The state's OWN updater (captured inside its callbacks) fires a
            // mutation — this is exactly what `TextButton`'s hover-exit does.
            current_live_updater(&live_updater).set_state(|s| s.counter = 42);

            // The live element's next frame must observe the mutation. Before
            // the dirty-flag-sharing fix, `new.dirty` was disconnected from the
            // updater's flag, so this rebuild was a no-op and the observer stuck
            // at 1 (the frozen-hover symptom).
            new.rebuild_if_dirty(&ctx);
            assert_eq!(observer.get(), 42, "the preserved state's own set_state must drive the live element after reconcile");
        }

        // ─── Resize reproduction: selected-tab highlight goes stale ────────
        //
        // The reported bug: after picking a tab (e.g. "Android", index 3) and
        // resizing the window, the *content* follows the live selection but the
        // active/selected highlight snaps back to the initially-selected button
        // ("macOS", index 0).
        //
        // This reproduces the exact real-app nesting that the click-only test
        // (`switching_selected_tab_highlights_only_the_new_tab`) does NOT: the
        // tab row lives inside an OUTER `StatefulWidget` (`SameLookingSection`,
        // here `SectionWidget`) that itself is preserved across the resize via
        // `adopt_state_from`. On resize the parent rebuilds a fresh section
        // (current_index=0); the framework must preserve the live section state
        // (current_index=3) AND, when it eagerly rebuilds that preserved state's
        // subtree, refresh each button's `selected` config. If the eager rebuild
        // reconciles against the wrong (stale) child, the buttons keep their
        // initial highlight.

        struct SectionWidget {
            observers: Rc<Vec<Rc<Cell<i32>>>>,
            live_updater: Rc<RefCell<Option<StateUpdater<SectionState>>>>,
        }

        struct SectionState {
            current_index: usize,
            observers: Rc<Vec<Rc<Cell<i32>>>>,
            live_updater: Rc<RefCell<Option<StateUpdater<Self>>>>,
            updater: StateUpdater<Self>,
        }

        impl StatefulWidget for SectionWidget {
            type State = SectionState;

            fn create_state(&self) -> Self::State {
                // Fresh section always starts at index 0 (mirrors
                // `SameLookingSectionState::create_state`).
                SectionState {
                    current_index: 0,
                    observers: self.observers.clone(),
                    live_updater: self.live_updater.clone(),
                    updater: StateUpdater::new(),
                }
            }
        }

        impl State<SectionWidget> for SectionState {
            fn init_state(&mut self, updater: StateUpdater<Self>) {
                self.updater = updater;
            }

            fn build(&self, ctx: &BuildContext) -> impl Widget {
                *self.live_updater.borrow_mut() = Some(self.updater.clone());
                // Build the row of tab buttons based on the live selection,
                // exactly like `SameLookingSectionState::build`.
                let selected = self.current_index;
                let mut children: Vec<Box<dyn Element>> = Vec::with_capacity(TAB_COUNT);
                for index in 0..TAB_COUNT {
                    let widget = TabButtonWidget { index, selected: index == selected, observer: self.observers[index].clone() };
                    let (stateful, _updater) = StatefulElement::new_with_name(&widget, ctx, "Unknown", None);
                    let wrapped = StatelessElement::wrapper(stateful.boxed(), None, "TextButton");
                    children.push(Box::new(wrapped));
                }
                // The section's own build returns an element tree; wrap the row
                // in a couple of container-like wrappers so the button subtree
                // is nested (mirroring Container -> Column -> Row).
                ElementWidget::new(FakeContainer::new(FakeFlex::new(children).boxed(), 400.0, 60.0).boxed())
            }
        }

        fn current_section_updater(live_updater: &Rc<RefCell<Option<StateUpdater<SectionState>>>>) -> StateUpdater<SectionState> {
            live_updater
                .borrow()
                .as_ref()
                .cloned()
                .expect("section updater should be published from build()")
        }

        fn build_section_home(
            ctx: &BuildContext,
            observers: Rc<Vec<Rc<Cell<i32>>>>,
            live_updater: Rc<RefCell<Option<StateUpdater<SectionState>>>>,
        ) -> Box<dyn Element> {
            let section = SectionWidget { observers, live_updater };
            let (stateful, _updater) = StatefulElement::new_with_name(&section, ctx, "SameLookingSection", None);
            // Nest the section deep, like the real HomePage tree.
            FakeContainer::new(
                FakeStack::new(vec![
                    FakePositioned::new(
                        FakeScrollable::new(
                            FakeFlex::new(vec![
                                FakeContainer::new(FakeLeaf::new("Hero", 200.0, 100.0).boxed(), 200.0, 100.0).boxed(),
                                stateful.boxed(),
                            ])
                            .boxed(),
                        )
                        .boxed(),
                    )
                    .boxed(),
                ])
                .boxed(),
                400.0,
                400.0,
            )
            .boxed()
        }

        #[test]
        fn resize_keeps_selected_tab_highlight() {
            let ctx = dummy_build_context();
            let observers: Rc<Vec<Rc<Cell<i32>>>> = Rc::new((0..TAB_COUNT).map(|_| Rc::new(Cell::new(-1))).collect());
            let live_updater: Rc<RefCell<Option<StateUpdater<SectionState>>>> = Rc::new(RefCell::new(None));

            let initial_child = build_section_home(&ctx, observers.clone(), live_updater.clone());
            let rebuild_observers = observers.clone();
            let rebuild_live_updater = live_updater.clone();
            let driver = StatelessElement::new(
                initial_child,
                move |ctx| build_section_home(ctx, rebuild_observers.clone(), rebuild_live_updater.clone()),
                None,
                "HomePage",
            );

            // Initial draw: section index 0 -> only tab 0 highlighted.
            driver.draw(&ctx);
            let initial: Vec<i32> = observers.iter().map(|o| o.get()).collect();
            assert_eq!(initial, vec![1, 0, 0, 0], "initial render highlights tab 0");

            // User picks "Android" (index 3): the section's own set_state.
            for o in observers.iter() {
                o.set(-1);
            }
            current_section_updater(&live_updater).set_state(|s| s.current_index = 3);
            driver.draw(&ctx);
            let after_pick: Vec<i32> = observers.iter().map(|o| o.get()).collect();
            assert_eq!(after_pick, vec![0, 0, 0, 1], "after picking tab 3, only tab 3 is highlighted (got {:?})", after_pick);

            // Window resize: the parent rebuilds the whole tree (fresh section
            // at index 0), reconciles, and must preserve the live selection AND
            // refresh the button highlight to match it.
            for o in observers.iter() {
                o.set(-1);
            }
            driver.mark_needs_rebuild();
            driver.draw(&ctx);

            let after_resize: Vec<i32> = observers.iter().map(|o| o.get()).collect();
            assert_eq!(after_resize, vec![0, 0, 0, 1], "after resize, ONLY tab 3 must stay highlighted (got {:?})", after_resize);
        }
    }
}

/// Wrapper element that mimics `RawScrollableContainer`: its `event_children`
/// is intentionally empty (events handled via `on_event`) but its
/// `update_from_widget` recurses into the child via `try_update_element` —
/// the shape used by the framework's only built-in scroll container.
#[allow(unused)]
struct ScrollableLikeWrapper {
    child: Box<dyn crate::Element>,
    key: Option<crate::Key>,
}

impl crate::VisitorElement for ScrollableLikeWrapper {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn crate::Element)) {
        visitor(self.child.as_ref());
    }
    fn debug_name(&self) -> &'static str {
        "ScrollableLikeWrapper"
    }
}

impl crate::EventElement for ScrollableLikeWrapper {
    fn event_children<'a>(&'a self, _: &mut dyn FnMut(&'a dyn crate::Element)) {}
}

impl crate::Drawable for ScrollableLikeWrapper {
    fn draw(&self, _ctx: &BuildContext) {}
}

impl crate::LayoutElement for ScrollableLikeWrapper {}

impl crate::Rebuildable for ScrollableLikeWrapper {}

impl crate::Reconcilable for ScrollableLikeWrapper {
    fn key(&self) -> Option<crate::Key> {
        self.key.clone()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn update_from_widget(&self, new_element: &dyn crate::Element, ctx: &BuildContext) -> bool {
        let new = new_element
            .as_any()
            .downcast_ref::<ScrollableLikeWrapper>()
            .expect("new element is a ScrollableLikeWrapper");
        try_update_element(self.child.as_ref(), new.child.as_ref(), ctx);
        false
    }
}
