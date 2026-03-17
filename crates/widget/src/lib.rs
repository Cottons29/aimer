mod attribute;
pub mod components;
mod widget;
pub mod text;
pub mod style;
pub mod layout_cache;

// #[cfg(debug_assertions)]
pub mod inspector_overlay {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::RwLock;
    pub static INSPECTOR_ENABLED: AtomicBool = AtomicBool::new(false);
    pub static HOVERED_WIDGET: RwLock<Option<(&'static str, crate::base::Vec2d, crate::base::Vec2d)>> =
        RwLock::new(None);
    pub fn is_enabled() -> bool {
        INSPECTOR_ENABLED.load(Ordering::Relaxed)
    }
    pub fn set_enabled(v: bool) {
        INSPECTOR_ENABLED.store(v, Ordering::Relaxed);
    }
}


pub mod base {
    pub use attribute::position::Vec2d;
    pub use attribute::size::{ResolvedSize, Size};
    pub use attribute::dimension::Dimension;
    pub use crate::components::context::BuildContext;
    pub use color::prelude::*;
}
pub use crate::widget::Widget;
pub use crate::components::element::Element;
pub use crate::components::element::{ dispatch_event};
pub use crate::widget::stateful::{StatefulElement, StatefulWidget, State, StateUpdater};
pub use crate::widget::stateless::{ StatelessElement, StatelessWidget, NamedWidget};
pub use crate::text::Text;
pub use crate::style::text_style::{TextStyle, TextOverflow};
pub use crate::style::layout_spacing::{LayoutSpacing, Spacing};
pub use widget_attr;
pub use constructor::*;
pub use crate::layout_cache::LayoutCache;
pub use components::drawable::Drawable;
