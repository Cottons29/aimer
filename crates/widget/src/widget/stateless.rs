use crate::{base::*, Drawable, Element, Widget};
use attribute::position::Vec2d;
use attribute::size::{ResolvedSize, Size};
// StatelessWidget is effectively just a Widget.
// We rely on direct Widget implementation to avoid blanket implementation conflicts.
// The trait is kept for backward compatibility if needed, but generally users should implement Widget directly.

pub trait StatelessWidget {
    fn build(&self, ctx: &BuildContext) -> impl Widget;
}

pub struct StatelessElement {
    pub child: Box<dyn Element>,
}

impl Drawable for StatelessElement {
    fn draw(&self, ctx: &BuildContext) {
        self.child.draw(ctx);
    }
}

impl Element for StatelessElement {

    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }
    fn size(&self) -> Option<Size> {
        self.child.size()
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }
    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }
}





