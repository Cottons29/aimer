use constructor::Constructor;
use widget::base::BuildContext;
use widget::{Element, LayoutSpacing, Widget};
use crate::flex::LayoutDirection;

#[derive(Constructor)]
pub struct AspectRatio<W: Widget> {
    #[constructor(default, into)]
    pub aspect_ratio: f32,

    pub child: W,
}

impl<W: Widget> Widget for AspectRatio<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        unimplemented!()
    }
}
