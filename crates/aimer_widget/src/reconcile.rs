use crate::base::BuildContext;
use crate::components::element::Element;







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




#[cfg(test)]
mod tests {
    use super::*;
    use crate::Drawable;
    use crate::components::event_element::EventElement;
    use crate::components::layout_element::LayoutElement;
    use crate::components::rebuildable::Rebuildable;
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

    // ─── Draw-path regression: the DRAWN subtree must reflect set_state ────
    //
    // Every other stateful test asserts on a BUILD observer (what `build()`
    // saw). That leaves a gap: none of them check that the element actually
    // *drawn* after a `set_state` is the freshly-rebuilt one. The reported
    // "counter freezes on screen while `self.count` keeps incrementing in the
    // console" symptom lives precisely in that gap — `build()` runs with the
    // new value, yet if the stale child subtree were drawn the screen would
    // stay frozen.
    //
    // This mirrors the real app: a stateful counter whose `build()` returns a
    // container -> row -> [text-leaf, nested stateful button] tree, then fires
    // `set_state` through the state's own updater exactly like the button's
    // `on_press`, and asserts the DRAWN leaf shows the new value.

    /// Leaf that records the value it renders when DRAWN (not when built),
    /// so the test can prove which element actually reaches the screen.
    struct RecordingLeaf {
        value: usize,
        drawn: Rc<Cell<usize>>,
    }
    impl VisitorElement for RecordingLeaf {
        fn debug_name(&self) -> &'static str {
            "RecordingLeaf"
        }
    }
    impl Drawable for RecordingLeaf {
        fn draw(&self, _ctx: &BuildContext) {
            self.drawn.set(self.value);
        }
    }
    impl EventElement for RecordingLeaf {}
    impl LayoutElement for RecordingLeaf {}
    impl Rebuildable for RecordingLeaf {}

    /// Single-child wrapper that DRAWS its child (like `Container`) and is
    /// always replaced on reconcile (`update_from_widget` -> false, default).
    struct DrawWrapper(Box<dyn Element>);
    impl VisitorElement for DrawWrapper {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.0.as_ref());
        }
        fn debug_name(&self) -> &'static str {
            "DrawWrapper"
        }
    }
    impl Drawable for DrawWrapper {
        fn draw(&self, ctx: &BuildContext) {
            self.0.draw(ctx);
        }
    }
    impl EventElement for DrawWrapper {
        fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            visitor(self.0.as_ref());
        }
    }
    impl LayoutElement for DrawWrapper {}
    impl Rebuildable for DrawWrapper {}
    /// Multi-child wrapper that DRAWS every child in order (like `Flex`).
    struct DrawRow(Vec<Box<dyn Element>>);
    impl VisitorElement for DrawRow {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for c in &self.0 {
                visitor(c.as_ref());
            }
        }
        fn debug_name(&self) -> &'static str {
            "DrawRow"
        }
    }
    impl Drawable for DrawRow {
        fn draw(&self, ctx: &BuildContext) {
            for c in &self.0 {
                c.draw(ctx);
            }
        }
    }
    impl EventElement for DrawRow {
        fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for c in &self.0 {
                visitor(c.as_ref());
            }
        }
    }
    impl LayoutElement for DrawRow {}
    impl Rebuildable for DrawRow {}

    /// A nested stateful widget standing in for the `Button` sibling, so the
    /// counter's rebuild has to reconcile a real `StatefulElement` sibling
    /// alongside the text leaf (matching jaime's flex children).
    struct NestedButtonWidget;
    struct NestedButtonState {
        #[allow(dead_code)]
        updater: StateUpdater<Self>,
    }
    impl StatefulWidget for NestedButtonWidget {
        type State = NestedButtonState;
        fn create_state(&self) -> Self::State {
            NestedButtonState { updater: StateUpdater::new() }
        }
    }
    impl State<NestedButtonWidget> for NestedButtonState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            self.updater = updater;
        }
        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            EmptyWidget
        }
    }

    struct DrawCounterWidget {
        drawn: Rc<Cell<usize>>,
        live_updater: Rc<RefCell<Option<StateUpdater<DrawCounterState>>>>,
    }
    struct DrawCounterState {
        counter: usize,
        drawn: Rc<Cell<usize>>,
        live_updater: Rc<RefCell<Option<StateUpdater<Self>>>>,
        updater: StateUpdater<Self>,
    }
    impl StatefulWidget for DrawCounterWidget {
        type State = DrawCounterState;
        fn create_state(&self) -> Self::State {
            DrawCounterState {
                counter: 1,
                drawn: self.drawn.clone(),
                live_updater: self.live_updater.clone(),
                updater: StateUpdater::new(),
            }
        }
    }
    impl State<DrawCounterWidget> for DrawCounterState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            self.updater = updater;
        }
        fn build(&self, ctx: &BuildContext) -> impl Widget {
            *self.live_updater.borrow_mut() = Some(self.updater.clone());

            // container -> row -> [ text-leaf(counter), nested stateful button ]
            let leaf: Box<dyn Element> = Box::new(RecordingLeaf { value: self.counter, drawn: self.drawn.clone() });
            let (button, _ctor) = StatefulElement::new_with_name(&NestedButtonWidget, ctx, "NestedButton", None);
            let row: Box<dyn Element> = Box::new(DrawRow(vec![leaf, button.boxed()]));
            ElementWidget::new(Box::new(DrawWrapper(row)))
        }
    }

    #[test]
    fn set_state_updates_the_drawn_subtree() {
        let ctx = dummy_build_context();
        let drawn = Rc::new(Cell::new(0usize));
        let live_updater: Rc<RefCell<Option<StateUpdater<DrawCounterState>>>> = Rc::new(RefCell::new(None));

        let widget = DrawCounterWidget { drawn: drawn.clone(), live_updater: live_updater.clone() };
        let (root, _ctor) = StatefulElement::new_with_name(&widget, &ctx, "DrawCounter", None);

        // First frame: the initial state (counter = 1) is drawn.
        root.draw(&ctx);
        assert_eq!(drawn.get(), 1, "initial draw must render counter = 1");

        // Fire a mutation through the state's OWN updater — exactly what the
        // `Increase` button's `on_press` does.
        live_updater
            .borrow()
            .as_ref()
            .expect("build() publishes the live updater")
            .set_state(|s| s.counter = 2);

        // Next frame: the DRAWN subtree must reflect the new value. If the old
        // child were kept/drawn, `drawn` would stay 1 — the on-screen freeze.
        root.draw(&ctx);
        assert_eq!(drawn.get(), 2, "after set_state the DRAWN subtree must render counter = 2");
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



        struct FakeScrollable {
            child: Box<dyn Element>,
            key: Option<Key>,
        }

        impl FakeScrollable {
            fn new(child: Box<dyn Element>) -> Self {
                Self { child, key: Some(Key::Static("scrollable-default")) }
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
