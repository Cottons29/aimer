//! Files dragged in from the operating system, through the real element tree.
//!
//! winit reports a file drag one file at a time and attaches no cursor position
//! to any of it. Both of those are load-bearing here: the coalescing test fails
//! if a five-file drag produces five callbacks, and the two-zone test fails if
//! the position is not plumbed through, because the wrong zone answers.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use aimer::quiver::winit::dpi::PhysicalPosition;
use aimer::quiver::winit::event::{DeviceId, WindowEvent};
use aimer::{AimerApp, AnyWidget, Container, DragTargetState, DropZone, Row, SizedBox, Widget};

/// Every batch a zone received, in the order they arrived.
type Batches = Rc<RefCell<Vec<Vec<PathBuf>>>>;

/// Whether a zone is currently highlighted, sampled on its last build.
type Highlighted = Rc<RefCell<bool>>;

fn zone(batches: Batches, highlighted: Highlighted, extensions: Option<&[&str]>) -> AnyWidget {
    let zone = DropZone::new().on_drop(move |paths: Vec<PathBuf>| batches.borrow_mut().push(paths));
    let zone = match extensions {
        Some(extensions) => zone.extensions(extensions.to_vec()),
        None => zone,
    };
    zone.child(move |state: DragTargetState| {
        *highlighted.borrow_mut() = state.is_hovered;
        SizedBox::new().width(100).height(100)
    })
    .boxed()
}

fn batches() -> Batches {
    Rc::new(RefCell::new(Vec::new()))
}

fn highlighted() -> Highlighted {
    Rc::new(RefCell::new(false))
}

/// Puts the cursor at `x`, so the file events that follow are hit-tested there.
fn point_at<W: Widget + 'static>(
    app: &mut aimer::quiver::aimer_app::HeadlessAimerApp<W>,
    x: f64,
    y: f64,
) {
    app.send_window_event(WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(x, y),
    });
}

fn hover<W: Widget + 'static>(
    app: &mut aimer::quiver::aimer_app::HeadlessAimerApp<W>,
    path: &str,
) {
    app.send_window_event(WindowEvent::HoveredFile(PathBuf::from(path)));
}

fn drop_file<W: Widget + 'static>(
    app: &mut aimer::quiver::aimer_app::HeadlessAimerApp<W>,
    path: &str,
) {
    app.send_window_event(WindowEvent::DroppedFile(PathBuf::from(path)));
}

#[test]
fn one_dropped_file_is_delivered_once() {
    let received = batches();
    let lit = highlighted();
    let page = Container::new().child(zone(received.clone(), lit.clone(), None));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    point_at(&mut app, 50.0, 50.0);
    hover(&mut app, "/tmp/a.png");
    app.render_frame();
    assert!(*lit.borrow(), "a hovering file must highlight the zone");

    drop_file(&mut app, "/tmp/a.png");
    app.render_frame();

    assert_eq!(received.borrow().len(), 1);
    assert_eq!(received.borrow()[0], vec![PathBuf::from("/tmp/a.png")]);
    assert!(!*lit.borrow(), "the highlight must clear on a drop");
}

/// The platform reports five files as five events. The application asked for a
/// drag, not for five drags.
#[test]
fn five_files_dropped_together_arrive_as_one_batch() {
    let received = batches();
    let page = Container::new().child(zone(received.clone(), highlighted(), None));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    point_at(&mut app, 50.0, 50.0);
    let paths = ["a.png", "b.png", "c.png", "d.png", "e.png"];
    for path in paths {
        hover(&mut app, path);
    }
    for path in paths {
        drop_file(&mut app, path);
    }
    app.render_frame();

    assert_eq!(received.borrow().len(), 1, "one drag, one callback");
    assert_eq!(received.borrow()[0].len(), 5);
}

/// Two zones side by side. This is the test that fails when the file events
/// carry no position: whichever zone answers first takes everything.
#[test]
fn only_the_zone_under_the_cursor_receives_the_drop() {
    let left = batches();
    let right = batches();
    let page = Container::new().child(Row::new().children([
        zone(left.clone(), highlighted(), None),
        zone(right.clone(), highlighted(), None),
    ]));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    point_at(&mut app, 150.0, 50.0);
    hover(&mut app, "/tmp/a.png");
    drop_file(&mut app, "/tmp/a.png");
    app.render_frame();

    assert!(left.borrow().is_empty(), "the zone beside the cursor fired");
    assert_eq!(right.borrow().len(), 1);
}

/// A restricted zone is invisible to files it does not want: no highlight, no
/// callback.
#[test]
fn a_zone_restricted_by_extension_ignores_everything_else() {
    let received = batches();
    let lit = highlighted();
    let page = Container::new().child(zone(
        received.clone(),
        lit.clone(),
        Some(&["png", "jpg"]),
    ));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    point_at(&mut app, 50.0, 50.0);
    hover(&mut app, "/tmp/notes.txt");
    app.render_frame();
    assert!(!*lit.borrow(), "a filtered zone must not highlight");

    drop_file(&mut app, "/tmp/notes.txt");
    app.render_frame();
    assert!(received.borrow().is_empty());

    // The same zone still takes what it asked for.
    hover(&mut app, "/tmp/photo.PNG");
    drop_file(&mut app, "/tmp/photo.PNG");
    app.render_frame();
    assert_eq!(received.borrow().len(), 1);
}

/// The platform announces a file *entering* the window and then goes quiet, but
/// the file keeps moving. The zones have to follow it: the one under it lights
/// up, the one it left goes dark, and background leaves nothing lit.
#[test]
fn a_hovering_file_is_tracked_on_every_move() {
    let left_lit = highlighted();
    let right_lit = highlighted();
    let page = Container::new().child(Row::new().children([
        zone(batches(), left_lit.clone(), None),
        zone(batches(), right_lit.clone(), None),
    ]));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    point_at(&mut app, 50.0, 50.0);
    hover(&mut app, "/tmp/a.png");
    app.render_frame();
    assert!(*left_lit.borrow(), "the zone under the file must light up");
    assert!(!*right_lit.borrow());

    // The file travels on. The platform says nothing about it.
    point_at(&mut app, 150.0, 50.0);
    app.render_frame();
    assert!(!*left_lit.borrow(), "the zone the file left stayed lit");
    assert!(*right_lit.borrow(), "the zone the file moved onto never lit");

    // And off both of them, onto the background.
    point_at(&mut app, 150.0, 250.0);
    app.render_frame();
    assert!(!*left_lit.borrow());
    assert!(!*right_lit.borrow(), "background left a zone lit");
}

/// Where the file was picked up is irrelevant; where it was let go is not.
#[test]
fn a_file_is_delivered_where_it_was_released_not_where_it_arrived() {
    let left = batches();
    let right = batches();
    let page = Container::new().child(Row::new().children([
        zone(left.clone(), highlighted(), None),
        zone(right.clone(), highlighted(), None),
    ]));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    point_at(&mut app, 50.0, 50.0);
    hover(&mut app, "/tmp/a.png");
    point_at(&mut app, 150.0, 50.0);
    drop_file(&mut app, "/tmp/a.png");
    app.render_frame();

    assert!(left.borrow().is_empty(), "the zone the file only passed over fired");
    assert_eq!(right.borrow().len(), 1);
    assert_eq!(right.borrow()[0], vec![PathBuf::from("/tmp/a.png")]);
}

/// A drag that leaves the window takes the highlight with it and leaves nothing
/// behind for the next one.
#[test]
fn a_cancelled_drag_clears_the_highlight_and_the_collected_paths() {
    let received = batches();
    let lit = highlighted();
    let page = Container::new().child(zone(received.clone(), lit.clone(), None));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    point_at(&mut app, 50.0, 50.0);
    hover(&mut app, "/tmp/a.png");
    app.render_frame();
    assert!(*lit.borrow());

    app.send_window_event(WindowEvent::HoveredFileCancelled);
    app.render_frame();

    assert!(!*lit.borrow(), "a cancelled drag left the zone highlighted");
    assert!(received.borrow().is_empty());

    // Nothing was carried over into the next drag.
    hover(&mut app, "/tmp/b.png");
    drop_file(&mut app, "/tmp/b.png");
    app.render_frame();

    assert_eq!(received.borrow().len(), 1);
    assert_eq!(received.borrow()[0], vec![PathBuf::from("/tmp/b.png")]);
}
