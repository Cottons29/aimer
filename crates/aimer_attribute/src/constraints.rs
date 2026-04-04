
#[derive(aimer_macro::Constructor, Default, Clone, Copy, PartialEq, Debug)]
pub struct BoxConstraint{
    #[constructor(default, into)]
    pub min_width: f32,
    #[constructor(default, into)]
    pub min_height: f32,
    #[constructor(default, into)]
    pub max_width: f32,
    #[constructor(default, into)]
    pub max_height: f32,
}
