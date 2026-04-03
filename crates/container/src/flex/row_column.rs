use attribute::CacheBounds;
use crate::flex::raw_flex::RawFlex;
use crate::flex::{BoxAlignment, LayoutDirection, OverflowBehavior};
use constructor::WidgetConstructor;
use widget::base::BuildContext;
use widget::{Element, LayoutSpacing, Widget};


#[derive(WidgetConstructor)]
/// A flex container that arranges its children in a vertical direction
pub struct Column<W: Widget + 'static> {
    #[constructor(default)]
    vertical_alignment: BoxAlignment,
    #[constructor(default)]
    horizontal_alignment: BoxAlignment,
    #[constructor(default)]
    gaps: LayoutSpacing,
    #[constructor(default)]
    overflow: OverflowBehavior,
    #[constructor(default, into)]
    children: Vec<W>,
}

impl<W: Widget + 'static> Widget for Column<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.max_height = f32::MAX;
        let children = self.children.iter().map(|c| c.to_element(&child_ctx)).collect();
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

#[derive(WidgetConstructor)]
/// A flex container that arranges its children in a horizontal direction
pub struct Row<W: Widget + 'static> {
    #[constructor(default)]
    vertical_alignment: BoxAlignment,
    #[constructor(default)]
    horizontal_alignment: BoxAlignment,
    #[constructor(default)]
    gaps: LayoutSpacing,
    #[constructor(default)]
    overflow: OverflowBehavior,
    #[constructor(default, into)]
    children: Vec<W>,
}

impl<W: Widget + 'static> Widget for Row<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.max_width = f32::MAX;
        let children = self.children.iter().map(|c| c.to_element(&child_ctx)).collect();
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
