use aimer_style::LayoutSpacing;

use crate::flex::LayoutDirection;
use aimer_widget::Widget;

#[allow(dead_code)]
pub struct RawGridLayout {
    pub is_reversed: bool,
    pub gaps: LayoutSpacing,
    direction: LayoutDirection,
    padding: LayoutSpacing,
    pub children: Vec<Box<dyn Widget>>,
}
