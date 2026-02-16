use crate::base::BuildContext;

pub trait Drawable {
    fn draw(&self, ctx: &BuildContext);
}
