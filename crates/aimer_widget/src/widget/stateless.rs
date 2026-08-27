use std::cell::{Cell, UnsafeCell};
use std::rc::Rc;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};

use crate::base::*;
use crate::widget::AnyWidgetExt;
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
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(
    manual_lowering,
    materializer = materialize_named_widget
)]
pub struct NamedWidget {
    #[portable_child]
    inner: AnyWidget,
    #[portable_skip]
    name: &'static str,
}

/// Materializes the portable form of [`NamedWidget`] as its child.
///
/// The inspector name is a native-only static string and is intentionally not
/// part of the portable schema. Returning the already materialized child keeps
/// layout, events, and reconciliation behavior bounded and transparent rather
/// than inventing a name that could be wrong for the source wrapper.
fn materialize_named_widget(
    _document: &crate::portable::__anteros::WidgetDocumentView<'_>,
    _node: crate::portable::__anteros::WidgetNodeView<'_>,
    mut children: Vec<AnyWidget>,
) -> Result<AnyWidget, crate::portable::PortableMaterializeError> {
    if children.len() != 1 {
        return Err(crate::portable::PortableMaterializeError::InvalidChildCount {
            expected: 1,
            actual: children.len(),
        });
    }
    Ok(children
        .pop()
        .ok_or(crate::portable::PortableMaterializeError::InvalidChildCount {
            expected: 1,
            actual: 0,
        })?)
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
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let child = self.inner.into_element(ctx);
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

impl crate::widget::PortableWidget for NamedWidget {
    #[cfg(feature = "portable-guest")]
    fn to_portable_node(
        self,
        ctx: &mut crate::portable::PortableBuildContext,
        source: crate::portable::SourceFingerprint,
    ) -> Result<crate::portable::PortableNodeId, crate::portable::PortableBuildError> {
        self.inner.into_portable_node(ctx, source)
    }
}

impl EventElement for StatelessElement {}

impl Rebuildable for StatelessElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        StatelessElement::rebuild_if_dirty(self, ctx);
    }

    fn mark_needs_rebuild(&self) {
        let _mark = crate::components::element::begin_rebuild_mark();
        crate::components::element::with_rebuild_invalidation(|| {
            self.dirty_source.mark();
            // Safety: single-threaded rendering pipeline.
            let child = unsafe { &*self.child.0.get() };
            child.mark_needs_rebuild();
        });
    }

    fn option_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

pub struct StatelessElement {
    /// Swappable child, so a rebuild can replace the subtree in place while
    /// `visit_children<'a>` can still hand out `&'a` references to it.
    pub(crate) child: SyncChild,
    pub(crate) dirty: Rc<Cell<bool>>,
    pub(crate) dirty_source: Rc<crate::components::element::DirtySource>,
    /// Re-runs the source widget's `build()` (re-reading `MediaQuery`).
    /// `None` for pure wrappers (e.g. `NamedWidget`) that cannot rebuild
    /// themselves.
    pub(crate) rebuild_fn: Option<Rc<RebuildCallBack>>,
    /// Last invalidation generation whose descendant rebuild work was visited.
    /// A clean element can skip the entire retained subtree until a state,
    /// dependency, or explicit dirty mark advances the generation.
    rebuild_invalidation_generation: Cell<u64>,
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
        let dirty_source = consumer.dirty_source();
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
            dirty_source,
            rebuild_fn: Some(rebuild),
            rebuild_invalidation_generation: Cell::new(u64::MAX),
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
        let dirty = Rc::new(Cell::new(false));
        Self {
            child: SyncChild(UnsafeCell::new(child)),
            dirty_source: crate::components::element::DirtySource::new(dirty.clone()),
            dirty,
            rebuild_fn: Some(Rc::new(rebuild_fn)),
            rebuild_invalidation_generation: Cell::new(u64::MAX),
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
        let dirty = Rc::new(Cell::new(false));
        Self {
            child: SyncChild(UnsafeCell::new(child)),
            dirty_source: crate::components::element::DirtySource::new(dirty.clone()),
            dirty,
            rebuild_fn: None,
            rebuild_invalidation_generation: Cell::new(u64::MAX),
            key,
            debug_name,
            bounds: Cell::new(None),
        }
    }

    /// If dirty, rebuild the child and preserve live state from the old
    /// subtree.
    pub fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        let invalidation_generation =
            crate::components::element::rebuild_invalidation_generation();
        if !self.dirty.get()
            && self.rebuild_invalidation_generation.get() == invalidation_generation
        {
            return;
        }
        self.rebuild_invalidation_generation
            .set(invalidation_generation);

        let Some(rebuild_fn) = self.rebuild_fn.clone() else {
            // Pure wrapper: cannot rebuild itself, only propagate.
            let child = unsafe { &*self.child.0.get() };
            child.rebuild_if_dirty(ctx);
            self.dirty_source.clear();
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
            crate::components::element::reconcile_generated_tree(
                old_child.as_ref(),
                new_child.as_ref(),
            );
        }

        unsafe {
            *self.child.0.get() = new_child;
        }

        self.dirty_source.clear();
    }
}

impl Drawable for StatelessElement {
    fn draw(&self, ctx: &BuildContext) {
        #[cfg(debug_assertions)]
        {
            if crate::inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx.canvas.get_transform_translation();
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
                self.bounds.set(Some((l_start, l_end)));

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

    #[inline]
    fn is_paint_stable(&self) -> bool {
        // A self-rebuilding element may replace its child on the next dirty
        // pass, so only a pure wrapper can transparently expose the child's
        // retained-paint contract.
        self.rebuild_fn.is_none()
            && unsafe { &*self.child.0.get() }.is_paint_stable()
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
    fn flex(&self) -> Option<f32> {
        unsafe { &*self.child.0.get() }.flex()
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

    fn reconciliation_key(&self) -> Option<&crate::Key> {
        self.key.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use aimer_anteros::{ModelLimits, Version, WidgetDocument, WidgetNode, WidgetSchemaId};

    use super::*;
    use crate::portable::{
        PortableNativeWidget, PortableWidgetSchema, linked_portable_native_widget_registrations,
    };
    use crate::PortableWidget;

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
        BuildContext::new(
            canvas,
            Default::default(),
            1.0,
            Default::default(),
            Default::default(),
            WindowHandle::headless(Default::default(), 1.0),
            #[cfg(not(target_arch = "wasm32"))]
            dummy_async_handle(),
        )
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

    struct CountingLeaf {
        rebuilds: Rc<Cell<usize>>,
    }

    impl VisitorElement for CountingLeaf {
        fn debug_name(&self) -> &'static str {
            "CountingLeaf"
        }
    }

    impl Drawable for CountingLeaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl LayoutElement for CountingLeaf {}
    impl EventElement for CountingLeaf {}

    impl Rebuildable for CountingLeaf {
        fn rebuild_if_dirty(&self, _ctx: &BuildContext) {
            self.rebuilds.set(self.rebuilds.get() + 1);
        }
    }

    struct SiblingBranch {
        children: Vec<AnyElement>,
    }

    impl VisitorElement for SiblingBranch {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "SiblingBranch"
        }
    }

    impl Drawable for SiblingBranch {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl LayoutElement for SiblingBranch {}
    impl EventElement for SiblingBranch {}
    impl Rebuildable for SiblingBranch {}

    /// A child whose flex factor must remain visible through transparent
    /// element wrappers, as it does for an `Expanded` inside a keyed tree.
    struct FlexibleLeaf;
    impl VisitorElement for FlexibleLeaf {
        fn debug_name(&self) -> &'static str {
            "FlexibleLeaf"
        }
    }
    impl Drawable for FlexibleLeaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl LayoutElement for FlexibleLeaf {
        fn flex(&self) -> Option<f32> {
            Some(2.5)
        }
    }
    impl EventElement for FlexibleLeaf {}
    impl Rebuildable for FlexibleLeaf {}

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
    fn clean_wrapper_does_not_rescan_unchanged_subtree() {
        let rebuilds = Rc::new(Cell::new(0));
        let wrapper = StatelessElement::wrapper(
            CountingLeaf {
                rebuilds: rebuilds.clone(),
            }
            .boxed(),
            None,
            "Wrapper",
        );
        let context = dummy_build_context();

        wrapper.rebuild_if_dirty(&context);
        wrapper.rebuild_if_dirty(&context);
        assert_eq!(rebuilds.get(), 1);

        wrapper.mark_needs_rebuild();
        wrapper.rebuild_if_dirty(&context);
        assert_eq!(rebuilds.get(), 2);
    }

    #[test]
    fn dirty_subtree_does_not_rebuild_clean_sibling() {
        let clean_rebuilds = Rc::new(Cell::new(0));
        let dirty_rebuilds = Rc::new(Cell::new(0));
        let root = SiblingBranch {
            children: vec![
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: clean_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "CleanSibling",
                )
                .boxed(),
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: dirty_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "DirtySibling",
                )
                .boxed(),
            ],
        }
        .boxed();
        let context = dummy_build_context();

        root.rebuild_if_dirty(&context);
        clean_rebuilds.set(0);
        dirty_rebuilds.set(0);

        let mut marked = false;
        root.visit_children(&mut |child| {
            if child.debug_name() == "DirtySibling" {
                child.mark_needs_rebuild();
                marked = true;
            }
        });
        assert!(marked, "the test did not find the intended dirty sibling");

        root.rebuild_if_dirty(&context);

        assert_eq!(clean_rebuilds.get(), 0);
        assert_eq!(dirty_rebuilds.get(), 1);
    }

    #[test]
    fn dirty_subtree_index_is_used_by_direct_draw() {
        let clean_rebuilds = Rc::new(Cell::new(0));
        let dirty_rebuilds = Rc::new(Cell::new(0));
        let root = SiblingBranch {
            children: vec![
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: clean_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "CleanSibling",
                )
                .boxed(),
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: dirty_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "DirtySibling",
                )
                .boxed(),
            ],
        }
        .boxed();
        let context = dummy_build_context();

        root.draw(&context);
        clean_rebuilds.set(0);
        dirty_rebuilds.set(0);

        root.visit_children(&mut |child| {
            if child.debug_name() == "DirtySibling" {
                child.mark_needs_rebuild();
            }
        });
        root.draw(&context);

        assert_eq!(clean_rebuilds.get(), 0);
        assert_eq!(dirty_rebuilds.get(), 1);
    }

    #[test]
    fn independent_subtree_walk_falls_back_to_the_full_root() {
        let clean_rebuilds = Rc::new(Cell::new(0));
        let dirty_rebuilds = Rc::new(Cell::new(0));
        let root = SiblingBranch {
            children: vec![
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: clean_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "CleanSibling",
                )
                .boxed(),
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: dirty_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "DirtySibling",
                )
                .boxed(),
            ],
        }
        .boxed();
        let context = dummy_build_context();

        root.rebuild_if_dirty(&context);
        clean_rebuilds.set(0);
        dirty_rebuilds.set(0);

        root.visit_children(&mut |child| {
            if child.debug_name() == "DirtySibling" {
                child.rebuild_if_dirty(&context);
            }
        });
        clean_rebuilds.set(0);
        dirty_rebuilds.set(0);

        root.visit_children(&mut |child| {
            if child.debug_name() == "DirtySibling" {
                child.mark_needs_rebuild();
            }
        });
        root.rebuild_if_dirty(&context);

        assert_eq!(clean_rebuilds.get(), 1);
        assert_eq!(dirty_rebuilds.get(), 1);
    }

    #[test]
    fn multiple_dirty_subtrees_release_only_their_own_paths() {
        let clean_rebuilds = Rc::new(Cell::new(0));
        let first_rebuilds = Rc::new(Cell::new(0));
        let second_rebuilds = Rc::new(Cell::new(0));
        let root = SiblingBranch {
            children: vec![
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: clean_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "CleanSibling",
                )
                .boxed(),
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: first_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "FirstDirtySibling",
                )
                .boxed(),
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: second_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "SecondDirtySibling",
                )
                .boxed(),
            ],
        }
        .boxed();
        let context = dummy_build_context();

        root.rebuild_if_dirty(&context);
        clean_rebuilds.set(0);
        first_rebuilds.set(0);
        second_rebuilds.set(0);

        root.visit_children(&mut |child| {
            if matches!(child.debug_name(), "FirstDirtySibling" | "SecondDirtySibling") {
                child.mark_needs_rebuild();
            }
        });
        root.rebuild_if_dirty(&context);
        assert_eq!(clean_rebuilds.get(), 0);
        assert_eq!(first_rebuilds.get(), 1);
        assert_eq!(second_rebuilds.get(), 1);

        clean_rebuilds.set(0);
        first_rebuilds.set(0);
        second_rebuilds.set(0);
        root.visit_children(&mut |child| {
            if child.debug_name() == "FirstDirtySibling" {
                child.mark_needs_rebuild();
            }
        });
        root.rebuild_if_dirty(&context);

        assert_eq!(clean_rebuilds.get(), 0);
        assert_eq!(first_rebuilds.get(), 1);
        assert_eq!(second_rebuilds.get(), 0);
    }

    #[test]
    fn custom_dirty_mark_keeps_the_conservative_fallback() {
        let first_rebuilds = Rc::new(Cell::new(0));
        let second_rebuilds = Rc::new(Cell::new(0));
        let root = SiblingBranch {
            children: vec![
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: first_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "FirstSibling",
                )
                .boxed(),
                StatelessElement::wrapper(
                    CountingLeaf {
                        rebuilds: second_rebuilds.clone(),
                    }
                    .boxed(),
                    None,
                    "SecondSibling",
                )
                .boxed(),
            ],
        }
        .boxed();
        let context = dummy_build_context();

        root.rebuild_if_dirty(&context);
        first_rebuilds.set(0);
        second_rebuilds.set(0);

        root.mark_needs_rebuild();
        root.rebuild_if_dirty(&context);

        assert_eq!(first_rebuilds.get(), 1);
        assert_eq!(second_rebuilds.get(), 1);
    }

    #[test]
    fn transparent_wrapper_preserves_child_flex_factor() {
        let wrapper = StatelessElement::wrapper(FlexibleLeaf.boxed(), None, "Wrapper");

        assert_eq!(wrapper.flex(), Some(2.5));
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
    fn compatible_generated_child_keeps_identity_across_rebuild() {
        let element = StatelessElement::new(Leaf.boxed(), |_| Leaf.boxed(), None, "Rebuildable");
        let original_id = unsafe { &*element.child.0.get() }.id();
        let generation = crate::element_tree_generation();
        element.mark_needs_rebuild();

        element.rebuild_if_dirty(&dummy_build_context());

        let rebuilt_id = unsafe { &*element.child.0.get() }.id();
        assert_eq!(rebuilt_id, original_id);
        assert!(crate::element_tree_generation() > generation);
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
                if context.current_build_consumer().is_some() {
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

    #[test]
    fn named_widget_materializes_a_single_child_as_a_transparent_wrapper() {
        const LIMITS: ModelLimits = ModelLimits::new(1_024, 4, 8, 8).max_widget_depth(2);
        let nodes = [WidgetNode::new(WidgetSchemaId::new(1), Version::new(1, 0))];
        let image = WidgetDocument::new(0, 0, 0, &nodes, &[], &[])
            .encode(LIMITS)
            .unwrap();
        let document = aimer_anteros::WidgetDocumentView::decode(&image, LIMITS).unwrap();
        let node = document.node(0).unwrap();
        let materialized = <NamedWidget as PortableNativeWidget>::materialize_widget(
            &document,
            node,
            vec![crate::ErrorWidget::new("child").boxed()],
        )
        .unwrap();

        assert_eq!(Widget::debug_name(&materialized), "ErrorWidget");
        let widget_type = <NamedWidget as PortableWidgetSchema>::SCHEMA.widget().id();
        assert!(linked_portable_native_widget_registrations()
            .iter()
            .any(|registration| registration.widget_type() == widget_type));
        assert!(matches!(
            <NamedWidget as PortableNativeWidget>::materialize_widget(
                &document,
                document.node(0).unwrap(),
                vec![],
            ),
            Err(crate::portable::PortableMaterializeError::InvalidChildCount {
                expected: 1,
                actual: 0,
            }),
        ));
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn named_widget_guest_lowering_is_transparent_to_its_inner_widget() {
        use crate::portable::{
            PortableBuildContext, PortableLimits, PortableWidgetLimits, SourceFingerprint,
            StableId128,
        };

        let mut context = PortableBuildContext::new(
            1,
            1,
            PortableWidgetLimits::new(8, 8, 8, 8, 64, 2_048),
            PortableLimits::new(8, 16, 64, 128, 1_024),
        )
        .unwrap();
        let root = NamedWidget::new(crate::ErrorWidget::new("child").boxed(), "InspectorName")
            .to_portable_node(
                &mut context,
                SourceFingerprint::new(StableId128::from_bytes([1; 16])),
            )
            .unwrap();
        let graph = context.finish_graph(root).unwrap();

        assert_eq!(graph.node_count(), 1);
        assert_eq!(
            graph.node(root).unwrap().widget_type(),
            <crate::ErrorWidget as PortableWidgetSchema>::SCHEMA
                .widget()
                .id(),
        );
    }
}
