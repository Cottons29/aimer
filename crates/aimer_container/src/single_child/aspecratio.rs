use crate::single_child::sized_box::RawSizedBox;
use aimer_macro::WidgetConstructor;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, Widget};

#[allow(dead_code)]
#[derive(WidgetConstructor)]
pub struct AspectRatio<W: Widget + 'static> {
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
            debug_name: "AspectRatio",
            bounds: std::cell::Cell::new(None),
        })
    }
}
