use aimer_style::LayoutSpacing;
use aimer_widget::Widget;

use crate::flex::LayoutDirection;

#[allow(dead_code)]
pub struct RawGridLayout {
    pub is_reversed: bool,
    pub gaps: LayoutSpacing,
    direction: LayoutDirection,
    padding: LayoutSpacing,
    pub children: Vec<Box<dyn Widget>>,
}
