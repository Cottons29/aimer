use aimer_widget::{Element, Widget, base::BuildContext, Drawable};
pub struct ZeroSizedBox;

impl Drawable for ZeroSizedBox {
    fn draw(&self, _: &BuildContext) {}
}

impl Element for ZeroSizedBox {}

impl Widget for ZeroSizedBox {
    fn to_element(&self, _ : &BuildContext) -> Box<dyn Element> {
        Box::new(ZeroSizedBox)
    }
}
