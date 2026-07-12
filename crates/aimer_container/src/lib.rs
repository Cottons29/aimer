pub mod flex;
pub mod grid;
pub mod scrollable;
mod single_child;
pub mod space;

pub use scrollable::scroll_behavior::*;
pub use scrollable::*;
pub use single_child::container::Container;
pub use single_child::sized_box::SizedBox;
pub use single_child::zero_size_box::ZeroSizedBox;
pub use space::positioned::Positioned;
pub use space::stack::Stack;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flex::Column;
    use crate::flex::LayoutDirection;
    use crate::flex::Row;
    use crate::flex::flex_child::RawExpanded;
    use crate::flex::raw_flex::RawFlex;
    use crate::scrollable::raw_scroll::RawScrollableContainer;
    use aimer_attribute::BoxConstraint;
    use aimer_attribute::CacheBounds;
    use aimer_attribute::size::ResolvedSize;
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_macro::key;
    use aimer_widget::Key;
    use aimer_widget::base::BuildContext;
    use aimer_widget::{
        Drawable, Element, EventElement, LayoutElement, NamedWidget, Rebuildable, State, StateUpdater, StatefulElement,
        StatefulWidget, StatelessElement, VisitorElement, Widget,
    };
    use std::any::{Any, TypeId};
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::{OnceLock, RwLock};

    // ─── A faithful stand-in for a `TextButton` ───────────────────────────
    //
    // Mirrors `crates/aimer_input/src/text_button.rs`: a `StatefulWidget`
    // whose `State` stores the parent-provided config (`index`, `selected`)
    // plus a runtime field (`hovered`). It refreshes the config on reconcile
    // via `adopt_config_from` exactly like `ButtonState` does, and records its
    // rendered `selected` value into a shared observer so the test can see
    // which button believes it is highlighted. Constructed via `NamedWidget`
    // (as `#[derive(WidgetConstructor)]` does for `TextButton`) so the element
    // goes through the exact same wrapper path.
    struct ButtonLike {
        index: usize,
        selected: bool,
        observers: Rc<Vec<Rc<Cell<i32>>>>,
    }

    struct ButtonLikeState {
        index: usize,
        selected: bool,
        #[allow(dead_code)]
        hovered: bool,
        observers: Rc<Vec<Rc<Cell<i32>>>>,
        updater: StateUpdater<Self>,
    }

    impl StatefulWidget for ButtonLike {
        type State = ButtonLikeState;

        fn create_state(&self) -> Self::State {
            ButtonLikeState {
                index: self.index,
                selected: self.selected,
                hovered: false,
                observers: self.observers.clone(),
                updater: StateUpdater::new(),
            }
        }
    }

    impl Widget for ButtonLike {
        fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
            // Bare stateful with the default "Unknown" name, exactly like
            // `TextButton::to_element`. The surrounding `NamedWidget` then
            // wraps it in a `StatelessElement("ButtonLike")`.
            Box::new(StatefulElement::new(self, ctx).0)
        }
    }

    impl State<ButtonLike> for ButtonLikeState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            self.updater = updater;
        }

        // Mirrors `ButtonState::adopt_config_from`: refresh the config
        // (`index`, `selected`) while keeping runtime (`hovered`).
        fn adopt_config_from(&mut self, new: &Self) {
            self.index = new.index;
            self.selected = new.selected;
        }

        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            self.observers[self.index].set(if self.selected { 1 } else { 0 });
            Container!(
                height: 32,
                child: crate::ZeroSizedBox
            )
        }
    }

    fn button(index: usize, selected: bool, observers: Rc<Vec<Rc<Cell<i32>>>>) -> Box<dyn Widget> {
        Box::new(NamedWidget::new(Box::new(ButtonLike { index, selected, observers }), "ButtonLike"))
    }

    struct TabWidget {
        observer: Rc<Cell<usize>>,
        live_updater: Rc<RefCell<Option<StateUpdater<TabState>>>>,
        button_observers: Rc<Vec<Rc<Cell<i32>>>>,
    }

    struct TabState {
        index: usize,
        observer: Rc<Cell<usize>>,
        live_updater: Rc<RefCell<Option<StateUpdater<Self>>>>,
        button_observers: Rc<Vec<Rc<Cell<i32>>>>,
        updater: StateUpdater<Self>,
    }

    impl StatefulWidget for TabWidget {
        type State = TabState;

        fn create_state(&self) -> Self::State {
            TabState {
                index: 0,
                observer: self.observer.clone(),
                live_updater: self.live_updater.clone(),
                button_observers: self.button_observers.clone(),
                updater: StateUpdater::new(),
            }
        }
    }

    impl Widget for TabWidget {
        fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
            Box::new(StatefulElement::new_with_name(self, ctx, "TabWidget", None).0)
        }

        fn debug_name(&self) -> &'static str {
            "TabWidget"
        }
    }

    impl State<TabWidget> for TabState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            self.updater = updater;
        }

        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            self.observer.set(self.index);
            *self.live_updater.borrow_mut() = Some(self.updater.clone());
            // Content follows the selection (the image in the real app) AND a
            // Row of buttons whose highlight must follow the selection too.
            Column!(
                children: [
                    Container!(
                        height: 180,
                        child: crate::ZeroSizedBox
                    ),
                    Row!(
                        children: [
                            button(0, self.index == 0, self.button_observers.clone()),
                            button(1, self.index == 1, self.button_observers.clone()),
                            button(2, self.index == 2, self.button_observers.clone()),
                            button(3, self.index == 3, self.button_observers.clone()),
                        ]
                    ),
                ]
            )
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Verdict {
        Survived,
        Reset,
    }

    impl Verdict {
        fn classify(observer_after_resize: usize, live_index_after_resize: usize) -> Self {
            match (observer_after_resize, live_index_after_resize) {
                (3, 3) => Self::Survived,
                (0, 0) => Self::Reset,
                other => panic!("unexpected post-resize state: {other:?}"),
            }
        }
    }

    #[derive(Debug)]
    struct VariantResult {
        label: &'static str,
        observer_after_resize: usize,
        live_index_after_resize: usize,
        verdict: Verdict,
        /// For each of the 4 buttons: 1 if it rendered as selected/highlighted,
        /// 0 if not. After picking tab 3 and resizing this must be [0,0,0,1].
        button_highlight_after_resize: Vec<i32>,
    }

    fn dummy_window() -> &'static winit::window::Window {
        const SIZE: usize = 16384;
        static SLOT: OnceLock<usize> = OnceLock::new();
        let addr = *SLOT.get_or_init(|| {
            let leaked: &'static mut [u8; SIZE] = Box::leak(Box::new([0u8; SIZE]));
            leaked.as_mut_ptr() as usize
        });
        unsafe { &*(addr as *const winit::window::Window) }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn dummy_async_handle() -> tokio::runtime::Handle {
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap());
        let _guard = runtime.enter();
        tokio::runtime::Handle::current()
    }

    fn dummy_build_context(width: f32, height: f32, visible_rect: Option<(f32, f32, f32, f32)>) -> BuildContext<'static> {
        let canvas = {
            let leaked: &'static InnerCanvas = Box::leak(Box::new(InnerCanvas::new()));
            Canvas::new(leaked)
        };

        BuildContext {
            parent_size: ResolvedSize { width, height },
            canvas,
            scale: 1.0,
            parent_pos: Default::default(),
            cursor_pos: Default::default(),
            box_constraint: BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: width, max_height: height },
            visible_rect,
            window: dummy_window(),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: dummy_async_handle(),
            inherited_states: Rc::new(RwLock::new(HashMap::<TypeId, Rc<dyn Any>>::new())),
        }
    }

    fn placeholder_section(height: i32) -> Box<dyn Widget> {
        Container!(
            height: height,
            child: crate::ZeroSizedBox
        )
    }

    fn build_home_page(
        ctx: &BuildContext,
        observer: Rc<Cell<usize>>,
        live_updater: Rc<RefCell<Option<StateUpdater<TabState>>>>,
        button_observers: Rc<Vec<Rc<Cell<i32>>>>,
    ) -> Box<dyn Element> {
        Container!(
            child: Stack!(
                children: [
                    Positioned!(
                        top: 0,
                        left: 0,
                        layer: 1,
                        child: Container!(
                            height: 48,
                            child: crate::ZeroSizedBox
                        )
                    ),
                    Positioned!(
                        top: 0,
                        left: 0,
                        layer: 0,
                        child: Scrollable!(
                            axis: crate::ScrollAxis::Vertical,
                            child: Column!(
                                children: [
                                    placeholder_section(100),
                                    placeholder_section(100),
                                    placeholder_section(100),
                                    Box::new(TabWidget {
                                        observer,
                                        live_updater,
                                        button_observers,
                                    }) as Box<dyn Widget>,
                                ]
                            )
                        )
                    )
                ]
            )
        )
        .to_element(ctx)
    }

    fn current_live_updater(live_updater: &Rc<RefCell<Option<StateUpdater<TabState>>>>) -> StateUpdater<TabState> {
        live_updater
            .borrow()
            .as_ref()
            .cloned()
            .expect("current live updater should be published from build()")
    }

    fn run_variant(culled: bool, resize_count: usize) -> VariantResult {
        let initial_ctx = dummy_build_context(500.0, 600.0, None);
        let observer = Rc::new(Cell::new(usize::MAX));
        let live_updater = Rc::new(RefCell::new(None));
        let button_observers: Rc<Vec<Rc<Cell<i32>>>> = Rc::new((0..4).map(|_| Rc::new(Cell::new(-1))).collect());

        let initial_child = build_home_page(&initial_ctx, observer.clone(), live_updater.clone(), button_observers.clone());
        let rebuild_observer = observer.clone();
        let rebuild_live_updater = live_updater.clone();
        let rebuild_button_observers = button_observers.clone();
        let driver = StatelessElement::new(
            initial_child,
            move |ctx| build_home_page(ctx, rebuild_observer.clone(), rebuild_live_updater.clone(), rebuild_button_observers.clone()),
            None,
            "Root",
        );

        driver.draw(&initial_ctx);
        assert_eq!(observer.get(), 0, "initial build should publish the default tab index");

        current_live_updater(&live_updater).set_state(|state| state.index = 3);
        driver.draw(&initial_ctx);

        assert_eq!(observer.get(), 3, "setup failed: observer should record the selected tab before resize");
        assert_eq!(
            current_live_updater(&live_updater).read(|state| state.index),
            3,
            "setup failed: live state should store index=3 before resize"
        );

        let resize_ctx = if culled {
            dummy_build_context(640.0, 250.0, Some((0.0, 0.0, 640.0, 250.0)))
        } else {
            dummy_build_context(640.0, 600.0, None)
        };

        for _ in 0..resize_count {
            Rebuildable::mark_needs_rebuild(&driver);
            driver.draw(&resize_ctx);
        }

        let observer_after_resize = observer.get();
        let live_index_after_resize = current_live_updater(&live_updater).read(|state| state.index);
        let verdict = Verdict::classify(observer_after_resize, live_index_after_resize);
        let button_highlight_after_resize: Vec<i32> = button_observers.iter().map(|o| o.get()).collect();

        VariantResult {
            label: match (culled, resize_count) {
                (false, 1) => "visible / one resize",
                (false, 2) => "visible / two resizes",
                (true, 1) => "culled / one resize",
                (true, 2) => "culled / two resizes",
                _ => "unexpected",
            },
            observer_after_resize,
            live_index_after_resize,
            verdict,
            button_highlight_after_resize,
        }
    }

    /// Regression for the reported "selected tab is lost on window resize" bug.
    ///
    /// Faithfully reproduces the real `website` tree
    /// (`Container → Stack → Positioned → Scrollable → Column → [.. , Tab]`)
    /// with the actual container widgets and a real `StatefulElement`, picks
    /// tab index 3, then simulates one/two window resizes for both the
    /// on-screen and the flex-culled cases. The selected tab MUST survive every
    /// variant.
    #[test]
    fn real_widget_resize_repro_keeps_selected_tab() {
        let results = [run_variant(false, 1), run_variant(false, 2), run_variant(true, 1), run_variant(true, 2)];

        for result in &results {
            eprintln!(
                "{} => observer={}, live_index={}, verdict={:?}, buttons={:?}",
                result.label,
                result.observer_after_resize,
                result.live_index_after_resize,
                result.verdict,
                result.button_highlight_after_resize
            );
        }

        for result in &results {
            assert_eq!(
                result.verdict,
                Verdict::Survived,
                "the selected tab must survive a window resize ({}): observer={}, live_index={}",
                result.label,
                result.observer_after_resize,
                result.live_index_after_resize
            );
        }
    }

    // Locate the `RawScrollableContainer` buried anywhere in an element tree so
    // a test can read its live scroll offset / cached scroll range.
    // fn find_scrollable(el: &dyn Element) -> Option<&RawScrollableContainer<Box<dyn Element>>> {
    //     if let Some(s) = el.as_any().downcast_ref::<RawScrollableContainer<Box<dyn Element>>>() {
    //         return Some(s);
    //     }
    //     let mut found: Option<&RawScrollableContainer<Box<dyn Element>>> = None;
    //     // Some layout elements (e.g. `Positioned`) expose their child through
    //     // `event_children` rather than `visit_children`, so walk both.
    //     el.visit_children(&mut |c| {
    //         if found.is_none() {
    //             found = find_scrollable(c);
    //         }
    //     });
    //     if found.is_none() {
    //         el.event_children(&mut |c| {
    //             if found.is_none() {
    //                 found = find_scrollable(c);
    //             }
    //         });
    //     }
    //     found
    // }

    /// End-to-end regression for "the Scroll is not able to scroll with mouse
    /// wheel or trackpad": build the real website layout
    /// (`Container → Stack → Positioned → Scrollable → Column`) with content
    /// taller than the viewport, draw it (which computes the scroll range and
    /// bounds), then dispatch a wheel `Scroll` event exactly like the platform
    /// layer does and assert the content actually moves.
    // #[test]
    // fn wheel_scroll_moves_content_in_website_layout() {
    //     use aimer_attribute::position::Vec2d;
    //     use aimer_events::element::{ElementEvent, TouchPhase};
    //
    //     // 500 × 600 viewport, content 2000 px tall → must be scrollable.
    //     // Root at the `Positioned` (inside a `Stack`) exactly as the website
    //     // nests it — this is the wrapping that recomputes the child's viewport
    //     // constraint, the part suspected of collapsing the scroll range.
    //     let ctx = dummy_build_context(500.0, 600.0, None);
    //     let root = Stack!(
    //         children: [
    //             Positioned!(
    //                 top: 0,
    //                 left: 0,
    //                 layer: 0,
    //                 child: Scrollable!(
    //                     axis: crate::ScrollAxis::Vertical,
    //                     child: Column!(
    //                         children: [placeholder_section(2000)]
    //                     )
    //                 )
    //             )
    //         ]
    //     )
    //     .to_element(&ctx);
    //
    //     // First frame: seeds cached scroll range, viewport bounds and cursor.
    //     root.draw(&ctx);
    //
    //     let scr = find_scrollable(root.as_ref()).expect("the tree must contain a scrollable");
    //     let max = scr.ctrl.cached_max_scroll.get();
    //     assert!(
    //         max.y > 0.0,
    //         "content (2000px) is taller than the viewport (600px), so the scroll range must be positive; \
    //          got max_scroll.y={} (a zero range means the Stack/Positioned wrapping collapsed the viewport)",
    //         max.y
    //     );
    //
    //     let before = scr.ctrl.scroll_offset.get().y;
    //
    //     // Dispatch a wheel scroll-down (negative delta = content moves up) at the
    //     // viewport centre, exactly like the windowing layer's mouse-wheel path.
    //     let handled = aimer_widget::dispatch_event(
    //         root.as_ref(),
    //         Vec2d { x: 250.0, y: 300.0 },
    //         &ElementEvent::Scroll { delta: Vec2d { x: 0.0, y: -60.0 }, phase: TouchPhase::Moved },
    //     );
    //     assert!(handled, "the wheel Scroll event must be consumed by the scrollable");
    //
    //     let after = scr.ctrl.scroll_offset.get().y;
    //     assert!(after < before, "a wheel scroll-down must move the offset (before={before}, after={after})");
    //
    //     // Simulate follow-up animation frames (momentum/spring settle). An
    //     // in-range scroll must NOT spring back to the top — that would look
    //     // like "the content can't be scrolled" even though the event fired.
    //     for _ in 0..8 {
    //         root.draw(&ctx);
    //     }
    //     let settled = scr.ctrl.scroll_offset.get().y;
    //     assert!(
    //         settled < -1.0,
    //         "after scrolling down the content must stay scrolled (settled offset={settled}), not spring back to the top"
    //     );
    // }
    //
    // /// Regression for "the scroll is applied even when the pointer is over the
    // /// `HeaderSection`": the website stacks a full-screen `Scrollable`
    // /// (layer 0) under an opaque `HeaderSection` bar (layer 1). A wheel /
    // /// trackpad scroll whose pointer sits on the header must be absorbed by the
    // /// opaque header and must NOT reach — nor move — the `Scrollable` behind it;
    // /// a scroll whose pointer sits on the exposed content below the header must
    // /// still scroll.
    // #[test]
    // fn scroll_over_opaque_header_does_not_reach_scrollable_below() {
    //     use aimer_attribute::position::Vec2d;
    //     use aimer_events::element::{ElementEvent, TouchPhase};
    //     use aimer_widget::base::Color;
    //
    //     let ctx = dummy_build_context(500.0, 600.0, None);
    //     let root = Stack!(
    //         children: [
    //             // layer 0: the full-screen scrollable content.
    //             Positioned!(
    //                 top: 0,
    //                 left: 0,
    //                 layer: 0,
    //                 child: Scrollable!(
    //                     axis: crate::ScrollAxis::Vertical,
    //                     child: Column!(
    //                         children: [placeholder_section(2000)]
    //                     )
    //                 )
    //             ),
    //             // layer 1: an opaque header bar pinned to the top (0..100 px).
    //             Positioned!(
    //                 top: 0,
    //                 left: 0,
    //                 layer: 1,
    //                 child: Container!(
    //                     height: 100,
    //                     color: Color::Rgba(255, 0, 0, 255),
    //                     child: crate::ZeroSizedBox
    //                 )
    //             )
    //         ]
    //     )
    //     .to_element(&ctx);
    //
    //     // First frame seeds the scroll range and every element's on-screen bounds.
    //     root.draw(&ctx);
    //
    //     let scr = find_scrollable(root.as_ref()).expect("the tree must contain a scrollable");
    //     assert!(scr.ctrl.cached_max_scroll.get().y > 0.0, "content must be taller than the viewport so it is scrollable");
    //
    //     // ── Scroll with the pointer ON the header (y = 50, inside 0..100). ──
    //     let before_header = scr.ctrl.scroll_offset.get().y;
    //     let handled_header = aimer_widget::dispatch_event(
    //         root.as_ref(),
    //         Vec2d { x: 250.0, y: 50.0 },
    //         &ElementEvent::Scroll { delta: Vec2d { x: 0.0, y: -60.0 }, phase: TouchPhase::Moved },
    //     );
    //     assert!(handled_header, "the opaque header must consume (absorb) the scroll");
    //     assert_eq!(scr.ctrl.scroll_offset.get().y, before_header, "a scroll over the opaque header must NOT move the scrollable behind it");
    //
    //     // ── Scroll with the pointer on the exposed content (y = 300, below the header). ──
    //     let before_content = scr.ctrl.scroll_offset.get().y;
    //     let handled_content = aimer_widget::dispatch_event(
    //         root.as_ref(),
    //         Vec2d { x: 250.0, y: 300.0 },
    //         &ElementEvent::Scroll { delta: Vec2d { x: 0.0, y: -60.0 }, phase: TouchPhase::Moved },
    //     );
    //     assert!(handled_content, "a scroll over the exposed content must be consumed by the scrollable");
    //     assert!(
    //         scr.ctrl.scroll_offset.get().y < before_content,
    //         "a scroll over the exposed content (below the header) must move the scrollable"
    //     );
    // }

    /// Regression for the reported "button active/selected highlight is stuck
    /// on the initially-selected tab after a window resize" bug.
    ///
    /// The section content follows the live selection (verified by
    /// `real_widget_resize_repro_keeps_selected_tab`), but each platform button
    /// is its own `StatefulWidget` (`TextButton`) whose `State` mirrors the
    /// parent-provided `selected` prop. After picking tab 3 ("Android") and
    /// resizing, ONLY button 3 must render highlighted — the buttons' `selected`
    /// config must be refreshed to match the live selection.
    #[test]
    fn real_widget_resize_repro_keeps_button_highlight() {
        let results = [run_variant(false, 1), run_variant(false, 2), run_variant(true, 1), run_variant(true, 2)];

        for result in &results {
            eprintln!("{} => buttons={:?}", result.label, result.button_highlight_after_resize);
        }

        for result in &results {
            assert_eq!(
                result.button_highlight_after_resize,
                vec![0, 0, 0, 1],
                "after picking tab 3 and resizing, ONLY button 3 must stay highlighted ({}): got {:?}",
                result.label,
                result.button_highlight_after_resize
            );
        }
    }

    // A leaf element that records the main-axis (`max_width`) constraint it is
    // laid out with, so a test can observe exactly how much space its flex
    // parent handed it. It reports a size that fills whatever it is given.
    struct MainAxisProbe {
        seen: Rc<Cell<f32>>,
    }

    impl VisitorElement for MainAxisProbe {
        fn debug_name(&self) -> &'static str {
            "MainAxisProbe"
        }
    }
    impl Drawable for MainAxisProbe {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl EventElement for MainAxisProbe {}
    impl LayoutElement for MainAxisProbe {
        fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
            self.seen.set(ctx.box_constraint.max_width);
            ResolvedSize { width: ctx.box_constraint.max_width, height: ctx.box_constraint.max_height }
        }
    }
    impl Rebuildable for MainAxisProbe {}


    fn expanded_probe(flex: f32, seen: &Rc<Cell<f32>>) -> Box<dyn Element> {
        Box::new(RawExpanded { child: MainAxisProbe { seen: seen.clone() }, flex, debug_name: "Expanded" })
    }

    // A leaf element with a fixed intrinsic main-axis size that ignores the
    // constraint it is given — emulating a `Text` (which reports no explicit
    // `size()` yet measures to its content width). It records the constraint it
    // saw so a test can assert it was *not* stretched.
    struct IntrinsicProbe {
        intrinsic_width: f32,
        seen: Rc<Cell<f32>>,
    }

    impl VisitorElement for IntrinsicProbe {
        fn debug_name(&self) -> &'static str {
            "IntrinsicProbe"
        }
    }
    impl Drawable for IntrinsicProbe {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl EventElement for IntrinsicProbe {}
    impl LayoutElement for IntrinsicProbe {
        fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
            self.seen.set(ctx.box_constraint.max_width);
            ResolvedSize { width: self.intrinsic_width, height: ctx.box_constraint.max_height }
        }
    }
    impl Rebuildable for IntrinsicProbe {}

    fn intrinsic_probe(intrinsic_width: f32, seen: &Rc<Cell<f32>>) -> Box<dyn Element> {
        Box::new(IntrinsicProbe { intrinsic_width, seen: seen.clone() })
    }

    fn row_of(children: Vec<Box<dyn Element>>) -> RawFlex {
        RawFlex {
            direction: LayoutDirection::Row,
            vertical_alignment: Default::default(),
            horizontal_alignment: Default::default(),
            gaps: Default::default(),
            children,
            cache: Default::default(),
            overflow_behavior: Default::default(),
            debug_name: "Row",
            cache_bound: CacheBounds::new(),
        }
    }

    /// A single `Expanded` in a `Row` fills the whole parent width.
    #[test]
    fn expanded_single_child_fills_row() {
        let ctx = dummy_build_context(300.0, 100.0, None);
        let c1 = Rc::new(Cell::new(0.0));
        let row = row_of(vec![expanded_probe(1.0, &c1)]);

        let _ = row.computed_size(&ctx);

        assert_eq!(c1.get(), 300.0, "the only Expanded must receive the full width");
    }

    /// Two equal `Expanded` children split the width evenly.
    #[test]
    fn expanded_two_equal_children_split_evenly() {
        let ctx = dummy_build_context(300.0, 100.0, None);
        let c1 = Rc::new(Cell::new(0.0));
        let c2 = Rc::new(Cell::new(0.0));
        let row = row_of(vec![expanded_probe(1.0, &c1), expanded_probe(1.0, &c2)]);

        let _ = row.computed_size(&ctx);

        assert_eq!(c1.get(), 150.0);
        assert_eq!(c2.get(), 150.0);
    }

    /// `flex = 1` and `flex = 2` split the width 1/3 : 2/3.
    #[test]
    fn expanded_weighted_children_split_proportionally() {
        let ctx = dummy_build_context(300.0, 100.0, None);
        let c1 = Rc::new(Cell::new(0.0));
        let c2 = Rc::new(Cell::new(0.0));
        let row = row_of(vec![expanded_probe(1.0, &c1), expanded_probe(2.0, &c2)]);

        let _ = row.computed_size(&ctx);

        assert_eq!(c1.get(), 100.0, "flex=1 child gets 1/3 of the width");
        assert_eq!(c2.get(), 200.0, "flex=2 child gets 2/3 of the width");
    }

    /// A fixed-size sibling is subtracted first; the remaining space is shared
    /// by the flex children according to their weights.
    #[test]
    fn expanded_shares_space_left_by_fixed_sibling() {
        let ctx = dummy_build_context(300.0, 100.0, None);
        let c1 = Rc::new(Cell::new(0.0));
        let c2 = Rc::new(Cell::new(0.0));
        let fixed: Box<dyn Element> = Container!(
            width: 60,
            child: crate::ZeroSizedBox
        )
        .to_element(&ctx);
        let row = row_of(vec![fixed, expanded_probe(1.0, &c1), expanded_probe(2.0, &c2)]);

        let _ = row.computed_size(&ctx);

        // 300 - 60 = 240 free, split 1:2 => 80 and 160.
        assert_eq!(c1.get(), 80.0, "flex=1 child gets 1/3 of the remaining 240px");
        assert_eq!(c2.get(), 160.0, "flex=2 child gets 2/3 of the remaining 240px");
    }

    /// Regression for the website header: a size-less intrinsic child (a `Text`)
    /// next to an `Expanded` must NOT be treated as flexible. The text keeps its
    /// intrinsic width and the `Expanded` fills *all* the remaining space, not
    /// half of it.
    #[test]
    fn intrinsic_child_does_not_steal_flex_space_from_expanded() {
        let ctx = dummy_build_context(300.0, 100.0, None);
        let text = Rc::new(Cell::new(0.0));
        let exp = Rc::new(Cell::new(0.0));
        // Row: [ Text(width 50), Expanded ]
        let row = row_of(vec![intrinsic_probe(50.0, &text), expanded_probe(1.0, &exp)]);

        let _ = row.computed_size(&ctx);

        // The Expanded must get the whole remaining 300 - 50 = 250, not (300)/2.
        assert_eq!(exp.get(), 250.0, "the Expanded must fill ALL space left by the intrinsic-sized child");
    }

    /// Two intrinsic (size-less) children and a single `Expanded`: the plain
    /// children keep their own widths and the `Expanded` swallows the rest.
    #[test]
    fn multiple_intrinsic_children_keep_size_expanded_fills_rest() {
        let ctx = dummy_build_context(300.0, 100.0, None);
        let a = Rc::new(Cell::new(0.0));
        let b = Rc::new(Cell::new(0.0));
        let exp = Rc::new(Cell::new(0.0));
        let row = row_of(vec![intrinsic_probe(30.0, &a), intrinsic_probe(70.0, &b), expanded_probe(1.0, &exp)]);

        let _ = row.computed_size(&ctx);

        assert_eq!(exp.get(), 200.0, "Expanded fills 300 - 30 - 70 = 200");
    }
}
