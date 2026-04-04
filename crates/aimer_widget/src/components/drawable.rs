use crate::base::BuildContext;
use crate::Element;

pub trait Drawable {
    fn draw(&self, ctx: &BuildContext);
}

impl Drawable for Box<dyn Drawable> {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx);
    }
}