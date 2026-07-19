use std::rc::Rc;

use aimer_widget::base::BuildContext;
use aimer_widget::{Element, Widget};

/// Type-erased builder for the active child route rendered inside an
/// [`Outlet`].
pub type OutletChildBuilder = Rc<dyn Fn(&BuildContext) -> Box<dyn Widget>>;

/// State injected by a [`crate::shell::Shell`] into the [`BuildContext`] so a
/// descendant [`Outlet`] knows which child to render. Cheaply cloneable (holds
/// an `Rc` closure).
#[derive(Clone)]
pub struct OutletSlot {
    build: OutletChildBuilder,
}

impl OutletSlot {
    /// Creates a slot from the type-erased active-child builder injected by a
    /// shell.
    pub fn new(build: OutletChildBuilder) -> Self {
        Self { build }
    }

    /// Build the active child widget for the current context.
    pub fn build_child(&self, ctx: &BuildContext) -> Box<dyn Widget> {
        (self.build)(ctx)
    }
}

/// A zero-configuration placeholder widget marking where a shell's active child
/// route is rendered.
///
/// Place an `Outlet` anywhere inside a [`crate::shell::Shell`]'s frame; the
/// shell injects an [`OutletSlot`] and the outlet builds the active child from
/// it. An `Outlet` used without an ancestor shell panics — that is a
/// programming error, mirroring `NavigatorController::of`.
pub struct Outlet;

impl Widget for Outlet {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let slot = ctx
            .get_state::<OutletSlot>()
            .expect("No Shell found in context. An `Outlet` must be rendered inside a `Shell`.");
        let child = slot.build_child(ctx);
        child.to_element(ctx)
    }

    fn debug_name(&self) -> &'static str {
        "Outlet"
    }
}
