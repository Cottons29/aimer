mod async_builder;
pub mod clipboard;
pub mod components;
pub mod key;
pub mod layout_cache;
pub mod page_storage;
pub mod reconcile;
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
/// `Deref` and `AsRef` expose the stored widget as `dyn Widget`, and
/// [`aimer_rubick::Rubick::is_inline`] and [`aimer_rubick::Rubick::is_heap`]
/// report the selected mode.
///
/// Moving an inline `AnyWidget` changes the address of its concrete widget. The
/// owner does not provide implicit unsizing or a stable-address guarantee;
/// construct it through [`Widget::boxed`].
///
/// ```
/// use aimer_widget::base::BuildContext;
/// use aimer_widget::{AnyElement, Widget};
///
/// struct Badge;
///
/// impl Widget for Badge {
///     fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
///         unreachable!("this example only erases the widget")
///     }
/// }
///
/// let widget = Badge.boxed();
/// assert!(widget.is_inline());
/// ```
pub type AnyWidget = aimer_rubick::Rubick<dyn Widget, 8>;

// #[cfg(debug_assertions)]
pub mod inspector_overlay {
    use std::sync::RwLock;
    use std::sync::atomic::{AtomicBool, Ordering};
    pub static INSPECTOR_ENABLED: AtomicBool = AtomicBool::new(false);
    /// (name, start, end)
    pub static HOVERED_WIDGET: RwLock<
        Option<(&'static str, crate::base::Vec2d, crate::base::Vec2d)>,
    > = RwLock::new(None);
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
    Element, ElementId, ElementPath, EventDispatcher, element_tree_generation,
};
pub use crate::components::event_element::{
    CaptureRequest, EventElement, EventResult, FollowUp, PointerKey,
};
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
pub use aimer_macro::{main, widget};

pub use crate::async_builder::{AsyncBuilder, AsyncSnapshot};
pub use crate::components::element::{broadcast_event, dispatch_event, dispatch_focused_event};
pub use crate::key::Key;
pub use crate::layout_cache::LayoutCache;
pub use crate::widget::Widget;
pub use crate::widget::stateful::{State, StateUpdater, StatefulElement, StatefulWidget};
pub use crate::widget::stateless::{NamedWidget, StatelessElement, StatelessWidget};
pub use crate::window_metrics::{WindowMetrics, notify_window_metrics_changed};

/// Carries the live state of an old element subtree into the subtree replacing
/// it.
///
/// This is the step a rebuild performs between building the replacement and
/// installing it: every state-owning element in `new` that corresponds to one in
/// `old` adopts its runtime state, so a `set_state` above a subtree does not
/// reset the widgets inside it. Containers that materialize their children on
/// demand also exchange them here, through
/// [`Rebuildable::take_retained_children`].
///
/// Safe to call on any pair: elements that do not correspond are left alone.
#[inline]
pub fn carry_element_state(old: &dyn Element, new: &dyn Element, ctx: &base::BuildContext) {
    crate::widget::stateful::carry_child_state(old, new, ctx);
}
