mod attribute;
pub mod components;
pub mod key;
pub mod page_storage;
pub mod reconcile;
mod widget;
pub mod layout_cache;

// #[cfg(debug_assertions)]
pub mod inspector_overlay {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::RwLock;
    pub static INSPECTOR_ENABLED: AtomicBool = AtomicBool::new(false);
    /// (name, start, end)
    pub static HOVERED_WIDGET: RwLock<Option<(&'static str, crate::base::Vec2d, crate::base::Vec2d)>> =
        RwLock::new(None);
    pub fn is_enabled() -> bool {
        INSPECTOR_ENABLED.load(Ordering::Relaxed)
    }
    pub fn set_enabled(v: bool) {
        INSPECTOR_ENABLED.store(v, Ordering::Relaxed);
    }
}

pub use crate::components::element::Element;
pub use crate::components::visitor_element::VisitorElement;
pub use crate::components::event_element::EventElement;
pub use crate::components::layout_element::LayoutElement;
pub use crate::components::drawable::Drawable;
pub use crate::components::rebuildable::Rebuildable;
pub use crate::components::reconcilable::Reconcilable;


pub mod base {
    pub use aimer_attribute::position::Vec2d;
    pub use aimer_attribute::size::{ResolvedSize, Size};
    pub use aimer_attribute::dimension::Dimension;
    pub use crate::components::context::BuildContext;
    pub use aimer_color::prelude::*;
}
pub use crate::widget::{Widget};
pub use crate::key::Key;

pub use crate::components::element::{dispatch_event, broadcast_event};
pub use crate::reconcile::try_update_element;
pub use crate::widget::stateful::{StatefulElement, StatefulWidget, State, StateUpdater};
pub use crate::widget::stateless::{ StatelessElement, StatelessWidget, NamedWidget};
pub use aimer_macro::{widget, Constructor, main, WidgetConstructor};
pub use aimer_canvas::TextOverflowMode;
pub use crate::layout_cache::LayoutCache;


