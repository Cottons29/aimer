pub(crate) mod children_source;
#[doc(hidden)]
pub mod flex_child;
pub(crate) mod flex_layout;
pub mod flex_list;
#[cfg(test)]
mod lazy_tests;
#[doc(hidden)]
pub mod raw_flex;
pub mod row_column;
#[cfg(test)]
pub(crate) mod test_support;
pub(crate) mod wrap_layout;

// pub use raw_flex::RawFlex;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_widget::base::BuildContext;
pub use flex_child::Expanded;
pub use flex_list::{FlexList, ListFlex};
pub use raw_flex::Flex;
pub use row_column::{Column, Row};

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

/// Controls how children are placed along a flex container's main axis.
///
/// `Start`, `Center`, and `End` place the group without changing the spacing
/// between children. The remaining variants distribute positive free space
/// between and around the children. When the children use all available space
/// or overflow it, every variant falls back to the closest position possible.
///
/// # Examples
///
/// ```rust
/// use aimer_flex::JustifyContent;
///
/// let start = JustifyContent::Start;
/// let centered = JustifyContent::Center;
/// let end = JustifyContent::End;
/// let between = JustifyContent::SpaceBetween;
/// let around = JustifyContent::SpaceAround;
/// let evenly = JustifyContent::SpaceEvenly;
/// ```
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
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
        match self {
            Self::Hidden => {
                ctx.canvas.set_clip(
                    Vec2d { x: 0.0, y: 0.0 },
                    ResolvedSize {
                        width: ctx.box_constraint.max_width,
                        height: ctx.box_constraint.max_height,
                    },
                );
            }
            Self::Wrap | Self::Visible => {}
        }
    }
}
