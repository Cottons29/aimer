use aimer_attribute::{Dimension, Size};
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement, Widget,
};
/// A leaf widget that occupies no space and paints nothing.
///
/// `ZeroSizedBox` has no constructor or child: instantiate the unit struct
/// directly. It is useful as an empty placeholder where a valid [`Widget`] or
/// element is required, and its layout size remains the default zero size.
pub struct ZeroSizedBox;

impl ZeroSizedBox {
    pub fn boxed() -> Box<Self> {
        Box::new(Self)
    }
}

impl Drawable for ZeroSizedBox {
    fn draw(&self, _: &BuildContext) {}
}

impl VisitorElement for ZeroSizedBox {
    fn debug_name(&self) -> &'static str {
        "ZeroSizedBox"
    }
}

impl EventElement for ZeroSizedBox {}

impl LayoutElement for ZeroSizedBox {
    fn size(&self) -> Option<Size> {
        Some(Size {
            width: Dimension::Px(0.0),
            height: Dimension::Px(0.0),
        })
    }
}

impl Rebuildable for ZeroSizedBox {}

impl Widget for ZeroSizedBox {
    fn to_element(&self, _: &BuildContext) -> AnyElement {
        Element::boxed(ZeroSizedBox)
    }
}
