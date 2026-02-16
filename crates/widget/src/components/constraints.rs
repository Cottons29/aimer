#[derive(constructor::Constructor)]
pub struct BoxConstraint{
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
}
