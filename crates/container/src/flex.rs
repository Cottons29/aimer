mod raw_flex;

pub use raw_flex::Flex;
use widget::base::BuildContext;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
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
                ctx.canvas.clip_rect(
                    skia_safe::Rect::from_xywh(0.0, 0.0, ctx.box_constraint.max_width, ctx.box_constraint.max_height),
                    skia_safe::ClipOp::Intersect,
                    true,
                );
            }
            _ => ()
        }
    }
}
