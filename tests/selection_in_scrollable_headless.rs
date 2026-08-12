//! Selecting text that lives inside a scroll view, driven through a headless
//! application.
//!
//! A press inside a `Scrollable` is ambiguous — it could be a tap, or the start
//! of a scroll — so the view arms a pending drag and takes the gesture from
//! whoever was under the pointer as soon as it travels past the drag threshold,
//! cancelling that element's capture. For a button that is exactly right. For a
//! `SelectionArea`, whose selection *is* a drag, it made text inside a scroll
//! view impossible to select: the highlight died on the first few pixels and the
//! page scrolled instead.
//!
//! A text that has begun selecting therefore claims the pointer, and the scroll
//! view leaves a claimed pointer alone. These tests drive the real pipeline —
//! window events into the app, through the element tree — and watch the scroll
//! offset the `ScrollController` reports.

use std::thread::sleep;
use std::time::Duration;

use aimer::events::pointer::PointerSource;
use aimer::quiver::winit::dpi::PhysicalPosition;
use aimer::quiver::winit::event::{
    DeviceId, ElementState, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent,
};
use aimer::{
    AnyWidget, BoxAlignment, Column, Container, PointerKey, ScrollAxis, ScrollController,
    Scrollable, SelectionArea, SizedBox, Text, Widget, is_pointer_claimed, release_all_pointers,
};
use aimer_quiver::AimerApp;

/// Enough lines to make the content far taller than the 800px headless
/// viewport, so there is somewhere to scroll to.
const LINE_COUNT: usize = 80;

/// Where the gesture starts: comfortably inside the content, far enough down
/// that dragging upwards has room to scroll.
const PRESS_Y: f64 = 400.0;

/// Where it ends: upwards by much more than the drag threshold, which is the
/// direction that scrolls a vertical view away from its top.
const RELEASE_Y: f64 = 120.0;

/// How often a touch screen reports the finger, roughly: one sample per frame.
const SCROLL_STEP_INTERVAL: Duration = Duration::from_millis(12);

/// Enough samples that the gesture as a whole outlasts the hold a touch
/// selection waits for, each one as short as a real finger's.
const SCROLL_STEP_COUNT: usize = 60;

type HeadlessApp<W> = aimer::quiver::aimer_app::HeadlessAimerApp<W>;

/// A tall column of selectable lines inside a vertical scroll view.
fn selectable_page(controller: &ScrollController) -> impl Widget + 'static {
    let lines = (0..LINE_COUNT)
        .map(|index| Text::new(format!("Line {index} of selectable prose")).boxed())
        .collect::<Vec<AnyWidget>>();

    Container::new().box_child(
        Scrollable::new()
            .controller(controller.clone())
            .axis(ScrollAxis::Vertical)
            .child(
                SelectionArea::new().child(
                    Column::new()
                        .horizontal_alignment(BoxAlignment::Start)
                        .children(lines),
                ),
            ),
    )
}

/// The same page with nothing selectable in it: the control for every
/// assertion below.
fn plain_page(controller: &ScrollController) -> impl Widget + 'static {
    Container::new().box_child(
        Scrollable::new()
            .controller(controller.clone())
            .axis(ScrollAxis::Vertical)
            .child(
                Container::new()
                    .child(SizedBox::new().height(LINE_COUNT as u32 * 40).width(400)),
            ),
    )
}

fn move_to<W: Widget + 'static>(app: &mut HeadlessApp<W>, x: f64, y: f64) {
    app.send_window_event(WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(x, y),
    });
    app.render_frame();
}

fn press<W: Widget + 'static>(app: &mut HeadlessApp<W>, x: f64, y: f64) {
    move_to(app, x, y);
    app.send_window_event(WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state: ElementState::Pressed,
        button: MouseButton::Left,
    });
    app.render_frame();
}

fn release<W: Widget + 'static>(app: &mut HeadlessApp<W>) {
    app.send_window_event(WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state: ElementState::Released,
        button: MouseButton::Left,
    });
    app.render_frame();
}

/// Drags from `(x, from)` to `(x, to)` in steps large enough to pass the drag
/// threshold, the way a real pointer arrives.
fn drag_up<W: Widget + 'static>(app: &mut HeadlessApp<W>, x: f64, from: f64, to: f64) {
    press(app, x, from);
    let steps = 8;
    let step = (to - from) / steps as f64;
    for index in 1..=steps {
        move_to(app, x, from + step * index as f64);
    }
    release(app);
}

/// Drags a finger from `(x, from)` to `(x, to)`, in the phases a touch screen
/// reports.
fn touch_drag_up<W: Widget + 'static>(app: &mut HeadlessApp<W>, x: f64, from: f64, to: f64) {
    let contact = |phase, y| {
        WindowEvent::Touch(Touch {
            device_id: DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(x, y),
            force: None,
            id: 0,
        })
    };
    app.send_window_event(contact(TouchPhase::Started, from));
    app.render_frame();
    let steps = 8;
    let step = (to - from) / steps as f64;
    for index in 1..=steps {
        app.send_window_event(contact(TouchPhase::Moved, from + step * index as f64));
        app.render_frame();
    }
    app.send_window_event(contact(TouchPhase::Ended, to));
    app.render_frame();
}

/// Drags a finger the way a thumb actually moves a page: far enough to hand the
/// gesture to the scroll view, and for longer than the hold a touch selection
/// waits for, so a press the view forgot to revoke has time to ripen.
fn touch_scroll_slowly<W: Widget + 'static>(app: &mut HeadlessApp<W>, x: f64, from: f64, to: f64) {
    let contact = |phase, y| {
        WindowEvent::Touch(Touch {
            device_id: DeviceId::dummy(),
            phase,
            location: PhysicalPosition::new(x, y),
            force: None,
            id: 0,
        })
    };
    app.send_window_event(contact(TouchPhase::Started, from));
    app.render_frame();
    let step = (to - from) / SCROLL_STEP_COUNT as f64;
    for index in 1..=SCROLL_STEP_COUNT {
        sleep(SCROLL_STEP_INTERVAL);
        app.send_window_event(contact(TouchPhase::Moved, from + step * index as f64));
        app.render_frame();
    }
}

/// The finger every touch platform reports as pointer zero.
const fn finger() -> PointerKey {
    PointerKey::new(PointerSource::Touch, 0)
}

/// Puts a finger on the glass at `(x, y)` and leaves it there.
fn touch_down<W: Widget + 'static>(app: &mut HeadlessApp<W>, x: f64, y: f64) {
    app.send_window_event(WindowEvent::Touch(Touch {
        device_id: DeviceId::dummy(),
        phase: TouchPhase::Started,
        location: PhysicalPosition::new(x, y),
        force: None,
        id: 0,
    }));
    app.render_frame();
}

/// Scrolls the page the way a platform that reads the finger itself does: as
/// scroll deltas with a contact still on the glass, and not a single pointer
/// move. This is how a touch browser reports a finger dragging a page, and how
/// momentum arrives everywhere.
fn platform_scroll<W: Widget + 'static>(app: &mut HeadlessApp<W>, dy: f64) {
    app.send_window_event(WindowEvent::MouseWheel {
        device_id: DeviceId::dummy(),
        delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, dy)),
        phase: TouchPhase::Moved,
    });
    app.render_frame();
}

/// A page that scrolls itself under a resting finger must not select a word.
///
/// The scroll view revokes a pending press when it wins the finger's *drag* —
/// but a page moves without any drag to win. A touch browser reports a finger
/// scrolling the page as scroll deltas, momentum carries a page on after the
/// finger is gone, and an animation moves a paragraph for reasons of its own. In
/// each case the text hears nothing at all, so the press must judge itself: the
/// glyph it was resting on has slid away, and a finger cannot hold still on
/// content that is moving.
#[test]
fn a_page_that_scrolls_itself_under_a_finger_never_starts_selecting() {
    release_all_pointers();
    let controller = ScrollController::new();
    let mut app = AimerApp::start_headless(selectable_page(&controller));
    app.render_frame();
    app.render_frame();
    assert!(controller.max_extent().y > 0.0);

    touch_down(&mut app, 40.0, PRESS_Y);
    for _ in 0..SCROLL_STEP_COUNT {
        sleep(SCROLL_STEP_INTERVAL);
        platform_scroll(&mut app, -20.0);
    }

    assert!(
        controller.offset().y > 0.0,
        "the platform scrolled the page while the finger was down"
    );
    assert!(
        !is_pointer_claimed(finger()),
        "the content moved under the finger, so the press was never a hold"
    );
}

/// A thumb that keeps scrolling must never start selecting.
///
/// The press a finger leaves behind ripens into a selection once the hold has
/// elapsed, and a frame promotes it without consulting how far the finger has
/// travelled since — it cannot, because a finger whose gesture the scroll view
/// took no longer reports its moves to the text. The view must therefore revoke
/// that press when it takes the gesture, and this is the test that it does:
/// eight moves spread over more than the hold, with the finger still down.
#[test]
fn a_thumb_that_keeps_scrolling_never_starts_selecting() {
    release_all_pointers();
    let controller = ScrollController::new();
    let mut app = AimerApp::start_headless(selectable_page(&controller));
    app.render_frame();
    app.render_frame();
    assert!(controller.max_extent().y > 0.0);

    touch_scroll_slowly(&mut app, 40.0, PRESS_Y, RELEASE_Y);

    assert!(
        controller.offset().y > 0.0,
        "the finger travelled far enough to be a scroll"
    );
    assert!(
        !is_pointer_claimed(finger()),
        "the scroll view owns the gesture, so no frame may turn the press into a selection"
    );
}

/// A finger dragged over selectable text scrolls the page.
///
/// A finger press means too many things to act on, so the text records it and
/// takes the *pointer* — the only way an enclosing view can tell it the gesture
/// is gone — while claiming nothing, which leaves that view free to take the
/// drag. Both halves matter: claiming here would make a page refuse to scroll
/// wherever there is text on it, and taking nothing would leave the recorded
/// press to ripen into a selection several frames after the finger has left the
/// glass.
#[test]
fn a_finger_dragged_over_selectable_text_scrolls_the_page() {
    let controller = ScrollController::new();
    let mut app = AimerApp::start_headless(selectable_page(&controller));
    app.render_frame();
    app.render_frame();
    assert!(controller.max_extent().y > 0.0);

    touch_drag_up(&mut app, 40.0, PRESS_Y, RELEASE_Y);

    assert!(
        controller.offset().y > 0.0,
        "a finger that travels over text is scrolling, not selecting"
    );
}

#[test]
fn dragging_across_text_inside_a_scroll_view_does_not_scroll_it() {
    let controller = ScrollController::new();
    let mut app = AimerApp::start_headless(selectable_page(&controller));
    app.render_frame();
    app.render_frame();
    assert!(
        controller.max_extent().y > 0.0,
        "the page must be scrollable for this test to mean anything"
    );

    drag_up(&mut app, 40.0, PRESS_Y, RELEASE_Y);

    assert_eq!(
        controller.offset().y, 0.0,
        "the drag selected text, so the scroll view must not have moved"
    );
}

/// Control: the very same gesture over content that owns no gesture of its own
/// scrolls, so the test above proves a claim was respected rather than that
/// dragging never scrolls.
#[test]
fn the_same_drag_over_plain_content_still_scrolls() {
    let controller = ScrollController::new();
    let mut app = AimerApp::start_headless(plain_page(&controller));
    app.render_frame();
    app.render_frame();
    assert!(controller.max_extent().y > 0.0);

    drag_up(&mut app, 40.0, PRESS_Y, RELEASE_Y);

    assert!(
        controller.offset().y > 0.0,
        "a drag over nothing selectable is a scroll"
    );
}

/// A gesture that is over releases its claim, so the next drag scrolls even
/// though the previous one selected. Without this the first selection would
/// deadlock the view forever.
#[test]
fn a_scroll_view_scrolls_again_after_a_selection_gesture_ends() {
    let controller = ScrollController::new();
    let mut app = AimerApp::start_headless(selectable_page(&controller));
    app.render_frame();
    app.render_frame();

    drag_up(&mut app, 40.0, PRESS_Y, RELEASE_Y);
    assert_eq!(controller.offset().y, 0.0);

    // The second gesture starts on the background to the right of the text,
    // where nothing selectable lives.
    drag_up(&mut app, 900.0, PRESS_Y, RELEASE_Y);

    assert!(
        controller.offset().y > 0.0,
        "the selection released the pointer, so this drag is a scroll"
    );
}
