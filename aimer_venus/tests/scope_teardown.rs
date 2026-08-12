//! What tearing down a widget subtree costs the scheduler.
//!
//! The framework keeps one [`aimer_venus::TaskScope`] per element and drops it
//! on unmount, so a route navigation or a closing dialog is a burst of scope
//! drops — most of them for elements that never spawned a task. Cancellation
//! therefore has to cost what the *scope* contains, not what the arena has
//! ever contained: a scheduler that sweeps every slot per drop turns one busy
//! afternoon of tasks into a permanent teardown tax, quadratic in the size of
//! the subtree.
//!
//! The bound is deliberately loose, in the same spirit as `frame_overhead.rs`:
//! this is not a benchmark, it is a guard against scope cancellation quietly
//! becoming linear in the number of slots.

use std::time::{Duration, Instant};

use aimer_venus::Venus;

/// The most a subtree's worth of empty-scope drops may cost.
///
/// The guarded regression — a full arena sweep per drop — costs hundreds of
/// milliseconds at this scale, two orders of magnitude above this ceiling,
/// while the intended cost is single-digit milliseconds, well below it.
const CEILING: Duration = Duration::from_millis(100);

#[test]
fn dropping_scopes_costs_the_scope_not_the_arena() {
    let venus = Venus::new();

    // One busy burst, long finished: the arena keeps the slots around for
    // reuse, and teardown must not pay for them ever after.
    for _ in 0..20_000 {
        venus.spawn(async {});
    }
    venus.run_microtasks();
    assert_eq!(venus.task_count(), 0);

    // A subtree's worth of elements unmounting, none of which spawned a task.
    let scopes = 20_000;
    let start = Instant::now();
    for _ in 0..scopes {
        drop(venus.scope());
    }
    let elapsed = start.elapsed();

    println!("{scopes} empty scope drops: {elapsed:?}");
    assert!(
        elapsed < CEILING,
        "tearing down {scopes} empty scopes took {elapsed:?} — cancellation is \
         scaling with the arena, not the scope"
    );
}

#[test]
fn a_scope_teardown_cancels_exactly_its_own_tasks() {
    let venus = Venus::new();

    let doomed = venus.scope();
    let kept = venus.scope();

    for _ in 0..8 {
        venus.spawn_in(doomed.id(), async { unreachable!("cancelled") });
        venus.spawn_in(kept.id(), async {});
    }

    drop(doomed);
    assert_eq!(venus.task_count(), 8);

    venus.run_microtasks();
    assert_eq!(venus.task_count(), 0);
}
