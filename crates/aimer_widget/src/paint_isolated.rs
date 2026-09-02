use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use aimer_attribute::size::ResolvedSize;
use aimer_canvas::{Canvas, RETAINED_LAYER_MAX_BYTES, RETAINED_LAYER_MAX_DIMENSION,
    RetainedLayerContent};

use crate::base::BuildContext;
use crate::components::element::Element;

/// The local bounds that a retained paint recording promises to cover.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaintBounds {
    bits: [u32; 4],
}

impl PaintBounds {
    /// Creates a bounds key from logical coordinates and dimensions.
    #[inline]
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            bits: [x.to_bits(), y.to_bits(), width.to_bits(), height.to_bits()],
        }
    }
}

/// The clip contract active while a retained layer is composed.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaintClip {
    bounds: PaintBounds,
    border_radius_bits: [u32; 4],
    active: bool,
}

impl PaintClip {
    /// Creates a key for an unclipped paint region.
    #[inline]
    pub const fn none() -> Self {
        Self {
            bounds: PaintBounds { bits: [0; 4] },
            border_radius_bits: [0; 4],
            active: false,
        }
    }

    /// Creates a key for an axis-aligned rectangular clip.
    #[inline]
    pub fn rect(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            bounds: PaintBounds::new(x, y, width, height),
            border_radius_bits: [0; 4],
            active: true,
        }
    }

    /// Creates a key for a rounded rectangular clip.
    #[inline]
    pub fn rounded(x: f32, y: f32, width: f32, height: f32, border_radius: [f32; 4]) -> Self {
        Self {
            bounds: PaintBounds::new(x, y, width, height),
            border_radius_bits: border_radius.map(f32::to_bits),
            active: true,
        }
    }
}

/// The local-to-device transform that affects retained paint rasterization.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaintTransform {
    matrix_bits: [u32; 9],
}

impl PaintTransform {
    /// Creates the identity transform key.
    #[inline]
    pub const fn identity() -> Self {
        Self {
            matrix_bits: [
                1.0_f32.to_bits(),
                0,
                0,
                0,
                1.0_f32.to_bits(),
                0,
                0,
                0,
                1.0_f32.to_bits(),
            ],
        }
    }

    /// Creates a transform key from the canvas's current matrix.
    #[inline]
    pub fn from_canvas(canvas: &Canvas) -> Self {
        Self::from_matrix(canvas.get_transform())
    }

    /// Creates a transform key from a canvas matrix.
    #[inline]
    pub fn from_matrix(matrix: aimer_canvas::Mat3) -> Self {
        Self {
            matrix_bits: [
                matrix.cols[0][0].to_bits(),
                matrix.cols[0][1].to_bits(),
                matrix.cols[0][2].to_bits(),
                matrix.cols[1][0].to_bits(),
                matrix.cols[1][1].to_bits(),
                matrix.cols[1][2].to_bits(),
                matrix.cols[2][0].to_bits(),
                matrix.cols[2][1].to_bits(),
                matrix.cols[2][2].to_bits(),
            ],
        }
    }
}

/// All framework-owned inputs that can change a retained paint recording.
///
/// The live scroll translation is intentionally not implicit in this value:
/// an adopter supplies the transform at the retained layer's local seam, while
/// a composition-only translation remains outside the contract. Every other
/// content, lifecycle, layout, geometry, scale, clip, transform, and renderer
/// resource input is represented explicitly so a cache hit is explainable.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PaintContract {
    pub content_generation: u64,
    pub rebuild_generation: u64,
    pub tree_generation: u64,
    pub layout_generation: u64,
    pub bounds: PaintBounds,
    pub device_scale_bits: u32,
    pub clip: PaintClip,
    pub transform: PaintTransform,
    pub resource_generation: u64,
}

impl PaintContract {
    /// Creates a complete retained-paint contract.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn new(
        content_generation: u64,
        rebuild_generation: u64,
        tree_generation: u64,
        layout_generation: u64,
        bounds: PaintBounds,
        device_scale: f32,
        clip: PaintClip,
        transform: PaintTransform,
        resource_generation: u64,
    ) -> Self {
        Self {
            content_generation,
            rebuild_generation,
            tree_generation,
            layout_generation,
            bounds,
            device_scale_bits: device_scale.to_bits(),
            clip,
            transform,
            resource_generation,
        }
    }

    /// Builds a contract from the current framework context and an adopter's
    /// retained-layer bounds and clip.
    #[inline]
    pub fn from_context(
        ctx: &BuildContext,
        content_generation: u64,
        rebuild_generation: u64,
        tree_generation: u64,
        layout_generation: u64,
        bounds: PaintBounds,
        clip: PaintClip,
        transform: PaintTransform,
    ) -> Self {
        Self::new(
            content_generation,
            rebuild_generation,
            tree_generation,
            layout_generation,
            bounds,
            ctx.scale,
            clip,
            transform,
            ctx.canvas.texture_cache_epoch(),
        )
    }

    /// Returns whether placement and resource inputs are unchanged.
    ///
    /// Dynamic-island adopters use this narrower comparison while their
    /// dynamic descendants continue through the live path.
    #[doc(hidden)]
    #[inline]
    pub fn placement_matches(&self, other: &Self) -> bool {
        self.layout_generation == other.layout_generation
            && self.bounds == other.bounds
            && self.device_scale_bits == other.device_scale_bits
            && self.clip == other.clip
            && self.transform == other.transform
            && self.resource_generation == other.resource_generation
    }
}

/// Result of applying the framework's retained-paint policy to one child.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaintIsolatedOutcome {
    /// The child was recorded into a new retained layer and emitted.
    Recorded,
    /// A previously recorded layer was emitted without calling the child paint.
    Replayed,
    /// The child was painted directly because retention was unsafe or unavailable.
    DirectFallback,
}

/// Shared validity state for a framework-owned retained paint cache.
///
/// The cache owns the recorded key and the element identities reached while
/// recording. An adopter supplies whether two different keys still describe
/// the same paint contract; this module owns the invalidation-epoch and
/// element-identity checks that make that contract safe to reuse.
#[doc(hidden)]
pub struct PaintCache<K: Copy + PartialEq> {
    key: Option<K>,
    element_ids: Vec<crate::components::element::ElementId>,
}

impl<K: Copy + PartialEq> PaintCache<K> {
    /// Creates an empty retained-paint validity cache.
    #[inline]
    pub fn new() -> Self {
        Self {
            key: None,
            element_ids: Vec::new(),
        }
    }

    /// Replaces the recorded key and the element identities it depends on.
    #[inline]
    pub fn record(
        &mut self,
        key: K,
        element_ids: Vec<crate::components::element::ElementId>,
    ) {
        self.key = Some(key);
        self.element_ids = element_ids;
    }

    /// Returns the key associated with the current recorded content.
    #[inline]
    pub fn key(&self) -> Option<K> {
        self.key
    }

    /// Returns the element identities recorded with the current content.
    #[inline]
    pub fn tracked_elements(&self) -> &[crate::components::element::ElementId] {
        &self.element_ids
    }

    /// Returns whether the recorded content is safe for `key`.
    ///
    /// `contract_matches` must be true only when the caller has established
    /// that a changed key leaves the recorded paint contract compatible. An
    /// identical key avoids the adopter's contract comparison; every key
    /// still requires known invalidation provenance and no invalidated
    /// recorded element.
    #[inline]
    pub fn can_reuse(&self, key: K, contract_matches: bool) -> bool {
        let Some(cached_key) = self.key else {
            return false;
        };
        let contract_matches = cached_key == key || contract_matches;
        contract_matches
            && crate::components::element::paint_invalidations_are_known()
            && self
                .element_ids
                .iter()
                .all(|element| !crate::components::element::paint_element_was_invalidated(*element))
    }

    /// Retires the recorded validity state and reports one invalidation.
    #[inline]
    pub fn invalidate(&mut self) -> bool {
        let had_cache = self.key.take().is_some();
        self.element_ids.clear();
        if had_cache {
            crate::paint_stats::record_paint_isolation_invalidation();
        }
        had_cache
    }

    /// Drops the recorded validity state without counting a repaint.
    ///
    /// This is used when an adopter changes retention policy, such as moving
    /// from one full layer to bounded tiles. The policy transition itself is
    /// not an invalidated paint dependency.
    #[inline]
    pub fn clear(&mut self) {
        self.key = None;
        self.element_ids.clear();
    }
}

impl<K: Copy + PartialEq> Default for PaintCache<K> {
    fn default() -> Self {
        Self::new()
    }
}

struct RetainedPaint<K: Copy + PartialEq> {
    cache: PaintCache<K>,
    content: Arc<RetainedLayerContent>,
}

/// Framework-owned retained paint for one independently invalidated child.
///
/// The owner remains responsible for choosing a safe visual seam and for
/// supplying a key that changes when its layout, resource, or composition
/// contract changes. The child itself remains available for rebuild, layout,
/// and event routing; this type only chooses whether its paint is recorded or
/// replayed for the current frame. Geometry synchronization is performed
/// separately from the retained paint path. Framework adopters should use
/// [`PaintContract`] as the common part of that key so content/rebuild
/// generations, bounds, scale, clip, transform, and resources are not omitted.
///
/// This is an internal framework API. Ordinary widget users do not construct
/// it; built-in Modules such as scroll views and panes own its lifetime.
#[doc(hidden)]
pub struct PaintIsolated<K: Copy + PartialEq> {
    layer_id: u64,
    cache: RefCell<Option<RetainedPaint<K>>>,
}

static NEXT_LAYER_ID: AtomicU64 = AtomicU64::new(1);

impl<K: Copy + PartialEq> PaintIsolated<K> {
    /// Creates an empty retained-paint seam.
    #[inline]
    pub fn new() -> Self {
        Self {
            layer_id: NEXT_LAYER_ID.fetch_add(1, Ordering::Relaxed),
            cache: RefCell::new(None),
        }
    }

    /// Drops the retained layer so the next draw uses a fresh recording.
    #[inline]
    pub fn clear(&self) {
        self.cache.borrow_mut().take();
    }

    /// Records, replays, or directly paints `child` for the current frame.
    ///
    /// `key` belongs to the framework-owned adopter. It must include every
    /// dependency that can change the child's local command stream, while the
    /// current canvas transform remains outside the key so an offset-only
    /// composition can replay the same local layer.
    #[inline]
    pub fn draw(
        &self,
        ctx: &BuildContext,
        child_ctx: &BuildContext,
        child: &dyn Element,
        key: K,
        content_size: ResolvedSize,
    ) -> PaintIsolatedOutcome {
        let stable = child.is_paint_stable();
        #[cfg(debug_assertions)]
        let stable = stable && !crate::inspector_overlay::is_enabled();
        if !stable || !retained_layer_size_is_supported(content_size) {
            self.invalidate_cached_layer();
            self.paint_directly(child, child_ctx);
            return PaintIsolatedOutcome::DirectFallback;
        }

        // Geometry is live even when the paint command stream is not. This is
        // deliberately done before both record and replay so a translated
        // retained child keeps hit-testing and focus bounds in the current
        // coordinate space without entering its paint path.
        child.sync_paint_geometry(child_ctx);

        let can_replay = self.cache.borrow().as_ref().is_some_and(|cached| {
            cached.cache.can_reuse(key, false)
        });
        if can_replay {
            let content = self
                .cache
                .borrow()
                .as_ref()
                .expect("retained-paint cache disappeared during replay")
                .content
                .clone();
            crate::paint_stats::record_paint_isolation_replay();
            ctx.canvas.draw_retained_layer(
                self.layer_id,
                content_size.width,
                content_size.height,
                content,
            );
            return PaintIsolatedOutcome::Replayed;
        }

        self.invalidate_cached_layer();

        let recording_canvas = ctx.canvas.fork_for_recording();
        let mut recording_ctx = child_ctx.clone();
        recording_ctx.replace_canvas(Canvas::new(&recording_canvas));
        recording_ctx.visible_rect = None;
        crate::components::element::begin_paint_tracking();
        child.paint(&recording_ctx);
        let element_ids = crate::components::element::take_paint_tracking();

        let recorded = recording_canvas.take_draw_list();
        let Some(commands) = recorded.retained_snapshot() else {
            self.paint_directly(child, child_ctx);
            return PaintIsolatedOutcome::DirectFallback;
        };
        if commands.is_empty() {
            self.paint_directly(child, child_ctx);
            return PaintIsolatedOutcome::DirectFallback;
        }

        let content = Arc::new(RetainedLayerContent::from_snapshot(commands));
        if !content.is_compositor_safe() {
            self.paint_directly(child, child_ctx);
            return PaintIsolatedOutcome::DirectFallback;
        }

        crate::paint_stats::record_paint_isolation_record();
        let mut cache = PaintCache::new();
        cache.record(key, element_ids);
        self.cache.borrow_mut().replace(RetainedPaint {
            cache,
            content: content.clone(),
        });
        ctx.canvas.draw_retained_layer(
            self.layer_id,
            content_size.width,
            content_size.height,
            content,
        );
        PaintIsolatedOutcome::Recorded
    }

    fn invalidate_cached_layer(&self) {
        if let Some(mut cached) = self.cache.borrow_mut().take() {
            cached.cache.invalidate();
        }
    }

    #[inline]
    fn paint_directly(&self, child: &dyn Element, child_ctx: &BuildContext) {
        crate::paint_stats::record_paint_isolation_fallback();
        child.draw(child_ctx);
    }
}

impl<K: Copy + PartialEq> Default for PaintIsolated<K> {
    fn default() -> Self {
        Self::new()
    }
}

#[inline]
fn retained_layer_size_is_supported(size: ResolvedSize) -> bool {
    if !size.width.is_finite() || !size.height.is_finite() || size.width <= 0.0 || size.height <= 0.0
    {
        return false;
    }
    let width = size.width.ceil().max(1.0) as u64;
    let height = size.height.ceil().max(1.0) as u64;
    width <= u64::from(RETAINED_LAYER_MAX_DIMENSION)
        && height <= u64::from(RETAINED_LAYER_MAX_DIMENSION)
        && width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .is_some_and(|bytes| bytes <= RETAINED_LAYER_MAX_BYTES)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use aimer_attribute::position::Vec2d;
    use aimer_attribute::size::ResolvedSize;
    use aimer_events::element::ElementEvent;

    use super::*;
    use crate::base::{BuildContext, WindowHandle};
    use crate::{Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement};

    struct CountingElement {
        draws: Rc<Cell<u32>>,
        stable: bool,
    }

    struct GeometryAwareElement {
        draws: Rc<Cell<u32>>,
        geometry_x: Rc<Cell<f32>>,
        geometry_y: Rc<Cell<f32>>,
    }

    impl VisitorElement for GeometryAwareElement {
        fn debug_name(&self) -> &'static str {
            "GeometryAwareElement"
        }
    }

    impl EventElement for GeometryAwareElement {}

    impl LayoutElement for GeometryAwareElement {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            ResolvedSize {
                width: 40.0,
                height: 40.0,
            }
        }
    }

    impl Drawable for GeometryAwareElement {
        fn draw(&self, ctx: &BuildContext) {
            self.draws.set(self.draws.get() + 1);
            ctx.canvas.fill_rect(
                Vec2d::ZERO,
                ResolvedSize {
                    width: 40.0,
                    height: 40.0,
                },
            );
        }

        fn sync_paint_geometry(&self, ctx: &BuildContext) {
            let (x, y) = ctx.canvas.get_transform_translation();
            self.geometry_x.set(x);
            self.geometry_y.set(y);
        }

        fn is_paint_stable(&self) -> bool {
            true
        }
    }

    impl Rebuildable for GeometryAwareElement {}

    impl VisitorElement for CountingElement {
        fn debug_name(&self) -> &'static str {
            "CountingElement"
        }
    }

    impl EventElement for CountingElement {
        fn on_event(&self, _event: &ElementEvent) -> crate::EventResult {
            crate::EventResult::ignored()
        }
    }

    impl LayoutElement for CountingElement {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            ResolvedSize {
                width: 40.0,
                height: 40.0,
            }
        }
    }

    impl Drawable for CountingElement {
        fn draw(&self, ctx: &BuildContext) {
            self.draws.set(self.draws.get() + 1);
            ctx.canvas.fill_rect(
                Vec2d::ZERO,
                ResolvedSize {
                    width: 40.0,
                    height: 40.0,
                },
            );
        }

        fn is_paint_stable(&self) -> bool {
            self.stable
        }
    }

    impl Rebuildable for CountingElement {}

    fn context() -> BuildContext<'static> {
        let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
        BuildContext::new(
            aimer_canvas::Canvas::new(inner),
            ResolvedSize {
                width: 100.0,
                height: 100.0,
            },
            1.0,
            Vec2d::ZERO,
            Vec2d::ZERO,
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        )
    }

    #[tokio::test]
    async fn records_then_replays_a_stable_child_without_drawing_it_again() {
        let draws = Rc::new(Cell::new(0));
        let child = CountingElement {
            draws: draws.clone(),
            stable: true,
        }
        .boxed();
        let ctx = context();
        let isolated = PaintIsolated::<u64>::new();
        let size = ResolvedSize {
            width: 40.0,
            height: 40.0,
        };

        assert_eq!(isolated.draw(&ctx, &ctx, child.as_ref(), 7, size), PaintIsolatedOutcome::Recorded);
        assert_eq!(draws.get(), 1);

        ctx.canvas.begin_frame();
        assert_eq!(isolated.draw(&ctx, &ctx, child.as_ref(), 7, size), PaintIsolatedOutcome::Replayed);
        assert_eq!(draws.get(), 1);
        assert_eq!(ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers, 1);
    }

    #[tokio::test]
    async fn a_contract_change_invalidates_and_records_a_fresh_child() {
        let draws = Rc::new(Cell::new(0));
        let child = CountingElement {
            draws: draws.clone(),
            stable: true,
        }
        .boxed();
        let ctx = context();
        let isolated = PaintIsolated::<u64>::new();
        let size = ResolvedSize {
            width: 40.0,
            height: 40.0,
        };

        assert_eq!(isolated.draw(&ctx, &ctx, child.as_ref(), 1, size), PaintIsolatedOutcome::Recorded);
        ctx.canvas.begin_frame();
        assert_eq!(isolated.draw(&ctx, &ctx, child.as_ref(), 2, size), PaintIsolatedOutcome::Recorded);
        assert_eq!(draws.get(), 2);
    }

    #[tokio::test]
    async fn an_invalidated_child_is_recorded_again_even_when_the_owner_key_is_unchanged() {
        let draws = Rc::new(Cell::new(0));
        let child = CountingElement {
            draws: draws.clone(),
            stable: true,
        }
        .boxed();
        let ctx = context();
        let isolated = PaintIsolated::<u64>::new();
        let size = ResolvedSize {
            width: 40.0,
            height: 40.0,
        };

        assert_eq!(
            isolated.draw(&ctx, &ctx, child.as_ref(), 1, size),
            PaintIsolatedOutcome::Recorded
        );
        child.mark_needs_rebuild();

        ctx.canvas.begin_frame();
        assert_eq!(
            isolated.draw(&ctx, &ctx, child.as_ref(), 1, size),
            PaintIsolatedOutcome::Recorded
        );
        assert_eq!(draws.get(), 2);
    }

    #[derive(Clone, Copy, PartialEq)]
    struct PaintInputs {
        clip_generation: u32,
        local_transform_generation: u32,
        resource_generation: u64,
    }

    #[tokio::test]
    async fn the_owner_key_covers_clip_transform_and_resource_changes() {
        let draws = Rc::new(Cell::new(0));
        let child = CountingElement {
            draws: draws.clone(),
            stable: true,
        }
        .boxed();
        let ctx = context();
        let isolated = PaintIsolated::<PaintInputs>::new();
        let size = ResolvedSize {
            width: 40.0,
            height: 40.0,
        };
        let base = PaintInputs {
            clip_generation: 1,
            local_transform_generation: 1,
            resource_generation: 1,
        };

        assert_eq!(
            isolated.draw(&ctx, &ctx, child.as_ref(), base, size),
            PaintIsolatedOutcome::Recorded
        );
        for changed in [
            PaintInputs {
                clip_generation: 2,
                ..base
            },
            PaintInputs {
                local_transform_generation: 2,
                ..base
            },
            PaintInputs {
                resource_generation: 2,
                ..base
            },
        ] {
            ctx.canvas.begin_frame();
            assert_eq!(
                isolated.draw(&ctx, &ctx, child.as_ref(), changed, size),
                PaintIsolatedOutcome::Recorded
            );
        }

        assert_eq!(draws.get(), 4);
    }

    #[tokio::test]
    async fn the_framework_contract_tracks_every_retained_paint_dependency() {
        let draws = Rc::new(Cell::new(0));
        let child = CountingElement {
            draws: draws.clone(),
            stable: true,
        }
        .boxed();
        let ctx = context();
        let isolated = PaintIsolated::<PaintContract>::new();
        let size = ResolvedSize {
            width: 40.0,
            height: 40.0,
        };
        let base = PaintContract::new(
            1,
            2,
            3,
            4,
            PaintBounds::new(5.0, 6.0, 40.0, 40.0),
            1.0,
            PaintClip::rect(0.0, 0.0, 40.0, 40.0),
            PaintTransform::identity(),
            9,
        );

        assert_eq!(
            isolated.draw(&ctx, &ctx, child.as_ref(), base, size),
            PaintIsolatedOutcome::Recorded
        );
        for changed in [
            PaintContract {
                content_generation: 2,
                ..base
            },
            PaintContract {
                rebuild_generation: 3,
                ..base
            },
            PaintContract {
                tree_generation: 4,
                ..base
            },
            PaintContract {
                layout_generation: 5,
                ..base
            },
            PaintContract {
                bounds: PaintBounds::new(5.0, 6.0, 41.0, 40.0),
                ..base
            },
            PaintContract {
                device_scale_bits: 2.0_f32.to_bits(),
                ..base
            },
            PaintContract {
                clip: PaintClip::rect(0.0, 0.0, 39.0, 40.0),
                ..base
            },
            PaintContract {
                transform: PaintTransform::from_matrix(aimer_canvas::Mat3::scale(2.0, 1.0)),
                ..base
            },
            PaintContract {
                resource_generation: 10,
                ..base
            },
        ] {
            ctx.canvas.begin_frame();
            assert_eq!(
                isolated.draw(&ctx, &ctx, child.as_ref(), changed, size),
                PaintIsolatedOutcome::Recorded
            );
        }

        assert_eq!(draws.get(), 10);
    }

    #[tokio::test]
    async fn context_contract_captures_scale_and_renderer_resource_epoch() {
        let ctx = context();
        let base = PaintContract::from_context(
            &ctx,
            1,
            2,
            3,
            4,
            PaintBounds::new(0.0, 0.0, 40.0, 40.0),
            PaintClip::none(),
            PaintTransform::from_canvas(&ctx.canvas),
        );

        ctx.canvas.load_image_with_id(91, &[255, 0, 0, 255], 1, 1);
        let mut scaled_ctx = ctx.clone();
        scaled_ctx.scale = 2.0;
        let changed = PaintContract::from_context(
            &scaled_ctx,
            1,
            2,
            3,
            4,
            PaintBounds::new(0.0, 0.0, 40.0, 40.0),
            PaintClip::none(),
            PaintTransform::from_canvas(&scaled_ctx.canvas),
        );

        assert_ne!(base.device_scale_bits, changed.device_scale_bits);
        assert_ne!(base.resource_generation, changed.resource_generation);
    }

    #[tokio::test]
    async fn an_unstable_child_uses_the_direct_fallback() {
        let draws = Rc::new(Cell::new(0));
        let child = CountingElement {
            draws: draws.clone(),
            stable: false,
        }
        .boxed();
        let ctx = context();
        let isolated = PaintIsolated::<u64>::new();

        assert_eq!(
            isolated.draw(
                &ctx,
                &ctx,
                child.as_ref(),
                1,
                ResolvedSize {
                    width: 40.0,
                    height: 40.0,
                },
            ),
            PaintIsolatedOutcome::DirectFallback
        );
        assert_eq!(draws.get(), 1);
        assert_eq!(ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers, 0);
    }

    #[tokio::test]
    async fn replay_syncs_geometry_without_repainting_a_stable_child() {
        let draws = Rc::new(Cell::new(0));
        let geometry_x = Rc::new(Cell::new(0.0));
        let geometry_y = Rc::new(Cell::new(0.0));
        let child = GeometryAwareElement {
            draws: draws.clone(),
            geometry_x: geometry_x.clone(),
            geometry_y: geometry_y.clone(),
        }
        .boxed();
        let isolated = PaintIsolated::<u64>::new();
        let size = ResolvedSize {
            width: 40.0,
            height: 40.0,
        };

        let first = context();
        first.canvas.translate(Vec2d { x: 10.0, y: 20.0 });
        assert_eq!(
            isolated.draw(&first, &first, child.as_ref(), 1, size),
            PaintIsolatedOutcome::Recorded
        );
        assert_eq!(draws.get(), 1);
        assert_eq!((geometry_x.get(), geometry_y.get()), (10.0, 20.0));

        let second = context();
        second.canvas.translate(Vec2d { x: 30.0, y: 40.0 });
        assert_eq!(
            isolated.draw(&second, &second, child.as_ref(), 1, size),
            PaintIsolatedOutcome::Replayed
        );
        assert_eq!(draws.get(), 1);
        assert_eq!((geometry_x.get(), geometry_y.get()), (30.0, 40.0));
    }

    #[test]
    fn shared_cache_requires_a_matching_contract_for_a_changed_key() {
        let mut cache = PaintCache::new();
        cache.record(7_u64, Vec::new());

        assert!(cache.can_reuse(7, false));
        assert!(!cache.can_reuse(8, false));
    }
}
