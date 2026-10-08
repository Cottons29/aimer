//! The per-element questions "was my paint invalidated?" are asked for every
//! element on every frame, almost always with nothing recorded. Answering them
//! must stay exact whether or not anything is.

use super::*;

#[test]
fn an_event_invalidation_is_consumed_by_the_first_question_only() {
    let marked = ElementId::next();
    let other = ElementId::next();

    assert!(!take_event_paint_invalidated(marked), "nothing was recorded yet");
    mark_event_paint_invalidated(marked);

    assert!(!take_event_paint_invalidated(other));
    assert!(take_event_paint_invalidated(marked));
    assert!(!take_event_paint_invalidated(marked), "it is consumed, not sticky");
}

#[test]
fn several_event_invalidations_are_each_consumed_once() {
    let ids = (0..5).map(|_| ElementId::next()).collect::<Vec<_>>();
    for id in &ids {
        mark_event_paint_invalidated(*id);
    }
    mark_event_paint_invalidated(ids[2]);

    for id in &ids {
        assert!(take_event_paint_invalidated(*id));
        assert!(!take_event_paint_invalidated(*id));
    }
}

#[test]
fn a_local_invalidation_is_reported_for_its_element_until_the_epoch_moves() {
    let marked = ElementId::next();
    let other = ElementId::next();
    assert!(!local_paint_element_was_invalidated(marked));

    mark_paint_element_invalidated(marked);
    assert!(local_paint_element_was_invalidated(marked));
    assert!(local_paint_element_was_invalidated(marked), "asking does not consume it");
    assert!(!local_paint_element_was_invalidated(other));

    // A rebuild starts a new epoch, which forgets the earlier invalidations.
    REBUILD_INVALIDATION_GENERATION.fetch_add(1, Ordering::AcqRel);
    assert!(!local_paint_element_was_invalidated(marked));
}
