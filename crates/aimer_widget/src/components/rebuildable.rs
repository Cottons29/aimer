use crate::base::BuildContext;
use crate::components::element::VisitorElement;

// Rebuild capabilities
pub trait Rebuildable: VisitorElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.visit_children(&mut |child| {
            child.rebuild_if_dirty(ctx);
        });
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        None
    }

    fn is_carry_state(&self) -> bool {
        false
    }

    /// Run reconciliation work with any inherited state published by this
    /// element available in `ctx`.
    ///
    /// Scope elements override this so eager descendant rebuilds performed
    /// during state carry observe the same context as normal draw and layout.
    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        callback(ctx);
    }

    /// Mark this element (and its subtree) as needing a rebuild on the next
    /// frame.
    ///
    /// The default just recurses through `visit_children`; elements that
    /// actually hold a build closure (`StatefulElement`,
    /// `StatelessElement`) override this to flip their own dirty flag so
    /// `rebuild_if_dirty` re-runs `build()`. Called on window resize so
    /// `MediaQuery`-dependent widgets rebuild.
    fn mark_needs_rebuild(&self) {
        self.visit_children(&mut |child| {
            child.mark_needs_rebuild();
        });
    }
}
