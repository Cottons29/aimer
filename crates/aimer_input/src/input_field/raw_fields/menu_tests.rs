//! The field's clipboard menu, driven through the element itself.
//!
//! What a menu offers depends on a selection, and a selection depends on a
//! canvas the field only has while painting, so these tests do what `draw`
//! does: fill the cached bounds, drive the gesture, then raise the pending
//! menu. Nothing here touches the clipboard except through
//! [`super::RawTextField::menu_state`], and every assertion is about verbs the
//! clipboard cannot take away.

use std::time::Duration;

use aimer_animation::AnimInstant;
use aimer_attribute::position::Vec2d;
use aimer_ctxmenu::ContextMenuStyle;
use aimer_events::element::ElementEvent;
use aimer_events::pointer::{PointerButton, PointerInfo, PointerSource};
use aimer_widget::EventElement;

use super::test_support::focused_field;
use super::{FieldAction, MenuOrigin, RawTextField};
use crate::gesture::LONG_PRESS_DURATION;
use crate::input_field::controller::TextFieldController;

/// A field holding `text`, painted once at a known place.
fn field(text: &str) -> RawTextField {
    let controller = TextFieldController::new();
    controller.set_text(text.to_owned());
    let field = focused_field(controller);
    field.cached_bounds.save(1.0, 0.0, 0.0, 200.0, 32.0);
    field.test_clock.set(Some(AnimInstant::now()));
    field
}

fn advance(field: &RawTextField, by: Duration) {
    let now = field.test_clock.get().expect("a fixed clock");
    field.test_clock.set(Some(now + by));
}

fn press(source: PointerSource, button: PointerButton, x: f32, y: f32) -> ElementEvent {
    ElementEvent::PointerDown(PointerInfo::new(Vec2d { x, y }, source, 0, button))
}

fn release(source: PointerSource, x: f32, y: f32) -> ElementEvent {
    ElementEvent::PointerUp(PointerInfo::new(
        Vec2d { x, y },
        source,
        0,
        PointerButton::Primary,
    ))
}

fn verbs(field: &RawTextField) -> Vec<FieldAction> {
    field.menu_actions.borrow().clone()
}

#[test]
fn a_finger_that_rests_raises_the_pill() {
    let field = field("hello world");
    let _ = field.on_event(&press(PointerSource::Touch, PointerButton::Primary, 20.0, 16.0));
    advance(&field, LONG_PRESS_DURATION);

    assert!(
        field
            .on_event(&release(PointerSource::Touch, 20.0, 16.0))
            .is_consumed()
    );
    field.raise_pending_menu();

    assert!(field.menu.is_visible());
    assert_eq!(field.menu.style(), ContextMenuStyle::Pill);
}

#[test]
fn a_finger_that_taps_raises_nothing() {
    let field = field("hello world");
    let _ = field.on_event(&press(PointerSource::Touch, PointerButton::Primary, 20.0, 16.0));
    advance(&field, Duration::from_millis(60));

    let _ = field.on_event(&release(PointerSource::Touch, 20.0, 16.0));
    field.raise_pending_menu();

    assert!(!field.menu.is_visible());
}

#[test]
fn a_finger_that_drags_raises_nothing() {
    let field = field("hello world");
    let _ = field.on_event(&press(PointerSource::Touch, PointerButton::Primary, 20.0, 16.0));
    let _ = field.on_event(&ElementEvent::PointerMove(PointerInfo::new(
        Vec2d { x: 180.0, y: 16.0 },
        PointerSource::Touch,
        0,
        PointerButton::Primary,
    )));
    advance(&field, LONG_PRESS_DURATION);

    let _ = field.on_event(&release(PointerSource::Touch, 180.0, 16.0));
    field.raise_pending_menu();

    assert!(!field.menu.is_visible(), "a drag is a selection, not a menu");
}

#[test]
fn a_right_click_opens_the_desktop_list_where_it_landed() {
    let field = field("hello world");

    assert!(
        field
            .on_event(&press(
                PointerSource::Mouse,
                PointerButton::Secondary,
                20.0,
                16.0
            ))
            .is_consumed()
    );
    field.raise_pending_menu();

    assert!(field.menu.is_visible());
    assert_eq!(field.menu.style(), ContextMenuStyle::List);
    assert!(
        field.menu.place(&[40.0, 40.0], 400.0, 800.0),
        "the list places itself"
    );
    let layout = field.menu.layout().expect("a placed list");
    assert_eq!(layout.bounds.x, 20.0, "opened at the click");
}

#[test]
fn a_right_click_does_not_start_a_drag_selection() {
    let field = field("hello world");
    let _ = field.on_event(&press(
        PointerSource::Mouse,
        PointerButton::Secondary,
        20.0,
        16.0,
    ));

    assert!(field.mouse_held.get().is_none());
}

#[test]
fn a_selection_puts_cut_and_copy_in_the_menu() {
    let field = field("hello world");
    field.cursor.set_selection_anchor(Some(0));
    field.cursor.set_offset(5);

    field.request_menu(MenuOrigin::Hold);
    field.raise_pending_menu();

    let verbs = verbs(&field);
    assert_eq!(verbs.first(), Some(&FieldAction::Cut));
    assert_eq!(verbs.get(1), Some(&FieldAction::Copy));
}

#[test]
fn select_all_from_the_menu_offers_it_again_with_copy_in_it() {
    let field = field("hello world");
    field.request_menu(MenuOrigin::Hold);
    field.raise_pending_menu();
    assert!(!verbs(&field).contains(&FieldAction::Copy));

    field.run_menu_action(FieldAction::SelectAll);
    field.raise_pending_menu();

    assert_eq!(
        field.cursor.selection_range(),
        Some((0, field.controller.grapheme_count()))
    );
    assert!(field.menu.is_visible(), "the menu stays, reshaped");
    assert!(verbs(&field).contains(&FieldAction::Copy));
}

#[test]
fn an_empty_read_only_field_raises_no_menu_at_all() {
    let controller = TextFieldController::new();
    let mut field = focused_field(controller);
    field.read_only = true;
    field.cached_bounds.save(1.0, 0.0, 0.0, 200.0, 32.0);

    field.request_menu(MenuOrigin::Hold);
    field.raise_pending_menu();

    assert!(!field.menu.is_visible());
}

#[test]
fn typing_dismisses_the_menu() {
    let field = field("hello world");
    field.request_menu(MenuOrigin::Hold);
    field.raise_pending_menu();
    assert!(field.menu.is_visible());

    let _ = field.on_event(&super::test_support::commit("x"));

    assert!(!field.menu.is_visible());
}

#[test]
fn a_press_outside_the_field_dismisses_the_menu() {
    let field = field("hello world");
    field.request_menu(MenuOrigin::Hold);
    field.raise_pending_menu();

    let _ = field.on_event(&press(
        PointerSource::Mouse,
        PointerButton::Primary,
        400.0,
        400.0,
    ));

    assert!(!field.menu.is_visible());
}

#[test]
fn a_cancelled_gesture_dismisses_the_menu_and_forgets_the_press() {
    let field = field("hello world");
    let _ = field.on_event(&press(PointerSource::Touch, PointerButton::Primary, 20.0, 16.0));
    field.request_menu(MenuOrigin::Hold);
    field.raise_pending_menu();

    let _ = field.on_event(&ElementEvent::Cancel);
    advance(&field, LONG_PRESS_DURATION);
    let _ = field.on_event(&release(PointerSource::Touch, 20.0, 16.0));
    field.raise_pending_menu();

    assert!(!field.menu.is_visible());
}

#[test]
fn the_placeholder_hides_while_an_input_method_composes() {
    let field = field("");

    assert!(field.placeholder_visible(), "an idle empty field shows it");

    let _ = field.on_event(&ElementEvent::ImePreedit {
        text: "ni".to_owned(),
        cursor: Some((2, 2)),
    });

    assert!(
        !field.placeholder_visible(),
        "the composition is the field's content now"
    );

    let _ = field.on_event(&ElementEvent::ImePreedit {
        text: String::new(),
        cursor: None,
    });

    assert!(field.placeholder_visible(), "and comes back when it ends");
}
