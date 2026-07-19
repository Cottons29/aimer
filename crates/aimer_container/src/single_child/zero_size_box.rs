use aimer_widget::base::BuildContext;
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement, Widget,
};
/// A leaf widget that occupies no space and paints nothing.
///
/// `ZeroSizedBox` has no constructor or child: instantiate the unit struct
/// directly. It is useful as an empty placeholder where a valid [`Widget`] or
/// element is required, and its layout size remains the default zero size.
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
