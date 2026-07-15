use aimer_widget::base::BuildContext;
use aimer_widget::{Element, RequiredChild, Widget};

pub struct Scalable<W = RequiredChild> {
    scale: f32,
    child: W,
}

impl Scalable {
    pub fn new() -> Self {
        Self { child: RequiredChild, scale: 1.0 }
    }

    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    pub fn child<W: Widget>(self, child: W) -> Scalable<W> {
        Scalable { child, scale: self.scale }
    }
}

impl<W: Widget + 'static> Widget for Scalable<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        todo!()
    }
}
