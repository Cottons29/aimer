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

pub type AnyElement = Box<dyn Element>;

/// An alias of Box<dyn Widget>
pub type AnyWidget = Box<dyn Widget>;

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
