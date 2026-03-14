use crate::flex::raw_flex::RawFlex;
use crate::flex::{BoxAlignment, Flex, LayoutDirection, OverflowBehavior};
use constructor::{Constructor, WidgetConstructor};
use widget::base::BuildContext;
use widget::{Element, LayoutSpacing, Widget};


#[cfg(not(target_arch = "wasm32"))]
type Float = f32;
#[cfg(target_arch = "wasm32")]
type Float = f64;

#[derive(WidgetConstructor)]
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
        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.max_height = Float::MAX;
        let children = self.children.iter().map(|c| c.to_element(&child_ctx)).collect();
        Box::new(RawFlex {
            direction: LayoutDirection::Column,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow_behavior: self.overflow,
            children,
            cache: Default::default(),
        })
    }
}

#[derive(WidgetConstructor)]
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
    #[constructor(default, into)]
    children: Vec<Box<dyn Widget>>,
}

impl Widget for Row {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.max_width = Float::MAX;
        let children = self.children.iter().map(|c| c.to_element(&child_ctx)).collect();
        Box::new(RawFlex {
            direction: LayoutDirection::Row,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            overflow_behavior: self.overflow,
            children,
            cache: Default::default(),
        })
    }
}
