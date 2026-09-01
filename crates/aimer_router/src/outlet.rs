use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use aimer_widget::base::BuildContext;
use aimer_widget::{AnyElement, AnyWidget, Widget};

/// Type-erased builder for the active child route rendered inside an
/// [`Outlet`].
pub type OutletChildBuilder = Rc<dyn Fn(&BuildContext) -> AnyWidget>;

/// Ambient values captured at a route boundary and restored while retained
/// route content is built or traversed.
///
/// `BuildContext` scopes inherited values dynamically. A route element can
/// outlive the call stack that first built it, though, so a later rebuild may
/// otherwise run without providers that were ancestors when the route was
/// mounted. The values are shared `Rc`s, not cloned provider state; this is a
/// context composition boundary rather than another provider store.
#[derive(Clone)]
pub(crate) struct RouteChildContext {
    states: HashMap<TypeId, Rc<dyn Any>>,
}

impl RouteChildContext {
    pub(crate) fn capture(ctx: &BuildContext) -> Self {
        Self {
            states: ctx.inherited_states.borrow().clone(),
        }
    }

    pub(crate) fn with<R>(
        &self,
        ctx: &BuildContext,
        callback: impl FnOnce(&BuildContext) -> R,
    ) -> R {
        let mut previous = Vec::new();
        {
            let mut states = ctx.inherited_states.borrow_mut();
            for (type_id, state) in &self.states {
                if !states.contains_key(type_id) {
                    states.insert(*type_id, state.clone());
                    previous.push(*type_id);
                }
            }
        }

        let _guard = RouteChildContextGuard {
            states: ctx.inherited_states.clone(),
            previous,
        };
        callback(ctx)
    }
}

struct RouteChildContextGuard {
    states: Rc<RefCell<HashMap<TypeId, Rc<dyn Any>>>>,
    previous: Vec<TypeId>,
}

impl Drop for RouteChildContextGuard {
    fn drop(&mut self) {
        let mut states = self.states.borrow_mut();
        for type_id in self.previous.drain(..).rev() {
            states.remove(&type_id);
        }
    }
}

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
    pub fn build_child(&self, ctx: &BuildContext) -> AnyWidget {
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

impl aimer_widget::PortableWidget for Outlet {
    #[cfg(feature = "portable-guest")]
    fn to_portable_node(
        self,
        ctx: &mut aimer_widget::portable::PortableBuildContext,
        source: aimer_widget::portable::SourceFingerprint,
    ) -> Result<
        aimer_widget::portable::PortableNodeId,
        aimer_widget::portable::PortableBuildError,
    > {
        let build_context = ctx.build_context();
        let Some(slot) = build_context.get_state::<OutletSlot>() else {
            return Err(ctx.unsupported_widget("Outlet", source));
        };
        let child = slot.build_child(&build_context);
        aimer_widget::AnyWidgetExt::into_portable_node(child, ctx, source)
    }
}

impl Widget for Outlet {
    /// # Panics
    ///
    /// Panics when no [`crate::shell::Shell`] is in scope. The panic is raised
    /// in the body of this `#[track_caller]` method rather than inside a
    /// closure: a closure is an untracked frame, so panicking there blames this
    /// file instead of the code that placed the outlet.
    #[track_caller]
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let Some(slot) = ctx.get_state::<OutletSlot>() else {
            panic!("No Shell found in context. An `Outlet` must be rendered inside a `Shell`.")
        };
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

    use aimer_utils::PanicSite;
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
        let watch = PanicSite::watch();
        let rendered = catch_unwind(|| {
            let _ = Outlet.to_element(&context());
        })
        .err()
        .and_then(|_| watch.take_site())
        .expect("the panic site should be recorded")
        .to_string();

        assert!(rendered.starts_with("at "), "{rendered}");
        assert!(rendered.contains(file!()), "{rendered}");
        assert!(
            rendered.contains("Outlet.to_element(&context())"),
            "{rendered}"
        );
        assert!(
            rendered
                .lines()
                .any(|line| line.trim_start().starts_with("^^^^")),
            "{rendered}"
        );
    }
}
