//! Cached main-axis layout table shared by `RawFlex::computed_size` and
//! `RawFlex::draw`.
//!
//! A flex container used to resolve every child's size and position twice per
//! frame — once while measuring and once while painting — which made both
//! passes `O(children)` even when a viewport exposed a dozen of them. The table
//! built here holds that result for as long as the incoming
//! [`BoxConstraint`] and scale stay the same, so painting a scrolled list only
//! has to look up the index range it actually covers.
//!
//! Two properties make the lookup cheap:
//!
//! - Main-axis starts are accumulated in `f64`. A list of 120 000 rows of 110px
//!   reaches 13.2 million logical pixels, where the `f32` step is already ~2px
//!   and deep-scroll positions visibly drift.
//! - A list whose children all resolved to the same size stores one size and a
//!   single stride instead of a full offset table, which turns the range lookup
//!   into a division and keeps the memory flat.
//!
//! A table whose extents were *predicted* from a single probe carries a third
//! part: a [`Refinement`] holding the exact extent of every child a frame
//! actually painted. Correcting the prediction one child at a time is what lets
//! a list of non-uniform rows stay windowed — the alternative is to measure all
//! of them, which is the cost the prediction exists to avoid.

use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use aimer_attribute::BoxConstraint;
use aimer_attribute::size::ResolvedSize;
use aimer_widget::base::BuildContext;

use crate::flex::LayoutDirection;
use crate::flex::children_source::ChildrenSource;
use crate::flex::flex_child::distribute_flex_space;

/// Marks a child that does not participate in flex distribution.
///
/// Flex weights are non-negative, so a negative slot is unambiguous and lets
/// one vector carry both "is a flex child" and "with which weight" without a
/// second allocation.
const NOT_FLEX: f32 = -1.0;

/// Where a table's extents came from.
///
/// The distinction decides whether the table may be trusted between frames:
/// only [`Origin::Estimated`] describes children that were never looked at, so
/// only it has to be verified against the ones a frame actually paints.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// Every child was measured.
    Measured,
    /// The caller stated one extent for every child.
    Declared,
    /// One child was measured and the rest are assumed to match it.
    Estimated,
}

/// Exact extents recorded for the children of a predicted table.
///
/// A prediction assumes every child matches one probe. When a child that was
/// painted turns out not to, its exact extent is recorded here instead of the
/// whole table being thrown away and the list measured: children that were
/// never looked at keep the assumed extent, so the container stays windowed and
/// — because the assumption never changes — the offset of a child already on
/// screen never moves. Only the children *after* a correction shift, which is
/// what has to happen when one of them grew.
///
/// Corrections live in a Fenwick tree, so a prefix sum and an update both cost
/// `O(log n)`. The tree is allocated on the first disagreement, so a uniform
/// list — the shape a long list almost always has — never pays for it.
struct Refinement {
    /// One-indexed Fenwick tree of `measured - assumed` main-axis deltas.
    tree: Vec<f64>,
    /// Exact size of every recorded child.
    sizes: HashMap<usize, ResolvedSize>,
    /// Sum of every recorded delta, which is the correction the total carries.
    total: f64,
    /// Largest cross extent recorded.
    cross_max: f32,
}

impl Refinement {
    /// Creates an empty correction table for `len` children.
    fn new(len: usize) -> Self {
        Self {
            tree: vec![0.0; len + 1],
            sizes: HashMap::new(),
            total: 0.0,
            cross_max: 0.0,
        }
    }

    /// Sum of the corrections recorded for every child before `index`.
    #[inline]
    fn prefix(&self, index: usize) -> f64 {
        let mut sum = 0.0;
        let mut node = index.min(self.tree.len() - 1);
        while node > 0 {
            sum += self.tree[node];
            node &= node - 1;
        }
        sum
    }

    /// Adds `delta` to the correction recorded for child `index`.
    fn add(&mut self, index: usize, delta: f64) {
        self.total += delta;
        let len = self.tree.len();
        let mut node = index + 1;
        while node < len {
            self.tree[node] += delta;
            node += node & node.wrapping_neg();
        }
    }
}

/// Resolved sizes and main-axis offsets of one flex container's children.
///
/// Construct it with [`FlexLayout::build`] and keep it for as long as the
/// constraint it was measured under is still current.
pub(crate) struct FlexLayout {
    /// Main-axis start of every child followed by the total main extent.
    ///
    /// Empty when [`FlexLayout::stride`] is `Some`, in which case offsets are
    /// derived arithmetically.
    offsets: Vec<f64>,
    /// Resolved size per child, or a single shared size for a uniform list.
    sizes: Vec<ResolvedSize>,
    /// Distance between the starts of two adjacent children — the child extent
    /// plus the gap — when every child resolved to the same size.
    stride: Option<f64>,
    /// Number of children the table describes.
    len: usize,
    /// Size of the container itself, gaps included.
    total: ResolvedSize,
    /// Whether any child was sized by flex distribution.
    has_flex: bool,
    /// How the extents in this table were obtained.
    origin: Origin,
    /// Whether the main axis is the horizontal one, which is what decides
    /// which component of a [`ResolvedSize`] a correction applies to.
    is_row: bool,
    /// Exact extents recorded for the painted children of a predicted table.
    ///
    /// `None` until a child disagrees with the prediction, which is the case a
    /// uniform list never reaches.
    refinement: RefCell<Option<Box<Refinement>>>,
}

impl FlexLayout {
    /// Measures `children` along `direction` and records their positions.
    ///
    /// Non-flex children keep the intrinsic size they report under an unbounded
    /// main axis; flex children are measured with their share of the leftover
    /// space, exactly as [`distribute_flex_space`] hands it out. `gap_main` is
    /// the resolved spacing inserted between two adjacent children.
    ///
    /// Measuring requires every child to exist, so a child the source has not
    /// materialized contributes nothing. Callers holding a windowed source must
    /// materialize the whole range first; see
    /// [`RawFlex::measure_layout`](crate::flex::raw_flex::RawFlex).
    pub(crate) fn build(
        direction: LayoutDirection,
        children: &dyn ChildrenSource,
        ctx: &BuildContext,
        gap_main: f32,
    ) -> Self {
        let len = children.len();
        let is_row = !matches!(direction, LayoutDirection::Column);
        let (max_main, max_cross) = if is_row {
            (ctx.box_constraint.max_width, ctx.box_constraint.max_height)
        } else {
            (ctx.box_constraint.max_height, ctx.box_constraint.max_width)
        };
        let total_gap = if len > 1 {
            gap_main * (len - 1) as f32
        } else {
            0.0
        };

        let mut child_ctx = ctx.clone();
        let mut sizes: Vec<ResolvedSize> = Vec::with_capacity(len);
        // Allocated on first sight of a flex child; a plain list never pays for
        // it.
        let mut weights: Vec<f32> = Vec::new();
        let mut sized_main: f32 = 0.0;

        for index in 0..len {
            let Some(child) = children.get(index) else {
                sizes.push(ResolvedSize::default());
                continue;
            };
            if let Some(flex) = child.flex() {
                if weights.is_empty() {
                    weights = vec![NOT_FLEX; len];
                }
                weights[index] = flex.max(0.0);
                sizes.push(ResolvedSize::default());
                continue;
            }

            set_main(&mut child_ctx.box_constraint, is_row, f32::MAX);
            set_cross(&mut child_ctx.box_constraint, is_row, max_cross);
            let size = child.computed_size(&child_ctx);
            sized_main += main_of(size, is_row);
            sizes.push(size);
        }

        if !weights.is_empty() {
            let shares = if max_main == f32::MAX {
                vec![f32::MAX; len]
            } else {
                distribute_flex_space((max_main - sized_main - total_gap).max(0.0), &weights)
            };

            for index in 0..len {
                if weights[index] < 0.0 {
                    continue;
                }
                let Some(child) = children.get(index) else {
                    continue;
                };
                set_main(&mut child_ctx.box_constraint, is_row, shares[index]);
                set_cross(&mut child_ctx.box_constraint, is_row, max_cross);
                let mut size = child.computed_size(&child_ctx);
                if weights[index] > 0.0 && shares[index] != f32::MAX {
                    set_main_of(&mut size, is_row, shares[index]);
                }
                sizes[index] = size;
            }
        }

        Self::from_sizes(sizes, is_row, gap_main, !weights.is_empty())
    }

    /// Builds the table for a list whose children all occupy `main` along the
    /// main axis, without measuring a single child.
    ///
    /// This is the constructor behind
    /// [`FlexList::item_extent`](crate::FlexList::item_extent). Because the
    /// extent is declared, the total is pure arithmetic — `stride * len - gap` —
    /// so a container reports its scroll extent in `O(1)` and only the children
    /// a viewport exposes are ever touched. `cross` is the extent handed to every
    /// child across the axis, which is the container's own cross-axis maximum.
    pub(crate) fn declared(
        len: usize,
        main: f32,
        cross: f32,
        is_row: bool,
        gap_main: f32,
    ) -> Self {
        Self::uniform(
            len,
            sized(main, cross, is_row),
            is_row,
            gap_main,
            Origin::Declared,
        )
    }

    /// Builds the table for a list whose children are *assumed* to occupy the
    /// size a single probed child reported.
    ///
    /// This is the prediction used when no extent was declared. It costs one
    /// measure instead of `len`, which is what removes the cold-start pass over
    /// a long list, and it is exact whenever the list really is uniform — the
    /// overwhelmingly common shape of a scrolled list.
    ///
    /// Unlike [`FlexLayout::declared`] the result is *not* authoritative: it has
    /// to be verified against the children a frame paints, and the container has
    /// to fall back to measuring when they disagree. See
    /// [`RawFlex::estimated_layout`](crate::flex::raw_flex::RawFlex).
    pub(crate) fn estimated(
        len: usize,
        probe: ResolvedSize,
        is_row: bool,
        gap_main: f32,
    ) -> Self {
        Self::uniform(len, probe, is_row, gap_main, Origin::Estimated)
    }

    /// Builds a table in which every child shares `size`.
    ///
    /// One stored size and one stride replace the offset table entirely, so the
    /// memory is flat and [`FlexLayout::visible_range`] is a division.
    fn uniform(
        len: usize,
        size: ResolvedSize,
        is_row: bool,
        gap_main: f32,
        origin: Origin,
    ) -> Self {
        if len == 0 {
            return Self::from_sizes(Vec::new(), is_row, gap_main, false);
        }

        let cross = cross_of(size, is_row);
        let stride = main_of(size, is_row) as f64 + gap_main as f64;
        let main_total = stride * len as f64 - gap_main as f64;

        Self {
            offsets: Vec::new(),
            sizes: vec![size],
            stride: Some(stride),
            len,
            total: sized(main_total as f32, cross, is_row),
            has_flex: false,
            origin,
            is_row,
            refinement: RefCell::new(None),
        }
    }

    /// Turns per-child sizes into offsets, a total, and — when possible — a
    /// stride.
    fn from_sizes(
        mut sizes: Vec<ResolvedSize>,
        is_row: bool,
        gap_main: f32,
        has_flex: bool,
    ) -> Self {
        let len = sizes.len();
        let mut cross_max: f32 = 0.0;
        let mut uniform = len > 0;
        for size in &sizes {
            cross_max = cross_max.max(cross_of(*size, is_row));
            uniform &= *size == sizes[0];
        }

        if uniform {
            let extent = main_of(sizes[0], is_row) as f64;
            let stride = extent + gap_main as f64;
            let main_total = stride * len as f64 - gap_main as f64;
            sizes.truncate(1);
            sizes.shrink_to_fit();
            return Self {
                offsets: Vec::new(),
                sizes,
                stride: Some(stride),
                len,
                total: sized(main_total as f32, cross_max, is_row),
                has_flex,
                origin: Origin::Measured,
                is_row,
                refinement: RefCell::new(None),
            };
        }

        let mut offsets = Vec::with_capacity(len + 1);
        let mut main_total: f64 = 0.0;
        for (index, size) in sizes.iter().enumerate() {
            offsets.push(main_total);
            main_total += main_of(*size, is_row) as f64;
            if index + 1 < len {
                main_total += gap_main as f64;
            }
        }
        offsets.push(main_total);

        Self {
            offsets,
            sizes,
            stride: None,
            len,
            total: sized(main_total as f32, cross_max, is_row),
            has_flex,
            origin: Origin::Measured,
            is_row,
            refinement: RefCell::new(None),
        }
    }

    /// Whether a child was sized by flex distribution.
    ///
    /// Such a table cannot be revalidated one child at a time, because every
    /// share depends on what the others consumed.
    #[inline]
    pub(crate) fn has_flex(&self) -> bool {
        self.has_flex
    }

    /// Whether the table was derived from a declared item extent.
    ///
    /// Such a table is authoritative: the caller stated the extent, so it is
    /// never revalidated against the children — which is exactly what keeps a
    /// long list off the measuring path.
    #[inline]
    pub(crate) fn is_declared(&self) -> bool {
        self.origin == Origin::Declared
    }

    /// Whether the table was extrapolated from a single probed child.
    ///
    /// Such a table is a prediction: it holds only for as long as the children a
    /// frame paints agree with it.
    #[inline]
    pub(crate) fn is_estimated(&self) -> bool {
        self.origin == Origin::Estimated
    }

    /// Whether the table states what the children measured, and so stops
    /// describing them once the element tree below the container changes.
    ///
    /// A [`Origin::Declared`] extent comes from the caller and holds whatever
    /// the children turn out to be. A [`Origin::Estimated`] one is a prediction
    /// that every painted frame re-verifies through
    /// [`refine`](FlexLayout::refine), so it repairs itself and must *not* be
    /// thrown away — the corrections it accumulated are the only record of the
    /// rows a scrolled list has already resolved. Only a measured table has to
    /// be taken at its word, which is exactly why it has to expire.
    #[inline]
    pub(crate) fn describes_measured_children(&self) -> bool {
        self.origin == Origin::Measured
    }

    /// Number of children described by this table.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Size of the flex container, gaps included.
    ///
    /// A predicted table adds the corrections recorded so far, so the size grows
    /// or shrinks as painted children turn out to disagree with the prediction.
    #[inline]
    pub(crate) fn total(&self) -> ResolvedSize {
        let slot = self.refinement.borrow();
        let Some(refinement) = slot.as_ref() else {
            return self.total;
        };

        let mut total = self.total;
        let main = main_of(total, self.is_row) as f64 + refinement.total;
        set_main_of(&mut total, self.is_row, main as f32);
        if refinement.cross_max > cross_of(total, self.is_row) {
            set_cross_of(&mut total, self.is_row, refinement.cross_max);
        }
        total
    }

    /// Resolved size of child `index`.
    ///
    /// A child whose exact size was recorded through [`FlexLayout::refine`]
    /// reports that size; every other child of a predicted table reports the
    /// probe.
    ///
    /// # Panics
    ///
    /// Panics when `index` is out of bounds.
    #[inline]
    pub(crate) fn size(&self, index: usize) -> ResolvedSize {
        if let Some(refinement) = self.refinement.borrow().as_ref()
            && let Some(size) = refinement.sizes.get(&index)
        {
            return *size;
        }
        if self.stride.is_some() {
            self.sizes[0]
        } else {
            self.sizes[index]
        }
    }

    /// Main-axis start of child `index`, relative to the container's content
    /// box.
    #[inline]
    pub(crate) fn offset(&self, index: usize) -> f64 {
        match self.stride {
            Some(stride) => {
                let base = stride * index as f64;
                match self.refinement.borrow().as_ref() {
                    Some(refinement) => base + refinement.prefix(index),
                    None => base,
                }
            }
            None => self.offsets[index],
        }
    }

    /// Records the exact size `size` of child `index`.
    ///
    /// This is how a prediction is corrected without giving up on windowing:
    /// the children that were never looked at keep the probed extent, so the
    /// offsets of `index` and everything before it are untouched and nothing
    /// already on screen moves. The container's total shifts by the difference,
    /// which is what a scroll view sees as its extent converging on the truth.
    ///
    /// Returns `false`, changing nothing, for a table that is not a prediction:
    /// a declared extent is authoritative and a measured one is exact already.
    pub(crate) fn refine(&self, index: usize, size: ResolvedSize) -> bool {
        if self.origin != Origin::Estimated || index >= self.len {
            return false;
        }

        let assumed = main_of(self.sizes[0], self.is_row) as f64;
        let mut slot = self.refinement.borrow_mut();
        let refinement = slot.get_or_insert_with(|| Box::new(Refinement::new(self.len)));
        let previous = refinement
            .sizes
            .insert(index, size)
            .map_or(assumed, |old| main_of(old, self.is_row) as f64);
        let delta = main_of(size, self.is_row) as f64 - previous;
        if delta != 0.0 {
            refinement.add(index, delta);
        }
        refinement.cross_max = refinement.cross_max.max(cross_of(size, self.is_row));
        true
    }

    /// Index of the first child whose main-axis start no longer satisfies
    /// `keep`.
    ///
    /// Offsets are non-decreasing, so this is a plain binary search. It costs
    /// `O(log n)` lookups, each of which is itself `O(log n)` once corrections
    /// were recorded.
    fn partition(&self, keep: impl Fn(f64) -> bool) -> usize {
        let (mut low, mut high) = (0usize, self.len);
        while low < high {
            let middle = low + (high - low) / 2;
            if keep(self.offset(middle)) {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        low
    }

    /// Returns the half-open index range whose children intersect the main-axis
    /// span `[start, end)`.
    ///
    /// Resolution is `O(1)` for a uniform list and `O(log n)` otherwise. The
    /// range may include one child on each side that only touches the span.
    pub(crate) fn visible_range(&self, start: f64, end: f64) -> Range<usize> {
        if self.len == 0 || end < start {
            return 0..0;
        }

        if let Some(stride) = self.stride {
            if self.refinement.borrow().is_some() {
                // Corrections broke the uniform spacing, so the range has to be
                // searched for. The bounds match the arithmetic below: a child
                // starting exactly on the far edge of the span still counts.
                let first = self.partition(|offset| offset <= start).saturating_sub(1);
                let last = self.partition(|offset| offset <= end);
                return first.min(self.len)..last.min(self.len);
            }
            if stride <= 0.0 {
                // Zero-extent children all sit on the same offset, so no span
                // can narrow the list.
                return 0..self.len;
            }
            let first = (start / stride).floor().max(0.0) as usize;
            if first >= self.len {
                return self.len..self.len;
            }
            let last = ((end / stride).floor() as usize).saturating_add(1);
            return first..last.min(self.len);
        }

        // `offsets` is non-decreasing, so the last child starting at or before
        // `start` is the one covering it.
        let first = self
            .offsets
            .partition_point(|offset| *offset <= start)
            .saturating_sub(1);
        let last = self.offsets.partition_point(|offset| *offset < end);
        first.min(self.len)..last.min(self.len)
    }
}

/// A [`FlexLayout`] together with everything that decides whether it still
/// describes the container.
struct CachedTable {
    constraint: BoxConstraint,
    scale_bits: u32,
    generation: u64,
    layout: Rc<FlexLayout>,
}

/// Holds one flex container's [`FlexLayout`] between frames.
///
/// The table is keyed by the constraint and scale it was measured under, so a
/// scroll — which only changes the visible rectangle — reuses it instead of
/// re-measuring the child list. The last painted index range is kept alongside
/// it so hit testing can skip the children that were never painted.
///
/// A table that measured its children also carries the [element-tree
/// generation](aimer_widget::element_tree_generation) it was built in, for the
/// same reason [`LayoutCache`](aimer_widget::LayoutCache) does: it states what
/// the children were, so replacing a generated subtree below this container — a
/// `setState`, or an `AsyncBuilder` swapping its loading state for the data it
/// waited on — retires it. A container that itself never rebuilt would
/// otherwise keep reporting the extent its content had before the swap, and a
/// scroll view above it would never learn that there is something to scroll.
///
/// A declared or predicted table is kept across generations, see
/// [`FlexLayout::describes_measured_children`].
pub(crate) struct FlexLayoutCache {
    table: UnsafeCell<Option<CachedTable>>,
    painted: Cell<Option<(usize, usize)>>,
}

impl FlexLayoutCache {
    /// Creates an empty cache.
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            table: UnsafeCell::new(None),
            painted: Cell::new(None),
        }
    }

    /// Returns the cached table when it was measured under the same inputs and
    /// the element tree still holds the children it was measured from.
    #[inline]
    pub(crate) fn get(&self, constraint: BoxConstraint, scale_bits: u32) -> Option<Rc<FlexLayout>> {
        // Only ever read through this exclusive reference, and never while `set`
        // holds its own; the element tree is single-threaded.
        let slot = unsafe { &*self.table.get() };
        let cached = slot.as_ref()?;
        if cached.constraint != constraint || cached.scale_bits != scale_bits {
            return None;
        }
        if cached.generation != aimer_widget::element_tree_generation()
            && cached.layout.describes_measured_children()
        {
            return None;
        }
        Some(Rc::clone(&cached.layout))
    }

    /// Stores `layout` as the table for `constraint` and `scale_bits`.
    #[inline]
    pub(crate) fn set(&self, constraint: BoxConstraint, scale_bits: u32, layout: Rc<FlexLayout>) {
        let slot = unsafe { &mut *self.table.get() };
        *slot = Some(CachedTable {
            constraint,
            scale_bits,
            generation: aimer_widget::element_tree_generation(),
            layout,
        });
    }

    /// Takes over the table and painted range of the cache this one replaces.
    ///
    /// Everything a predicted table learned by painting — the exact extent of
    /// every row a frame corrected — lives in the table. A rebuild of the
    /// container would otherwise start from the prediction again, which a scroll
    /// view sees as its content suddenly changing size.
    ///
    /// The table keeps the generation it was measured in, so the replacement
    /// inherits exactly as much of it as [`FlexLayoutCache::get`] still trusts:
    /// a prediction or a declared extent, which never described a particular
    /// child, comes across whole, while extents measured from rows that were
    /// built anew are retired by the very rebuild that carried them. That is
    /// what a `Column` an `AsyncBuilder` rebuilds needs — same direction, same
    /// count, completely different children.
    ///
    /// Only sound for a container describing the same children under the same
    /// rules, which the caller establishes. Nothing already cached here is
    /// overwritten.
    pub(crate) fn adopt(&self, old: &Self) {
        let slot = unsafe { &mut *self.table.get() };
        if slot.is_none() {
            // Taken rather than cloned: the cache it came from is dropped with
            // the container being replaced.
            *slot = unsafe { (*old.table.get()).take() };
        }
        if self.painted.get().is_none() {
            self.painted.set(old.painted.get());
        }
    }

    /// Drops the table and the painted range.
    #[inline]
    pub(crate) fn invalidate(&self) {
        unsafe {
            *self.table.get() = None;
        }
        self.painted.set(None);
    }

    /// Records the index range painted by the most recent frame.
    #[inline]
    pub(crate) fn set_painted(&self, range: &Range<usize>) {
        self.painted.set(Some((range.start, range.end)));
    }

    /// Returns the index range painted by the most recent frame.
    #[inline]
    pub(crate) fn painted(&self) -> Option<Range<usize>> {
        self.painted.get().map(|(start, end)| start..end)
    }
}

impl Default for FlexLayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads the main-axis extent of `size`.
#[inline]
fn main_of(size: ResolvedSize, is_row: bool) -> f32 {
    if is_row { size.width } else { size.height }
}

/// Reads the cross-axis extent of `size`.
#[inline]
fn cross_of(size: ResolvedSize, is_row: bool) -> f32 {
    if is_row { size.height } else { size.width }
}

/// Overwrites the main-axis extent of `size`.
#[inline]
fn set_main_of(size: &mut ResolvedSize, is_row: bool, value: f32) {
    if is_row {
        size.width = value;
    } else {
        size.height = value;
    }
}

/// Overwrites the cross-axis extent of `size`.
#[inline]
fn set_cross_of(size: &mut ResolvedSize, is_row: bool, value: f32) {
    if is_row {
        size.height = value;
    } else {
        size.width = value;
    }
}

/// Builds a size from main- and cross-axis extents.
#[inline]
fn sized(main: f32, cross: f32, is_row: bool) -> ResolvedSize {
    if is_row {
        ResolvedSize {
            width: main,
            height: cross,
        }
    } else {
        ResolvedSize {
            width: cross,
            height: main,
        }
    }
}

/// Replaces the main-axis maximum of `constraint`.
#[inline]
fn set_main(constraint: &mut BoxConstraint, is_row: bool, value: f32) {
    if is_row {
        constraint.max_width = value;
    } else {
        constraint.max_height = value;
    }
}

/// Replaces the cross-axis maximum of `constraint`.
#[inline]
fn set_cross(constraint: &mut BoxConstraint, is_row: bool, value: f32) {
    if is_row {
        constraint.max_height = value;
    } else {
        constraint.max_width = value;
    }
}

#[cfg(test)]
#[path = "flex_layout_tests.rs"]
mod tests;
