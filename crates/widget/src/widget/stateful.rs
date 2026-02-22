use crate::{base::*, Element, Widget};

pub trait StatefulWidget: Sized + Send + Sync {
    type State: State<Self>;
    fn create_state(&self) -> Self::State;
}

pub trait State<W: StatefulWidget>: Send + Sync + 'static {
    fn build(&self) -> impl Widget;
}

pub struct StatefulElement {
    pub child: Box<dyn Element>,
    pub state: Box<dyn std::any::Any + Send + Sync>,
}

impl Element for StatefulElement {
    fn draw(&self, ctx: &BuildContext) {
        self.child.draw(ctx);
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }
    fn size(&self) -> Option<Size> {
        self.child.size()
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
