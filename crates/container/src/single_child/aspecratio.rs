use crate::flex::LayoutDirection;
use crate::single_child::container::RawContainer;
use crate::single_child::sized_box::RawSizedBox;
use constructor::Constructor;
use widget::base::BuildContext;
use widget::{Element, LayoutSpacing, Widget};

#[derive(Constructor)]
pub struct AspectRatio<W: Widget> {
    #[constructor(default, into)]
    pub aspect_ratio: f32,
    pub child: W,
}

impl<W: Widget> Widget for AspectRatio<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawSizedBox {
            width: Default::default(),
            height: Default::default(),
            color: Default::default(),
            child: self.child.to_element(ctx),
            cache: Default::default(),
        })
    }
}
