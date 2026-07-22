use std::panic::Location;
use std::rc::Rc;

use aimer_utils::PanicHelper;
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
    #[track_caller]
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let caller = Location::caller();
        let slot = ctx.get_state::<OutletSlot>().unwrap_or_else(|| {
            panic!(
                "No Shell found in context. An `Outlet` must be rendered inside a `Shell`.\n\n{}",
                PanicHelper::location(caller),
            )
        });
        let child = slot.build_child(ctx);
        child.to_element(ctx)
    }

    fn debug_name(&self) -> &'static str {
        "Outlet"
    }
}

#[cfg(test)]
mod tests {
    use std::panic::catch_unwind;

    use aimer_widget::base::{BuildContext, ResolvedSize, WindowHandle};

    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    fn context() -> BuildContext<'static> {
        use std::sync::OnceLock;

        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        });
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        let _guard = runtime.enter();
        BuildContext::new(
            canvas,
            ResolvedSize::default(),
            1.0,
            Default::default(),
            Default::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn missing_shell_diagnostic_highlights_the_outlet_caller() {
        let panic = catch_unwind(|| {
            let _ = Outlet.to_element(&context());
        })
        .expect_err("an outlet without a shell should panic");
        let message = panic
            .downcast_ref::<String>()
            .expect("outlet panic should use an owned diagnostic");

        assert!(message.contains(file!()), "{message}");
        assert!(
            message.contains("Outlet.to_element(&context())"),
            "{message}"
        );
        assert!(
            message
                .lines()
                .any(|line| line
                    .trim_start()
                    .starts_with("^^^^")),
            "{message}"
        );
    }
}
