#[cfg(not(target_arch = "wasm32"))]
#[derive(constructor::Constructor, Default, Clone, Copy, PartialEq, Debug)]
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

#[cfg(target_arch = "wasm32")]
#[derive(constructor::Constructor, Default, Clone, Copy, PartialEq,Debug)]
pub struct BoxConstraint{
    #[constructor(default, into)]
    pub min_width: f64,
    #[constructor(default, into)]
    pub min_height: f64,
    #[constructor(default, into)]
    pub max_width: f64,
    #[constructor(default, into)]
    pub max_height: f64,
}
