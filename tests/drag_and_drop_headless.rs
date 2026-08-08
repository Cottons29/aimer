//! Dragging one widget onto another, through the real element tree.
//!
//! Everything here is driven by window events, the same ones a mouse produces:
//! nothing reaches into the drag session directly. That is deliberate — the
//! interesting claims are about *routing* (does the drop find the topmost
//! target that wants it?) and routing is exactly what a unit test of the
//! session cannot check.

use std::cell::RefCell;
use std::rc::Rc;

use aimer::quiver::winit::dpi::PhysicalPosition;
use aimer::quiver::winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};
use aimer::{AimerApp, AnyWidget, Container, Row, SizedBox, Stack, Positioned, Widget};
use aimer_dnd::{DragTarget, DragTargetState, Draggable};

/// The value carried by a drag in these tests.
#[derive(Clone, Debug, PartialEq)]
struct CardId(u32);

/// A payload no target here understands.
#[derive(Clone, Debug, PartialEq)]
struct Unrelated(u32);

/// Records what a target accepted.
type Accepted = Rc<RefCell<Vec<CardId>>>;

fn tile(size: u32) -> AnyWidget {
    SizedBox::new().width(size).height(size).boxed()
}

/// A draggable 100x100 card carrying `id`.
fn card(id: CardId) -> AnyWidget {
    Draggable::new()
        .data(id)
        .feedback(|| tile(100))
        .child(tile(100))
        .boxed()
}


/// A 100x100 target that records everything it accepts.
fn column(accepted: Accepted) -> AnyWidget {
    DragTarget::<CardId>::new()
        .on_accept(move |id: CardId| accepted.borrow_mut().push(id))
        .child(|_state: DragTargetState| tile(100))
        .boxed()
}

/// Moves the cursor and presses, drags to `to`, and releases there.
fn drag<W: Widget + 'static>(
    app: &mut aimer::quiver::aimer_app::HeadlessAimerApp<W>,
    from: (f64, f64),
    to: (f64, f64),
) {
    let device_id = DeviceId::dummy();

    app.send_window_event(WindowEvent::CursorMoved {
        device_id,
        position: PhysicalPosition::new(from.0, from.1),
    });
    app.send_window_event(WindowEvent::MouseInput {
        device_id,
        state: ElementState::Pressed,
        button: MouseButton::Left,
    });
    app.send_window_event(WindowEvent::CursorMoved {
        device_id,
        position: PhysicalPosition::new(to.0, to.1),
    });
    app.send_window_event(WindowEvent::MouseInput {
        device_id,
        state: ElementState::Released,
        button: MouseButton::Left,
    });
    // The drop is settled on the frame the release asked for: whether a target
    // took the payload is only knowable after the routed drop pass.
    app.render_frame();
}

/// A card at `0..100` and a target at `100..200`, side by side.
#[test]
fn a_card_dropped_on_a_target_is_accepted_once() {
    let accepted: Accepted = Rc::new(RefCell::new(Vec::new()));
    let page = Container::new().child(
        Row::new().children([card(CardId(1)), column(accepted.clone())]),
    );

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    drag(&mut app, (50.0, 50.0), (150.0, 50.0));

    assert_eq!(*accepted.borrow(), vec![CardId(1)]);
}

/// A press that never travels past the tap slop is a tap, not a drag.
#[test]
fn a_press_that_does_not_travel_drops_nothing() {
    let accepted: Accepted = Rc::new(RefCell::new(Vec::new()));
    let page = Container::new().child(
        Row::new().children([card(CardId(1)), column(accepted.clone())]),
    );

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    drag(&mut app, (50.0, 50.0), (54.0, 52.0));

    assert!(accepted.borrow().is_empty(), "a tap must not drop anything");
}

/// A target bound to one payload type never sees a drag carrying another.
#[test]
fn a_payload_of_another_type_never_reaches_the_target() {
    let accepted: Accepted = Rc::new(RefCell::new(Vec::new()));
    let unrelated = Draggable::new()
        .data(Unrelated(1))
        .feedback(|| tile(100))
        .child(tile(100))
        .boxed();
    let page = Container::new()
        .child(Row::new().children([unrelated, column(accepted.clone())]));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    drag(&mut app, (50.0, 50.0), (150.0, 50.0));

    assert!(accepted.borrow().is_empty());
}

/// A predicate that says no keeps the payload, and the target stays silent.
#[test]
fn a_refused_drop_never_reaches_on_accept() {
    let accepted: Accepted = Rc::new(RefCell::new(Vec::new()));
    let recorder = accepted.clone();
    let locked = DragTarget::<CardId>::new()
        .will_accept(|id: &CardId| id.0 != 1)
        .on_accept(move |id: CardId| recorder.borrow_mut().push(id))
        .child(|_state: DragTargetState| tile(100))
        .boxed();
    let page = Container::new().child(Row::new().children([card(CardId(1)), locked]));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    drag(&mut app, (50.0, 50.0), (150.0, 50.0));

    assert!(
        accepted.borrow().is_empty(),
        "a target that refused the payload must not receive it"
    );
}

/// Two targets in the same place: the drop belongs to the one on top.
#[test]
fn the_topmost_target_takes_the_drop() {
    let below: Accepted = Rc::new(RefCell::new(Vec::new()));
    let above: Accepted = Rc::new(RefCell::new(Vec::new()));

    let page = Container::new().child(Stack::new().children([
        Positioned::new()
            .left(0.0)
            .top(0.0)
            .child(card(CardId(3)))
            .boxed(),
        Positioned::new()
            .left(150.0)
            .top(0.0)
            .child(column(below.clone()))
            .boxed(),
        Positioned::new()
            .left(150.0)
            .top(0.0)
            .child(column(above.clone()))
            .boxed(),
    ]));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    drag(&mut app, (50.0, 50.0), (200.0, 50.0));

    assert!(below.borrow().is_empty(), "a covered target took the drop");
    assert_eq!(*above.borrow(), vec![CardId(3)]);
}
