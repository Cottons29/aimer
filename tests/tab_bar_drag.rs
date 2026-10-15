//! A tab bar's indicator follows the pointer when it is dragged, as the glass
//! lens in the iOS 26 tab bar does, and lands on the nearest tab when let go.
//!
//! These drive real mouse events through a headless window: the press, the
//! moves and the release all go through the gesture recognizer and the event
//! dispatcher, so what they check is what a user would get.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use aimer::indicator::{IndicatorController, TabBar, TabItem};
use aimer::quiver::aimer_app::HeadlessAimerApp;
use aimer::quiver::winit::dpi::{PhysicalPosition, PhysicalSize};
use aimer::quiver::winit::event::{
    DeviceId, ElementState, MouseButton, Touch, TouchPhase, WindowEvent,
};
use aimer::modal::ModalHost;
use aimer::{AimerApp, HeadlessOptions};

const ITEM_WIDTH: f64 = 96.0;
/// The bar pads its items by this much on every side.
const PADDING: f64 = 6.0;
const ROW_Y: f64 = PADDING + 28.0;

/// The horizontal centre of item `index`.
fn centre(index: f64) -> f64 {
    PADDING + ITEM_WIDTH * index + ITEM_WIDTH / 2.0
}

struct Bar {
    app: HeadlessAimerApp<ModalHost<TabBar>>,
    controller: IndicatorController,
    selections: Rc<RefCell<Vec<usize>>>,
}

fn bar(selected: usize) -> Bar {
    bar_with(Some(selected))
}

fn bar_with(selection: Option<usize>) -> Bar {
    let controller = IndicatorController::new(selection.unwrap_or(0) as f32);
    let selections = Rc::new(RefCell::new(Vec::new()));
    let recorded = selections.clone();
    let widget = TabBar::new()
        .controller(controller.clone())
        .selection(selection)
        .on_select(move |index| recorded.borrow_mut().push(index))
        .item(TabItem::new().label("Recents"))
        .item(TabItem::new().label("Shared"))
        .item(TabItem::new().label("Browse"));
    let mut app = AimerApp::start_headless_with(
        widget,
        HeadlessOptions { size: PhysicalSize::new(400, 200), scale_factor: 1.0 },
    );
    app.pump_frames(3);
    Bar { app, controller, selections }
}

impl Bar {
    fn move_to(&mut self, x: f64, y: f64) {
        self.app.send_window_event(WindowEvent::CursorMoved {
            device_id: DeviceId::dummy(),
            position: PhysicalPosition::new(x, y),
        });
    }

    fn button(&mut self, state: ElementState) {
        self.app.send_window_event(WindowEvent::MouseInput {
            device_id: DeviceId::dummy(),
            state,
            button: MouseButton::Left,
        });
    }

    /// Draws frames for `millis`, as a window would while the indicator moves.
    fn run_for(&mut self, millis: u64) {
        for _ in 0..millis / 16 {
            self.app.render_frame();
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    fn press(&mut self, x: f64) {
        // The recognizer merges two taps within 300 ms into a double tap.
        std::thread::sleep(Duration::from_millis(350));
        self.move_to(x, ROW_Y);
        self.button(ElementState::Pressed);
        self.app.render_frame();
    }

    /// Moves the pointer to `x` in small steps, as a hand does.
    fn drag_to(&mut self, from: f64, to: f64) {
        let steps = 12;
        for step in 1..=steps {
            self.move_to(from + (to - from) * f64::from(step) / f64::from(steps), ROW_Y);
            self.app.render_frame();
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    fn release(&mut self) {
        self.button(ElementState::Released);
        self.app.render_frame();
    }

    fn touch(&mut self, phase: TouchPhase, x: f64) {
        self.app.send_window_event(WindowEvent::Touch(Touch {
            device_id: DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(x, ROW_Y),
            force: None,
            id: 1,
        }));
        self.app.render_frame();
        std::thread::sleep(Duration::from_millis(16));
    }

    fn selections(&self) -> Vec<usize> {
        self.selections.borrow().clone()
    }
}

#[test]
fn tapping_an_item_proposes_it() {
    let mut bar = bar(0);

    bar.press(centre(2.0));
    bar.release();

    assert_eq!(bar.selections(), [2]);
}

#[test]
fn holding_an_item_lifts_the_indicator_until_it_is_let_go() {
    let mut bar = bar(0);

    bar.press(centre(0.0));
    bar.run_for(400);
    assert!(bar.controller.lift() > 0.9, "the lens did not lift: {}", bar.controller.lift());

    bar.release();
    bar.run_for(800);
    assert!(bar.controller.lift() < 0.05, "the lens did not settle: {}", bar.controller.lift());
}

#[test]
fn dragging_across_the_bar_carries_the_indicator_and_selects_where_it_was_let_go() {
    let mut bar = bar(0);

    bar.press(centre(0.0));
    bar.drag_to(centre(0.0), centre(2.0));

    // The indicator is under the pointer, lifted, and nothing is proposed yet.
    assert!(bar.controller.is_dragging());
    assert!((bar.controller.target() - 2.0).abs() < 0.05, "target {}", bar.controller.target());
    assert_eq!(bar.selections(), [] as [usize; 0]);
    bar.run_for(400);
    assert!(bar.controller.lift() > 0.9);
    assert!((bar.controller.position() - 2.0).abs() < 0.1, "position {}", bar.controller.position());

    bar.release();
    assert!(!bar.controller.is_dragging());
    assert_eq!(bar.selections(), [2]);
}

#[test]
fn letting_go_between_two_items_picks_the_nearer_one() {
    let mut bar = bar(0);

    bar.press(centre(0.0));
    bar.drag_to(centre(0.0), centre(1.4));
    bar.release();

    assert_eq!(bar.selections(), [1]);
}

#[test]
fn dragging_past_either_end_stays_on_the_bar() {
    let mut bar = bar(1);

    bar.press(centre(1.0));
    bar.drag_to(centre(1.0), 390.0);
    assert_eq!(bar.controller.target(), 2.0);
    bar.drag_to(390.0, 2.0);
    assert_eq!(bar.controller.target(), 0.0);
    bar.release();

    assert_eq!(bar.selections(), [0]);
}

#[test]
fn dragging_back_to_where_it_started_proposes_nothing_and_settles_home() {
    let mut bar = bar(1);

    bar.press(centre(1.0));
    bar.drag_to(centre(1.0), centre(2.0));
    bar.drag_to(centre(2.0), centre(1.0));
    bar.release();
    bar.run_for(900);

    assert_eq!(bar.selections(), [] as [usize; 0]);
    assert_eq!(bar.controller.target(), 1.0);
    assert!((bar.controller.position() - 1.0).abs() < 0.01);
}

#[test]
fn pressing_an_unselected_item_and_dragging_moves_the_indicator_from_that_item() {
    let mut bar = bar(0);

    bar.press(centre(2.0));
    bar.drag_to(centre(2.0), centre(2.0) - ITEM_WIDTH);

    // The indicator came to the pressed item and then back by one slot.
    assert!((bar.controller.target() - 1.0).abs() < 0.05, "target {}", bar.controller.target());
    bar.release();
    assert_eq!(bar.selections(), [1]);
}

#[test]
fn a_finger_drags_the_indicator_like_a_mouse() {
    let mut bar = bar(0);

    std::thread::sleep(Duration::from_millis(350));
    bar.touch(TouchPhase::Started, centre(0.0));
    for step in 1..=12 {
        bar.touch(
            TouchPhase::Moved,
            centre(0.0) + (centre(2.0) - centre(0.0)) * f64::from(step) / 12.0,
        );
    }
    assert!((bar.controller.target() - 2.0).abs() < 0.05, "target {}", bar.controller.target());
    assert_eq!(bar.selections(), [] as [usize; 0]);
    bar.touch(TouchPhase::Ended, centre(2.0));

    assert_eq!(bar.selections(), [2]);
}

#[test]
fn a_drag_the_platform_cancels_puts_the_indicator_back_and_proposes_nothing() {
    let mut bar = bar(1);

    std::thread::sleep(Duration::from_millis(350));
    bar.touch(TouchPhase::Started, centre(1.0));
    for step in 1..=8 {
        bar.touch(TouchPhase::Moved, centre(1.0) + ITEM_WIDTH * f64::from(step) / 8.0);
    }
    assert!((bar.controller.target() - 2.0).abs() < 0.05, "target {}", bar.controller.target());

    bar.touch(TouchPhase::Cancelled, centre(2.0));
    bar.run_for(900);

    assert!(!bar.controller.is_dragging());
    assert_eq!(bar.selections(), [] as [usize; 0]);
    assert_eq!(bar.controller.target(), 1.0);
    assert!(bar.controller.lift() < 0.05);
}

#[test]
fn a_bar_with_nothing_selected_hides_its_indicator() {
    let mut bar = bar_with(None);

    bar.run_for(400);

    assert!(bar.controller.is_hidden());
    assert_eq!(bar.controller.presence(), 0.0);
}

#[test]
fn tapping_the_item_the_hidden_indicator_was_parked_on_still_proposes_it() {
    // With nothing selected the indicator rests on the first slot, but that is
    // not a selection, so a tap there is a choice like any other.
    let mut bar = bar_with(None);

    bar.press(centre(0.0));
    bar.release();

    assert_eq!(bar.selections(), [0]);
}

#[test]
fn pressing_a_bar_with_nothing_selected_brings_the_indicator_under_the_pointer() {
    let mut bar = bar_with(None);

    bar.press(centre(2.0));
    bar.run_for(400);
    assert!(!bar.controller.is_hidden());
    assert!(bar.controller.presence() > 0.95, "presence {}", bar.controller.presence());
    assert!((bar.controller.position() - 2.0).abs() < 0.1, "position {}", bar.controller.position());
    bar.release();

    assert_eq!(bar.selections(), [2]);
}

#[test]
fn a_cancelled_press_on_a_bar_with_nothing_selected_hides_the_indicator_again() {
    let mut bar = bar_with(None);

    std::thread::sleep(Duration::from_millis(350));
    bar.touch(TouchPhase::Started, centre(1.0));
    bar.touch(TouchPhase::Moved, centre(1.0) + 40.0);
    bar.touch(TouchPhase::Cancelled, centre(1.0) + 40.0);
    bar.run_for(600);

    assert!(bar.controller.is_hidden());
    assert_eq!(bar.controller.presence(), 0.0);
    assert_eq!(bar.selections(), [] as [usize; 0]);
}
