use constructor::Constructor;
use widget::{Element, LayoutSpacing, Widget};
use widget::base::BuildContext;
use crate::flex::{BoxAlignment, Flex, FlexDirection, OverflowBehavior};
use crate::flex::raw_flex::RawFlex;

#[derive(Constructor)]
/// A flex container that arranges its children in a vertical directions
pub struct Column {
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
/// A flex container that arranges its children in a vertical directions
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