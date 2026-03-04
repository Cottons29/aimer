use crate::flex::raw_flex::RawFlex;
use crate::flex::{BoxAlignment, Flex, FlexDirection, OverflowBehavior};
use constructor::Constructor;
use widget::base::BuildContext;
use widget::{Element, LayoutSpacing, Widget};

#[derive(Constructor)]
/// A flex container that arranges its children in a vertical direction
pub struct Column {
    #[constructor(default)]
    vertical_alignment: BoxAlignment,
    #[constructor(default)]
    horizontal_alignment: BoxAlignment,
    #[constructor(default)]
    gaps: LayoutSpacing,
    #[constructor(default)]
    overflow: OverflowBehavior,
    #[constructor(default, into)]
    children: Vec<Box<dyn Widget>>,
}

impl Widget for Column {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let children = self.children.iter().map(|c| c.to_element(ctx)).collect();
        Box::new(RawFlex {
            direction: FlexDirection::Column,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow_behavior: self.overflow,
            children,
            cache: Default::default(),
        })
    }
}

#[derive(Constructor)]
/// A flex container that arranges its children in a horizontal direction
pub struct Row {
    #[constructor(default)]
    vertical_alignment: BoxAlignment,
    #[constructor(default)]
    horizontal_alignment: BoxAlignment,
    #[constructor(default)]
    gaps: LayoutSpacing,
    #[constructor(default)]
    overflow: OverflowBehavior,
    #[constructor(default)]
    children: Vec<Box<dyn Widget>>,
}

impl Widget for Row {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let children = self.children.iter().map(|c| c.to_element(ctx)).collect();
        Box::new(RawFlex {
            direction: FlexDirection::Row,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow_behavior: self.overflow,
            children,
            cache: Default::default(),
        })
    }
}
