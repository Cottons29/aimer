mod raw_flex;


pub use raw_flex::Flex;

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
    End
}
