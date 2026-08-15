//! Tests for the viewport-proportional behaviour of [`RawFlex`].
//!
//! A flex container measures and paints through one cached main-axis table, so
//! the work a frame does is meant to follow the viewport rather than the child
//! count. These tests pin that with a hundred-thousand-child column: the probe
//! children count every measure and every paint they receive, so a regression to
//! an `O(children)` pass shows up as a number instead of a slowdown.

use std::cell::Cell;
use std::rc::Rc;

use aimer_widget::{Drawable, Element, EventElement, LayoutElement, Rebuildable};

use crate::flex::raw_flex::RawFlex;
use crate::flex::test_support::{
    CountingChild, ResizingChild, dummy_build_context, replace_a_generated_subtree,
};
use crate::flex::FlexDirection;

const CHILD_COUNT: usize = 100_000;
const CHILD_HEIGHT: f32 = 80.0;
const VIEWPORT: f32 = 600.0;

fn tall_column(measured: &Rc<Cell<usize>>, drawn: &Rc<Cell<usize>>) -> RawFlex {
    let children = (0..CHILD_COUNT)
        .map(|_| CountingChild::boxed_new(200.0, CHILD_HEIGHT, measured, drawn))
        .collect();
    RawFlex::new(FlexDirection::Column, children, "Column")
}

/// A `Column` under a viewport must paint only the children intersecting it,
/// and must not re-measure the whole list to find them.
#[test]
fn draw_only_touches_the_visible_children() {
    let measured = Rc::new(Cell::new(0));
    let drawn = Rc::new(Cell::new(0));
    let column = tall_column(&measured, &drawn);
    let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));

    // The first pass has to size the list once to know the scroll extent.
    column.computed_size(&ctx);
    measured.set(0);
    drawn.set(0);

    column.draw(&ctx);

    let visible = (VIEWPORT / CHILD_HEIGHT).ceil() as usize + 1;
    assert!(
        drawn.get() <= visible,
        "painted {} children for a {VIEWPORT}px viewport",
        drawn.get()
    );
    assert!(
        measured.get() <= visible,
        "measured {} children while painting {} of them",
        measured.get(),
        drawn.get()
    );
}

/// Scrolling only changes the offset, so a later frame must stay cheap and
/// paint the slice that the offset exposes.
#[test]
fn scrolled_draw_stays_proportional_to_the_viewport() {
    let measured = Rc::new(Cell::new(0));
    let drawn = Rc::new(Cell::new(0));
    let column = tall_column(&measured, &drawn);
    let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));
    column.draw(&ctx);

    // Emulate a `Scrollable` that has scrolled 4_000 children down.
    let offset = 4_000.0 * CHILD_HEIGHT;
    let scrolled = dummy_build_context(200.0, VIEWPORT, Some((0.0, offset, 200.0, VIEWPORT)));
    measured.set(0);
    drawn.set(0);
    column.draw(&scrolled);

    let visible = (VIEWPORT / CHILD_HEIGHT).ceil() as usize + 1;
    assert!(
        drawn.get() > 0 && drawn.get() <= visible,
        "painted {} children after scrolling",
        drawn.get()
    );
    assert!(
        measured.get() <= visible,
        "measured {} children after scrolling",
        measured.get()
    );
}

/// Hit testing must only consider the children of the last painted frame;
/// nothing else can be under the pointer.
#[test]
fn hit_testing_visits_only_the_painted_children() {
    let measured = Rc::new(Cell::new(0));
    let drawn = Rc::new(Cell::new(0));
    let column = tall_column(&measured, &drawn);
    let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));
    column.draw(&ctx);

    let mut hit_tested = 0;
    column.hit_test_children(&mut |_| hit_tested += 1);

    assert_eq!(hit_tested, drawn.get());
}

/// Focus and broadcast delivery must still reach every child, painted or
/// not, otherwise an off-screen input field would stop receiving keys.
#[test]
fn event_children_still_visits_the_whole_list() {
    let measured = Rc::new(Cell::new(0));
    let drawn = Rc::new(Cell::new(0));
    let column = tall_column(&measured, &drawn);
    let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));
    column.draw(&ctx);

    let mut visited = 0;
    column.event_children(&mut |_| visited += 1);

    assert_eq!(visited, CHILD_COUNT);
}

/// Before the first frame nothing is known to be off-screen, so hit testing
/// must fall back to the whole list.
#[test]
fn hit_testing_before_the_first_frame_visits_everything() {
    let measured = Rc::new(Cell::new(0));
    let drawn = Rc::new(Cell::new(0));
    let column = tall_column(&measured, &drawn);

    let mut hit_tested = 0;
    column.hit_test_children(&mut |_| hit_tested += 1);

    assert_eq!(hit_tested, CHILD_COUNT);
}

/// A child that resizes itself between frames — an implicitly animated
/// container does exactly that inside its own `draw` — must still push its
/// siblings, even though the cached table was measured before the change.
#[test]
fn a_resized_child_moves_its_siblings_on_the_next_frame() {
    let height = Rc::new(Cell::new(20.0));
    let first_at = Rc::new(Cell::new((0.0, 0.0)));
    let second_at = Rc::new(Cell::new((0.0, 0.0)));
    let column = RawFlex::new(
        FlexDirection::Column,
        vec![
            ResizingChild::boxed_new(&height, &first_at),
            ResizingChild::boxed_new(&Rc::new(Cell::new(20.0)), &second_at),
        ],
        "Column",
    );
    let ctx = dummy_build_context(200.0, 600.0, Some((0.0, 0.0, 200.0, 600.0)));

    column.draw(&ctx);
    assert_eq!(second_at.get().1, 20.0);

    height.set(50.0);
    column.draw(&ctx);

    assert_eq!(first_at.get().1, 0.0);
    assert_eq!(second_at.get().1, 50.0);
}

/// Replacing the children below a container must retire the cached table.
///
/// A list that grew — an `AsyncBuilder` swapping a spinner for the rows it
/// waited on — is measured by a `Scrollable` through the container above it. If
/// that container answers from a table measured before the swap, the scroll
/// view is told the content still fits and the page will not scroll.
#[test]
fn a_replaced_child_list_is_measured_again() {
    let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));

    // The frame that only had the loading state to show.
    let height = Rc::new(Cell::new(CHILD_HEIGHT));
    let placement = Rc::new(Cell::new((0.0, 0.0)));
    let column = RawFlex::new(
        FlexDirection::Column,
        vec![ResizingChild::boxed_new(&height, &placement)],
        "Column",
    );
    assert_eq!(column.computed_size(&ctx).height, CHILD_HEIGHT);

    // The frame the completed future produced: taller content under the very
    // same container.
    height.set(CHILD_HEIGHT * 20.0);
    replace_a_generated_subtree(&ctx);

    assert_eq!(
        column.computed_size(&ctx).height,
        CHILD_HEIGHT * 20.0,
        "the container reported the extent it had before the rebuild"
    );
}

/// A table measured from rows that did not survive the rebuild describes
/// nothing and must not be trusted, however well the two containers match.
///
/// This is the `Column` an `AsyncBuilder` rebuilds when the data arrives: same
/// direction, same gaps, same number of children — and completely different
/// children. Trusting the table there is a page that paints its content and
/// refuses to scroll, because the `Scrollable` above is told the old extent.
#[test]
fn an_adopted_table_is_not_trusted_when_the_rows_were_rebuilt() {
    let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));
    let placement = Rc::new(Cell::new((0.0, 0.0)));

    // The container that measured the loading state.
    let short = Rc::new(Cell::new(CHILD_HEIGHT));
    let old = RawFlex::new(
        FlexDirection::Column,
        vec![
            ResizingChild::boxed_new(&short, &placement),
            ResizingChild::boxed_new(&short, &placement),
        ],
        "Column",
    );
    assert_eq!(old.computed_size(&ctx).height, CHILD_HEIGHT * 2.0);

    // The container the completed request produced: the same shape, rows built
    // from scratch out of the data that arrived.
    let tall = Rc::new(Cell::new(CHILD_HEIGHT * 20.0));
    let new = RawFlex::new(
        FlexDirection::Column,
        vec![
            ResizingChild::boxed_new(&tall, &placement),
            ResizingChild::boxed_new(&tall, &placement),
        ],
        "Column",
    );
    new.adopt_runtime_state_from(&old as &dyn Element);
    replace_a_generated_subtree(&ctx);

    assert_eq!(
        new.computed_size(&ctx).height,
        CHILD_HEIGHT * 40.0,
        "the rebuilt container answered from the table its predecessor measured"
    );
}

/// The total main-axis extent must survive the lazy path: 100_000 children
/// of 80px are 8_000_000px tall, which `f32` cannot accumulate exactly.
#[test]
fn computed_size_reports_the_full_extent() {
    let measured = Rc::new(Cell::new(0));
    let drawn = Rc::new(Cell::new(0));
    let column = tall_column(&measured, &drawn);
    let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));

    let size = column.computed_size(&ctx);

    assert_eq!(size.width, 200.0);
    assert_eq!(size.height, CHILD_COUNT as f32 * CHILD_HEIGHT);
}
