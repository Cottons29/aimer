//! The furniture a selection grows: two draggable knobs and a callout.
//!
//! A selection that can only be made, never adjusted, and only acted on through
//! a keyboard is a desktop selection. This module adds the other half — the two
//! blue handles at its ends and the floating menu above it — as state on the
//! [`SelectionSession`] plus one painter and one event interceptor.
//!
//! **The interceptor runs before the text does.** Both live *on top of* the
//! glyphs, so a press that grabbed a knob or tapped `Copy` would otherwise be
//! taken by the paragraph underneath and start a brand-new selection. Every
//! participant, and the region itself, therefore offers each pointer event to
//! [`intercept`] first and only handles what it gives back.
//!
//! **The callout is not painted by the region.** It floats clear of the
//! selection, which regularly puts it outside the text — and every ancestor
//! that clips, a `Scrollable` above all, would cut it off there. It is
//! therefore drawn by [`aimer_ctxmenu`], which paints it through an overlay
//! layer on the modal host: above every widget and every modal, in no one's
//! clip. The knobs stay with the text, because they mark glyphs and are
//! meaningless where the glyphs are not drawn.
//!
//! **The shape of the callout is the pointer's choice, not the platform's.** A
//! finger gets the pill floating above the selection; a right-click gets the
//! desktop list opening at the click. That is [`ContextMenuStyle::for_source`]
//! deciding, so a phone browser and a desktop browser each behave the way their
//! user expects.
//!
//! Knob placement lives in [`super::handles`], which knows nothing about
//! sessions, canvases or events; the menu lives in `aimer_ctxmenu`; what is
//! left here is the state machine tying the two to a selection.

use std::cell::Cell;
use std::rc::{Rc, Weak};

use aimer_attribute::{Bounds, ResolvedSize, Vec2d};
use aimer_ctxmenu::{
    ContextMenu, ContextMenuAnchor, ContextMenuItem, ContextMenuRequest, ContextMenuStyle,
};
use aimer_events::element::ElementEvent;
use aimer_events::pointer::PointerSource;
use aimer_widget::base::{BuildContext, Color, WindowHandle};
use aimer_widget::{EventResult, PointerKey};

use crate::selection::handles::{HANDLE_RADIUS, HandleCircle, HandleSide, handle_at};
use crate::selection::selectable::{Selectable, TextGeometry};
use crate::selection::session::{SelectionSession, SelectionSlot};
use crate::selection::touch_hold::enter_hold;

/// The blue of the handles — the platform tint every system uses for them.
const HANDLE_COLOR: Color = Color::Rgba(0, 122, 255, 255);

/// One verb of the callout.
///
/// The list is deliberately short: these are the two actions that need no
/// context beyond the selection itself, which is what lets the callout be
/// answered entirely inside this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MenuAction {
    /// Puts the selected text on the clipboard.
    Copy,
    /// Extends the selection to every participant of the session.
    SelectAll,
}

impl MenuAction {
    /// The label painted for this action.
    #[inline]
    const fn label(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::SelectAll => "Select All",
        }
    }
}

/// The actions the callout offers, in the order they are drawn.
const MENU_ACTIONS: [MenuAction; 2] = [MenuAction::Copy, MenuAction::SelectAll];

/// Everything the handles and the callout need to remember between frames.
///
/// It lives on the session rather than on an element because the two ends of a
/// selection can belong to two different widgets — neither of which is entitled
/// to own the furniture of the whole selection.
pub(crate) struct SelectionUi {
    /// The callout itself, closed until a gesture earns it.
    menu: Rc<ContextMenu>,
    /// The knob a finger is dragging, and the pointer dragging it.
    dragging: Cell<Option<(HandleSide, PointerKey)>>,
    /// Whether the current selection was made by a finger, which is what
    /// decides that handles are drawn at all.
    touch: Cell<bool>,
    /// The session this furniture belongs to, held weakly: a callout must never
    /// be the reason a region stays alive.
    session: Weak<SelectionSession>,
}

impl SelectionUi {
    /// Creates the state of a selection with no furniture yet, belonging to the
    /// session being constructed.
    #[inline]
    pub fn new(window: WindowHandle, session: Weak<SelectionSession>) -> Self {
        Self {
            menu: ContextMenu::new(window),
            dragging: Cell::new(None),
            touch: Cell::new(false),
            session,
        }
    }

    /// The callout, for the elements that must offer it their events first.
    #[inline]
    pub fn menu(&self) -> &Rc<ContextMenu> {
        &self.menu
    }

    /// Offers the pill above the selection, following it as it changes.
    ///
    /// This is the touch shape, so the anchor is *tracked*: a knob dragged to a
    /// new offset moves the selection, and the pill has to follow it rather
    /// than stay where the gesture happened to end.
    pub fn show_menu(&self) {
        let anchor = self.session.clone();
        self.menu.show(
            self.request(ContextMenuStyle::Pill)
                .tracking(move || {
                    anchor
                        .upgrade()
                        .and_then(|session| session.menu_anchor())
                        .map(ContextMenuAnchor::Rect)
                }),
        );
    }

    /// Opens the desktop list with its corner at `pos`, where a right-click
    /// landed.
    ///
    /// The anchor is fixed: a desktop menu stays where it was opened even as
    /// the selection under it changes, which is what makes `Select All` from it
    /// feel like a menu rather than a jumping target.
    pub fn show_menu_at(&self, pos: Vec2d) {
        self.menu
            .show(self.request(ContextMenuStyle::List).at(ContextMenuAnchor::Point(pos)));
    }

    /// The rows and the verb each of them runs, in either shape.
    fn request(&self, style: ContextMenuStyle) -> ContextMenuRequest {
        let session = self.session.clone();
        ContextMenuRequest::new()
            .style(style)
            .items(
                MENU_ACTIONS
                    .iter()
                    .map(|action| ContextMenuItem::new(action.label()))
                    .collect(),
            )
            .on_select(move |index| {
                if let Some(session) = session.upgrade()
                    && let Some(action) = MENU_ACTIONS.get(index).copied()
                {
                    session.perform(action);
                }
            })
    }

    /// Takes the callout away, and with it the painter and the rectangle
    /// presses are tested against.
    #[inline]
    pub fn hide_menu(&self) {
        self.menu.hide();
    }

    /// Records whether the current selection came from a finger.
    #[inline]
    pub fn set_touch(&self, touch: bool) {
        self.touch.set(touch);
    }

    /// Reports whether the current selection came from a finger, which is when
    /// handles are drawn.
    #[inline]
    pub fn is_touch(&self) -> bool {
        self.touch.get()
    }

    /// Forgets a half-finished interaction, as a cancelled gesture must.
    #[inline]
    pub fn forget_gesture(&self) {
        self.dragging.set(None);
    }

    /// The knob currently being dragged, if any.
    #[inline]
    pub fn dragging(&self) -> Option<(HandleSide, PointerKey)> {
        self.dragging.get()
    }
}

impl SelectionSession {
    /// The two knobs of the current selection, in document order.
    ///
    /// `None` unless the selection was made by a finger and both of its ends
    /// belong to participants that have painted.
    pub(crate) fn handle_circles(&self) -> Option<(HandleCircle, HandleCircle)> {
        if !self.ui.is_touch() || !self.has_selection() {
            return None;
        }
        let (start, end) = self.endpoint_carets()?;
        Some((
            HandleCircle::of(start, HandleSide::Start),
            HandleCircle::of(end, HandleSide::End),
        ))
    }

    /// Runs one of the callout's actions.
    ///
    /// Copying dismisses the callout, the way accepting any menu does;
    /// selecting all keeps it, because the user is plainly still working with
    /// the selection.
    pub(crate) fn perform(self: &Rc<Self>, action: MenuAction) {
        match action {
            MenuAction::Copy => {
                let text = self.selected_text();
                if !text.is_empty() {
                    let _ = aimer_native::clipboard::set_text(&text);
                }
                self.ui.hide_menu();
            }
            MenuAction::SelectAll => {
                // `select_all` does not know a finger from a mouse, and losing
                // that would take the knobs away from a touch selection.
                let touch = self.ui.is_touch();
                self.select_all();
                self.ui.set_touch(touch);
            }
        }
        self.window().request_redraw();
    }
}

/// Offers a pointer event to the selection's furniture before the text under it
/// sees it.
///
/// Returns [`Some`] when the knobs or the callout took the event, in which case
/// the caller must return that result unchanged and do nothing else.
pub(crate) fn intercept(session: &Rc<SelectionSession>, event: &ElementEvent) -> Option<EventResult> {
    // The callout is on top of everything, including the knobs, so it is asked
    // first. A press that missed it comes back as `None`, having dismissed it.
    if let Some(result) = session.ui.menu().handle_event(event) {
        return Some(result);
    }
    match event {
        ElementEvent::PointerDown(info) => {
            let pointer = PointerKey::new(info.source, info.id);
            let (start, end) = session.handle_circles()?;
            let side = handle_at(start, end, info.pos.x, info.pos.y)?;
            session.ui.dragging.set(Some((side, pointer)));
            session.ui.hide_menu();
            Some(EventResult::consumed().with_pointer_capture(pointer))
        }
        ElementEvent::PointerMove(info) => {
            let pointer = PointerKey::new(info.source, info.id);
            let (side, owner) = session.ui.dragging()?;
            if owner != pointer {
                return None;
            }
            session.set_endpoint(side, info.pos.x, info.pos.y);
            Some(EventResult::consumed())
        }
        ElementEvent::PointerUp(info) => {
            let pointer = PointerKey::new(info.source, info.id);
            let (_, owner) = session.ui.dragging()?;
            if owner != pointer {
                return None;
            }
            session.ui.dragging.set(None);
            if session.has_selection() {
                session.ui.show_menu();
            }
            session.window().request_redraw();
            Some(EventResult::consumed())
        }
        ElementEvent::Cancel => {
            session.ui.forget_gesture();
            None
        }
        _ => None,
    }
}

/// Offers the callout after a gesture that finished with something selected.
///
/// Only a finger asks for it: a mouse has a keyboard beside it, and a menu
/// popping up after every drag would be in the way.
pub(crate) fn offer_menu_after_gesture(session: &Rc<SelectionSession>, source: PointerSource) {
    if source == PointerSource::Mouse || !session.has_selection() {
        return;
    }
    session.ui.set_touch(true);
    session.ui.show_menu();
    session.window().request_redraw();
}

/// Answers a secondary press — a right-click, or whatever the platform maps to
/// it — over a selectable text.
///
/// A right-click on an existing selection acts on it; one that lands elsewhere
/// first selects the word under the pointer, which is the convention every
/// desktop follows. Reports whether the press was taken.
pub(crate) fn open_context_menu(
    session: &Rc<SelectionSession>,
    slot: &Rc<SelectionSlot>,
    geometry: &TextGeometry,
    pos: Vec2d,
    pointer: PointerKey,
) -> bool {
    if !geometry
        .painted_bounds()
        .is_some_and(|bounds| bounds.is_inside(pos.x, pos.y))
    {
        return false;
    }
    if !session.has_selection() {
        let Some(offset) = geometry.offset_at(pos.x, pos.y) else {
            return false;
        };
        enter_hold(session, slot, offset, pointer);
        session.end(pointer);
    }
    match ContextMenuStyle::for_source(pointer.source) {
        ContextMenuStyle::List => {
            // A right-click is not a finger: it earns the desktop list at the
            // click, and no knobs.
            session.ui.set_touch(false);
            session.ui.show_menu_at(pos);
        }
        ContextMenuStyle::Pill => {
            session.ui.set_touch(true);
            session.ui.show_menu();
        }
    }
    session.window().request_redraw();
    true
}

/// Turns absolute logical coordinates into the element-local physical ones the
/// canvas draws in.
///
/// Every knob is placed in the absolute logical space participants record their
/// geometry in, so the two spaces meet here and nowhere else.
fn placer(ctx: &BuildContext, scale: f32) -> impl Fn(Bounds) -> (Vec2d, ResolvedSize) {
    let (abs_x, abs_y) = ctx.canvas.get_transform_translation();
    move |bounds: Bounds| {
        (
            Vec2d {
                x: bounds.x * scale - abs_x,
                y: bounds.y * scale - abs_y,
            },
            ResolvedSize {
                width: bounds.width * scale,
                height: bounds.height * scale,
            },
        )
    }
}

/// Paints the handles of `session` over the subtree that was just drawn.
///
/// Called from the region's `draw` after its child, so the knobs are above the
/// text they mark. The callout is *not* painted here: it floats clear of the
/// selection and would be clipped by any ancestor that clips, so `aimer_ctxmenu`
/// paints it through an overlay layer instead.
pub(crate) fn paint_handles(ctx: &BuildContext, session: &Rc<SelectionSession>) {
    let Some((start, end)) = session.handle_circles() else {
        return;
    };
    let scale = if ctx.scale > 0.0 { ctx.scale } else { 1.0 };
    let place = placer(ctx, scale);
    for knob in [start, end] {
        let (bar_pos, bar_size) = place(knob.bar_bounds());
        ctx.canvas
            .fill_color_rect(bar_pos, bar_size, HANDLE_COLOR, [0.0; 4]);
        let (circle_pos, circle_size) = place(knob.circle_bounds());
        ctx.canvas.fill_color_rect(
            circle_pos,
            circle_size,
            HANDLE_COLOR,
            [HANDLE_RADIUS * scale; 4],
        );
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use aimer_attribute::Bounds;
    use aimer_events::pointer::{PointerButton, PointerInfo, PointerSource};
    use aimer_widget::PointerKey;
    use aimer_widget::base::{Color, WindowHandle};

    use aimer_ctxmenu::ContextMenuLayout;
    use aimer_modal::OverlayLayer;

    use super::*;
    use crate::selection::selectable::{SelectionCoordinator, TextGeometry};
    use crate::selection::session::{SelectionSession, SelectionSlot};
    use crate::selection::{SelectionPoint, TextHitRegion};

    const SELECTION_COLOR: Color = Color::Rgba(51, 153, 255, 96);

    fn window() -> WindowHandle {
        WindowHandle::headless(winit::dpi::PhysicalSize::new(400, 800), 1.0)
    }

    /// A session holding one ten-character participant whose glyphs are ten
    /// logical pixels wide and sixteen tall, on one line at `y = 100`.
    fn session() -> (Rc<SelectionSession>, Rc<SelectionSlot>, Rc<TextGeometry>) {
        let window = window();
        let session = SelectionSession::new(
            window.clone(),
            Rc::new(SelectionCoordinator::default()),
            SELECTION_COLOR,
        );
        let text = "abcdefghij";
        let geometry = Rc::new(TextGeometry::new(window));
        geometry.bounds.save(1.0, 0.0, 100.0, 100.0, 16.0);
        *geometry.regions.borrow_mut() = (0..text.len())
            .map(|index| {
                TextHitRegion::new(
                    index..index + 1,
                    Bounds::new(index as f32 * 10.0, 100.0, 10.0, 16.0),
                )
            })
            .collect();
        let slot = session.register(Rc::from(text), Rc::downgrade(&geometry) as _);
        slot.stamp();
        session.begin_frame();
        (session, slot, geometry)
    }

    fn finger() -> PointerKey {
        PointerKey::new(PointerSource::Touch, 0)
    }

    fn select(session: &Rc<SelectionSession>, slot: &Rc<SelectionSlot>, range: std::ops::Range<usize>) {
        session.begin_range(
            SelectionPoint::new(Rc::clone(slot), range.start),
            SelectionPoint::new(Rc::clone(slot), range.end),
            finger(),
        );
        session.end(finger());
    }

    fn press(x: f32, y: f32, source: PointerSource) -> ElementEvent {
        ElementEvent::PointerDown(PointerInfo::new(
            Vec2d { x, y },
            source,
            0,
            PointerButton::Primary,
        ))
    }

    fn moved(x: f32, y: f32, source: PointerSource) -> ElementEvent {
        ElementEvent::PointerMove(PointerInfo::new(
            Vec2d { x, y },
            source,
            0,
            PointerButton::Primary,
        ))
    }

    fn release(x: f32, y: f32, source: PointerSource) -> ElementEvent {
        ElementEvent::PointerUp(PointerInfo::new(
            Vec2d { x, y },
            source,
            0,
            PointerButton::Primary,
        ))
    }

    /// Places the open callout the way painting would, with label widths a
    /// canvas would have measured.
    fn place(session: &Rc<SelectionSession>) -> ContextMenuLayout {
        assert!(session.ui.menu().place(&[40.0, 70.0], 400.0, 800.0));
        session.ui.menu().layout().expect("a placed callout")
    }

    #[test]
    fn a_touch_selection_grows_a_knob_at_each_end() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);

        let (start, end) = session.handle_circles().expect("a touch selection has knobs");

        assert_eq!(start.center_x, 20.0);
        assert_eq!(end.center_x, 50.0);
        assert!(start.center_y < 100.0, "the start knob is above the line");
        assert!(end.center_y > 116.0, "the end knob is below the line");
    }

    #[test]
    fn the_callout_is_anchored_to_the_start_of_the_selection() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);

        let anchor = session.menu_anchor().expect("a selection has an anchor");

        assert_eq!(anchor.x, 20.0, "the start caret, not the whole selection");
        assert_eq!(anchor.y, 100.0);
        assert!(
            anchor.width < 10.0,
            "a caret is thin, so the pill sits over where the selection began"
        );
    }

    #[test]
    fn the_callout_falls_back_to_the_selection_when_the_first_end_has_no_glyphs() {
        let (session, slot, geometry) = session();
        select(&session, &slot, 2..5);
        geometry.regions.borrow_mut().clear();

        assert_eq!(session.menu_anchor(), session.selection_bounds());
    }

    #[test]
    fn a_mouse_selection_grows_no_knobs() {
        let (session, slot, _geometry) = session();
        session.begin(SelectionPoint::new(Rc::clone(&slot), 2), PointerKey::new(PointerSource::Mouse, 0));
        session.extend_to_position(55.0, 108.0, PointerKey::new(PointerSource::Mouse, 0));

        assert!(session.handle_circles().is_none());
    }

    #[test]
    fn a_collapsed_selection_grows_no_knobs() {
        let (session, slot, _geometry) = session();
        session.begin(SelectionPoint::new(Rc::clone(&slot), 2), finger());

        assert!(!session.has_selection());
        assert!(session.handle_circles().is_none());
    }

    #[test]
    fn dragging_the_end_knob_moves_only_that_end() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);

        let (_, end) = session.handle_circles().expect("knobs");
        assert!(intercept(&session, &press(end.center_x, end.center_y, PointerSource::Touch)).is_some());
        assert!(intercept(&session, &moved(85.0, 108.0, PointerSource::Touch)).is_some());

        assert_eq!(slot.selected_range(), Some(2..9));
    }

    #[test]
    fn dragging_the_start_knob_moves_only_that_end() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);

        let (start, _) = session.handle_circles().expect("knobs");
        assert!(intercept(&session, &press(start.center_x, start.center_y, PointerSource::Touch)).is_some());
        assert!(intercept(&session, &moved(2.0, 108.0, PointerSource::Touch)).is_some());

        assert_eq!(slot.selected_range(), Some(0..5));
    }

    #[test]
    fn a_press_that_misses_the_knobs_is_left_to_the_text() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);

        assert!(intercept(&session, &press(95.0, 300.0, PointerSource::Touch)).is_none());
    }

    #[test]
    fn only_the_finger_that_grabbed_a_knob_drags_it() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        let (_, end) = session.handle_circles().expect("knobs");
        intercept(&session, &press(end.center_x, end.center_y, PointerSource::Touch));

        let other = ElementEvent::PointerMove(PointerInfo::new(
            Vec2d { x: 85.0, y: 108.0 },
            PointerSource::Touch,
            1,
            PointerButton::Primary,
        ));

        assert!(intercept(&session, &other).is_none());
        assert_eq!(slot.selected_range(), Some(2..5));
    }

    #[test]
    fn letting_a_knob_go_offers_the_callout() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        let (_, end) = session.handle_circles().expect("knobs");
        intercept(&session, &press(end.center_x, end.center_y, PointerSource::Touch));
        assert!(!session.ui.menu().is_visible(), "grabbing hides it");

        intercept(&session, &moved(85.0, 108.0, PointerSource::Touch));
        intercept(&session, &release(85.0, 108.0, PointerSource::Touch));

        assert!(session.ui.menu().is_visible());
    }

    #[test]
    fn tapping_copy_puts_the_selection_on_the_clipboard() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        session.ui.show_menu();
        let item = place(&session).items[0];

        let (x, y) = (item.x + 4.0, item.y + 4.0);
        assert!(intercept(&session, &press(x, y, PointerSource::Touch)).is_some());
        assert!(intercept(&session, &release(x, y, PointerSource::Touch)).is_some());

        assert!(!session.ui.menu().is_visible(), "copying dismisses the callout");
    }

    #[test]
    fn tapping_select_all_extends_the_selection_and_keeps_the_callout() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        session.ui.show_menu();
        let item = place(&session).items[1];

        let (x, y) = (item.x + 4.0, item.y + 4.0);
        intercept(&session, &press(x, y, PointerSource::Touch));
        intercept(&session, &release(x, y, PointerSource::Touch));

        assert_eq!(slot.selected_range(), Some(0..10));
        assert!(session.ui.menu().is_visible());
        assert!(
            session.handle_circles().is_some(),
            "select-all from the callout keeps the knobs a finger earned"
        );
    }

    #[test]
    fn a_release_that_slid_off_the_item_runs_nothing() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        session.ui.show_menu();
        let item = place(&session).items[1];

        intercept(&session, &press(item.x + 4.0, item.y + 4.0, PointerSource::Touch));
        intercept(&session, &release(5.0, 500.0, PointerSource::Touch));

        assert_eq!(slot.selected_range(), Some(2..5), "select-all did not run");
    }

    #[test]
    fn a_press_beside_the_callout_dismisses_it_and_falls_through() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        session.ui.show_menu();
        place(&session);

        assert!(
            intercept(&session, &press(300.0, 700.0, PointerSource::Touch)).is_none(),
            "the press still means what it meant"
        );
        assert!(!session.ui.menu().is_visible());
    }

    #[test]
    fn a_right_click_opens_the_desktop_list_at_the_click() {
        let (session, slot, geometry) = session();
        let pointer = PointerKey::new(PointerSource::Mouse, 0);

        assert!(open_context_menu(
            &session,
            &slot,
            &geometry,
            Vec2d { x: 35.0, y: 108.0 },
            pointer
        ));

        assert_eq!(session.ui.menu().style(), ContextMenuStyle::List);
        let layout = place(&session);
        assert_eq!(
            (layout.bounds.x, layout.bounds.y),
            (35.0, 108.0),
            "its corner sits where the click landed"
        );
        assert!(
            session.handle_circles().is_none(),
            "a right-click earns the menu but no knobs"
        );
    }

    #[test]
    fn a_finger_gesture_opens_the_pill_above_the_selection() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);

        offer_menu_after_gesture(&session, PointerSource::Touch);

        assert_eq!(session.ui.menu().style(), ContextMenuStyle::Pill);
        let layout = place(&session);
        assert!(
            layout.bounds.y + layout.bounds.height < 100.0,
            "floating clear of the line it acts on"
        );
    }

    #[test]
    fn a_mouse_gesture_is_never_offered_the_callout() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        session.ui.hide_menu();

        offer_menu_after_gesture(&session, PointerSource::Mouse);

        assert!(!session.ui.menu().is_visible());
    }

    #[test]
    fn a_finger_gesture_that_selected_nothing_is_not_offered_the_callout() {
        let (session, slot, _geometry) = session();
        session.begin(SelectionPoint::new(Rc::clone(&slot), 2), finger());

        offer_menu_after_gesture(&session, PointerSource::Touch);

        assert!(!session.ui.menu().is_visible());
    }

    #[test]
    fn clearing_the_selection_takes_the_callout_with_it() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        session.ui.show_menu();

        session.clear();

        assert!(!session.ui.menu().is_visible());
        assert!(session.handle_circles().is_none());
    }

    #[test]
    fn the_callout_paints_through_an_overlay_layer_rather_than_the_text_it_belongs_to() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        assert!(
            !OverlayLayer::is_installed(),
            "a selection alone installs nothing"
        );

        session.ui.show_menu();
        assert!(
            OverlayLayer::is_installed(),
            "the callout paints above every clip"
        );

        session.ui.hide_menu();
        assert!(!OverlayLayer::is_installed(), "and takes itself down again");
    }

    #[test]
    fn clearing_the_selection_takes_the_overlay_down_with_the_callout() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        session.ui.show_menu();

        session.clear();

        assert!(!OverlayLayer::is_installed());
    }

    #[test]
    fn a_dropped_session_leaves_no_painter_behind() {
        let (session, slot, _geometry) = session();
        select(&session, &slot, 2..5);
        session.ui.show_menu();

        drop(slot);
        drop(session);

        assert!(
            !OverlayLayer::is_installed(),
            "a region that goes away takes its callout with it"
        );
    }
}
