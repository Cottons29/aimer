use aimer_widget::base::BuildContext;
use aimer_widget::{Element, Widget};

use crate::single_child::sized_box::RawSizedBox;

#[allow(dead_code)]
pub struct AspectRatio<W: Widget + 'static = crate::ZeroSizedBox> {
    pub aspect_ratio: f32,
    pub child: W,
}

impl AspectRatio {
    pub fn new() -> Self {
        Self { aspect_ratio: 0.0, child: crate::ZeroSizedBox }
    }
}

impl<W: Widget + 'static> AspectRatio<W> {
    pub fn aspect_ratio(mut self, aspect_ratio: f32) -> Self {
        self.aspect_ratio = aspect_ratio;
        self
    }

    pub fn child<C: Widget>(self, child: C) -> AspectRatio<C> {
        AspectRatio { aspect_ratio: self.aspect_ratio, child }
    }
}

impl<W: Widget> Widget for AspectRatio<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawSizedBox {
            width: Default::default(),
            height: Default::default(),
            color: Default::default(),
            child: self
                .child
                .to_element(ctx),
            cache: Default::default(),
            debug_name: "AspectRatio",
            bounds: std::cell::Cell::new(None),
        })
    }
}
