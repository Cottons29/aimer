mod raw_flex;
pub mod row_column;

// pub use raw_flex::RawFlex;
pub use row_column::Column;
pub use row_column::Row;

pub use raw_flex::Flex;
use aimer_widget::base::BuildContext;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum LayoutDirection {
    Row,
    Column,
    #[default]
    Inherit,
}
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum BoxAlignment {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    #[default]
    Hidden,
    Wrap,
    Visible,
}

impl OverflowBehavior {
    fn apply_overflow_behave(&self, ctx: &BuildContext) {
        #[allow(clippy::single_match)]
        match self {
            Self::Hidden => {
                ctx.canvas.set_clip(
                    Vec2d { x: 0.0, y: 0.0 },
                    ResolvedSize { width: ctx.box_constraint.max_width, height: ctx.box_constraint.max_height },
                );
            }
            _ => ()
        }
    }
}
