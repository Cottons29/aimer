pub(crate) mod border_radius;
pub(crate) mod box_shadow;
pub(crate) mod shapes;

use color::prelude::Color;
use constructor::Constructor;
use crate::style::border::BoxBorder;
use crate::style::box_decoration::border_radius::BorderRadius;
use crate::style::box_decoration::box_shadow::BoxShadow;


#[allow(dead_code)]
#[derive(Default, Clone, Constructor)]
pub struct BoxDecoration {
    #[constructor(default)]
    pub border: BoxBorder,
    #[constructor(default)]
    pub border_radius: BorderRadius,
    #[constructor(default,dyn_iter)]
    pub box_shadow: Vec<BoxShadow>,
    #[constructor(default, into)]
    pub background_color: Option<Color>,
}