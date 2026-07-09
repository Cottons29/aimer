use crate::base::BuildContext;
use crate::components::element::VisitorElement;

// Rebuild capabilities
pub trait Rebuildable: VisitorElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.visit_children(&mut |child| {
            child.rebuild_if_dirty(ctx);
        });
    }

    /// Mark this element (and its subtree) as needing a rebuild on the next frame.
    ///
    /// The default just recurses through `visit_children`; elements that actually
    /// hold a build closure (`StatefulElement`, `StatelessElement`) override this
    /// to flip their own dirty flag so `rebuild_if_dirty` re-runs `build()`.
    /// Called on window resize so `MediaQuery`-dependent widgets rebuild.
    fn mark_needs_rebuild(&self) {
        self.visit_children(&mut |child| {
            child.mark_needs_rebuild();
        });
    }
}
