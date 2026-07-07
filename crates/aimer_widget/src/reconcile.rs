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
pub fn try_update_element(
    existing: &dyn Element,
    new_element: &dyn Element,
    ctx: &BuildContext,
) -> bool {
    // Type check: same widget type produces same element debug_name
    if existing.debug_name() != new_element.debug_name() {
        // Different widget types — can't reconcile, but if both happen to be
        // the same StatefulElement widget type, carry the state forward so a
        // resize that swaps an outer wrapper doesn't reset the user.
        carry_stateful(existing, new_element);
        return false;
    }

    // Key check: both must have matching keys (or both None)
    if existing.key() != new_element.key() {
        // Key mismatch — the wrapper identity changed but the underlying
        // stateful widget may still be the same. Carry state if so.
        carry_stateful(existing, new_element);
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
fn carry_stateful(old: &dyn Element, new: &dyn Element) {
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
    new_s.adopt_state_from(old_s);
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
    carry_stateful(existing, new_element);

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
fn event_children_of<'a>(element: &'a dyn Element) -> Vec<&'a dyn Element> {
    let mut children: Vec<&dyn Element> = Vec::new();
    element.event_children(&mut |c| children.push(c));
    children
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::event_element::EventElement;
    use crate::components::layout_element::LayoutElement;
    use crate::components::rebuildable::Rebuildable;
    use crate::components::reconcilable::Reconcilable;
    use crate::components::visitor_element::VisitorElement;
    use crate::Drawable;
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

    use crate::widget::stateful::{State, StatefulElement, StatefulWidget, StateUpdater};
    use crate::Widget;
    use crate::key::Key;
    use std::any::TypeId;
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

    /// A minimal `StatefulWidget` for reconcile tests. The state struct
    /// exists only to satisfy the trait — its `build()` is a no-op.
    struct CounterWidget;
    struct CounterState {
        #[allow(dead_code)]
        updater: StateUpdater<Self>,
    }
    impl StatefulWidget for CounterWidget {
        type State = CounterState;
        fn create_state(&self) -> Self::State {
            CounterState {
                updater: StateUpdater::new(),
            }
        }
    }
    impl State<CounterWidget> for CounterState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            self.updater = updater;
        }
        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            EmptyWidget
        }
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
        // SAFETY: see `dummy_window`. `InnerCanvas::new()` requires no GPU;
        // leaked so the canvas reference has the `'static` lifetime
        // `BuildContext` demands.
        let canvas = {
            let leaked: &'static aimer_canvas::InnerCanvas =
                Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
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
        let widget = CounterWidget;

        let (old, _old_updater) = StatefulElement::new_with_name(
            &widget,
            &ctx,
            "Counter",
            Some(Key::Value("tab1".into())),
        );
        let (new, _new_updater) =
            StatefulElement::new_with_name(&widget, &ctx, "Counter", None);

        // Baseline: a freshly-built `StatefulElement` is not dirty.
        assert!(!new.dirty.get(), "new element must start clean");
        assert!(old.key().is_some(), "old has an explicit key");
        assert!(new.key().is_none(), "new has no key");

        let old_dyn: &dyn Element = &old;
        let new_dyn: &dyn Element = &new;
        try_update_element(old_dyn, new_dyn, &ctx);

        // After the fix: `carry_stateful` reaches `adopt_state_from` on the
        // inner `StatefulElement`, which flips `new.dirty = true`. Without
        // the fix, `try_update_element` early-returns on the key check and
        // the flag stays false — exactly the bug the user sees.
        assert!(
            new.dirty.get(),
            "key mismatch must not silently drop the StatefulElement's live state"
        );
    }
}
