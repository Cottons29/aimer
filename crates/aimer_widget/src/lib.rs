mod async_builder;
mod attribute;
pub mod clipboard;
pub mod components;
pub mod key;
pub mod layout_cache;
pub mod page_storage;
pub mod reconcile;
mod widget;

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

/// An owned, type-erased [`Element`] with inline storage and heap fallback.
///
/// `AnyElement` embeds a concrete element directly when its size and alignment
/// fit [`aimer_rubick::Rubick`]'s inline capacity. Larger or over-aligned
/// elements use one heap allocation. Borrowing through `Deref` or `AsRef`
/// provides a `dyn Element` view with normal dynamic dispatch.
///
/// Moving an inline owner also moves its concrete element, so the element's
/// address is not stable. Use Rust pinning when an element requires a stable
/// address. The name of [`Element::boxed`] is retained for source familiarity;
/// that method returns this owner and does not necessarily allocate.
pub type AnyElement = aimer_rubick::Rubick<dyn Element>;

/// An owned, type-erased [`Widget`] with inline storage and heap fallback.
///
/// Small, sufficiently aligned widgets are embedded in the owner without an
/// additional allocation. Larger or over-aligned widgets transparently use one
/// heap allocation. `Deref` and `AsRef` expose the stored widget as
/// `dyn Widget`, and [`aimer_rubick::Rubick::is_inline`] and
/// [`aimer_rubick::Rubick::is_heap`] report the selected mode.
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
pub type AnyWidget = aimer_rubick::Rubick<dyn Widget>;

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
pub use crate::components::element::Element;
pub use crate::components::event_element::EventElement;
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
pub use aimer_canvas::TextOverflowMode;
pub use aimer_macro::{main, widget};

pub use crate::async_builder::{AsyncBuilder, AsyncSnapshot};
pub use crate::components::element::{broadcast_event, cancel_pointer, dispatch_event};
pub use crate::key::Key;
pub use crate::layout_cache::LayoutCache;
pub use crate::widget::Widget;
pub use crate::widget::stateful::{State, StateUpdater, StatefulElement, StatefulWidget};
pub use crate::widget::stateless::{NamedWidget, StatelessElement, StatelessWidget};
