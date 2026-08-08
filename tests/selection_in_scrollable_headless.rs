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

use aimer::quiver::winit::dpi::PhysicalPosition;
use aimer::quiver::winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};
use aimer::{
    AnyWidget, BoxAlignment, Column, Container, ScrollAxis, ScrollController, Scrollable,
    SelectionArea, SizedBox, Text, Widget,
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
