use crate::base::BuildContext;
use crate::components::element::VisitorElement;

// Rebuild capabilities
pub trait Rebuildable: VisitorElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.visit_children(&mut |child| {
            child.rebuild_if_dirty(ctx);
        });
    }
}