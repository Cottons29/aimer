//! A finger that scrolls a list of buttons does not press them, and a phone's
//! lack of a cursor does not leave them hovered.
//!
//! The rows are built the way `Button` builds itself, a hover region around a
//! gesture detector, so the tests can count what the button would have shown:
//! the press highlight follows `TapDown` until `TapUp` or `TapCancel`, and the
//! hover highlight follows the region's enter and exit.

use super::*;

use aimer_input::gesture::GestureEvent;
use aimer_input::gesture::gesture_detector::{GestureDetector, GestureDetectorBehavior};
use aimer_input::mouse_region::MouseRegion;
use winit::event::Touch;

const ROWS: usize = 20;
const ROW_HEIGHT: f32 = 60.0;

#[derive(Default)]
struct Shown {
    /// Rows whose press highlight is on right now.
    pressed: Cell<i32>,
    /// Rows whose hover highlight is on right now.
    hovered: Cell<i32>,
    taps: Cell<usize>,
}

struct Rows {
    shown: Rc<Shown>,
}

impl Widget for Rows {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let rows = (0..ROWS)
            .map(|_| {
                let (enter, exit, gesture, tap) =
                    (self.shown.clone(), self.shown.clone(), self.shown.clone(), self.shown.clone());
                MouseRegion::new()
                    .on_hover_enter(move || enter.hovered.set(enter.hovered.get() + 1))
                    .on_hover_exit(move || exit.hovered.set(exit.hovered.get() - 1))
                    .child(
                        GestureDetector::new()
                            .behavior(GestureDetectorBehavior::BlockChild)
                            .on_tap(move || tap.taps.set(tap.taps.get() + 1))
                            .on_gesture(move |event: GestureEvent| match event {
                                GestureEvent::TapDown { .. } => gesture.pressed.set(gesture.pressed.get() + 1),
                                GestureEvent::TapUp { .. } | GestureEvent::TapCancel => {
                                    gesture.pressed.set(gesture.pressed.get() - 1)
                                }
                                _ => {}
                            })
                            .child(SizedBox::new().width(Dimension::Px(200.0)).height(Dimension::Px(ROW_HEIGHT))),
                    )
                    .boxed()
            })
            .collect::<Vec<_>>();
        SizedBox::new()
            .width(Dimension::Px(200.0))
            .height(Dimension::Px(400.0))
            .child(
                Scrollable::new()
                    .vertical_scroll_bar(None)
                    .horizontal_scroll_bar(None)
                    .child(Column::new().children(rows)),
            )
            .to_element(ctx)
    }
}

impl aimer_widget::PortableWidget for Rows {}

fn start() -> (HeadlessAimerApp<impl Widget + 'static>, Rc<Shown>) {
    let shown = Rc::new(Shown::default());
    let mut app = AimerApp::start_headless_with(Rows { shown: shown.clone() }, HeadlessOptions {
        size: PhysicalSize::new(200, 400), scale_factor: 1.0,
    });
    app.pump_frames(3);
    (app, shown)
}

fn touch<W: Widget + 'static>(app: &mut HeadlessAimerApp<W>, phase: TouchPhase, x: f64, y: f64) {
    app.send_window_event(WindowEvent::Touch(Touch {
        device_id: DeviceId::dummy(),
        phase,
        location: PhysicalPosition::new(x, y),
        force: None,
        id: 1,
    }));
    app.pump_frames(1);
}

/// A finger goes down on a row, drags `distance` pixels up and lifts.
fn swipe<W: Widget + 'static>(app: &mut HeadlessAimerApp<W>, from_y: f64, distance: f64) {
    touch(app, TouchPhase::Started, 100.0, from_y);
    for step in 1..=8 {
        touch(app, TouchPhase::Moved, 100.0, from_y - distance * f64::from(step) / 8.0);
    }
    touch(app, TouchPhase::Ended, 100.0, from_y - distance);
}

#[test]
fn a_swipe_over_a_row_does_not_leave_it_pressed() {
    let _serial = VIRTUALIZED_RENDER_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (mut app, shown) = start();

    swipe(&mut app, 130.0, 120.0);

    assert_eq!(shown.pressed.get(), 0, "a row is still pressed after the finger lifted");
    assert_eq!(shown.taps.get(), 0, "a scroll must not tap the row it started on");
}

#[test]
fn several_swipes_leave_no_row_pressed() {
    let _serial = VIRTUALIZED_RENDER_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (mut app, shown) = start();

    for from in [330.0, 250.0, 170.0, 330.0, 90.0] {
        swipe(&mut app, from, 70.0);
        app.pump_frames(3);
    }

    assert_eq!(shown.pressed.get(), 0, "rows were left pressed");
}

#[test]
fn touching_and_scrolling_a_list_never_hovers_its_rows() {
    let _serial = VIRTUALIZED_RENDER_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (mut app, shown) = start();

    swipe(&mut app, 200.0, 90.0);
    app.pump_frames(10);

    assert_eq!(shown.hovered.get(), 0, "a touch left a row hovered, but there is no cursor");
}

#[test]
fn a_tap_still_presses_and_activates_a_row() {
    let _serial = VIRTUALIZED_RENDER_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (mut app, shown) = start();

    touch(&mut app, TouchPhase::Started, 100.0, 100.0);
    assert_eq!(shown.pressed.get(), 1, "a touch must press the row it lands on");
    touch(&mut app, TouchPhase::Ended, 100.0, 100.0);

    assert_eq!(shown.pressed.get(), 0);
    assert_eq!(shown.taps.get(), 1, "a tap activates the row");
    assert_eq!(shown.hovered.get(), 0);
}

#[test]
fn a_mouse_still_hovers_a_row_after_a_touch() {
    let _serial = VIRTUALIZED_RENDER_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (mut app, shown) = start();
    swipe(&mut app, 200.0, 40.0);
    assert_eq!(shown.hovered.get(), 0);

    app.send_window_event(WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(100.0, 100.0),
    });
    app.pump_frames(3);

    assert_eq!(shown.hovered.get(), 1, "a real cursor must hover the row under it");
}
