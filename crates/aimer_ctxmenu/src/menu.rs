//! The controller: what is open, where it is, and what a press on it did.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use aimer_events::element::ElementEvent;
use aimer_modal::{OverlayLayer, OverlayLayerHandle};
use aimer_widget::PointerKey;
use aimer_widget::base::{BuildContext, WindowHandle};
use aimer_widget::EventResult;

use crate::item::ContextMenuItem;
use crate::layout::{ContextMenuAnchor, ContextMenuLayout, ContextMenuStyle};

/// Where an open menu keeps finding its anchor.
///
/// A menu opened at a right-click stays where the click was, so its anchor is a
/// [`Fixed`] point. A menu offered above something that moves — a selection
/// being adjusted by a dragged handle — has to be re-placed every frame, so its
/// anchor is [`Tracked`]: a closure the painter asks each time, which also says
/// when the thing is gone and the menu should retire.
///
/// [`Fixed`]: ContextMenuAnchorSource::Fixed
/// [`Tracked`]: ContextMenuAnchorSource::Tracked
#[derive(Clone)]
pub enum ContextMenuAnchorSource {
    /// An anchor decided once, when the menu opened.
    Fixed(ContextMenuAnchor),
    /// An anchor asked for again on every frame; `None` retires the menu.
    Tracked(Rc<dyn Fn() -> Option<ContextMenuAnchor>>),
}

impl ContextMenuAnchorSource {
    /// The anchor as it stands now.
    #[inline]
    pub fn resolve(&self) -> Option<ContextMenuAnchor> {
        match self {
            Self::Fixed(anchor) => Some(*anchor),
            Self::Tracked(source) => source(),
        }
    }
}

/// Everything needed to open a menu, in one value.
///
/// # Examples
///
/// ```
/// use aimer_attribute::Vec2d;
/// use aimer_ctxmenu::{ContextMenuAnchor, ContextMenuItem, ContextMenuRequest, ContextMenuStyle};
///
/// let request = ContextMenuRequest::new()
///     .style(ContextMenuStyle::List)
///     .at(ContextMenuAnchor::Point(Vec2d { x: 12.0, y: 30.0 }))
///     .items(vec![ContextMenuItem::new("Copy")])
///     .on_select(|index| assert_eq!(index, 0));
///
/// assert_eq!(request.style_value(), ContextMenuStyle::List);
/// ```
#[derive(Clone)]
pub struct ContextMenuRequest {
    style: ContextMenuStyle,
    anchor: ContextMenuAnchorSource,
    items: Vec<ContextMenuItem>,
    on_select: Rc<dyn Fn(usize)>,
}

impl Default for ContextMenuRequest {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl ContextMenuRequest {
    /// An empty pill anchored at the origin, doing nothing when chosen.
    #[inline]
    pub fn new() -> Self {
        Self {
            style: ContextMenuStyle::Pill,
            anchor: ContextMenuAnchorSource::Fixed(ContextMenuAnchor::Point(Default::default())),
            items: Vec::new(),
            on_select: Rc::new(|_| {}),
        }
    }

    /// Sets the shape the menu is drawn in.
    #[inline]
    pub fn style(mut self, style: ContextMenuStyle) -> Self {
        self.style = style;
        self
    }

    /// Pins the menu to an anchor decided now.
    #[inline]
    pub fn at(mut self, anchor: ContextMenuAnchor) -> Self {
        self.anchor = ContextMenuAnchorSource::Fixed(anchor);
        self
    }

    /// Places the menu against an anchor asked for on every frame.
    #[inline]
    pub fn tracking(mut self, anchor: impl Fn() -> Option<ContextMenuAnchor> + 'static) -> Self {
        self.anchor = ContextMenuAnchorSource::Tracked(Rc::new(anchor));
        self
    }

    /// Sets the rows, in the order they are drawn.
    #[inline]
    pub fn items(mut self, items: Vec<ContextMenuItem>) -> Self {
        self.items = items;
        self
    }

    /// Sets what happens when a row is chosen, by its index.
    #[inline]
    pub fn on_select(mut self, on_select: impl Fn(usize) + 'static) -> Self {
        self.on_select = Rc::new(on_select);
        self
    }

    /// The shape the menu will be drawn in.
    #[inline]
    pub const fn style_value(&self) -> ContextMenuStyle {
        self.style
    }
}

/// An openable context menu, owned by whatever offers it.
///
/// One instance is one *place* a menu can appear — a text selection, a list row
/// — reopened as often as the user asks. Opening installs an overlay painter on
/// the modal host, which is what keeps the panel clear of every clipping
/// ancestor a `Scrollable` puts in its way, and closing takes it down again.
///
/// The menu does not route events itself: whoever owns it must offer every
/// pointer event to [`ContextMenu::handle_event`] *before* handling it, because
/// the panel is drawn on top of whatever it belongs to.
///
/// # Examples
///
/// ```no_run
/// use std::rc::Rc;
///
/// use aimer_attribute::Vec2d;
/// use aimer_ctxmenu::{
///     ContextMenu, ContextMenuAnchor, ContextMenuItem, ContextMenuRequest, ContextMenuStyle,
/// };
/// use aimer_widget::base::WindowHandle;
///
/// # fn demo(window: WindowHandle) {
/// let menu = ContextMenu::new(window);
/// menu.show(
///     ContextMenuRequest::new()
///         .style(ContextMenuStyle::List)
///         .at(ContextMenuAnchor::Point(Vec2d { x: 20.0, y: 40.0 }))
///         .items(vec![ContextMenuItem::new("Copy")])
///         .on_select(|index| println!("chose {index}")),
/// );
/// assert!(menu.is_visible());
/// # }
/// ```
pub struct ContextMenu {
    pub(crate) window: WindowHandle,
    pub(crate) items: RefCell<Vec<ContextMenuItem>>,
    pub(crate) style: Cell<ContextMenuStyle>,
    pub(crate) anchor: RefCell<ContextMenuAnchorSource>,
    pub(crate) on_select: RefCell<Rc<dyn Fn(usize)>>,
    pub(crate) visible: Cell<bool>,
    pub(crate) layout: RefCell<Option<ContextMenuLayout>>,
    pub(crate) pressed: Cell<Option<usize>>,
    pub(crate) hovered: Cell<Option<usize>>,
    overlay: Cell<Option<OverlayLayerHandle>>,
}

impl Drop for ContextMenu {
    fn drop(&mut self) {
        if let Some(overlay) = self.overlay.take() {
            overlay.remove();
        }
    }
}

impl ContextMenu {
    /// Creates a closed menu painting into `window`.
    pub fn new(window: WindowHandle) -> Rc<Self> {
        Rc::new(Self {
            window,
            items: RefCell::new(Vec::new()),
            style: Cell::new(ContextMenuStyle::Pill),
            anchor: RefCell::new(ContextMenuAnchorSource::Fixed(ContextMenuAnchor::Point(
                Default::default(),
            ))),
            on_select: RefCell::new(Rc::new(|_| {})),
            visible: Cell::new(false),
            layout: RefCell::new(None),
            pressed: Cell::new(None),
            hovered: Cell::new(None),
            overlay: Cell::new(None),
        })
    }

    /// Opens the menu, replacing whatever it was showing before.
    pub fn show(self: &Rc<Self>, request: ContextMenuRequest) {
        self.style.set(request.style);
        *self.anchor.borrow_mut() = request.anchor;
        *self.items.borrow_mut() = request.items;
        *self.on_select.borrow_mut() = request.on_select;
        self.pressed.set(None);
        self.hovered.set(None);
        *self.layout.borrow_mut() = None;
        self.visible.set(true);
        self.install_overlay();
        self.window.request_redraw();
    }

    /// Closes the menu and takes its painter down.
    ///
    /// Repeated calls are harmless, which is what lets every dismissal path —
    /// a press elsewhere, a cancelled gesture, the thing the menu belonged to
    /// going away — call it without asking first.
    pub fn hide(&self) {
        let was_visible = self.visible.replace(false);
        self.pressed.set(None);
        self.hovered.set(None);
        *self.layout.borrow_mut() = None;
        if let Some(overlay) = self.overlay.take() {
            overlay.remove();
        }
        if was_visible {
            self.window.request_redraw();
        }
    }

    /// Whether the menu is open.
    #[inline]
    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    /// The shape the open menu is drawn in.
    #[inline]
    pub fn style(&self) -> ContextMenuStyle {
        self.style.get()
    }

    /// Where the panel and its rows were last placed.
    #[inline]
    pub fn layout(&self) -> Option<ContextMenuLayout> {
        self.layout.borrow().clone()
    }

    /// The window the menu paints into.
    #[inline]
    pub fn window(&self) -> &WindowHandle {
        &self.window
    }

    /// Places the menu from pre-measured label widths, reporting whether it
    /// stays open.
    ///
    /// Painting calls this once it has measured the labels. It is public
    /// because placement is the whole of a menu's behaviour that a headless
    /// test can reach: there is no canvas to measure with, but the widths a
    /// canvas would return are just numbers.
    pub fn place(&self, item_widths: &[f32], viewport_width: f32, viewport_height: f32) -> bool {
        if !self.visible.get() {
            return false;
        }
        let Some(anchor) = self.anchor.borrow().resolve() else {
            // The thing the menu hung off is gone; so is the menu.
            self.visible.set(false);
            self.pressed.set(None);
            *self.layout.borrow_mut() = None;
            return false;
        };
        *self.layout.borrow_mut() = Some(ContextMenuLayout::place(
            self.style.get(),
            anchor,
            viewport_width,
            viewport_height,
            item_widths,
        ));
        true
    }

    /// The size of the window in logical pixels, which is what the panel is
    /// kept inside of.
    pub(crate) fn viewport(&self) -> (f32, f32) {
        let physical = self.window.inner_size();
        let scale = self.window.scale_factor() as f32;
        let scale = if scale > 0.0 { scale } else { 1.0 };
        (
            physical.width as f32 / scale,
            physical.height as f32 / scale,
        )
    }

    /// Offers a pointer event to the open menu before anything underneath sees
    /// it.
    ///
    /// Returns [`Some`] when the menu took the event, in which case the caller
    /// must return that result unchanged and do nothing else. A press that
    /// missed the panel closes the menu and is handed back as [`None`], so it
    /// goes on to mean whatever it would have meant without the menu open —
    /// the behaviour every platform's menus have.
    pub fn handle_event(self: &Rc<Self>, event: &ElementEvent) -> Option<EventResult> {
        if !self.visible.get() {
            return None;
        }
        match event {
            ElementEvent::PointerDown(info) => {
                let layout = self.layout()?;
                if !layout.contains(info.pos.x, info.pos.y) {
                    self.hide();
                    return None;
                }
                let pointer = PointerKey::new(info.source, info.id);
                self.pressed.set(self.enabled_at(&layout, info.pos.x, info.pos.y));
                self.window.request_redraw();
                Some(EventResult::consumed().with_pointer_capture(pointer))
            }
            ElementEvent::PointerMove(info) => {
                let layout = self.layout()?;
                let over = self.enabled_at(&layout, info.pos.x, info.pos.y);
                if self.hovered.replace(over) != over {
                    self.window.request_redraw();
                }
                // A finger sliding off the row it pressed must un-arm it, the
                // way every button does.
                if self.pressed.get().is_some() {
                    return Some(EventResult::consumed());
                }
                layout
                    .contains(info.pos.x, info.pos.y)
                    .then(EventResult::consumed)
            }
            ElementEvent::PointerUp(info) => {
                let layout = self.layout()?;
                let pressed = self.pressed.take();
                let inside = layout.contains(info.pos.x, info.pos.y);
                if !inside && pressed.is_none() {
                    return None;
                }
                let chosen = pressed
                    .filter(|index| self.enabled_at(&layout, info.pos.x, info.pos.y) == Some(*index));
                self.window.request_redraw();
                if let Some(index) = chosen {
                    // The callback decides what becomes of the menu: a verb
                    // that finishes the job closes it, one that reshapes what
                    // the menu acts on leaves it up.
                    let on_select = Rc::clone(&self.on_select.borrow());
                    on_select(index);
                }
                Some(EventResult::consumed())
            }
            ElementEvent::Cancel => {
                self.pressed.set(None);
                self.hovered.set(None);
                None
            }
            _ => None,
        }
    }

    /// The index of the *choosable* row under a position.
    fn enabled_at(&self, layout: &ContextMenuLayout, x: f32, y: f32) -> Option<usize> {
        let index = layout.action_at(x, y)?;
        self.items
            .borrow()
            .get(index)
            .filter(|item| item.is_enabled())
            .map(|_| index)
    }

    /// Installs the painter that draws the panel above every clip, once.
    fn install_overlay(self: &Rc<Self>) {
        if self.overlay.get().is_some() {
            return;
        }
        let menu: Weak<Self> = Rc::downgrade(self);
        let handle = OverlayLayer::install(Rc::new(move |ctx: &BuildContext| {
            let Some(menu) = menu.upgrade() else {
                return false;
            };
            if !menu.is_visible() || !crate::paint::paint(&menu, ctx) {
                menu.overlay.set(None);
                menu.visible.set(false);
                return false;
            }
            true
        }));
        self.overlay.set(Some(handle));
    }
}

#[cfg(test)]
mod tests {
    use aimer_attribute::{Bounds, Vec2d};
    use aimer_events::pointer::{PointerButton, PointerInfo, PointerSource};

    use super::*;

    fn window() -> WindowHandle {
        WindowHandle::headless(winit::dpi::PhysicalSize::new(400, 800), 1.0)
    }

    fn menu(items: Vec<ContextMenuItem>, chosen: Rc<Cell<Option<usize>>>) -> Rc<ContextMenu> {
        let menu = ContextMenu::new(window());
        menu.show(
            ContextMenuRequest::new()
                .style(ContextMenuStyle::List)
                .at(ContextMenuAnchor::Point(Vec2d { x: 40.0, y: 60.0 }))
                .items(items)
                .on_select(move |index| chosen.set(Some(index))),
        );
        menu.place(&[40.0, 70.0], 400.0, 800.0);
        menu
    }

    fn two_items() -> Vec<ContextMenuItem> {
        vec![
            ContextMenuItem::new("Copy"),
            ContextMenuItem::new("Select All"),
        ]
    }

    fn press(x: f32, y: f32) -> ElementEvent {
        ElementEvent::PointerDown(PointerInfo::new(
            Vec2d { x, y },
            PointerSource::Mouse,
            0,
            PointerButton::Primary,
        ))
    }

    fn release(x: f32, y: f32) -> ElementEvent {
        ElementEvent::PointerUp(PointerInfo::new(
            Vec2d { x, y },
            PointerSource::Mouse,
            0,
            PointerButton::Primary,
        ))
    }

    fn centre(item: Bounds) -> (f32, f32) {
        (item.x + item.width * 0.5, item.y + item.height * 0.5)
    }

    #[test]
    fn choosing_a_row_reports_its_index() {
        let chosen = Rc::new(Cell::new(None));
        let menu = menu(two_items(), Rc::clone(&chosen));
        let (x, y) = centre(menu.layout().unwrap().items[1]);

        assert!(menu.handle_event(&press(x, y)).is_some());
        assert!(menu.handle_event(&release(x, y)).is_some());

        assert_eq!(chosen.get(), Some(1));
    }

    #[test]
    fn a_release_that_slid_off_the_row_chooses_nothing() {
        let chosen = Rc::new(Cell::new(None));
        let menu = menu(two_items(), Rc::clone(&chosen));
        let (x, y) = centre(menu.layout().unwrap().items[0]);

        menu.handle_event(&press(x, y));
        menu.handle_event(&release(300.0, 700.0));

        assert_eq!(chosen.get(), None);
    }

    #[test]
    fn a_disabled_row_swallows_the_press_but_runs_nothing() {
        let chosen = Rc::new(Cell::new(None));
        let menu = menu(
            vec![
                ContextMenuItem::new("Copy").enabled(false),
                ContextMenuItem::new("Select All"),
            ],
            Rc::clone(&chosen),
        );
        let (x, y) = centre(menu.layout().unwrap().items[0]);

        assert!(
            menu.handle_event(&press(x, y)).is_some(),
            "the panel still takes the press"
        );
        menu.handle_event(&release(x, y));

        assert_eq!(chosen.get(), None);
        assert!(menu.is_visible(), "and the menu stays up");
    }

    #[test]
    fn a_press_outside_the_panel_closes_it_and_falls_through() {
        let chosen = Rc::new(Cell::new(None));
        let menu = menu(two_items(), Rc::clone(&chosen));

        assert!(
            menu.handle_event(&press(300.0, 700.0)).is_none(),
            "the press still means what it meant"
        );
        assert!(!menu.is_visible());
    }

    #[test]
    fn a_closed_menu_takes_no_events_at_all() {
        let chosen = Rc::new(Cell::new(None));
        let menu = menu(two_items(), Rc::clone(&chosen));
        let (x, y) = centre(menu.layout().unwrap().items[0]);
        menu.hide();

        assert!(menu.handle_event(&press(x, y)).is_none());
    }

    #[test]
    fn choosing_leaves_the_menu_open_for_the_callback_to_decide() {
        let chosen = Rc::new(Cell::new(None));
        let menu = menu(two_items(), Rc::clone(&chosen));
        let (x, y) = centre(menu.layout().unwrap().items[0]);

        menu.handle_event(&press(x, y));
        menu.handle_event(&release(x, y));

        assert!(
            menu.is_visible(),
            "a verb that finishes the job closes the menu itself"
        );
    }

    #[test]
    fn a_menu_paints_above_every_clip_while_it_is_open() {
        let chosen = Rc::new(Cell::new(None));
        let menu = menu(two_items(), Rc::clone(&chosen));
        assert!(OverlayLayer::is_installed());

        menu.hide();

        assert!(!OverlayLayer::is_installed());
    }

    #[test]
    fn a_dropped_menu_leaves_no_painter_behind() {
        let chosen = Rc::new(Cell::new(None));
        let menu = menu(two_items(), Rc::clone(&chosen));

        drop(menu);

        assert!(!OverlayLayer::is_installed());
    }

    #[test]
    fn a_tracked_anchor_that_disappears_retires_the_menu() {
        let alive = Rc::new(Cell::new(true));
        let menu = ContextMenu::new(window());
        let watched = Rc::clone(&alive);
        menu.show(
            ContextMenuRequest::new()
                .tracking(move || {
                    watched
                        .get()
                        .then(|| ContextMenuAnchor::Rect(Bounds::new(10.0, 20.0, 30.0, 16.0)))
                })
                .items(two_items()),
        );

        assert!(menu.place(&[40.0, 70.0], 400.0, 800.0));

        alive.set(false);

        assert!(!menu.place(&[40.0, 70.0], 400.0, 800.0));
        assert!(!menu.is_visible());
    }
}
