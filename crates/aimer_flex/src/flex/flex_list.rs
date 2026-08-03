//! Data-driven children for the flex containers.
//!
//! [`Column::children`](crate::Column::children) takes a collection of widgets,
//! which forces the caller to materialize one widget per item — usually through
//! a `Vec<AnyWidget>` and a `.boxed()` per element:
//!
//! ```ignore
//! let items: Vec<AnyWidget> = (0..120_000)
//!     .map(|i| Container::new().child(Text::new(format!("Item {i}"))).boxed())
//!     .collect();
//! Column::new().children(items)
//! ```
//!
//! [`Column::list`](crate::Column::list) keeps the *data* instead and maps it to
//! children only while the element tree is built:
//!
//! ```ignore
//! Column::new()
//!     .horizontal_alignment(BoxAlignment::Start)
//!     .list(0..120_000)
//!     .builder(|i| Container::new().child(Text::new(format!("Item {i}"))))
//! ```
//!
//! Two costs disappear. The mapper's return type is one concrete widget, so no
//! item is type-erased, and the retained widget tree holds `Vec<T>` — four bytes
//! per item for a range of `u32` — instead of a vector of boxed containers, each
//! carrying decoration, borders, and an owned string.
//!
//! The data source is a `Vec<T>` rather than an
//! [`Iterator`](core::iter::Iterator) on purpose: [`Widget::to_element`] takes
//! `&self` and runs again on every rebuild, so the source has to be replayable
//! and indexable. `IntoIterator` is accepted at the call boundary, which keeps
//! ranges, arrays, vectors, and `map` chains ergonomic.

use std::rc::Rc;

use aimer_attribute::{CacheBounds, Dimension};
use aimer_style::LayoutSpacing;
use aimer_widget::base::BuildContext;
use aimer_widget::{AnyElement, Element, Key, Widget};

use crate::flex::children_source::{ChildrenSource, EagerChildren, KeyMapper, WindowedChildren};
use crate::flex::raw_flex::RawFlex;
use crate::flex::{BoxAlignment, LayoutDirection, OverflowBehavior};

/// Debug name reported by the element a list produces.
#[inline]
fn debug_name_of(direction: LayoutDirection) -> &'static str {
    match direction {
        LayoutDirection::Column => "Column",
        LayoutDirection::Row => "Row",
        LayoutDirection::Inherit => "Flex",
    }
}

/// A flex container that has a data source but no mapper yet.
///
/// This is an intermediate builder produced by
/// [`Column::list`](crate::Column::list), [`Row::list`](crate::Row::list), or
/// [`Flex::list`](crate::Flex::list). It is not a [`Widget`]; complete it with
/// [`FlexList::builder`].
pub struct FlexList<T> {
    direction: LayoutDirection,
    vertical_alignment: BoxAlignment,
    horizontal_alignment: BoxAlignment,
    gaps: LayoutSpacing,
    overflow: OverflowBehavior,
    item_extent: Option<Dimension>,
    keyed: Option<KeyMapper<T>>,
    items: Vec<T>,
}

impl<T> FlexList<T> {
    /// Creates a list source for `direction`, collecting `items` once.
    #[doc(hidden)]
    #[inline]
    pub fn new(
        direction: LayoutDirection,
        vertical_alignment: BoxAlignment,
        horizontal_alignment: BoxAlignment,
        gaps: LayoutSpacing,
        overflow: OverflowBehavior,
        items: impl IntoIterator<Item = T>,
    ) -> Self {
        Self {
            direction,
            vertical_alignment,
            horizontal_alignment,
            gaps,
            overflow,
            item_extent: None,
            keyed: None,
            items: items.into_iter().collect(),
        }
    }

    /// Derives a stable identity for every datum.
    ///
    /// Rows are materialized on demand and rebuilt whenever anything above the
    /// container rebuilds, so a row's live state — a ticked checkbox, a caret, a
    /// hover flag — is carried from the old row into the new one. Without a key
    /// the two are matched by position, which is exactly wrong once the data is
    /// inserted into, removed from, or reordered: state stays on index 3 while the
    /// datum that owned it moves to index 4.
    ///
    /// Supply a key derived from whatever identifies the datum — a database id, a
    /// path, a name — and the state follows the datum instead. Keys must be
    /// unique within one list; duplicates make the first match win.
    ///
    /// # Example
    ///
    /// ```rust
    /// use aimer_container::SizedBox;
    /// use aimer_flex::Column;
    /// use aimer_widget::Key;
    ///
    /// let column = Column::new()
    ///     .list([7u32, 4, 9])
    ///     .key(|id| Key::Value(id.to_string()))
    ///     .builder(|_| SizedBox::new().height(40.0));
    /// ```
    #[inline]
    pub fn key(mut self, key: impl Fn(&T) -> Key + 'static) -> Self {
        self.keyed = Some(Rc::new(key));
        self
    }

    /// Declares the main-axis extent every child occupies.
    ///
    /// The container needs its total extent before anything can be painted,
    /// because that is what a scroll view derives its scroll range from.
    /// Declaring it replaces the measuring pass with arithmetic: the total is
    /// `(extent + gap) * len - gap`, so a list of a hundred thousand rows
    /// resolves its extent in constant time and the first frame only touches the
    /// children it paints.
    ///
    /// Leaving it unset is not the slow path it used to be: inside a scroll view
    /// the container predicts the same table from a single probed row, which is
    /// exact for a uniform list, and records the exact extent of every row it
    /// paints, so a list whose rows *do* vary converges on its true extent as it
    /// is scrolled. Declaring the extent is still the stronger statement — it is
    /// exact from the very first frame, and it is never verified, so the total
    /// never moves under a scroll bar.
    ///
    /// The extent is the child's *outer* main-axis size, margins included, and
    /// resolves like every other [`Dimension`]: `Px` is scaled by the device
    /// scale factor, `Percent` is taken from the container's main-axis maximum.
    /// Children are laid out across the container's full cross-axis maximum,
    /// since nothing measured them.
    ///
    /// A child that ends up a different size is *not* re-measured — the
    /// declaration wins, and the child is clipped or leaves a gap. Leave the
    /// extent unset for genuinely variable rows, and do not combine it with
    /// [`Expanded`](crate::Expanded) children, whose size is decided by
    /// distribution rather than declaration.
    ///
    /// # Example
    ///
    /// ```rust
    /// use aimer_attribute::Dimension;
    /// use aimer_container::SizedBox;
    /// use aimer_flex::Column;
    ///
    /// let column = Column::new()
    ///     .list(0..120_000)
    ///     .item_extent(Dimension::Px(200.0))
    ///     .builder(|_| SizedBox::new().height(200.0));
    /// ```
    #[inline]
    pub fn item_extent(mut self, extent: impl Into<Dimension>) -> Self {
        self.item_extent = Some(extent.into());
        self
    }

    /// Sets alignment on the physical vertical axis.
    ///
    /// The default is inherited from the container the list came from.
    #[inline]
    pub fn vertical_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.vertical_alignment = alignment;
        self
    }

    /// Sets alignment on the physical horizontal axis.
    #[inline]
    pub fn horizontal_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.horizontal_alignment = alignment;
        self
    }

    /// Sets the spacing inserted between adjacent children.
    #[inline]
    pub fn gaps(mut self, gaps: impl Into<LayoutSpacing>) -> Self {
        self.gaps = gaps.into();
        self
    }

    /// Sets how children exceeding the available constraints are handled.
    #[inline]
    pub fn overflow(mut self, overflow: OverflowBehavior) -> Self {
        self.overflow = overflow;
        self
    }

    /// Maps every datum to a child and completes the builder.
    ///
    /// The mapper takes `&T` so it works with data that is neither `Copy` nor
    /// cheap to clone, and it is `Fn` because [`Widget::to_element`] receives
    /// `&self` and runs again on every rebuild.
    ///
    /// # Example
    ///
    /// ```rust
    /// use aimer_container::SizedBox;
    /// use aimer_flex::Column;
    ///
    /// let column = Column::new()
    ///     .list([40, 60, 80])
    ///     .builder(|width| SizedBox::new().width(*width));
    /// ```
    #[inline]
    pub fn builder<W, F>(self, builder: F) -> ListFlex<T, F>
    where
        W: Widget + 'static,
        F: Fn(&T) -> W + 'static,
    {
        ListFlex {
            direction: self.direction,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow: self.overflow,
            item_extent: self.item_extent,
            keyed: self.keyed,
            items: Rc::new(self.items),
            builder: Rc::new(builder),
        }
    }
}

/// A flex container whose children are built from data on demand.
///
/// Produced by [`FlexList::builder`] and immediately a valid [`Widget`]. No
/// intermediate collection of widgets is ever retained.
///
/// The mapper runs only for the rows a frame needs, as long as the container can
/// state its total main-axis size without walking the list. That holds when
/// [`FlexList::item_extent`] declares the extent, and — inside a scroll view,
/// where the main axis is unbounded — when the extent can be predicted from a
/// single probed row. The prediction is verified against every row that is
/// painted, and a row that disagrees has its exact extent recorded: rows already
/// on screen keep the position they were painted at, and the reported total moves
/// by the difference. A genuinely variable list therefore stays windowed and its
/// scroll extent converges as it is scrolled; declaring the extent is what makes
/// the total exact from the first frame instead.
///
/// Per-row state survives what a caller expects it to survive: a scroll that
/// takes the row off screen, and a rebuild of anything above the container. Both
/// rely on the row being recognised again, so supply [`FlexList::key`] whenever
/// the data can be inserted into, removed from, or reordered — index identity is
/// only correct for a list that never changes shape. Read the
/// [sparse-children contract](crate::flex::children_source) for the limits: a row
/// scrolled a long way out of view is eventually dropped, and one that has no
/// element cannot receive a broadcast.
pub struct ListFlex<T, F> {
    direction: LayoutDirection,
    vertical_alignment: BoxAlignment,
    horizontal_alignment: BoxAlignment,
    gaps: LayoutSpacing,
    overflow: OverflowBehavior,
    item_extent: Option<Dimension>,
    /// Identity of each datum, when the caller supplied one — see
    /// [`FlexList::key`].
    keyed: Option<KeyMapper<T>>,
    /// Shared so [`Widget::to_element`], which only borrows `self`, can hand the
    /// data to a source that outlives the call.
    items: Rc<Vec<T>>,
    builder: Rc<F>,
}

impl<T, F> ListFlex<T, F> {
    /// Declares the main-axis extent every child occupies.
    ///
    /// See [`FlexList::item_extent`]; declaring it after the mapper is
    /// equivalent to declaring it before.
    #[inline]
    pub fn item_extent(mut self, extent: impl Into<Dimension>) -> Self {
        self.item_extent = Some(extent.into());
        self
    }

    /// Derives a stable identity for every datum.
    ///
    /// See [`FlexList::key`]; declaring it after the mapper is equivalent to
    /// declaring it before.
    #[inline]
    pub fn key(mut self, key: impl Fn(&T) -> Key + 'static) -> Self {
        self.keyed = Some(Rc::new(key));
        self
    }

    /// Number of children this container will produce.
    #[inline]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the data source is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T, W, F> ListFlex<T, F>
where
    T: 'static,
    W: Widget + 'static,
    F: Fn(&T) -> W + 'static,
{
    /// Picks the child storage that matches what the container can know.
    ///
    /// Children are materialized a viewport at a time whenever the container can
    /// state its total main-axis size without walking the list: either from a
    /// declared [`FlexList::item_extent`], or by predicting it from a single
    /// probed row. A source that turns out to be wrong about the prediction
    /// materializes everything and stays that way, so nothing is built twice.
    ///
    /// [`OverflowBehavior::Wrap`] is the one exception: a line break depends on
    /// every preceding child, so wrapping is eager by construction and never has
    /// a window to speak of.
    fn children_source(&self, ctx: &BuildContext) -> Box<dyn ChildrenSource> {
        if self.overflow != OverflowBehavior::Wrap {
            return Box::new(WindowedChildren::new(
                Rc::clone(&self.items),
                Rc::clone(&self.builder),
                self.keyed.clone(),
            ));
        }

        Box::new(EagerChildren(
            self.items
                .iter()
                .map(|item| (self.builder)(item).to_element(ctx))
                .collect(),
        ))
    }
}

impl<T, W, F> Widget for ListFlex<T, F>
where
    T: 'static,
    W: Widget + 'static,
    F: Fn(&T) -> W + 'static,
{
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        RawFlex {
            direction: self.direction,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow_behavior: self.overflow,
            children: self.children_source(ctx),
            cache: Default::default(),
            layout: Default::default(),
            item_extent: self.item_extent,
            debug_name: debug_name_of(self.direction),
            cache_bound: CacheBounds::new(),
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        debug_name_of(self.direction)
    }
}

#[cfg(test)]
#[path = "flex_list_tests.rs"]
mod tests;
