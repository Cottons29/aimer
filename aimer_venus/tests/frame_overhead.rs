//! What the scheduler itself costs a frame that has nothing to do.
//!
//! Every frame of every Aimer application pays this, whether or not anything
//! asynchronous is happening, so it is worth a number rather than an assumption.
//! When an application feels slow the first question is whether the runtime is
//! the reason, and this answers it: the phases of an empty frame cost tens of
//! nanoseconds, roughly two clock reads, which is four orders of magnitude
//! below a 120 Hz frame.
//!
//! The bound is deliberately loose. This is not a benchmark — it is a guard
//! against a drain or a queue scan quietly becoming linear in the number of
//! tasks, or a phase starting to allocate. A real measurement belongs in a
//! release build with a benchmark harness.

use std::time::{Duration, Instant};

use aimer_venus::Venus;

/// The most this may cost before something is structurally wrong.
///
/// Two orders of magnitude above what it measures, so a loaded CI machine
/// cannot fail it, and still two orders of magnitude below a frame.
const CEILING: Duration = Duration::from_micros(50);

/// One frame's worth of scheduler work, exactly as `aimer_quiver` drives it.
fn drive_one_empty_frame(venus: &Venus) {
    venus.begin_frame();
    venus.run_frame_tasks();
    venus.run_microtasks();
    let budget = venus.idle_budget();
    venus.run_idle(&budget);
    venus.end_frame();
    let _ = venus.has_ready_work();
}

#[test]
fn an_empty_frame_costs_the_scheduler_almost_nothing() {
    let venus = Venus::new();
    let rounds = 200_000;

    for _ in 0..1_000 {
        drive_one_empty_frame(&venus);
    }

    let start = Instant::now();
    for _ in 0..rounds {
        drive_one_empty_frame(&venus);
    }
    let per_frame = start.elapsed() / rounds;

    println!("empty frame: {per_frame:?} of scheduler work");
    assert!(
        per_frame < CEILING,
        "the scheduler alone costs {per_frame:?} of every frame"
    );
}

/// An application that is idle must let the loop sleep.
///
/// A runtime reporting work it does not have turns every frame into a request
/// for the next one, and an application that renders flat out forever reads to a
/// user as a slow one.
#[test]
fn an_empty_frame_reports_no_work_to_come_back_for() {
    let venus = Venus::new();

    for _ in 0..8 {
        drive_one_empty_frame(&venus);
        assert!(!venus.has_ready_work(), "an idle frame claimed to have work");
    }
}
