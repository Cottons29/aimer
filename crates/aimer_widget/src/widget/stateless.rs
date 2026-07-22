use std::cell::{Cell, UnsafeCell};
use std::rc::Rc;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};

use crate::base::*;
use crate::widget::recovery::{BuildPhase, build_or_error};
use crate::widget::stateful::{RebuildCallBack, SyncChild};
use crate::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, LayoutElement, Rebuildable,
    VisitorElement, Widget,
};
// StatelessWidget is effectively just a Widget.
// We rely on direct Widget implementation to avoid blanket implementation
// conflicts. The trait is kept for backward compatibility if needed, but
// generally users should implement Widget directly.

pub trait StatelessWidget {
    fn build(&self, ctx: &BuildContext) -> impl Widget;
}

/// Wraps any [`Widget`] and attaches a static name used by the inspector
/// overlay. Used by `#[derive(WidgetConstructor)]` to provide inspector
/// support. It does not change layout, drawing, events, or child identity. If
/// the produced element already reports the requested name, no extra wrapper
/// is created.
pub struct NamedWidget {
    inner: AnyWidget,
    name: &'static str,
}

impl NamedWidget {
    /// Wraps an already type-erased widget with a static inspector name.
    ///
    /// The wrapper forwards dirty rebuilding to its child but cannot recreate
    /// the source widget itself because it stores no build closure.
    pub fn new(inner: AnyWidget, name: &'static str) -> Self {
        Self { inner, name }
    }
}

impl Widget for NamedWidget {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let child = self.inner.to_element(ctx);
        if child.debug_name() == self.name {
            return child;
        }
        // A `NamedWidget` only wraps an already-built element for the inspector;
        // it has no build closure of its own, so it is not self-rebuildable —
        // it still forwards rebuild/dirty marking to its child.
        StatelessElement::wrapper(child, None, self.name).boxed()
    }

    fn debug_name(&self) -> &'static str {
        self.name
    }
}

impl EventElement for StatelessElement {}

impl Rebuildable for StatelessElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        StatelessElement::rebuild_if_dirty(self, ctx);
    }

    fn mark_needs_rebuild(&self) {
        // eprintln!("[diag] StatelessElement.mark_needs_rebuild");
        self.dirty.set(true);
        // Safety: single-threaded rendering pipeline.
        let child = unsafe { &*self.child.0.get() };
        child.mark_needs_rebuild();
    }
}

pub struct StatelessElement {
    /// Swappable child, so a rebuild can replace the subtree in place while
    /// `visit_children<'a>` can still hand out `&'a` references to it.
    pub(crate) child: SyncChild,
    pub(crate) dirty: Rc<Cell<bool>>,
    /// Re-runs the source widget's `build()` (re-reading `MediaQuery`).
    /// `None` for pure wrappers (e.g. `NamedWidget`) that cannot rebuild
    /// themselves.
    pub(crate) rebuild_fn: Option<Rc<RebuildCallBack>>,
    pub key: Option<crate::key::Key>,
    pub debug_name: &'static str,
    pub bounds: Cell<Option<(Vec2d, Vec2d)>>,
}

impl StatelessElement {
    pub fn from_builder(
        ctx: &BuildContext,
        rebuild_fn: impl Fn(&BuildContext) -> AnyElement + 'static,
        key: Option<crate::key::Key>,
        debug_name: &'static str,
    ) -> Self {
        let dirty = Rc::new(Cell::new(false));
        let consumer = BuildConsumer::new(dirty.clone());
        let rebuild_fn: Rc<RebuildCallBack> = Rc::new(rebuild_fn);
        let child = ctx.with_build_consumer(consumer.clone(), |ctx| {
            build_or_error(debug_name, BuildPhase::Build, || rebuild_fn(ctx))
        });
        let rebuild = Rc::new(move |ctx: &BuildContext| {
            ctx.with_build_consumer(consumer.clone(), |ctx| {
                build_or_error(debug_name, BuildPhase::Build, || rebuild_fn(ctx))
            })
        });
        Self {
            child: SyncChild(UnsafeCell::new(child)),
            dirty,
            rebuild_fn: Some(rebuild),
            key,
            debug_name,
            bounds: Cell::new(None),
        }
    }

    /// Create a self-rebuildable stateless element. `rebuild_fn` re-invokes the
    /// widget's `build()` with a fresh `BuildContext`, so
    /// `MediaQuery`-dependent widgets update when marked dirty (e.g. on
    /// window resize).
    pub fn new(
        child: AnyElement,
        rebuild_fn: impl Fn(&BuildContext) -> AnyElement + 'static,
        key: Option<crate::key::Key>,
        debug_name: &'static str,
    ) -> Self {
        Self {
            child: SyncChild(UnsafeCell::new(child)),
            dirty: Rc::new(Cell::new(false)),
            rebuild_fn: Some(Rc::new(rebuild_fn)),
            key,
            debug_name,
            bounds: Cell::new(None),
        }
    }

    /// Create a non-rebuildable wrapper. It never re-runs a `build()` of its
    /// own but still propagates dirty marking and rebuilds to its child.
    pub fn wrapper(
        child: AnyElement,
        key: Option<crate::key::Key>,
        debug_name: &'static str,
    ) -> Self {
        Self {
            child: SyncChild(UnsafeCell::new(child)),
            dirty: Rc::new(Cell::new(false)),
            rebuild_fn: None,
            key,
            debug_name,
            bounds: Cell::new(None),
        }
    }

    /// If dirty, rebuild the child and preserve live state from the old subtree.
    pub fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        let Some(rebuild_fn) = self.rebuild_fn.clone() else {
            // Pure wrapper: cannot rebuild itself, only propagate.
            let child = unsafe { &*self.child.0.get() };
            child.rebuild_if_dirty(ctx);
            return;
        };

        if !self.dirty.get() {
            let child = unsafe { &*self.child.0.get() };
            child.rebuild_if_dirty(ctx);
            return;
        }

        let new_child = rebuild_fn(ctx);

        {
            let child = unsafe { &*self.child.0.get() };
            child.rebuild_if_dirty(ctx);
        }

        {
            let old_child = unsafe { &*self.child.0.get() };
            crate::widget::stateful::carry_child_state(old_child.as_ref(), new_child.as_ref(), ctx);
        }

        unsafe {
            *self.child.0.get() = new_child;
        }

        self.dirty.set(false);
    }
}

impl Drawable for StatelessElement {
    fn draw(&self, ctx: &BuildContext) {
        #[cfg(debug_assertions)]
        {
            if crate::inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx
                    .canvas
                    .get_transform_translation();
                let size = self.content_size(ctx);
                let end_x = start_x + size.width;
                let end_y = start_y + size.height;

                let scale = ctx.scale;
                let l_start = Vec2d {
                    x: start_x / scale,
                    y: start_y / scale,
                };
                let l_end = Vec2d {
                    x: end_x / scale,
                    y: end_y / scale,
                };
                self.bounds
                    .set(Some((l_start, l_end)));

                let cp = ctx.cursor_pos;
                if cp.x >= l_start.x
                    && cp.x <= l_end.x
                    && cp.y >= l_start.y
                    && cp.y <= l_end.y
                    && let Ok(mut hovered) = crate::inspector_overlay::HOVERED_WIDGET.write()
                {
                    *hovered = Some((self.debug_name, l_start, l_end));
                }
            }
        }
        self.rebuild_if_dirty(ctx);
        // Safety: single-threaded rendering pipeline.
        let child = unsafe { &*self.child.0.get() };
        child.draw(ctx);
    }
}

impl LayoutElement for StatelessElement {
    fn pos(&self) -> Option<Vec2d> {
        unsafe { &*self.child.0.get() }.pos()
    }

    fn size(&self) -> Option<Size> {
        unsafe { &*self.child.0.get() }.size()
    }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe { &*self.child.0.get() }.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe { &*self.child.0.get() }.content_size(ctx)
    }
    fn get_size_from_child(&self) -> Option<Size> {
        unsafe { &*self.child.0.get() }.get_size_from_child()
    }
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        if self.bounds.get().is_some() {
            return self.bounds.get();
        }
        unsafe { &*self.child.0.get() }.pos_start_end()
    }
}

impl VisitorElement for StatelessElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Safety: single-threaded rendering pipeline; the returned reference is
        // valid for `'a` because the child lives inside `self`.
        let child = unsafe { &*self.child.0.get() };
        visitor(child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    fn dummy_async_handle() -> tokio::runtime::Handle {
        use std::sync::OnceLock;

        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        });
        let _guard = runtime.enter();
        tokio::runtime::Handle::current()
    }

    fn dummy_build_context() -> BuildContext<'static> {
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        BuildContext {
            parent_size: Default::default(),
            canvas,
            scale: 1.0,
            parent_pos: Default::default(),
            cursor_pos: Default::default(),
            box_constraint: Default::default(),
            visible_rect: None,
            window: WindowHandle::headless(Default::default(), 1.0),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: dummy_async_handle(),
            inherited_states: Default::default(),
        }
    }

    /// Minimal leaf element for exercising the rebuild-marking traversal.
    struct Leaf;
    impl VisitorElement for Leaf {
        fn debug_name(&self) -> &'static str {
            "Leaf"
        }
    }
    impl Drawable for Leaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl LayoutElement for Leaf {}
    impl EventElement for Leaf {}
    impl Rebuildable for Leaf {}

    // The core "ring the bell" wiring for responsive-on-resize:
    // `mark_needs_rebuild` must flip a rebuildable element's dirty flag AND
    // propagate through a non-rebuildable wrapper (e.g. NamedWidget) down to
    // the child that can rebuild.
    #[test]
    fn mark_needs_rebuild_propagates_through_wrapper() {
        let inner = StatelessElement::new(Leaf.boxed(), |_| Leaf.boxed(), None, "Inner");
        // Rebuildable elements start clean and carry a build closure.
        assert!(inner.rebuild_fn.is_some());
        assert!(!inner.dirty.get());
        let inner_dirty = inner.dirty.clone();

        // A wrapper cannot rebuild itself but must still forward the mark.
        let outer = StatelessElement::wrapper(inner.boxed(), None, "Outer");
        assert!(outer.rebuild_fn.is_none());
        assert!(!outer.dirty.get());

        outer.mark_needs_rebuild();

        assert!(outer.dirty.get(), "wrapper itself is marked");
        assert!(
            inner_dirty.get(),
            "mark reached the nested rebuildable child"
        );
    }

    #[test]
    fn dirty_stateless_element_runs_its_rebuild_closure() {
        let rebuilds = Rc::new(Cell::new(0));
        let rebuild_observer = rebuilds.clone();
        let element = StatelessElement::new(
            Leaf.boxed(),
            move |_| {
                rebuild_observer.set(rebuild_observer.get() + 1);
                Leaf.boxed()
            },
            None,
            "Rebuildable",
        );
        element.mark_needs_rebuild();

        let context = dummy_build_context();
        element.rebuild_if_dirty(&context);

        assert_eq!(rebuilds.get(), 1);
        assert!(!element.dirty.get());
    }

    #[test]
    fn initial_builder_panic_installs_error_child() {
        let context = dummy_build_context();
        let element = StatelessElement::from_builder(
            &context,
            |_| panic!("missing provider during initial build"),
            None,
            "InitialPanicWidget",
        );

        let child = unsafe { &*element.child.0.get() };
        assert_eq!(child.debug_name(), "ErrorWidget");
        assert!(!element.dirty.get());
    }

    #[test]
    fn rebuild_panic_installs_stable_error_child_and_clears_dirty() {
        let builds = Rc::new(Cell::new(0));
        let build_observer = builds.clone();
        let context = dummy_build_context();
        let element = StatelessElement::from_builder(
            &context,
            move |_| {
                build_observer.set(build_observer.get() + 1);
                if build_observer.get() == 1 {
                    Leaf.boxed()
                } else {
                    panic!("missing provider during rebuild")
                }
            },
            None,
            "RebuildPanicWidget",
        );

        element.mark_needs_rebuild();
        element.rebuild_if_dirty(&context);

        let child = unsafe { &*element.child.0.get() };
        assert_eq!(child.debug_name(), "ErrorWidget");
        assert!(!element.dirty.get());

        element.rebuild_if_dirty(&context);
        assert_eq!(
            builds.get(),
            2,
            "recovered subtree must not retry while clean"
        );
    }

    #[test]
    fn builder_runs_initial_and_rebuild_passes_with_a_consumer() {
        let builds_with_consumer = Rc::new(Cell::new(0));
        let observer = builds_with_consumer.clone();
        let context = dummy_build_context();
        let element = StatelessElement::from_builder(
            &context,
            move |context| {
                if context
                    .current_build_consumer()
                    .is_some()
                {
                    observer.set(observer.get() + 1);
                }
                Leaf.boxed()
            },
            None,
            "Reactive",
        );
        element.mark_needs_rebuild();
        element.rebuild_if_dirty(&context);

        assert_eq!(builds_with_consumer.get(), 2);
    }
}
