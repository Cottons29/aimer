use widget::{Element, Widget, base::BuildContext};
pub struct ZeroSizedBox;

impl Element for ZeroSizedBox {
    fn draw(&self, _: &BuildContext) {}
}

impl Widget for ZeroSizedBox {
    fn to_element(&self, _ : &BuildContext) -> Box<dyn Element> {
        Box::new(ZeroSizedBox)
    }
}
