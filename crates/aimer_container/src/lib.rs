mod single_child;
pub mod flex;
pub mod space;
pub mod scrollable;
pub mod grid;

pub use single_child::sized_box::SizedBox;
pub use single_child::container::Container;
pub use single_child::zero_size_box::ZeroSizedBox;
pub use space::positioned::Positioned;
pub use space::stack::Stack;
pub use scrollable::*;
pub use scrollable::scroll_behavior::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flex::Column;
    use aimer_attribute::BoxConstraint;
    use aimer_attribute::size::ResolvedSize;
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_macro::key;
    use aimer_widget::base::BuildContext;
    use aimer_widget::Key;
    use aimer_widget::{Drawable, Element, NamedWidget, Rebuildable, State, StateUpdater, StatefulElement, StatefulWidget, StatelessElement, Widget};
    use crate::flex::Row;
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
        Box::new(NamedWidget::new(
            Box::new(ButtonLike {
                index,
                selected,
                observers,
            }),
            "ButtonLike",
        ))
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
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        });
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
            box_constraint: BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: width,
                max_height: height,
            },
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

    fn current_live_updater(
        live_updater: &Rc<RefCell<Option<StateUpdater<TabState>>>>,
    ) -> StateUpdater<TabState> {
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
        let button_observers: Rc<Vec<Rc<Cell<i32>>>> =
            Rc::new((0..4).map(|_| Rc::new(Cell::new(-1))).collect());

        let initial_child = build_home_page(
            &initial_ctx,
            observer.clone(),
            live_updater.clone(),
            button_observers.clone(),
        );
        let rebuild_observer = observer.clone();
        let rebuild_live_updater = live_updater.clone();
        let rebuild_button_observers = button_observers.clone();
        let driver = StatelessElement::new(
            initial_child,
            move |ctx| {
                build_home_page(
                    ctx,
                    rebuild_observer.clone(),
                    rebuild_live_updater.clone(),
                    rebuild_button_observers.clone(),
                )
            },
            None,
            "Root",
        );

        driver.draw(&initial_ctx);
        assert_eq!(observer.get(), 0, "initial build should publish the default tab index");

        current_live_updater(&live_updater).set_state(|state| state.index = 3);
        driver.draw(&initial_ctx);

        assert_eq!(observer.get(), 3, "setup failed: observer should record the selected tab before resize");
        assert_eq!(current_live_updater(&live_updater).read(|state| state.index), 3, "setup failed: live state should store index=3 before resize");

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
        let button_highlight_after_resize: Vec<i32> =
            button_observers.iter().map(|o| o.get()).collect();

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
        let results = [
            run_variant(false, 1),
            run_variant(false, 2),
            run_variant(true, 1),
            run_variant(true, 2),
        ];

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
        let results = [
            run_variant(false, 1),
            run_variant(false, 2),
            run_variant(true, 1),
            run_variant(true, 2),
        ];

        for result in &results {
            eprintln!(
                "{} => buttons={:?}",
                result.label, result.button_highlight_after_resize
            );
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
}
