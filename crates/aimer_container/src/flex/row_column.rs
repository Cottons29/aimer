use aimer_attribute::CacheBounds;
use aimer_style::LayoutSpacing;
use aimer_widget::base::BuildContext;
use aimer_widget::{AnyElement, AnyWidget, Element, RequiredChild, Widget};

use crate::flex::raw_flex::RawFlex;
use crate::flex::{BoxAlignment, LayoutDirection, OverflowBehavior};

/// A flex container that arranges a homogeneous collection vertically.
///
/// Children run from top to bottom. Vertical alignment controls the main axis,
/// horizontal alignment controls the cross axis, and overflow defaults to
/// clipping. `Column::new()` is not a valid [`Widget`] until
/// [`Column::children`] supplies the terminal child collection.
///
/// # Example
///
/// ```rust
/// use aimer_container::SizedBox;
/// use aimer_container::flex::{Column, Row};
///
/// let column = Column::new().children([
///     Row::new().children([SizedBox::new().width(40), SizedBox::new().width(60)]),
///     Row::new().children([SizedBox::new().width(100), SizedBox::new().width(20)]),
/// ]);
/// ```
pub struct Column<W = RequiredChild> {
    vertical_alignment: BoxAlignment,
    horizontal_alignment: BoxAlignment,
    gaps: LayoutSpacing,
    overflow: OverflowBehavior,
    children: Vec<W>,
}

impl Default for Column {
    fn default() -> Self {
        Self::new()
    }
}

impl Column {
    /// Creates an empty column builder.
    ///
    /// Both alignments default to [`BoxAlignment::Start`], gaps to zero, and
    /// overflow to [`OverflowBehavior::Hidden`]. Finish with
    /// [`Column::children`] to obtain a valid [`Widget`].
    pub fn new() -> Self {
        Self {
            vertical_alignment: Default::default(),
            horizontal_alignment: Default::default(),
            gaps: Default::default(),
            overflow: Default::default(),
            children: Default::default(),
        }
    }

    /// Sets main-axis alignment for the column's children.
    ///
    /// The default is [`BoxAlignment::Start`].
    pub fn vertical_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.vertical_alignment = alignment;
        self
    }

    /// Sets horizontal cross-axis alignment for the column's children.
    ///
    /// The default is [`BoxAlignment::Start`].
    pub fn horizontal_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.horizontal_alignment = alignment;
        self
    }

    /// Sets logical-pixel spacing between adjacent children.
    ///
    /// The default is zero. For a column, the top and bottom components of the
    /// converted [`LayoutSpacing`] determine the inter-child gap.
    pub fn gaps(mut self, gaps: impl Into<LayoutSpacing>) -> Self {
        self.gaps = gaps.into();
        self
    }

    /// Sets clipping or wrapping behavior when children exceed constraints.
    ///
    /// The default is [`OverflowBehavior::Hidden`]; use
    /// [`OverflowBehavior::Visible`] to paint beyond the bounds or
    /// [`OverflowBehavior::Wrap`] to continue in additional columns.
    pub fn overflow(mut self, overflow: OverflowBehavior) -> Self {
        self.overflow = overflow;
        self
    }

    /// Replaces the children and returns an erased vertical layout.
    ///
    /// This is equivalent to [`Column::children`] followed by [`Widget::boxed`].
    /// Use it when different branches need to return one [`AnyWidget`] type.
    pub fn box_children<W: Widget + 'static>(
        self,
        children: impl IntoIterator<Item = W>,
    ) -> AnyWidget {
        Column {
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow: self.overflow,
            children: children.into_iter().collect(),
        }
        .boxed()
    }

    /// Replaces the child collection and completes this builder.
    ///
    /// All iterator items have one concrete widget type. This terminal operation
    /// returns a valid [`Column`], including for an empty iterator.
    pub fn children<W: Widget>(self, children: impl IntoIterator<Item = W>) -> Column<W> {
        Column {
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow: self.overflow,
            children: children.into_iter().collect(),
        }
    }
}

impl<W: Widget + 'static> Widget for Column<W> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let children = self
            .children
            .iter()
            .map(|c| c.to_element(ctx))
            .collect();
        RawFlex {
            direction: LayoutDirection::Column,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow_behavior: self.overflow,
            children,
            cache: Default::default(),
            debug_name: "Column",
            cache_bound: CacheBounds::new(),
        }
        .boxed()
    }
}

/// A flex container that arranges its children horizontally.
///
/// Children run from left to right. Horizontal alignment controls the main
/// axis, vertical alignment controls the cross axis, and overflow defaults to
/// clipping. Unlike [`Column`], an empty `Row::new()` is already a valid erased
/// widget and supports incremental insertion.
pub struct Row<W: Widget + 'static = AnyWidget> {
    vertical_alignment: BoxAlignment,
    horizontal_alignment: BoxAlignment,
    gaps: LayoutSpacing,
    overflow: OverflowBehavior,
    children: Vec<W>,
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

impl Row {
    /// Creates an empty row.
    ///
    /// Both alignments default to [`BoxAlignment::Start`], gaps to zero, and
    /// overflow to [`OverflowBehavior::Hidden`].
    pub fn new() -> Self {
        Self {
            vertical_alignment: Default::default(),
            horizontal_alignment: Default::default(),
            gaps: Default::default(),
            overflow: Default::default(),
            children: Default::default(),
        }
    }

    /// Sets vertical cross-axis alignment for the row's children.
    ///
    /// The default is [`BoxAlignment::Start`].
    pub fn vertical_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.vertical_alignment = alignment;
        self
    }

    /// Sets main-axis alignment for the row's children.
    ///
    /// The default is [`BoxAlignment::Start`].
    pub fn horizontal_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.horizontal_alignment = alignment;
        self
    }

    /// Sets logical-pixel spacing between adjacent children.
    ///
    /// The default is zero. For a row, the left and right components of the
    /// converted [`LayoutSpacing`] determine the inter-child gap.
    pub fn gaps(mut self, gaps: impl Into<LayoutSpacing>) -> Self {
        self.gaps = gaps.into();
        self
    }

    /// Sets clipping or wrapping behavior when children exceed constraints.
    ///
    /// The default is [`OverflowBehavior::Hidden`]; use
    /// [`OverflowBehavior::Visible`] to paint beyond the bounds or
    /// [`OverflowBehavior::Wrap`] to continue in additional rows.
    pub fn overflow(mut self, overflow: OverflowBehavior) -> Self {
        self.overflow = overflow;
        self
    }

    /// Replaces all children with a homogeneous collection.
    ///
    /// This is not an append operation. The returned row adopts the iterator's
    /// concrete item type and remains valid when the iterator is empty.
    pub fn children<W: Widget>(self, children: impl IntoIterator<Item = W>) -> Row<W> {
        Row {
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow: self.overflow,
            children: children.into_iter().collect(),
        }
    }

    /// Replaces the children and erases the completed row's concrete type.
    ///
    /// This is equivalent to [`Row::children`] followed by [`Widget::boxed`].
    /// Use it when different branches need to return one [`AnyWidget`] type.
    pub fn box_children<W: Widget + 'static>(
        self,
        children: impl IntoIterator<Item = W>,
    ) -> AnyWidget {
        Row {
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow: self.overflow,
            children: children.into_iter().collect(),
        }
        .boxed()
    }

    /// Appends a child to the erased child collection.
    ///
    /// The child is boxed internally, so successive calls may use different
    /// concrete widget types. Existing children are retained.
    pub fn add_child<W: Widget + 'static>(mut self, child: W) -> Self {
        self.children
            .push(child.boxed());
        self
    }

    /// Inserts a child at `index` in the erased child collection.
    ///
    /// Existing children at and after `index` move one position to the right.
    /// This method panics when `index` is greater than the current length, just
    /// like [`Vec::insert`].
    pub fn insert_child<W: Widget + 'static>(mut self, index: usize, child: W) -> Self {
        self.children
            .insert(index, child.boxed());
        self
    }
}
//
// impl<W: Widget + 'static> Iterator for Row<W> {
//     type Item = W;
//
//     fn next(&mut self) -> Option<Self::Item> {
//         self.children.pop()
//     }
// }

impl<W: Widget + 'static> Widget for Row<W> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let children = self
            .children
            .iter()
            .map(|c| c.to_element(ctx))
            .collect();
        RawFlex {
            direction: LayoutDirection::Row,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow_behavior: self.overflow,
            children,
            cache: Default::default(),
            debug_name: "Row",
            cache_bound: CacheBounds::new(),
        }
        .boxed()
    }
}
