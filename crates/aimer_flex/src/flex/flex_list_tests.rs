//! Tests for the data-driven flex containers built in
//! [`flex_list`](super).
//!
//! They live in their own file because the container itself is close to the
//! nine-hundred-line ceiling this crate keeps per file.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use aimer_attribute::size::ResolvedSize;
use aimer_widget::{
    Drawable, Key, LayoutElement, State, StateUpdater, StatefulElement, StatefulWidget,
    VisitorElement, carry_element_state,
};

use super::*;
use crate::flex::test_support::{CountingChild, dummy_build_context};
use crate::flex::{Column, Row};

/// Main-axis extent shared by the declared-extent tests.
const ROW_EXTENT: f32 = 200.0;

/// Rows a viewport of `VIEWPORT` logical pixels can expose, generously
/// rounded up to cover the overscan a windowed source materializes.
const VISIBLE_BUDGET: usize = 32;

/// Viewport height shared by the declared-extent tests.
const VIEWPORT: f32 = 600.0;

/// How many rows a pass touched, split by the kind of work.
#[derive(Clone, Default)]
struct Counters {
    built: Rc<Cell<usize>>,
    measured: Rc<Cell<usize>>,
    drawn: Rc<Cell<usize>>,
}

/// A widget whose element counts every build, measure, and paint.
struct Counting {
    counters: Counters,
    extent: f32,
}

impl Counting {
    /// A row of the extent every uniform test list uses.
    fn new(counters: &Counters) -> Self {
        Self {
            counters: counters.clone(),
            extent: ROW_EXTENT,
        }
    }
}

impl Widget for Counting {
    fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
        self.counters.built.set(self.counters.built.get() + 1);
        CountingChild::boxed_new(
            10.0,
            self.extent,
            &self.counters.measured,
            &self.counters.drawn,
        )
    }
}

/// A hundred-thousand-row column whose extent is declared rather than
/// measured.
fn declared_column(counters: &Counters) -> impl Widget + use<> {
    let counters = counters.clone();
    Column::new()
        .list(0..100_000u32)
        .item_extent(Dimension::Px(ROW_EXTENT))
        .builder(move |_| Counting::new(&counters))
}

/// The same column with nothing declared, so its extent has to be predicted.
fn undeclared_column(counters: &Counters) -> impl Widget + use<> {
    let counters = counters.clone();
    Column::new()
        .list(0..100_000u32)
        .builder(move |_| Counting::new(&counters))
}

/// A short column of uniform rows in which `tall` is twice as tall as the
/// rest, so a prediction taken from row zero is wrong.
fn varying_column(counters: &Counters, tall: u32, len: u32) -> impl Widget + use<> {
    let counters = counters.clone();
    Column::new()
        .list(0..len)
        .builder(move |index: &u32| Counting {
            counters: counters.clone(),
            extent: if *index == tall {
                2.0 * ROW_EXTENT
            } else {
                ROW_EXTENT
            },
        })
}

/// The context a `Scrollable` hands its child: an unbounded main axis, plus
/// the viewport it exposes at `offset`.
fn scrolled_context(offset: f32) -> BuildContext<'static> {
    let mut ctx =
        dummy_build_context(400.0, VIEWPORT, Some((0.0, offset, 400.0, VIEWPORT)));
    ctx.box_constraint.max_height = f32::MAX;
    ctx
}

/// A leaf widget with a fixed size, standing in for a real row.
struct Leaf(f32);

impl Widget for Leaf {
    fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
        LeafElement { height: self.0 }.boxed()
    }
}

struct LeafElement {
    height: f32,
}

impl VisitorElement for LeafElement {
    fn debug_name(&self) -> &'static str {
        "Leaf"
    }
}
impl aimer_widget::EventElement for LeafElement {}
impl aimer_widget::Rebuildable for LeafElement {}
impl aimer_widget::Drawable for LeafElement {
    fn draw(&self, _ctx: &BuildContext) {}
}
impl LayoutElement for LeafElement {
    fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
        ResolvedSize {
            width: 10.0,
            height: self.height,
        }
    }
}

/// State a row owns, standing in for a checkbox, a caret, or a hover flag.
struct RowState {
    /// The datum the row was built from, which is what the observations are
    /// keyed by.
    item: u32,
    counter: usize,
    seen: Rc<RefCell<HashMap<u32, usize>>>,
    updaters: Rc<RefCell<HashMap<u32, StateUpdater<RowState>>>>,
    updater: StateUpdater<RowState>,
}

struct StatefulRowWidget {
    item: u32,
    seen: Rc<RefCell<HashMap<u32, usize>>>,
    updaters: Rc<RefCell<HashMap<u32, StateUpdater<RowState>>>>,
}

impl StatefulWidget for StatefulRowWidget {
    type State = RowState;

    fn create_state(&self) -> Self::State {
        RowState {
            item: self.item,
            counter: 0,
            seen: self.seen.clone(),
            updaters: self.updaters.clone(),
            updater: StateUpdater::new(),
        }
    }
}

impl State<StatefulRowWidget> for RowState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        self.seen.borrow_mut().insert(self.item, self.counter);
        self.updaters
            .borrow_mut()
            .insert(self.item, self.updater.clone());
        Leaf(ROW_EXTENT)
    }
}

/// A row that owns state, so losing its element is observable.
struct StatefulRow {
    item: u32,
    seen: Rc<RefCell<HashMap<u32, usize>>>,
    updaters: Rc<RefCell<HashMap<u32, StateUpdater<RowState>>>>,
}

impl Widget for StatefulRow {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let (element, _) = StatefulElement::new_with_name(
            &StatefulRowWidget {
                item: self.item,
                seen: self.seen.clone(),
                updaters: self.updaters.clone(),
            },
            ctx,
            "StatefulRow",
            None,
        );
        element.boxed()
    }
}

/// Everything a stateful row publishes about itself, by datum.
#[derive(Clone, Default)]
struct Observed {
    seen: Rc<RefCell<HashMap<u32, usize>>>,
    updaters: Rc<RefCell<HashMap<u32, StateUpdater<RowState>>>>,
}

impl Observed {
    /// Counter the row built from `item` reported the last time it was built.
    fn counter_of(&self, item: u32) -> Option<usize> {
        self.seen.borrow().get(&item).copied()
    }

    /// Mutates the state of the row built from `item`.
    fn bump(&self, item: u32, counter: usize) {
        self.updaters
            .borrow()
            .get(&item)
            .expect("the row has to have been built")
            .set_state(move |state| state.counter = counter);
    }

    /// A list of `items` whose rows each own state.
    fn column(&self, items: Vec<u32>) -> impl Widget + use<> {
        let observed = self.clone();
        Column::new().list(items).builder(move |item: &u32| StatefulRow {
            item: *item,
            seen: observed.seen.clone(),
            updaters: observed.updaters.clone(),
        })
    }

    /// The same list, with the datum itself as the reconciliation key.
    fn keyed_column(&self, items: Vec<u32>) -> impl Widget + use<> {
        let observed = self.clone();
        Column::new()
            .list(items)
            .key(|item: &u32| Key::Value(item.to_string()))
            .builder(move |item: &u32| StatefulRow {
                item: *item,
                seen: observed.seen.clone(),
                updaters: observed.updaters.clone(),
            })
    }
}

/// A windowed list is rebuilt from scratch whenever anything above it rebuilds,
/// so without a hand-off every visible row would lose its state on an unrelated
/// `set_state` — a checkbox would untick itself, a caret would jump home.
#[test]
fn row_state_survives_a_rebuild_of_the_container() {
    let ctx = scrolled_context(0.0);
    let observed = Observed::default();
    let items: Vec<u32> = (0..100).collect();

    let old = observed.column(items.clone()).to_element(&ctx);
    old.draw(&ctx);
    observed.bump(1, 7);
    old.draw(&ctx);
    assert_eq!(observed.counter_of(1), Some(7), "the row kept no state at all");

    // Exactly what a rebuild above the list does: build the replacement, hand
    // the live state over, then drop the old tree.
    let new = observed.column(items).to_element(&ctx);
    observed.seen.borrow_mut().clear();
    carry_element_state(old.as_ref(), new.as_ref(), &ctx);
    new.draw(&ctx);

    assert_eq!(
        observed.counter_of(1),
        Some(7),
        "row 1 lost its state across the rebuild"
    );
}

/// Everything the list learned by painting lives in its layout table, and a
/// rebuild builds a new container. Losing the table would snap the scroll extent
/// back to the prediction on an unrelated `set_state`, which a scroll view sees
/// as its content shrinking under the viewport.
#[test]
fn a_corrected_extent_survives_a_rebuild_of_the_container() {
    let counters = Counters::default();
    let ctx = scrolled_context(0.0);
    let exact = 21.0 * ROW_EXTENT;

    let old = varying_column(&counters, 1, 20).to_element(&ctx);
    old.draw(&ctx);
    assert_eq!(old.computed_size(&ctx).height, exact);

    let new = varying_column(&counters, 1, 20).to_element(&ctx);
    carry_element_state(old.as_ref(), new.as_ref(), &ctx);

    assert_eq!(
        new.computed_size(&ctx).height,
        exact,
        "the rebuilt container forgot the extents it had measured"
    );
}

/// Without keys a row's identity is its index, so inserting an item at the front
/// shifts everybody's state down by one. A key mapper makes the state follow the
/// datum instead.
#[test]
fn a_keyed_list_follows_its_data_across_an_insertion() {
    let ctx = scrolled_context(0.0);
    let observed = Observed::default();

    let old = observed.keyed_column((0..100).collect()).to_element(&ctx);
    old.draw(&ctx);
    observed.bump(2, 7);
    old.draw(&ctx);

    let mut shifted: Vec<u32> = vec![900];
    shifted.extend(0..100);
    let new = observed.keyed_column(shifted).to_element(&ctx);
    observed.seen.borrow_mut().clear();
    carry_element_state(old.as_ref(), new.as_ref(), &ctx);
    new.draw(&ctx);

    assert_eq!(
        observed.counter_of(2),
        Some(7),
        "the state stayed on the index instead of following the datum"
    );
    assert_eq!(
        observed.counter_of(900),
        Some(0),
        "the inserted row must start fresh"
    );
}

#[test]
fn builder_runs_once_per_item() {
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let column = Column::new().list(0..5).builder(move |index: &i32| {
        counted.set(counted.get() + 1);
        Leaf(*index as f32)
    });

    let ctx = dummy_build_context(100.0, 100.0, None);
    let element = column.to_element(&ctx);
    // A bounded main axis has to be measured, which is what materializes the
    // rows; nothing is built before the container is asked for a size.
    element.computed_size(&ctx);

    assert_eq!(calls.get(), 5);
    let mut children = 0;
    element.visit_children(&mut |_| children += 1);
    assert_eq!(children, 5);
}

#[test]
fn list_lays_out_like_an_equivalent_children_call() {
    let ctx = dummy_build_context(100.0, 100.0, None);

    let listed = Column::new()
        .list([20.0_f32, 30.0, 40.0])
        .builder(|height| Leaf(*height))
        .to_element(&ctx);
    let explicit = Column::new()
        .children([Leaf(20.0), Leaf(30.0), Leaf(40.0)])
        .to_element(&ctx);

    assert_eq!(listed.computed_size(&ctx), explicit.computed_size(&ctx));
}

#[test]
fn row_list_keeps_the_row_direction() {
    let ctx = dummy_build_context(100.0, 100.0, None);

    let row = Row::new()
        .list([20.0_f32, 30.0])
        .builder(|height| Leaf(*height))
        .to_element(&ctx);

    assert_eq!(row.debug_name(), "Row");
    // A row accumulates the cross axis as a maximum, not a sum.
    assert_eq!(row.computed_size(&ctx).height, 30.0);
    assert_eq!(row.computed_size(&ctx).width, 20.0);
}

/// The whole point of a declared extent: the container reports its scroll
/// extent without a single child being measured.
#[test]
fn item_extent_reports_the_total_without_measuring_children() {
    let counters = Counters::default();
    let ctx = dummy_build_context(400.0, VIEWPORT, Some((0.0, 0.0, 400.0, VIEWPORT)));

    let element = declared_column(&counters).to_element(&ctx);

    assert_eq!(
        element.computed_size(&ctx).height,
        100_000.0 * ROW_EXTENT,
        "a declared extent has to be exact, not estimated"
    );
    assert_eq!(counters.measured.get(), 0, "no child may be measured");
}

/// Painting resolves the viewport slice only, and still measures nothing.
#[test]
fn item_extent_paints_only_the_visible_slice() {
    let counters = Counters::default();
    let ctx = dummy_build_context(400.0, VIEWPORT, Some((0.0, 0.0, 400.0, VIEWPORT)));

    let element = declared_column(&counters).to_element(&ctx);
    element.draw(&ctx);

    // Three 200px rows fill a 600px viewport, plus the row that starts
    // exactly on its bottom edge.
    assert_eq!(counters.drawn.get(), 4);
    assert_eq!(
        counters.measured.get(),
        0,
        "painting must not measure either"
    );
}

/// The cold-start freeze: building the element tree must not create one
/// element per item.
#[test]
fn item_extent_builds_no_child_before_the_first_frame() {
    let counters = Counters::default();
    let ctx = dummy_build_context(400.0, VIEWPORT, Some((0.0, 0.0, 400.0, VIEWPORT)));

    let element = declared_column(&counters).to_element(&ctx);
    // Reporting the scroll extent is arithmetic, so it needs no rows either.
    element.computed_size(&ctx);

    assert_eq!(
        counters.built.get(),
        0,
        "built {} of 100 000 rows before the first frame",
        counters.built.get()
    );
}

/// The first frame materializes the viewport slice and nothing more.
#[test]
fn item_extent_builds_only_the_windowed_rows() {
    let counters = Counters::default();
    let ctx = dummy_build_context(400.0, VIEWPORT, Some((0.0, 0.0, 400.0, VIEWPORT)));

    let element = declared_column(&counters).to_element(&ctx);
    element.draw(&ctx);

    assert!(
        counters.built.get() <= VISIBLE_BUDGET,
        "built {} rows for a {VIEWPORT}px viewport",
        counters.built.get()
    );
}

/// Scrolling deep into the list must stay proportional to the viewport, both
/// in what is built and in what is painted.
#[test]
fn a_deep_scroll_builds_only_its_own_window() {
    let counters = Counters::default();
    let ctx = dummy_build_context(400.0, VIEWPORT, Some((0.0, 0.0, 400.0, VIEWPORT)));
    let element = declared_column(&counters).to_element(&ctx);
    element.draw(&ctx);

    let offset = 50_000.0 * ROW_EXTENT;
    let scrolled = dummy_build_context(400.0, VIEWPORT, Some((0.0, offset, 400.0, VIEWPORT)));
    counters.built.set(0);
    counters.drawn.set(0);
    element.draw(&scrolled);

    assert!(counters.drawn.get() > 0, "nothing was painted");
    assert!(
        counters.built.get() <= VISIBLE_BUDGET,
        "built {} rows after scrolling to row 50 000",
        counters.built.get()
    );
}

/// Sliding by a single row must not rebuild the rows that stayed on screen —
/// that is what keeps their state and layout caches alive.
#[test]
fn scrolling_one_row_rebuilds_one_row() {
    let counters = Counters::default();
    let ctx = dummy_build_context(400.0, VIEWPORT, Some((0.0, 0.0, 400.0, VIEWPORT)));
    let element = declared_column(&counters).to_element(&ctx);
    element.draw(&ctx);

    let nudged = dummy_build_context(
        400.0,
        VIEWPORT,
        Some((0.0, ROW_EXTENT, 400.0, VIEWPORT)),
    );
    counters.built.set(0);
    element.draw(&nudged);

    assert_eq!(
        counters.built.get(),
        1,
        "built {} rows for a one-row scroll",
        counters.built.get()
    );
}

/// The sparse half of the contract: a windowed container exposes the rows it
/// holds, not the rows it describes.
#[test]
fn a_windowed_container_visits_only_its_live_rows() {
    let counters = Counters::default();
    let ctx = dummy_build_context(400.0, VIEWPORT, Some((0.0, 0.0, 400.0, VIEWPORT)));
    let element = declared_column(&counters).to_element(&ctx);
    element.draw(&ctx);

    let mut visited = 0;
    element.visit_children(&mut |_| visited += 1);

    assert_eq!(visited, counters.built.get());
    assert!(visited <= VISIBLE_BUDGET);
}

/// A uniform list without a declared extent must derive its total from one
/// probed row. Predicting is what removes the requirement to declare an
/// extent for the shape a scrolled list almost always has.
#[test]
fn an_undeclared_list_predicts_its_total_from_one_probe() {
    let counters = Counters::default();
    let ctx = scrolled_context(0.0);

    let element = undeclared_column(&counters).to_element(&ctx);

    assert_eq!(
        element.computed_size(&ctx).height,
        100_000.0 * ROW_EXTENT,
        "a uniform list is exact even when its extent is predicted"
    );
    assert!(
        counters.measured.get() <= 2,
        "measured {} rows to predict one extent",
        counters.measured.get()
    );
    assert!(
        counters.built.get() <= VISIBLE_BUDGET,
        "built {} rows to predict one extent",
        counters.built.get()
    );
}

/// The prediction has to carry the whole lazy path, not just the extent: a
/// predicted list paints and builds a viewport's worth of rows.
#[test]
fn an_undeclared_list_paints_only_the_visible_slice() {
    let counters = Counters::default();
    let ctx = scrolled_context(0.0);

    let element = undeclared_column(&counters).to_element(&ctx);
    element.draw(&ctx);

    // Three 200px rows fill a 600px viewport, plus the row that starts
    // exactly on its bottom edge.
    assert_eq!(counters.drawn.get(), 4);
    assert!(
        counters.built.get() <= VISIBLE_BUDGET,
        "built {} rows for a {VIEWPORT}px viewport",
        counters.built.get()
    );
}

/// A row on screen that disagrees with the probe is recorded exactly, and the
/// rest of the list is left alone. Correcting in place is what keeps a list of
/// varying rows windowed instead of making one odd row cost a full measure.
#[test]
fn a_disagreeing_painted_row_corrects_the_table_in_place() {
    let counters = Counters::default();
    let ctx = scrolled_context(0.0);
    let element = varying_column(&counters, 1, 20).to_element(&ctx);
    let exact = 21.0 * ROW_EXTENT;

    element.draw(&ctx);

    assert_eq!(
        element.computed_size(&ctx).height,
        exact,
        "the correction has to land in the reported extent"
    );
    assert!(
        counters.built.get() <= VISIBLE_BUDGET,
        "built {} rows to correct one of them",
        counters.built.get()
    );
    let mut visited = 0;
    element.visit_children(&mut |_| visited += 1);
    assert!(visited < 20, "the list stopped being windowed");

    let built = counters.built.get();
    counters.measured.set(0);
    element.draw(&ctx);

    assert_eq!(
        element.computed_size(&ctx).height,
        exact,
        "a recorded row must not fall back to the prediction"
    );
    assert_eq!(
        counters.built.get(),
        built,
        "an unchanged frame must not rebuild its rows"
    );
    assert!(
        counters.measured.get() < 20,
        "re-measured {} rows on an unchanged frame",
        counters.measured.get()
    );
}

/// A prediction can only be corrected against rows that were painted, so a
/// taller row further down leaves the total predicted until it is reached.
/// That is the honest limit of predicting, and the reason
/// [`FlexList::item_extent`] still earns its place.
#[test]
fn an_undeclared_list_is_honest_only_about_the_rows_it_painted() {
    let counters = Counters::default();
    let ctx = scrolled_context(0.0);

    let element = varying_column(&counters, 15, 20).to_element(&ctx);
    element.draw(&ctx);

    assert_eq!(element.computed_size(&ctx).height, 20.0 * ROW_EXTENT);
}

/// Scrolling onto the row that disagrees has to correct the extent, and the
/// correction must survive scrolling away again — otherwise the scroll range
/// would oscillate for the rest of the session.
#[test]
fn a_scrolled_undeclared_list_converges_on_the_exact_extent() {
    let counters = Counters::default();
    let element = varying_column(&counters, 15, 20).to_element(&scrolled_context(0.0));
    let predicted = 20.0 * ROW_EXTENT;
    let exact = 21.0 * ROW_EXTENT;

    let top = scrolled_context(0.0);
    element.draw(&top);
    assert_eq!(element.computed_size(&top).height, predicted);

    let onto_row_15 = scrolled_context(14.0 * ROW_EXTENT);
    element.draw(&onto_row_15);

    assert_eq!(
        element.computed_size(&onto_row_15).height,
        exact,
        "reaching the taller row has to correct the extent"
    );
    assert!(
        counters.built.get() <= 2 * VISIBLE_BUDGET,
        "built {} rows across two viewports",
        counters.built.get()
    );

    element.draw(&top);

    assert_eq!(
        element.computed_size(&top).height,
        exact,
        "scrolling away must not discard what was already corrected"
    );
}

/// A bounded main axis means the container is not sizing itself for a scroll
/// view, so there is nothing to win by predicting: it measures every child
/// and reports the exact total.
#[test]
fn a_bounded_list_measures_every_child() {
    let counters = Counters::default();
    let ctx = dummy_build_context(400.0, VIEWPORT, Some((0.0, 0.0, 400.0, VIEWPORT)));

    let element = varying_column(&counters, 1, 20).to_element(&ctx);

    assert_eq!(element.computed_size(&ctx).height, 21.0 * ROW_EXTENT);
    assert_eq!(counters.built.get(), 20);
    let mut visited = 0;
    element.visit_children(&mut |_| visited += 1);
    assert_eq!(visited, 20);
}

/// Wrapping resolves line breaks from every preceding child, so it cannot be
/// windowed even when an extent is declared.
#[test]
fn a_wrapping_list_stays_eager() {
    let counters = Counters::default();
    let counted = counters.clone();
    let ctx = dummy_build_context(400.0, VIEWPORT, None);

    Column::new()
        .list(0..50u32)
        .overflow(OverflowBehavior::Wrap)
        .item_extent(Dimension::Px(ROW_EXTENT))
        .builder(move |_| Counting::new(&counted))
        .to_element(&ctx);

    assert_eq!(counters.built.get(), 50);
}

/// A declared extent has to place children exactly where the stride says,
/// whatever the children themselves would report.
#[test]
fn item_extent_overrides_the_measured_child_size() {
    let ctx = dummy_build_context(100.0, 100.0, None);

    let element = Column::new()
        .list([1_u8, 2, 3])
        .item_extent(Dimension::Px(50.0))
        .builder(|_| Leaf(20.0))
        .to_element(&ctx);

    assert_eq!(element.computed_size(&ctx).height, 150.0);
    // Nothing measured the children, so they span the cross axis.
    assert_eq!(element.computed_size(&ctx).width, 100.0);
}

/// A percentage extent cannot be resolved without a bounded main axis, so
/// the container falls back to measuring instead of reporting nonsense.
#[test]
fn unresolvable_item_extent_falls_back_to_measuring() {
    let mut ctx = dummy_build_context(100.0, 100.0, None);
    ctx.box_constraint.max_height = f32::MAX;

    let element = Column::new()
        .list([1_u8, 2])
        .item_extent(Dimension::Percent(50.0))
        .builder(|_| Leaf(20.0))
        .to_element(&ctx);

    assert_eq!(element.computed_size(&ctx).height, 40.0);
}

#[test]
fn empty_list_with_a_declared_extent_has_no_extent() {
    let ctx = dummy_build_context(100.0, 100.0, None);

    let element = Column::new()
        .list(Vec::<f32>::new())
        .item_extent(Dimension::Px(200.0))
        .builder(|height| Leaf(*height))
        .to_element(&ctx);

    assert_eq!(element.computed_size(&ctx), ResolvedSize::default());
}

#[test]
fn empty_list_is_a_valid_widget() {
    let ctx = dummy_build_context(100.0, 100.0, None);

    let column = Column::new()
        .list(Vec::<f32>::new())
        .builder(|height| Leaf(*height));

    assert!(column.is_empty());
    assert_eq!(column.len(), 0);
    assert_eq!(
        column.to_element(&ctx).computed_size(&ctx),
        ResolvedSize::default()
    );
}
