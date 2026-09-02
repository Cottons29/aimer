extern crate self as aimer_widget;

mod async_builder;
pub mod components;
pub mod focus_scope;
pub mod focusable;
pub mod key;
pub mod layout_cache;
pub mod page_storage;
pub mod platform_brightness;
pub mod pointer_claim;
#[doc(hidden)]
pub mod portable;
pub mod reconcile;
mod reconciliation_plan;
mod frame_work_stats;
#[cfg(not(feature = "portable-guest"))]
mod paint_isolated;
mod paint_stats;
mod rebuild_stats;
pub mod safe_area;
mod widget;
pub mod window_metrics;

/// An Empty Widget that using as placeholder
///
/// ## Example
///
/// ```rust ignore
/// pub struct MyWidget<W = EmptyWidget> {
///     child: W,
///     // any fields here
/// }
/// ```
pub struct RequiredChild;

/// An owned, type-erased [`Element`], stored as a thin two-word owner.
///
/// Elements are the retained side of the tree and are large: the smallest
/// documented layout is well over a hundred bytes, so no realistic inline
/// capacity would ever avoid their allocation. `AnyElement` therefore asks
/// [`aimer_rubick::Rubick`] for the minimum capacity of one word, which is
/// exactly the heap pointer. The owner costs two words instead of carrying an
/// inline buffer that could never be used, which matters because the retained
/// tree holds one owner per node for the lifetime of the application.
///
/// Borrowing through `Deref` or `AsRef` provides a `dyn Element` view with
/// normal dynamic dispatch and no adapter call. The name of [`Element::boxed`]
/// is retained for source familiarity.
pub type AnyElement = aimer_rubick::Rubick<dyn Element, 1>;

/// An owned, type-erased [`Widget`] with inline storage and heap fallback.
///
/// Widgets are the throwaway side of the tree: they are rebuilt every frame
/// and are small, so avoiding their allocation is worth spending owner bytes
/// on. `AnyWidget` reserves eight words of inline capacity, which covers the
/// common containers — a `Row` or `Column` is eight words — and leaves the
/// owner itself at nine words. Larger or over-aligned widgets transparently
/// take one pooled heap block.
///
/// `Deref` and `AsRef` expose the stored widget as [`DynWidget`], the
/// object-safe half of [`Widget`], and [`aimer_rubick::Rubick::is_inline`] and
/// [`aimer_rubick::Rubick::is_heap`] report the selected mode. The conversion
/// itself consumes the handle: [`AnyWidgetExt::into_element`] moves the widget
/// out of this storage, so the erased path copies no more than the direct one.
///
/// Moving an inline `AnyWidget` changes the address of its concrete widget. The
/// owner does not provide implicit unsizing or a stable-address guarantee;
/// construct it through [`Widget::boxed`].
///
/// ```
/// use aimer_widget::base::BuildContext;
/// use aimer_widget::{AnyElement, PortableWidget, Widget};
///
/// struct Badge;
///
/// impl PortableWidget for Badge {}
///
/// impl Widget for Badge {
///     fn to_element(self, _ctx: &BuildContext) -> AnyElement {
///         unreachable!("this example only erases the widget")
///     }
/// }
///
/// let widget = Badge.boxed();
/// assert!(widget.is_inline());
/// ```
pub type AnyWidget = aimer_rubick::Rubick<dyn DynWidget, 8>;

// #[cfg(debug_assertions)]
pub mod inspector_overlay {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub static INSPECTOR_ENABLED: AtomicBool = AtomicBool::new(false);

    /// The widget currently under the inspector cursor.
    pub type HoveredWidget = (&'static str, crate::base::Vec2d, crate::base::Vec2d);

    // Hover collection and overlay painting happen on the window's UI/render
    // thread. Keeping this state thread-local avoids a process-wide lock and
    // keeps the hot per-element write allocation-free.
    thread_local! {
        static HOVERED_WIDGET: Cell<Option<HoveredWidget>> = const { Cell::new(None) };
    }

    /// Publishes the widget currently under the inspector cursor.
    pub fn set_hovered_widget(widget: HoveredWidget) {
        HOVERED_WIDGET.with(|hovered| hovered.set(Some(widget)));
    }

    /// Clears the widget currently under the inspector cursor for this UI thread.
    pub fn clear_hovered_widget() {
        HOVERED_WIDGET.with(|hovered| hovered.set(None));
    }

    /// Returns the widget currently under the inspector cursor for this UI thread.
    pub fn hovered_widget() -> Option<HoveredWidget> {
        HOVERED_WIDGET.with(Cell::get)
    }

    pub fn is_enabled() -> bool {
        INSPECTOR_ENABLED.load(Ordering::Relaxed)
    }
    pub fn set_enabled(v: bool) {
        INSPECTOR_ENABLED.store(v, Ordering::Relaxed);
    }
}

pub use crate::components::diagnostics::{
    ErrorElement, ErrorWidget, OverflowEdges, OverflowIndicator, detect_overflow,
    paint_overflow_indicator,
};
pub use crate::components::drawable::Drawable;
pub use crate::components::element::{
    Element, ElementId, ElementPath, EventDispatchContext, EventDispatcher, begin_event_frame,
    element_tree_generation, layout_invalidation_generation, rebuild_invalidation_generation,
};
pub use crate::rebuild_stats::RebuildStats;
pub use crate::frame_work_stats::FrameWorkStats;
pub use crate::frame_work_stats::{
    record_hit_test_visit, record_layout_call, record_paint_call, record_redraw_request,
    record_root_draw_call, record_scroll_event, record_scroll_offset_update, record_scroll_step,
    record_smoothing_step, record_state_update, reset_frame_work_stats, take_frame_work_stats,
};
pub use crate::paint_stats::PaintStats;
pub use crate::paint_stats::{
    record_paint_isolation_candidate, record_paint_isolation_fallback,
    record_paint_isolation_invalidation, record_paint_isolation_record,
    record_paint_isolation_replay, record_paint_isolation_tile_record,
    record_paint_isolation_tile_replay, reset_paint_stats, take_paint_stats,
};
#[cfg(not(feature = "portable-guest"))]
#[doc(hidden)]
pub use crate::paint_isolated::{
    PaintBounds, PaintCache, PaintClip, PaintContract, PaintIsolated, PaintIsolatedOutcome,
    PaintTransform,
};
#[cfg(any(debug_assertions, feature = "frame-stats"))]
pub use crate::components::element::{
    reset_draw_traversal_count, reset_routed_event_visit_count, take_draw_traversal_count,
    take_routed_event_visit_count,
};
#[cfg(any(debug_assertions, feature = "frame-stats"))]
pub use crate::rebuild_stats::{reset as reset_rebuild_stats, take as take_rebuild_stats};
pub use crate::components::event_element::{
    CaptureRequest, EventElement, EventResult, FollowUp, PointerKey,
};
/// Keyboard focus lives in its own crate; it is re-exported here so
/// `aimer_widget::focus::*` keeps naming the focus system.
pub use aimer_focus as focus;
pub use aimer_focus::{
    FocusBehavior, FocusCallback, FocusCandidate, FocusManager, FocusNode, FocusTrap, FocusTrapId,
    FocusTransition, active_focus_trap,
};
pub use crate::focus_scope::FocusScope;
pub use crate::focusable::{Focusable, FocusableState, RawFocusable};
pub use crate::components::layout_element::LayoutElement;
pub use crate::components::rebuildable::Rebuildable;
pub use crate::components::visitor_element::VisitorElement;

pub mod base {
    pub use aimer_attribute::dimension::Dimension;
    pub use aimer_attribute::position::Vec2d;
    pub use aimer_attribute::size::{ResolvedSize, Size};
    pub use aimer_color::prelude::*;

    #[doc(hidden)]
    pub use crate::components::context::BuildConsumer;
    pub use crate::components::context::{BuildContext, WindowHandle};
}
pub use aimer_canvas::{TextHorizontalAlign, TextOverflowMode};
pub use aimer_macro::{PortableValue, PortableWidget, main, widget};

pub use crate::async_builder::{AsyncBuilder, AsyncSnapshot};
pub use crate::components::element::{broadcast_event, dispatch_event, dispatch_focused_event};
pub use crate::key::Key;
pub use crate::layout_cache::LayoutCache;
pub use crate::platform_brightness::{Brightness, platform_brightness, set_platform_brightness};
pub use crate::pointer_claim::{
    claim_pointer, claimed_pointer_count, is_pointer_claimed, release_all_pointers,
    release_pointer,
};
pub use crate::reconciliation_plan::{
    ReconciliationMatch, ReconciliationMatchKind, ReconciliationPlan, ReconciliationPlanError,
    plan_element_reconciliation,
};
pub use crate::safe_area::{SafeAreaInsets, safe_area_insets, set_safe_area_insets};
pub use crate::widget::{AnyWidgetExt, DynWidget, PortableWidget, Widget};
pub use crate::widget::child_builder::ChildBuilder;
pub use crate::widget::stateful::{State, StateUpdater, StatefulElement, StatefulWidget};
#[cfg(feature = "portable-guest")]
#[doc(hidden)]
pub use crate::widget::stateful::StateReadGuard;
pub use crate::widget::stateless::{NamedWidget, StatelessElement, StatelessWidget};
pub use crate::window_metrics::{WindowMetrics, notify_window_metrics_changed};

/// Carries the live state of an old element subtree into the subtree replacing
/// it, and transfers its logical identities onto the replacement.
///
/// This is the step a rebuild performs between building the replacement and
/// installing it: every state-owning element in `new` that corresponds to one in
/// `old` adopts its runtime state, so a `set_state` above a subtree does not
/// reset the widgets inside it. Identities travel the same way, because the
/// dispatcher's captured pointers and focus records name elements by id: a
/// pointer captured before the rebuild must still reach the element that
/// replaced its owner, or the capture turns into a ghost and swallows every
/// event routed its way. Containers that materialize their children on demand
/// also exchange them here, through
/// [`Rebuildable::take_retained_children`].
///
/// Safe to call on any pair: elements that do not correspond are left alone.
#[inline]
pub fn carry_element_state(old: &dyn Element, new: &dyn Element, ctx: &base::BuildContext) {
    plan_element_reconciliation(old, new)
        .commit(ctx)
        .expect("fresh reconciliation plan must remain valid until commit");
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::cell::Cell;

    use crate::base::{BuildContext, WindowHandle};
    use crate::components::element::test_generation_guard;
    use crate::{
        AnyElement, Drawable, Element, EventElement, LayoutElement, Rebuildable,
        ReconciliationMatchKind, VisitorElement, element_tree_generation,
        plan_element_reconciliation,
    };

    #[test]
    fn planning_reconciliation_does_not_mutate_either_tree() {
        let old = Branch::new([
            StateLeaf::keyed("first", 11).boxed(),
            StateLeaf::keyed("second", 22).boxed(),
        ]);
        let new = Branch::new([
            StateLeaf::keyed("second", 0).boxed(),
            StateLeaf::keyed("first", 0).boxed(),
        ]);
        let old_ids = child_ids(old.as_ref());
        let new_ids = child_ids(new.as_ref());
        let _generation_guard = test_generation_guard();
        let generation = element_tree_generation();

        let plan = plan_element_reconciliation(old.as_ref(), new.as_ref());

        assert_eq!(child_ids(old.as_ref()), old_ids);
        assert_eq!(child_ids(new.as_ref()), new_ids);
        assert_eq!(leaf_states(new.as_ref()), [0, 0]);
        assert_eq!(element_tree_generation(), generation);
        assert_eq!(
            plan.matches()
                .iter()
                .map(|element_match| element_match.kind())
                .collect::<Vec<_>>(),
            [
                ReconciliationMatchKind::Root,
                ReconciliationMatchKind::Keyed,
                ReconciliationMatchKind::Keyed,
            ]
        );
        plan.validate().unwrap();
    }

    #[test]
    fn reconciliation_commit_applies_keyed_and_positional_matches_atomically() {
        let old = Branch::new([
            StateLeaf::keyed("first", 11).boxed(),
            StateLeaf::keyed("second", 22).boxed(),
            StateLeaf::unkeyed(33).boxed(),
        ]);
        let new = Branch::new([
            StateLeaf::keyed("second", 0).boxed(),
            StateLeaf::keyed("first", 0).boxed(),
            StateLeaf::unkeyed(0).boxed(),
        ]);
        let old_ids = child_ids(old.as_ref());
        let generation = element_tree_generation();
        let plan = plan_element_reconciliation(old.as_ref(), new.as_ref());

        assert_eq!(
            plan.matches()
                .iter()
                .map(|element_match| element_match.kind())
                .collect::<Vec<_>>(),
            [
                ReconciliationMatchKind::Root,
                ReconciliationMatchKind::Keyed,
                ReconciliationMatchKind::Keyed,
                ReconciliationMatchKind::Positional,
            ]
        );

        plan.commit(&context()).unwrap();

        assert_eq!(child_ids(new.as_ref()), [old_ids[1], old_ids[0], old_ids[2]]);
        assert_eq!(leaf_states(new.as_ref()), [11, 22, 33]);
        assert!(element_tree_generation() > generation);
    }

    struct StateLeaf {
        key: Option<crate::Key>,
        state: Cell<u32>,
    }

    impl StateLeaf {
        fn keyed(key: &'static str, state: u32) -> Self {
            Self {
                key: Some(crate::Key::Static(key)),
                state: Cell::new(state),
            }
        }

        fn unkeyed(state: u32) -> Self {
            Self {
                key: None,
                state: Cell::new(state),
            }
        }
    }

    impl VisitorElement for StateLeaf {
        fn debug_name(&self) -> &'static str {
            "StateLeaf"
        }

        fn reconciliation_key(&self) -> Option<&crate::Key> {
            self.key.as_ref()
        }
    }

    impl EventElement for StateLeaf {}
    impl LayoutElement for StateLeaf {}
    impl Drawable for StateLeaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for StateLeaf {
        fn option_any(&self) -> Option<&dyn Any> {
            Some(self)
        }

        fn adopt_runtime_state_from(&self, old: &dyn Element) {
            let old = old
                .option_any()
                .and_then(|value| value.downcast_ref::<Self>())
                .unwrap();
            self.state.set(old.state.get());
        }
    }

    struct Branch(Vec<AnyElement>);

    impl Branch {
        fn new<const N: usize>(children: [AnyElement; N]) -> AnyElement {
            Self(children.into()).boxed()
        }
    }

    impl VisitorElement for Branch {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.0 {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "Branch"
        }
    }

    impl EventElement for Branch {}
    impl LayoutElement for Branch {}
    impl Drawable for Branch {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl Rebuildable for Branch {}

    fn child_ids(element: &dyn Element) -> Vec<crate::ElementId> {
        let mut ids = Vec::new();
        element.visit_children(&mut |child| ids.push(child.id()));
        ids
    }

    fn leaf_states(element: &dyn Element) -> Vec<u32> {
        let mut states = Vec::new();
        element.visit_children(&mut |child| {
            let leaf = child
                .option_any()
                .and_then(|value| value.downcast_ref::<StateLeaf>())
                .unwrap();
            states.push(leaf.state.get());
        });
        states
    }

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

    fn context() -> BuildContext<'static> {
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        BuildContext::new(
            canvas,
            Default::default(),
            1.0,
            Default::default(),
            Default::default(),
            WindowHandle::headless(Default::default(), 1.0),
            #[cfg(not(target_arch = "wasm32"))]
            dummy_async_handle(),
        )
    }
}
