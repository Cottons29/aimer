use aimer_widget::{Drawable, Element, EventElement, LayoutElement, Rebuildable, Reconcilable, VisitorElement, Widget, base::BuildContext};
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

impl Reconcilable for ZeroSizedBox {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_from_widget(&self, _new_element: &dyn Element, _ctx: &BuildContext) -> bool {
        // ZeroSizedBox has no state to update.
        true
    }
}

impl Widget for ZeroSizedBox {
    fn to_element(&self, _: &BuildContext) -> Box<dyn Element> {
        Box::new(ZeroSizedBox)
    }
}
