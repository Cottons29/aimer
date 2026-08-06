//! Context menus, in the shape the pointer that asked for one expects.
//!
//! A context menu is the same idea on every platform — a short list of verbs
//! about the thing under the pointer — drawn two very different ways. A finger
//! gets the **pill**: a squat horizontal bar of two or three verbs, floating
//! clear of what it acts on, because a finger has no cursor to aim with and
//! covers whatever it touches. A mouse gets the **list**: a narrow column
//! opening at the click, because that is what a right-click has meant on every
//! desktop for decades. The rule is the *input device's*, not the operating
//! system's, so a phone browser and a desktop browser each get the shape their
//! user is actually holding — see [`ContextMenuStyle::for_source`].
//!
//! The menu is drawn by Aimer rather than by the platform. A native popup
//! exists only on desktop, runs its own event loop — which ends whatever
//! pointer capture the gesture that opened it relies on — and has no equivalent
//! on iOS, Android or the web. Drawing it in-tree renders identically
//! everywhere, needs no dependency, and stays testable without a window.
//!
//! # Examples
//!
//! ```no_run
//! use aimer_attribute::Vec2d;
//! use aimer_ctxmenu::{
//!     ContextMenu, ContextMenuAnchor, ContextMenuItem, ContextMenuRequest, ContextMenuStyle,
//! };
//! use aimer_events::pointer::PointerSource;
//! use aimer_widget::base::WindowHandle;
//!
//! # fn demo(window: WindowHandle, source: PointerSource, at: Vec2d) {
//! let menu = ContextMenu::new(window);
//!
//! menu.show(
//!     ContextMenuRequest::new()
//!         .style(ContextMenuStyle::for_source(source))
//!         .at(ContextMenuAnchor::Point(at))
//!         .items(vec![
//!             ContextMenuItem::new("Copy"),
//!             ContextMenuItem::new("Select All"),
//!         ])
//!         .on_select(|index| println!("chose row {index}")),
//! );
//! # }
//! ```
//!
//! # Ownership
//!
//! A [`ContextMenu`] is held by whatever offers it and lives as long as that
//! does. It routes no events by itself: its owner must give every pointer event
//! to [`ContextMenu::handle_event`] *before* acting on it, since the panel is
//! painted on top of the thing it belongs to.

mod item;
mod layout;
mod menu;
mod paint;

pub use item::ContextMenuItem;
pub use layout::{
    ContextMenuAnchor, ContextMenuLayout, ContextMenuStyle, LIST_FONT_SIZE, LIST_ITEM_PADDING,
    LIST_MIN_WIDTH, LIST_RADIUS, LIST_ROW_HEIGHT, LIST_VERTICAL_PADDING, MENU_MARGIN, PILL_FONT_SIZE,
    PILL_GAP, PILL_HEIGHT, PILL_ITEM_PADDING,
};
pub use menu::{ContextMenu, ContextMenuAnchorSource, ContextMenuRequest};
pub use paint::{
    DISABLED_LABEL_COLOR, HIGHLIGHT_COLOR, LABEL_COLOR, PANEL_COLOR, SEPARATOR_COLOR,
};
