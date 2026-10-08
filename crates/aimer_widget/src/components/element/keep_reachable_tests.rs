//! A kept-reachable element registers its path in the dirty-path index and
//! takes it out again, however it stops holding.

use super::*;

fn with_pass_path<R>(path: &[ElementId], body: impl FnOnce() -> R) -> R {
    REBUILD_PATH.with(|current| *current.borrow_mut() = path.to_vec());
    let result = body();
    REBUILD_PATH.with(|current| current.borrow_mut().clear());
    result
}

#[test]
fn holding_registers_every_element_on_the_path_and_leaves_the_index_valid() {
    let path = [ElementId::next(), ElementId::next(), ElementId::next()];
    let outside = ElementId::next();
    let held = KeepReachable::new();
    DIRTY_PATHS_READY.with(|ready| ready.set(true));

    with_pass_path(&path, || held.hold());

    assert!(path.iter().all(|id| dirty_path_contains(*id)));
    assert!(!dirty_path_contains(outside));
    assert!(DIRTY_PATHS_READY.with(Cell::get), "holding must not discard the index");
    held.release();
}

#[test]
fn releasing_and_dropping_remove_the_path() {
    let path = [ElementId::next(), ElementId::next()];

    let released = KeepReachable::new();
    with_pass_path(&path, || released.hold());
    released.release();
    assert!(path.iter().all(|id| !dirty_path_contains(*id)), "release leaves a path behind");

    {
        let dropped = KeepReachable::new();
        with_pass_path(&path, || dropped.hold());
        assert!(dirty_path_contains(path[0]));
    }
    assert!(path.iter().all(|id| !dirty_path_contains(*id)), "drop leaves a path behind");
}

#[test]
fn holding_every_frame_registers_once_and_follows_a_path_that_moves() {
    let first = [ElementId::next(), ElementId::next()];
    let second = [first[0], ElementId::next()];
    let held = KeepReachable::new();

    for _ in 0..3 {
        with_pass_path(&first, || held.hold());
    }
    held.release();
    assert!(!dirty_path_contains(first[0]), "repeated holds must register the path once");

    with_pass_path(&first, || held.hold());
    with_pass_path(&second, || held.hold());
    assert!(!dirty_path_contains(first[1]), "the old path was not removed");
    assert!(dirty_path_contains(second[1]));
    held.release();
    assert!(!dirty_path_contains(second[0]));
}

#[test]
fn holding_outside_a_pass_falls_back_to_discarding_the_index() {
    DIRTY_PATHS_READY.with(|ready| ready.set(true));
    let held = KeepReachable::new();

    held.hold();

    assert!(!DIRTY_PATHS_READY.with(Cell::get), "there is no path to register");
}
