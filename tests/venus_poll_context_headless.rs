//! What a running application gives a task spawned from anywhere in the tree.
//!
//! Venus polls futures on the thread that owns the frame, where a runtime-backed
//! future would otherwise find no runtime to build its resources with — a
//! `reqwest` connector with no reactor to register a socket on, a `sleep` with
//! no timer wheel. `AimerApp` installs a
//! [`PollContext`](aimer::venus::PollContext) for the async runtime it created,
//! so the runtime is findable for the duration of every poll and no longer.
//!
//! The example `examples/http_request_button.rs` is this property with a socket
//! on the end of it; the timer here proves the same wiring without depending on
//! a network.

use std::cell::Cell;
use std::rc::Rc;
use std::thread::sleep;
use std::time::{Duration, Instant};

use aimer::quiver::aimer_app::HeadlessAimerApp;
use aimer::{AimerApp, ModalHost, SizedBox, Venus};

/// The application under test: what it draws is irrelevant, only that it is a
/// real one, started the way `main` starts it.
fn app() -> HeadlessAimerApp<ModalHost<SizedBox>> {
    AimerApp::start_headless(SizedBox::new().width(64).height(64))
}

#[test]
fn a_task_spawned_inside_a_running_application_can_find_the_async_runtime() {
    let mut app = app();
    let venus = Venus::current().expect("a running application installs its runtime");

    let found = Rc::new(Cell::new(false));
    let seen = found.clone();
    venus.spawn(async move { seen.set(tokio::runtime::Handle::try_current().is_ok()) });

    app.render_frame();

    assert!(found.get());
}

/// The completion half: the timer runs on the async runtime's own threads and
/// the task resumes on the UI thread, still holding a non-`Send` capture.
#[test]
fn a_runtime_backed_future_resolves_into_a_frame() {
    let mut app = app();
    let venus = Venus::current().expect("a running application installs its runtime");

    let slept = Rc::new(Cell::new(false));
    let flag = slept.clone();
    venus.spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;
        flag.set(true);
    });

    let deadline = Instant::now() + Duration::from_secs(5);
    while !slept.get() && Instant::now() < deadline {
        app.render_frame();
        sleep(Duration::from_millis(5));
    }

    assert!(slept.get(), "the timer never resolved on the UI thread");
}
