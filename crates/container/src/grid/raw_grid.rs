use constructor::Constructor;
use widget::{LayoutSpacing, Widget};
use crate::flex::LayoutDirection;

pub struct RawGridLayout  {
    pub is_reversed: bool,
    pub gaps: LayoutSpacing,
    direction: LayoutDirection,
    padding: LayoutSpacing,
    pub children: Vec<Box<dyn Widget>>,
}
