use crate::base::BuildContext;
use crate::Element;

pub mod stateful;
pub mod stateless;

pub trait Widget{
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element>;
}

impl Widget for Box<dyn Widget> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        self.as_ref().to_element(ctx)
    }
}

