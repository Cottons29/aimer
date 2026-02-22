use crate::base::Dimension;

#[derive(constructor::Constructor, Default, Clone, Copy)]
pub struct BoxConstraint{
    #[constructor(default, into)]
    pub min_width: u32,
    #[constructor(default, into)]
    pub min_height: u32,
    #[constructor(default, into)]
    pub max_width: u32,
    #[constructor(default, into)]
    pub max_height: u32,
}
