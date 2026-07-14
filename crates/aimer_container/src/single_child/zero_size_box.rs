use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement, Widget,
    base::BuildContext,
};
pub struct ZeroSizedBox;

impl Drawable for ZeroSizedBox {
    fn draw(&self, _: &BuildContext) {}
}

impl VisitorElement for ZeroSizedBox {
    fn debug_name(&self) -> &'static str {
        "ZeroSizedBox"
    }
}

impl EventElement for ZeroSizedBox {}

impl LayoutElement for ZeroSizedBox {}

impl Rebuildable for ZeroSizedBox {}

impl Widget for ZeroSizedBox {
    fn to_element(&self, _: &BuildContext) -> Box<dyn Element> {
        Box::new(ZeroSizedBox)
    }
}
