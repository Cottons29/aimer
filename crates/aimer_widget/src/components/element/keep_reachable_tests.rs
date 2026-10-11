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

#[test]
fn a_nested_walk_does_not_replace_the_path_from_the_root_with_its_tail() {
    let ids: Vec<ElementId> = (0..4).map(|_| ElementId::next()).collect();
    let held = KeepReachable::new();

    with_pass_path(&ids, || held.hold());
    for id in &ids {
        assert!(dirty_path_contains(*id), "the root's walk registers every ancestor");
    }

    // A stateful element on the path rebuilds itself and walks its own subtree:
    // the path it sees starts below the root.
    with_pass_path(&ids[2..], || held.hold());

    for id in &ids {
        assert!(
            dirty_path_contains(*id),
            "the ancestors above the nested walk's root must stay registered"
        );
    }
    held.release();
    for id in &ids {
        assert!(!dirty_path_contains(*id), "releasing removes the whole path");
    }
}

#[test]
fn a_holder_that_moved_under_another_root_asks_for_a_full_pass() {
    let old: Vec<ElementId> = (0..3).map(|_| ElementId::next()).collect();
    let new: Vec<ElementId> = (0..3).map(|_| ElementId::next()).collect();
    let held = KeepReachable::new();

    with_pass_path(&old, || held.hold());
    DIRTY_PATHS_READY.with(|ready| ready.set(true));
    with_pass_path(&new, || held.hold());

    assert!(!DIRTY_PATHS_READY.with(Cell::get), "the index cannot answer a different root");
    for id in &new {
        assert!(dirty_path_contains(*id));
    }
    for id in &old {
        assert!(!dirty_path_contains(*id), "the old path is gone");
    }
    held.release();
}

#[test]
fn an_index_built_from_another_root_does_not_cover_the_traversal_in_progress() {
    let indexed = ElementId::next();
    let other = ElementId::next();
    DIRTY_PATHS_READY.with(|ready| ready.set(true));
    DIRTY_INDEXED_ROOTS.with(|roots| {
        let mut roots = roots.borrow_mut();
        roots.clear();
        roots.insert(indexed);
    });

    REBUILD_TRAVERSAL_ROOT.with(|root| root.set(Some(indexed)));
    assert!(dirty_paths_cover_current_traversal());
    REBUILD_TRAVERSAL_ROOT.with(|root| root.set(Some(other)));
    assert!(
        !dirty_paths_cover_current_traversal(),
        "a clean walk of one subtree says nothing about another"
    );
    REBUILD_TRAVERSAL_ROOT.with(|root| root.set(None));
    assert!(!dirty_paths_cover_current_traversal());
    invalidate_dirty_paths();
}

#[test]
fn a_holder_that_was_never_visited_reports_that_it_has_no_path() {
    let held = KeepReachable::new();
    assert!(!held.is_registered());
    with_pass_path(&[ElementId::next(), ElementId::next()], || held.hold());
    assert!(held.is_registered());
    held.release();
    assert!(!held.is_registered());
}

#[test]
fn a_dirty_source_keeps_the_path_from_the_root_when_a_nested_walk_sees_its_tail() {
    let ids: Vec<ElementId> = (0..4).map(|_| ElementId::next()).collect();
    let source = DirtySource::new(Rc::new(Cell::new(false)));
    source.set_path(&ids);
    DIRTY_PATHS_READY.with(|ready| ready.set(true));

    source.set_path(&ids[2..]);

    assert_eq!(source.path.borrow().as_deref(), Some(&ids[..]), "the full path stays");
    assert!(
        DIRTY_PATHS_READY.with(Cell::get),
        "a walk that saw only the tail says nothing against the index"
    );

    // A different root is a real move: the path is replaced and the index
    // cannot be trusted until the next full walk.
    let moved: Vec<ElementId> = (0..3).map(|_| ElementId::next()).collect();
    source.set_path(&moved);
    assert_eq!(source.path.borrow().as_deref(), Some(&moved[..]));
    assert!(!DIRTY_PATHS_READY.with(Cell::get));
}

