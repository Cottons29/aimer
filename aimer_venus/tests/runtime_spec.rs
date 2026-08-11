//! The properties that justify Venus existing at all.
//!
//! Each test here pins one thing a general-purpose runtime cannot do: keep a
//! non-`Send` capture, land an effect *before* the frame's build phase, stop
//! when the frame budget runs out, and forget a task when the element that
//! owned it went away.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use aimer_venus::{FrameBudget, Venus, yield_now};

/// The bound the whole design exists to delete: a task captures an `Rc` — a
/// `StateUpdater`, a controller — and still runs.
#[test]
fn a_local_task_keeps_a_non_send_capture() {
    let venus = Venus::new();
    let calls = Rc::new(Cell::new(0));

    let counted = calls.clone();
    venus.spawn(async move {
        counted.set(counted.get() + 1);
    });

    venus.run_microtasks();

    assert_eq!(calls.get(), 1);
    assert_eq!(venus.task_count(), 0);
}

/// The one-frame-latency bug, stated as a test: an effect produced by a
/// resolved future has to be visible to *this* frame's build, not the next.
#[test]
fn a_resolved_future_lands_before_the_build_phase() {
    let venus = Venus::new();
    let state = Rc::new(Cell::new(0));
    let observed_by_build = Rc::new(Cell::new(-1));

    let mutated = state.clone();
    venus.spawn(async move {
        yield_now().await;
        mutated.set(7);
    });

    let read = state.clone();
    let seen = observed_by_build.clone();
    venus.drive_frame(|| seen.set(read.get()));

    assert_eq!(observed_by_build.get(), 7);
}

/// Idle work is budget-gated: nothing runs without measured room, and what did
/// run keeps its state for the next frame.
#[test]
fn idle_work_waits_for_room_and_resumes_with_its_state_intact() {
    let venus = Venus::new();
    let progress = Rc::new(Cell::new(0));

    let counted = progress.clone();
    venus.spawn_idle(async move {
        for _ in 0..3 {
            counted.set(counted.get() + 1);
            yield_now().await;
        }
    });

    assert_eq!(venus.run_idle(&FrameBudget::exhausted()), 0);
    assert_eq!(progress.get(), 0);

    venus.run_idle(&FrameBudget::from_now(Duration::from_millis(4)));

    assert_eq!(progress.get(), 3);
    assert_eq!(venus.task_count(), 0);
}

/// Structural cancellation: an unmounted element drops its scope, and the tasks
/// it spawned are simply gone — no abort handle, no generation counter.
#[test]
fn dropping_a_scope_forgets_its_tasks() {
    let venus = Venus::new();
    let ran = Rc::new(Cell::new(false));

    let scope = venus.scope();
    let flag = ran.clone();
    venus.spawn_in(scope.id(), async move {
        flag.set(true);
    });
    assert_eq!(venus.task_count(), 1);

    drop(scope);
    venus.run_microtasks();

    assert!(!ran.get());
    assert_eq!(venus.task_count(), 0);
}

/// The scheduler must never be the reason a frame is missed: draining a
/// pathological number of ready microtasks costs a fraction of one frame.
#[test]
fn draining_ten_thousand_microtasks_costs_far_less_than_a_frame() {
    let venus = Venus::new();
    let calls = Rc::new(Cell::new(0));

    for _ in 0..10_000 {
        let counted = calls.clone();
        venus.spawn(async move {
            counted.set(counted.get() + 1);
        });
    }

    let started = web_time::Instant::now();
    venus.run_microtasks();
    let elapsed = started.elapsed();

    assert_eq!(calls.get(), 10_000);
    assert!(
        elapsed < Duration::from_millis(4),
        "draining 10_000 microtasks took {elapsed:?}"
    );
}

/// Blocking work leaves the UI thread and comes back as a microtask, so the
/// awaiting task is still holding its `Rc` when the value arrives.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn offloaded_work_comes_back_to_the_ui_thread() {
    let venus = Venus::new();
    let state = Rc::new(Cell::new(0));

    let venus_for_task = venus.clone();
    let mutated = state.clone();
    venus.spawn(async move {
        let sum = venus_for_task.offload(|| (1..=10).sum::<u32>()).await;
        mutated.set(sum);
    });

    while venus.task_count() > 0 {
        venus.run_microtasks();
    }

    assert_eq!(state.get(), 55);
}

/// A worker finishing while the event loop sleeps has to be able to wake it.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_wake_from_a_worker_notifies_the_event_loop() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let venus = Venus::new();
    let pings = Arc::new(AtomicUsize::new(0));

    let counted = pings.clone();
    venus.set_notifier(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    });

    let venus_for_task = venus.clone();
    venus.spawn(async move {
        venus_for_task.offload(|| ()).await;
    });

    while venus.task_count() > 0 {
        venus.run_microtasks();
    }

    assert!(pings.load(Ordering::SeqCst) >= 1);
}
