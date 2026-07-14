use crate::ZeroSizedBox;
use crate::flex::raw_flex::RawFlex;
use crate::flex::{BoxAlignment, LayoutDirection, OverflowBehavior};
use aimer_attribute::CacheBounds;
use aimer_style::LayoutSpacing;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, Widget};

/// A flex container that arranges its children in a vertical direction
pub struct Column<W: Widget + 'static = Box<dyn Widget>> {
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
    pub fn new() -> Self {
        Self {
            vertical_alignment: Default::default(),
            horizontal_alignment: Default::default(),
            gaps: Default::default(),
            overflow: Default::default(),
            children: Default::default(),
        }
    }

    pub fn vertical_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.vertical_alignment = alignment;
        self
    }

    pub fn horizontal_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.horizontal_alignment = alignment;
        self
    }

    pub fn gaps(mut self, gaps: impl Into<LayoutSpacing>) -> Self {
        self.gaps = gaps.into();
        self
    }

    pub fn overflow(mut self, overflow: OverflowBehavior) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn children<W: Widget>(self, children: impl IntoIterator<Item = W>) -> Column<W> {
        Column {
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow: self.overflow,
            children: children.into_iter().collect(),
        }
    }

    pub fn add_child<W: Widget + 'static>(mut self, child: W) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn insert_child<W: Widget + 'static>(mut self, index: usize, child: W) -> Self {
        self.children.insert(index, Box::new(child));
        self
    }
}

impl<W: Widget + 'static> Widget for Column<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let children = self.children.iter().map(|c| c.to_element(ctx)).collect();
        Box::new(RawFlex {
            direction: LayoutDirection::Column,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow_behavior: self.overflow,
            children,
            cache: Default::default(),
            debug_name: "Column",
            cache_bound: CacheBounds::new(),
        })
    }
}

/// A flex container that arranges its children in a horizontal direction
pub struct Row<W: Widget + 'static = Box<dyn Widget>> {
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
    pub fn new() -> Self {
        Self {
            vertical_alignment: Default::default(),
            horizontal_alignment: Default::default(),
            gaps: Default::default(),
            overflow: Default::default(),
            children: Default::default(),
        }
    }

    pub fn vertical_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.vertical_alignment = alignment;
        self
    }

    pub fn horizontal_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.horizontal_alignment = alignment;
        self
    }

    pub fn gaps(mut self, gaps: impl Into<LayoutSpacing>) -> Self {
        self.gaps = gaps.into();
        self
    }

    pub fn overflow(mut self, overflow: OverflowBehavior) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn children<W: Widget>(self, children: impl IntoIterator<Item = W>) -> Row<W> {
        Row {
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow: self.overflow,
            children: children.into_iter().collect(),
        }
    }

    pub fn add_child<W: Widget + 'static>(mut self, child: W) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn insert_child<W: Widget + 'static>(mut self, index: usize, child: W) -> Self {
        self.children.insert(index, Box::new(child));
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
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let children = self.children.iter().map(|c| c.to_element(ctx)).collect();
        Box::new(RawFlex {
            direction: LayoutDirection::Row,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow_behavior: self.overflow,
            children,
            cache: Default::default(),
            debug_name: "Row",
            cache_bound: CacheBounds::new(),
        })
    }
}
