use std::cell::{Cell, RefCell};
#[cfg(not(feature = "portable-guest"))]
use std::collections::HashMap;
use std::rc::Rc;

use aimer_attribute::CacheBounds;
use aimer_attribute::BoxConstraint;
use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
#[cfg(not(feature = "portable-guest"))]
use aimer_canvas::{
    Canvas, InnerCanvas, RETAINED_LAYER_MAX_BYTES, RETAINED_LAYER_MAX_DIMENSION,
    RETAINED_LAYER_MAX_TILES_PER_FRAME, RETAINED_LAYER_TILE_SIZE,
};
use aimer_widget::base::*;
use aimer_widget::{
    AnyElement, Element, EventDispatcher, Rebuildable, element_tree_generation,
    layout_invalidation_generation,
};
#[cfg(not(feature = "portable-guest"))]
use aimer_widget::{
    ElementId, rebuild_invalidation_generation,
};
#[cfg(not(feature = "portable-guest"))]
use std::sync::Arc;
#[cfg(not(feature = "portable-guest"))]
use std::sync::atomic::{AtomicU64, Ordering};

pub use crate::scrollable::controller::DragMode;
use crate::scrollable::controller::ScrollState;
use crate::scrollable::scroll_bar::ScrollBar;

#[derive(Clone, Copy, PartialEq)]
struct ScrollLayoutKey {
    constraint: BoxConstraint,
    parent_size: ResolvedSize,
    scale_bits: u32,
    axis: crate::ScrollAxis,
    viewport_w: f32,
    viewport_h: f32,
    vertical_bar_width: f32,
    horizontal_bar_height: f32,
    tree_generation: u64,
    layout_generation: u64,
}

#[derive(Clone, Copy)]
struct CachedScrollLayout {
    key: ScrollLayoutKey,
    extents: ((f32, f32), (f32, f32)),
    content_size: Option<ResolvedSize>,
}

/// Memoizes the scroll viewport and content measurements independently of the
/// live scroll offset.
///
/// The key includes the parent size as well as the child-facing constraint:
/// an unbounded scroll axis can resolve its viewport from the parent's size.
/// Scroll offset is deliberately absent, so a frame that only translates the
/// content keeps the same layout snapshot. Tree and layout generations retire
/// the snapshot when the content or its constraints can have changed.
#[derive(Default)]
pub(crate) struct ScrollLayoutCache {
    snapshot: Cell<Option<CachedScrollLayout>>,
}

#[cfg(not(feature = "portable-guest"))]
#[derive(Clone, Copy, PartialEq)]
struct ScrollPaintKey {
    contract: aimer_widget::PaintContract,
    layout: ScrollLayoutKey,
}

#[cfg(not(feature = "portable-guest"))]
struct RetainedPaint {
    cache: aimer_widget::PaintCache<ScrollPaintKey>,
    content: Arc<aimer_canvas::RetainedLayerContent>,
    dynamic_islands: bool,
}

#[cfg(not(feature = "portable-guest"))]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RetainedTileCoordinate {
    x: u32,
    y: u32,
}

#[cfg(not(feature = "portable-guest"))]
#[derive(Clone, Copy)]
struct RetainedTileRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[cfg(not(feature = "portable-guest"))]
struct RetainedPaintTile {
    layer_id: u64,
    rect: RetainedTileRect,
    content: Arc<aimer_canvas::RetainedLayerContent>,
    cache: aimer_widget::PaintCache<ScrollPaintKey>,
}

#[cfg(not(feature = "portable-guest"))]
struct RetainedTileDraw {
    layer_id: u64,
    rect: RetainedTileRect,
    content: Arc<aimer_canvas::RetainedLayerContent>,
}

#[cfg(not(feature = "portable-guest"))]
struct DynamicIslandPaintState {
    recording: Option<InnerCanvas>,
    tracking: bool,
    element_ids: Vec<ElementId>,
    static_content: Option<Arc<aimer_canvas::RetainedLayerContent>>,
    emitted: bool,
    failed: bool,
}

#[cfg(not(feature = "portable-guest"))]
impl DynamicIslandPaintState {
    fn cached(
        cached: Option<(
            Arc<aimer_canvas::RetainedLayerContent>,
            Vec<ElementId>,
        )>,
    ) -> Self {
        let (static_content, element_ids) = cached.unzip();
        Self {
            recording: None,
            tracking: false,
            element_ids: element_ids.unwrap_or_default(),
            static_content,
            emitted: false,
            failed: false,
        }
    }

    fn begin_recording(&mut self, canvas: &Canvas) {
        if self.recording.is_none() {
            self.recording = Some(canvas.fork_for_recording());
            aimer_widget::components::element::begin_paint_tracking();
            self.tracking = true;
        }
    }

    fn finish_recording(
        &mut self,
    ) -> Option<(
        Arc<aimer_canvas::RetainedLayerContent>,
        Vec<ElementId>,
    )> {
        if self.static_content.is_some() {
            return self
                .static_content
                .clone()
                .map(|content| (content, self.element_ids.clone()));
        }

        if self.tracking {
            self.element_ids = aimer_widget::components::element::take_paint_tracking();
            self.tracking = false;
        }
        let recording = self.recording.take()?;
        let commands = recording.take_draw_list().retained_snapshot()?;
        let content = Arc::new(aimer_canvas::RetainedLayerContent::from_snapshot(commands));
        if !content.is_compositor_safe() {
            self.failed = true;
            return None;
        }
        aimer_widget::record_paint_isolation_record();
        self.static_content = Some(content.clone());
        Some((content, self.element_ids.clone()))
    }
}

#[cfg(not(feature = "portable-guest"))]
#[derive(Clone, Copy)]
struct RetainedTileRange {
    x_start: u32,
    x_end: u32,
    y_start: u32,
    y_end: u32,
}

#[cfg(not(feature = "portable-guest"))]
const RETAINED_LAYER_TILE_OVERLAP_PX: f32 = 2.0;

#[cfg(not(feature = "portable-guest"))]
#[inline]
fn can_use_retained_layer(size: ResolvedSize) -> bool {
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

#[cfg(not(feature = "portable-guest"))]
impl ScrollPaintKey {
    #[inline]
    fn base_contract_matches(self, other: Self) -> bool {
        self.contract.placement_matches(&other.contract)
            && self.layout.constraint == other.layout.constraint
            && self.layout.parent_size == other.layout.parent_size
            && self.layout.scale_bits == other.layout.scale_bits
            && self.layout.axis == other.layout.axis
            && self.layout.viewport_w == other.layout.viewport_w
            && self.layout.viewport_h == other.layout.viewport_h
            && self.layout.vertical_bar_width == other.layout.vertical_bar_width
            && self.layout.horizontal_bar_height == other.layout.horizontal_bar_height
            && self.layout.layout_generation == other.layout.layout_generation
    }

    #[inline]
    fn dynamic_contract_matches(self, other: Self) -> bool {
        self.base_contract_matches(other)
            && self.contract.tree_generation == other.contract.tree_generation
    }

    #[inline]
    fn tile_contract_matches(self, other: Self) -> bool {
        self.dynamic_contract_matches(other)
            && self.contract.content_generation == other.contract.content_generation
    }
}

#[cfg(not(feature = "portable-guest"))]
fn retained_tile_axis_range(start: f32, extent: f32, content_extent: f32) -> Option<(u32, u32)> {
    if !start.is_finite()
        || !extent.is_finite()
        || !content_extent.is_finite()
        || extent <= 0.0
        || content_extent <= 0.0
    {
        return None;
    }

    let start = start.max(0.0).min(content_extent);
    let end = (start + extent).min(content_extent);
    if end <= start {
        return None;
    }

    let tile_size = RETAINED_LAYER_TILE_SIZE as f32;
    let first = (start / tile_size).floor() as u64;
    let last = (end / tile_size).ceil() as u64;
    (last > first && last <= u64::from(u32::MAX))
        .then_some((first as u32, last as u32))
}

#[cfg(not(feature = "portable-guest"))]
fn retained_tile_range(
    visible_rect: Option<(f32, f32, f32, f32)>,
    content_size: ResolvedSize,
) -> Option<RetainedTileRange> {
    let (x, y, width, height) = visible_rect?;
    let (x_start, x_end) = retained_tile_axis_range(x, width, content_size.width)?;
    let (y_start, y_end) = retained_tile_axis_range(y, height, content_size.height)?;
    let tile_count = u64::from(x_end - x_start).checked_mul(u64::from(y_end - y_start))?;
    (tile_count <= RETAINED_LAYER_MAX_TILES_PER_FRAME as u64).then_some(RetainedTileRange {
        x_start,
        x_end,
        y_start,
        y_end,
    })
}

#[cfg(not(feature = "portable-guest"))]
fn retained_tile_rect(
    coordinate: RetainedTileCoordinate,
    content_size: ResolvedSize,
) -> Option<RetainedTileRect> {
    let tile_size = RETAINED_LAYER_TILE_SIZE as f32;
    let x = coordinate.x as f32 * tile_size;
    let y = coordinate.y as f32 * tile_size;
    let width = (content_size.width - x).min(tile_size);
    let height = (content_size.height - y).min(tile_size);
    (x.is_finite()
        && y.is_finite()
        && width.is_finite()
        && height.is_finite()
        && width > 0.0
        && height > 0.0)
        .then_some(RetainedTileRect {
            x,
            y,
            width,
            height,
        })
}

#[cfg(not(feature = "portable-guest"))]
pub(crate) struct ScrollPaintCache {
    layer_id: u64,
    isolated: aimer_widget::PaintIsolated<ScrollPaintKey>,
    snapshot: RefCell<Option<RetainedPaint>>,
    tiles: RefCell<HashMap<RetainedTileCoordinate, RetainedPaintTile>>,
    tile_cache: RefCell<aimer_widget::PaintCache<ScrollPaintKey>>,
    tile_draws: RefCell<Vec<RetainedTileDraw>>,
}

#[cfg(not(feature = "portable-guest"))]
static NEXT_SCROLL_LAYER_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(not(feature = "portable-guest"))]
impl Default for ScrollPaintCache {
    fn default() -> Self {
        Self {
            layer_id: NEXT_SCROLL_LAYER_ID.fetch_add(1, Ordering::Relaxed),
            isolated: aimer_widget::PaintIsolated::new(),
            snapshot: RefCell::new(None),
            tiles: RefCell::new(HashMap::new()),
            tile_cache: RefCell::new(aimer_widget::PaintCache::new()),
            tile_draws: RefCell::new(Vec::new()),
        }
    }
}

#[cfg(not(feature = "portable-guest"))]
impl ScrollPaintCache {
    fn clear(&self) {
        self.isolated.clear();
        self.snapshot.borrow_mut().take();
        self.tiles.borrow_mut().clear();
        self.tile_cache.borrow_mut().clear();
        self.tile_draws.borrow_mut().clear();
    }

    fn clear_tiles(&self) {
        self.tiles.borrow_mut().clear();
        self.tile_cache.borrow_mut().clear();
        self.tile_draws.borrow_mut().clear();
    }

    fn invalidate_snapshot(&self) {
        if let Some(mut snapshot) = self.snapshot.borrow_mut().take() {
            snapshot.cache.invalidate();
        }
    }
}

pub struct RawScrollableContainer<E: Element> {
    pub(crate) child: E,
    /// The live scroll engine. Held behind an `Rc` so an app-supplied
    /// [`ScrollController`](crate::ScrollController) can share the very same
    /// state and drive it programmatically across rebuilds.
    pub(crate) ctrl: Rc<ScrollState>,
    pub(crate) vertical_scroll_bar: Option<AnyElement>,
    pub(crate) horizontal_scroll_bar: Option<AnyElement>,
    pub(crate) viewport_w: f32,
    pub(crate) viewport_h: f32,
    pub(crate) vertical_bar_width: f32,
    pub(crate) horizontal_bar_height: f32,
    pub(crate) bounds: CacheBounds,
    pub(crate) event_dispatcher: RefCell<EventDispatcher>,
    pub(crate) layout_cache: ScrollLayoutCache,
    #[cfg(not(feature = "portable-guest"))]
    pub(crate) paint_cache: ScrollPaintCache,
}

impl<E: Element + 'static> Rebuildable for RawScrollableContainer<E> {
    #[inline]
    fn option_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// Claims the pointer the element being replaced was routing.
    ///
    /// This container dispatches to its child itself, so the capture a pressed
    /// child took lives in the dispatcher *beside* the children rather than in
    /// them — precisely the state reconciliation's positional walk cannot reach.
    /// A press that rebuilds the subtree while the pointer is still down — a
    /// button in the list darkening under the finger — would leave the
    /// replacement owning nothing, and
    /// [`child_route_allowed`](crate::scrollable::handle_scroll) gates child
    /// routing on exactly that capture: the release would be dismissed as
    /// landing outside the viewport and the captured child would be stranded
    /// mid-gesture, never hearing the pointer lift.
    ///
    /// The dispatcher is *moved* out of `old`, which reconciliation drops
    /// immediately afterwards. Two containers both believing they own the
    /// pointer would deliver every remaining event twice. It names the old
    /// subtree's identities, and those identities are transferred onto the new
    /// elements in the pass that follows this one, so the carried capture
    /// resolves once reconciliation completes.
    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        let Some(old) = old
            .option_any()
            .and_then(|value| value.downcast_ref::<Self>())
        else {
            return;
        };

        *self.event_dispatcher.borrow_mut() =
            std::mem::take(&mut *old.event_dispatcher.borrow_mut());
    }
}

impl<E: Element> RawScrollableContainer<E> {
    #[inline]
    fn layout_key(&self, ctx: &BuildContext) -> ScrollLayoutKey {
        ScrollLayoutKey {
            constraint: ctx.box_constraint,
            parent_size: ctx.parent_size,
            scale_bits: ctx.scale.to_bits(),
            axis: self.ctrl.axis,
            viewport_w: self.viewport_w,
            viewport_h: self.viewport_h,
            vertical_bar_width: self.vertical_bar_width,
            horizontal_bar_height: self.horizontal_bar_height,
            tree_generation: element_tree_generation(),
            layout_generation: layout_invalidation_generation(),
        }
    }

    #[cfg(not(feature = "portable-guest"))]
    #[inline]
    fn paint_key(
        &self,
        ctx: &BuildContext,
        content_size: ResolvedSize,
        clip: aimer_widget::PaintClip,
        transform: aimer_widget::PaintTransform,
    ) -> ScrollPaintKey {
        let layout = self.layout_key(ctx);
        ScrollPaintKey {
            contract: aimer_widget::PaintContract::from_context(
                ctx,
                self.child.subtree_generation(),
                rebuild_invalidation_generation(),
                layout.tree_generation,
                layout.layout_generation,
                aimer_widget::PaintBounds::new(
                    ctx.parent_pos.x,
                    ctx.parent_pos.y,
                    content_size.width,
                    content_size.height,
                ),
                clip,
                transform,
            ),
            layout,
        }
    }

    /// Paints a static child from a local retained command stream when its
    /// invalidation key still matches. Recording uses a forked canvas with no
    /// viewport rectangle, so the snapshot contains the complete subtree
    /// rather than only the range that happened to be visible on its first
    /// frame. Scroll translation and clip remain owned by the caller.
    #[cfg(not(feature = "portable-guest"))]
    pub(crate) fn draw_child_with_retained_paint(
        &self,
        ctx: &BuildContext,
        child_ctx: &BuildContext,
        content_size: ResolvedSize,
        clip: aimer_widget::PaintClip,
        transform: aimer_widget::PaintTransform,
    ) {
        aimer_widget::record_paint_isolation_candidate();
        // Rebuild is a lifecycle phase, not paint. Retained recording bypasses
        // `draw`, so the owner must still service dirty descendants before it
        // evaluates stability or replays a cached layer. The erased rebuild
        // path prunes clean subtrees after the first visit.
        self.child.rebuild_if_dirty(child_ctx);
        let stable = self.child.is_paint_stable();
        #[cfg(debug_assertions)]
        let stable = stable && !aimer_widget::inspector_overlay::is_enabled();
        if !stable {
            if self.draw_child_with_dynamic_islands(ctx, child_ctx, content_size, clip, transform) {
                return;
            }
            self.paint_cache.clear();
            aimer_widget::record_paint_isolation_fallback();
            self.child.draw(child_ctx);
            return;
        }

        let key = self.paint_key(ctx, content_size, clip, transform);
        if can_use_retained_layer(content_size) {
            self.paint_cache.clear_tiles();
            self.paint_cache
                .isolated
                .draw(
                    ctx,
                    child_ctx,
                    &self.child,
                    key,
                    content_size,
                );
            return;
        }

        // A full layer would exceed the memory cap. Keep only the tiles around
        // the cache window, with a small overlap so elements crossing a tile
        // edge are recorded into the tile that owns their visible pixels.
        self.paint_cache.isolated.clear();
        self.paint_cache.invalidate_snapshot();
        if self.draw_child_with_retained_tiles(ctx, child_ctx, content_size, key) {
            let draws = self.paint_cache.tile_draws.borrow();
            for draw in draws.iter() {
                ctx.canvas.draw_retained_layer_at(
                    draw.layer_id,
                    draw.rect.x,
                    draw.rect.y,
                    draw.rect.width,
                    draw.rect.height,
                    draw.content.clone(),
                );
            }
        } else {
            self.paint_cache.clear();
            aimer_widget::record_paint_isolation_fallback();
            self.child.draw(child_ctx);
        }
    }

    /// Retains a stable prefix while drawing a dynamic suffix through the
    /// ordinary element path. The flex partitioner has already proved that
    /// the stable content precedes every dynamic island, so one retained layer
    /// preserves paint order and the dynamic work remains visible to normal
    /// rebuild, input, and animation machinery.
    #[cfg(not(feature = "portable-guest"))]
    fn draw_child_with_dynamic_islands(
        &self,
        ctx: &BuildContext,
        child_ctx: &BuildContext,
        content_size: ResolvedSize,
        clip: aimer_widget::PaintClip,
        transform: aimer_widget::PaintTransform,
    ) -> bool {
        if !can_use_retained_layer(content_size) {
            return false;
        }

        let key = self.paint_key(ctx, content_size, clip, transform);
        let (cache_was_present, cached_content) = {
            let cached = self.paint_cache.snapshot.borrow();
            (
                cached.is_some(),
                cached
                    .as_ref()
                    .filter(|cached| {
                        cached.dynamic_islands
                            && cached.cache.key().is_some_and(|cached_key| {
                                cached
                                    .cache
                                    .can_reuse(key, cached_key.dynamic_contract_matches(key))
                            })
                    })
                    .map(|cached| {
                        (
                            cached.content.clone(),
                            cached.cache.tracked_elements().to_vec(),
                        )
                    }),
            )
        };
        if cache_was_present && cached_content.is_none() {
            self.paint_cache.invalidate_snapshot();
        }
        let state = Rc::new(RefCell::new(DynamicIslandPaintState::cached(
            cached_content,
        )));

        let mut retained_ctx = child_ctx.clone();
        retained_ctx.visible_rect = None;

        let mut draw_stable = {
            let state = Rc::clone(&state);
            move |element: &dyn Element,
                  island_ctx: &BuildContext,
                  offset: Vec2d,
                  clip: Option<ResolvedSize>| {
                // Stable islands still need live interaction geometry in the
                // current composition space. Keep that bookkeeping on the
                // live canvas, including the island's flex offset, while the
                // retained command stream uses the paint-only hook below.
                let geometry_ctx = island_ctx.clone();
                geometry_ctx.canvas.save();
                geometry_ctx.canvas.translate(offset);
                element.sync_paint_geometry(&geometry_ctx);
                geometry_ctx.canvas.restore();

                let mut state = state.borrow_mut();
                if state.static_content.is_some() {
                    return;
                }
                state.begin_recording(&ctx.canvas);
                let Some(recording) = state.recording.as_ref() else {
                    state.failed = true;
                    return;
                };
                let mut paint_ctx = island_ctx.clone();
                paint_ctx.replace_canvas(Canvas::new(recording));
                recording.save();
                if let Some(clip) = clip {
                    recording.set_clip(0.0, 0.0, clip.width, clip.height);
                }
                recording.translate(offset.x, offset.y);
                element.paint(&paint_ctx);
                if clip.is_some() {
                    recording.clear_clip();
                }
                recording.restore();
            }
        };

        let mut draw_dynamic = {
            let state = Rc::clone(&state);
            move |element: &dyn Element,
                  island_ctx: &BuildContext,
                  offset: Vec2d,
                  clip: Option<ResolvedSize>| {
                let mut state = state.borrow_mut();
                if !state.emitted {
                    let reused_cached = state.static_content.is_some();
                    if state.static_content.is_none() {
                        let Some((content, element_ids)) = state.finish_recording() else {
                            state.failed = true;
                            return;
                        };
                        let mut cache = aimer_widget::PaintCache::new();
                        cache.record(key, element_ids);
                        self.paint_cache.snapshot.borrow_mut().replace(RetainedPaint {
                            cache,
                            content: content.clone(),
                            dynamic_islands: true,
                        });
                    }
                    let Some(content) = state.static_content.clone() else {
                        state.failed = true;
                        return;
                    };
                    ctx.canvas.draw_retained_layer(
                        self.paint_cache.layer_id,
                        content_size.width,
                        content_size.height,
                        content,
                    );
                    state.emitted = true;
                    if reused_cached {
                        aimer_widget::record_paint_isolation_replay();
                    }
                }
                let failed = state.failed;
                drop(state);
                if failed {
                    return;
                }

                ctx.canvas.save();
                if let Some(clip) = clip {
                    ctx.canvas.set_clip(Vec2d::ZERO, clip);
                }
                ctx.canvas.translate(offset);
                element.draw(island_ctx);
                if clip.is_some() {
                    ctx.canvas.clear_clip();
                }
                ctx.canvas.restore();
            }
        };

        let handled = self.child.draw_paint_islands(
            &retained_ctx,
            child_ctx,
            &mut draw_stable,
            &mut draw_dynamic,
        );

        let mut state = state.borrow_mut();
        if state.tracking {
            state.element_ids = aimer_widget::components::element::take_paint_tracking();
            state.tracking = false;
        }
        if !handled || state.failed {
            return false;
        }
        if !state.emitted {
            let Some((content, element_ids)) = state.finish_recording() else {
                return false;
            };
            let mut cache = aimer_widget::PaintCache::new();
            cache.record(key, element_ids);
            self.paint_cache.snapshot.borrow_mut().replace(RetainedPaint {
                cache,
                content: content.clone(),
                dynamic_islands: true,
            });
            ctx.canvas.draw_retained_layer(
                self.paint_cache.layer_id,
                content_size.width,
                content_size.height,
                content,
            );
            state.emitted = true;
        }
        true
    }

    #[cfg(not(feature = "portable-guest"))]
    fn draw_child_with_retained_tiles(
        &self,
        ctx: &BuildContext,
        child_ctx: &BuildContext,
        content_size: ResolvedSize,
        key: ScrollPaintKey,
    ) -> bool {
        let Some(range) = retained_tile_range(child_ctx.visible_rect, content_size) else {
            return false;
        };

        // Tile replay skips the child's paint path, so keep its interaction
        // geometry current on every composition frame just like the full
        // PaintIsolated path does.
        self.child.sync_paint_geometry(child_ctx);

        let cached_tile_key = { self.paint_cache.tile_cache.borrow().key() };
        if let Some(cached_tile_key) = cached_tile_key {
            if cached_tile_key != key {
                let can_reuse = self.paint_cache.tile_cache.borrow().can_reuse(
                    key,
                    cached_tile_key.tile_contract_matches(key),
                );
                if !can_reuse {
                    self.paint_cache.tile_cache.borrow_mut().invalidate();
                    self.paint_cache.tiles.borrow_mut().clear();
                }
            }
        }
        self.paint_cache
            .tile_cache
            .borrow_mut()
            .record(key, Vec::new());
        self.paint_cache.tiles.borrow_mut().retain(|coordinate, tile| {
            if !(coordinate.x >= range.x_start
                && coordinate.x < range.x_end
                && coordinate.y >= range.y_start
                && coordinate.y < range.y_end)
            {
                return false;
            }
            let Some(cached_key) = tile.cache.key() else {
                return false;
            };
            if tile
                .cache
                .can_reuse(key, cached_key.tile_contract_matches(key))
            {
                true
            } else {
                tile.cache.invalidate();
                false
            }
        });

        let mut draws = self.paint_cache.tile_draws.borrow_mut();
        draws.clear();
        for y in range.y_start..range.y_end {
            for x in range.x_start..range.x_end {
                let coordinate = RetainedTileCoordinate { x, y };
                let cached = self
                    .paint_cache
                    .tiles
                    .borrow()
                    .get(&coordinate)
                    .map(|tile| RetainedTileDraw {
                        layer_id: tile.layer_id,
                        rect: tile.rect,
                        content: tile.content.clone(),
                    });
                let draw = if let Some(cached) = cached {
                    aimer_widget::record_paint_isolation_replay();
                    aimer_widget::record_paint_isolation_tile_replay();
                    cached
                } else {
                    let Some(rect) = retained_tile_rect(coordinate, content_size) else {
                        return false;
                    };
                    let Some((content, element_ids)) =
                        self.record_retained_tile(ctx, child_ctx, content_size, rect)
                    else {
                        return false;
                    };
                    let tile = RetainedPaintTile {
                        layer_id: NEXT_SCROLL_LAYER_ID.fetch_add(1, Ordering::Relaxed),
                        rect,
                        content: content.clone(),
                        cache: {
                            let mut cache = aimer_widget::PaintCache::new();
                            cache.record(key, element_ids);
                            cache
                        },
                    };
                    let draw = RetainedTileDraw {
                        layer_id: tile.layer_id,
                        rect,
                        content,
                    };
                    self.paint_cache.tiles.borrow_mut().insert(coordinate, tile);
                    draw
                };
                draws.push(draw);
            }
        }
        !draws.is_empty()
    }

    #[cfg(not(feature = "portable-guest"))]
    fn record_retained_tile(
        &self,
        ctx: &BuildContext,
        child_ctx: &BuildContext,
        content_size: ResolvedSize,
        rect: RetainedTileRect,
    ) -> Option<(Arc<aimer_canvas::RetainedLayerContent>, Vec<ElementId>)> {
        let recording_canvas = ctx.canvas.fork_for_recording();
        recording_canvas.translate(-rect.x, -rect.y);
        let mut recording_ctx = child_ctx.clone();
        recording_ctx.replace_canvas(Canvas::new(&recording_canvas));
        recording_ctx.visible_rect = Some((
            (rect.x - RETAINED_LAYER_TILE_OVERLAP_PX).max(0.0),
            (rect.y - RETAINED_LAYER_TILE_OVERLAP_PX).max(0.0),
            (rect.width + 2.0 * RETAINED_LAYER_TILE_OVERLAP_PX)
                .min(content_size.width - rect.x + RETAINED_LAYER_TILE_OVERLAP_PX),
            (rect.height + 2.0 * RETAINED_LAYER_TILE_OVERLAP_PX)
                .min(content_size.height - rect.y + RETAINED_LAYER_TILE_OVERLAP_PX),
        ));
        aimer_widget::components::element::begin_paint_tracking();
        self.child.paint(&recording_ctx);
        let element_ids = aimer_widget::components::element::take_paint_tracking();

        let recorded = recording_canvas.take_draw_list();
        let commands = recorded.retained_snapshot()?;
        let content = Arc::new(aimer_canvas::RetainedLayerContent::from_snapshot(commands));
        if !content.is_compositor_safe() {
            return None;
        }
        aimer_widget::record_paint_isolation_record();
        aimer_widget::record_paint_isolation_tile_record();
        Some((content, element_ids))
    }

    /// Resolves the scroll-axis extent against the active constraints.
    #[inline]
    fn constrained_extent(
        viewport: f32,
        bar_extent: f32,
        min: f32,
        max: f32,
        parent: f32,
    ) -> (f32, f32) {
        let total = if max.is_finite() && max < f32::MAX {
            max
        } else if parent.is_finite() && parent < f32::MAX {
            parent.max(min)
        } else {
            viewport + bar_extent
        }
        .clamp(min, max);
        let bar_extent = bar_extent.min(total).max(0.0);
        (total, (total - bar_extent).max(0.0))
    }

    /// Resolves the cross-axis extent — the axis this viewport never scrolls.
    ///
    /// A bounded maximum is filled, exactly like any other box handed a
    /// definite extent. An *unbounded* maximum means the surrounding layout is
    /// asking the viewport how much space it needs — a `Column` measuring its
    /// children inside a vertical scroll viewport does exactly that — so the
    /// honest answer is the child's own extent plus the bar reserved on this
    /// axis. Falling back to the parent's resolved size here is what used to
    /// stretch a horizontal code-block scroller to the full height of the
    /// outer viewport.
    ///
    /// `content` is only invoked on the unbounded path, so the common bounded
    /// case never measures the child.
    #[inline]
    fn cross_extent(
        content: impl FnOnce() -> f32,
        bar_extent: f32,
        min: f32,
        max: f32,
    ) -> (f32, f32) {
        let total = if max.is_finite() && max < f32::MAX {
            max
        } else {
            content() + bar_extent
        }
        .clamp(min, max);
        let bar_extent = bar_extent.min(total).max(0.0);
        (total, (total - bar_extent).max(0.0))
    }

    /// Measures the child's extent across the scroll axis.
    ///
    /// The scroll axis is left unbounded exactly as the child is measured
    /// everywhere else in this container, so the result comes from the same
    /// per-constraint layout cache the draw pass resolves and costs nothing on
    /// a settled frame.
    fn content_cross_extent(&self, ctx: &BuildContext) -> f32 {
        let mut child_ctx = ctx.clone();
        match self.ctrl.axis {
            crate::ScrollAxis::Vertical => child_ctx.box_constraint.max_height = f32::MAX,
            crate::ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = f32::MAX,
        }
        let size = self.child.computed_size(&child_ctx);
        match self.ctrl.axis {
            crate::ScrollAxis::Vertical => size.width,
            crate::ScrollAxis::Horizontal => size.height,
        }
    }

    /// Resolves both extents this scrollable occupies under `ctx`.
    ///
    /// Returns `((width, inner_width), (height, inner_height))`, where the
    /// first value of each pair includes the bar reserved on that axis and the
    /// second is the content viewport that remains. The scroll axis fills the
    /// space it was given; the cross axis wraps the child when its constraint
    /// is unbounded — see [`RawScrollableContainer::cross_extent`].
    #[inline]
    fn resolved_extents(&self, ctx: &BuildContext) -> ((f32, f32), (f32, f32)) {
        let constraint = &ctx.box_constraint;
        match self.ctrl.axis {
            crate::ScrollAxis::Vertical => (
                Self::cross_extent(
                    || self.content_cross_extent(ctx),
                    self.vertical_bar_width,
                    constraint.min_width,
                    constraint.max_width,
                ),
                Self::constrained_extent(
                    self.viewport_h,
                    0.0,
                    constraint.min_height,
                    constraint.max_height,
                    ctx.parent_size.height,
                ),
            ),
            crate::ScrollAxis::Horizontal => (
                Self::constrained_extent(
                    self.viewport_w,
                    0.0,
                    constraint.min_width,
                    constraint.max_width,
                    ctx.parent_size.width,
                ),
                Self::cross_extent(
                    || self.content_cross_extent(ctx),
                    self.horizontal_bar_height,
                    constraint.min_height,
                    constraint.max_height,
                ),
            ),
        }
    }

    /// Resolves extents once for the current layout inputs.
    ///
    /// The live scroll offset is intentionally not part of the key: it changes
    /// only the canvas translation and scrollbar thumb position, never the
    /// child constraints or intrinsic content size.
    #[inline]
    fn cached_extents(&self, ctx: &BuildContext) -> ((f32, f32), (f32, f32)) {
        let key = self.layout_key(ctx);
        if let Some(snapshot) = self.layout_cache.snapshot.get()
            && snapshot.key == key
        {
            return snapshot.extents;
        }

        let extents = self.resolved_extents(ctx);
        self.layout_cache.snapshot.set(Some(CachedScrollLayout {
            key,
            extents,
            content_size: None,
        }));
        extents
    }

    /// Measures the child with the exact constraints used by the draw pass.
    #[inline]
    fn measure_content_size(
        &self,
        ctx: &BuildContext,
        viewport_w: f32,
        viewport_h: f32,
    ) -> ResolvedSize {
        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.min_width = child_ctx.box_constraint.min_width.min(viewport_w);
        child_ctx.box_constraint.min_height = child_ctx.box_constraint.min_height.min(viewport_h);
        child_ctx.box_constraint.max_width = viewport_w;
        child_ctx.box_constraint.max_height = viewport_h;
        child_ctx.parent_size = ResolvedSize {
            width: viewport_w,
            height: viewport_h,
        };
        match self.ctrl.axis {
            crate::ScrollAxis::Vertical => child_ctx.box_constraint.max_height = f32::MAX,
            crate::ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = f32::MAX,
        }
        self.child.computed_size(&child_ctx)
    }

    /// Returns the content extent without remeasuring it after an offset-only
    /// scroll frame.
    #[inline]
    pub(crate) fn cached_content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let extents = self.cached_extents(ctx);
        let key = self.layout_key(ctx);
        if let Some(snapshot) = self.layout_cache.snapshot.get()
            && snapshot.key == key
            && let Some(content_size) = snapshot.content_size
        {
            return content_size;
        }

        let ((_, viewport_w), (_, viewport_h)) = extents;
        let content_size = self.measure_content_size(ctx, viewport_w, viewport_h);
        let snapshot = self
            .layout_cache
            .snapshot
            .get()
            .filter(|snapshot| snapshot.key == key)
            .unwrap_or(CachedScrollLayout {
                key,
                extents,
                content_size: None,
            });
        self.layout_cache.snapshot.set(Some(CachedScrollLayout {
            content_size: Some(content_size),
            ..snapshot
        }));
        content_size
    }

    /// Computes the total size this scrollable occupies under `ctx`.
    #[inline]
    pub(crate) fn layout_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let ((width, _), (height, _)) = self.cached_extents(ctx);
        ResolvedSize { width, height }
    }

    /// Computes the content viewport from the active layout constraints.
    pub(crate) fn viewport_size(&self, ctx: &BuildContext) -> (f32, f32) {
        let ((_, width), (_, height)) = self.cached_extents(ctx);
        (width, height)
    }

    #[allow(dead_code)]
    pub(crate) fn draw_scrollbar(
        &self,
        ctx: &BuildContext,
        scroll_bar: &ScrollBar,
        viewport_w: f32,
        viewport_h: f32,
        is_vertical: bool,
    ) {
        let scale = ctx.scale;
        let offset = self.ctrl.visual_offset(self.ctrl.scroll_offset.get());

        let track_width = match scroll_bar.track.width {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => {
                if is_vertical {
                    viewport_w * (p / 100.0)
                } else {
                    viewport_h * (p / 100.0)
                }
            }
            Dimension::Auto => {
                #[cfg(any(target_os = "android", target_os = "ios"))]
                {
                    6.0 * scale
                }
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    12.0 * scale
                }
            }
        };

        // Cache track width for hit-testing track clicks.
        if is_vertical {
            self.ctrl.cached_v_track_width.set(track_width);
        } else {
            self.ctrl.cached_h_track_width.set(track_width);
        }

        let thumb_width = match scroll_bar.thumb.width {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => track_width * (p / 100.0),
            Dimension::Auto => (track_width * 0.6).max(4.0),
        };

        // Reuse the content size computed once at the start of this frame's draw
        // (see `draw_scroll`) to avoid recomputing the child layout.
        let content_size = self.ctrl.cached_content_size.get();
        let (track_length, content_extent, scroll_pos) = if is_vertical {
            (viewport_h, content_size.height, -offset.y)
        } else {
            (viewport_w, content_size.width, -offset.x)
        };

        let button_h = if is_vertical {
            let resolve_btn_h = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> f32 {
                match btn.height {
                    Dimension::Px(v) => v * scale,
                    Dimension::Percent(p) => track_length * (p / 100.0),
                    Dimension::Auto => track_width,
                }
            };
            let up_h = scroll_bar
                .up_button
                .as_ref()
                .map(&resolve_btn_h)
                .unwrap_or(0.0);
            let down_h = scroll_bar
                .down_button
                .as_ref()
                .map(resolve_btn_h)
                .unwrap_or(0.0);
            (up_h, down_h)
        } else {
            let resolve_btn_w = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> f32 {
                match btn.width {
                    Dimension::Px(v) => v * scale,
                    Dimension::Percent(p) => track_length * (p / 100.0),
                    Dimension::Auto => track_width,
                }
            };
            let left_w = scroll_bar
                .up_button
                .as_ref()
                .map(&resolve_btn_w)
                .unwrap_or(0.0);
            let right_w = scroll_bar
                .down_button
                .as_ref()
                .map(resolve_btn_w)
                .unwrap_or(0.0);
            (left_w, right_w)
        };

        let usable_track = (track_length - button_h.0 - button_h.1).max(0.0);
        let thumb_ratio = if content_extent > 0.0 {
            (track_length / content_extent).min(1.0)
        } else {
            1.0
        };
        let thumb_length = (usable_track * thumb_ratio).max(20.0 * scale);
        let max_thumb_move = (usable_track - thumb_length).max(0.0);
        let max_scroll = (content_extent - track_length).max(0.0);
        let multiplier = if max_thumb_move > 0.0 {
            max_scroll / max_thumb_move
        } else {
            0.0
        };
        if is_vertical {
            self.ctrl.v_scroll_multiplier.set(multiplier);
        } else {
            self.ctrl.h_scroll_multiplier.set(multiplier);
        }

        let scroll_ratio = if max_scroll > 0.0 {
            scroll_pos / max_scroll
        } else {
            0.0
        };
        let thumb_offset = button_h.0 + scroll_ratio * max_thumb_move;

        let thumb_radius = match scroll_bar.thumb.radius {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => thumb_width * (p / 100.0),
            Dimension::Auto => thumb_width / 2.0,
        };

        ctx.canvas.save();

        // Position the scrollbar at the edge of the viewport
        if is_vertical {
            ctx.canvas.translate(Vec2d {
                x: (viewport_w - track_width).round(),
                y: 0.0,
            });
        } else {
            ctx.canvas.translate(Vec2d {
                x: 0.0,
                y: (viewport_h - track_width).round(),
            });
        }

        // Draw track
        let track_color: Color = scroll_bar.track.color.into();
        let (track_w, track_h) = if is_vertical {
            (track_width, track_length)
        } else {
            (track_length, track_width)
        };
        ctx.canvas.fill_color_rect(
            Vec2d { x: 0.0, y: 0.0 },
            ResolvedSize {
                width: track_w,
                height: track_h,
            },
            track_color,
            [0.0; 4],
        );

        // Draw up/left button
        if let Some(ref btn) = scroll_bar.up_button {
            let btn_color: Color = btn.color.into();
            let (bw, bh) = if is_vertical {
                (track_width, button_h.0)
            } else {
                (button_h.0, track_width)
            };
            ctx.canvas.fill_color_rect(
                Vec2d { x: 0.0, y: 0.0 },
                ResolvedSize {
                    width: bw,
                    height: bh,
                },
                btn_color,
                [0.0; 4],
            );
        }

        // Draw down/right button
        if let Some(ref btn) = scroll_bar.down_button {
            let btn_color: Color = btn.color.into();
            let (bx, by, bw, bh) = if is_vertical {
                (0.0, track_length - button_h.1, track_width, button_h.1)
            } else {
                (track_length - button_h.1, 0.0, button_h.1, track_width)
            };
            ctx.canvas.fill_color_rect(
                Vec2d { x: bx, y: by },
                ResolvedSize {
                    width: bw,
                    height: bh,
                },
                btn_color,
                [0.0; 4],
            );
        }

        // Draw thumb. Pick the color based on drag (active) and cursor hover state.
        // The thumb hit-rect used for hover is the one stored on the previous frame.
        let is_active = if is_vertical {
            self.ctrl.drag_mode.get() == DragMode::VerticalScrollbar
        } else {
            self.ctrl.drag_mode.get() == DragMode::HorizontalScrollbar
        };
        let is_hover = self.ctrl.cursor_pos.get().is_some_and(|c| {
            if is_vertical {
                self.ctrl.hit_test_v_thumb(c)
            } else {
                self.ctrl.hit_test_h_thumb(c)
            }
        });
        let thumb_color: Color = if is_active {
            scroll_bar.thumb.active_color.into()
        } else if is_hover {
            scroll_bar.thumb.hover_color.into()
        } else {
            scroll_bar.thumb.color.into()
        };
        let thumb_x_offset = (track_width - thumb_width) / 2.0;
        let (tx, ty, tw, th) = if is_vertical {
            self.ctrl.v_thumb_rect.set(Some((
                viewport_w - track_width + thumb_x_offset,
                thumb_offset,
                thumb_width,
                thumb_length,
            )));
            (thumb_x_offset, thumb_offset, thumb_width, thumb_length)
        } else {
            self.ctrl.h_thumb_rect.set(Some((
                thumb_offset,
                viewport_h - track_width + thumb_x_offset,
                thumb_length,
                thumb_width,
            )));
            (thumb_offset, thumb_x_offset, thumb_length, thumb_width)
        };

        ctx.canvas.fill_color_rect(
            Vec2d { x: tx, y: ty },
            ResolvedSize {
                width: tw,
                height: th,
            },
            thumb_color,
            [thumb_radius; 4],
        );

        ctx.canvas.restore();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use aimer_events::element::ElementEvent;
    use aimer_events::pointer::{PointerInfo, PointerSource};
    use aimer_widget::{
        AnyElement, CaptureRequest, Drawable, EventElement, EventResult, LayoutElement, PointerKey,
        StatelessElement, VisitorElement,
    };

    use super::*;

    struct CapturingChild {
        events: Rc<Cell<usize>>,
    }

    impl VisitorElement for CapturingChild {
        fn debug_name(&self) -> &'static str {
            "CapturingChild"
        }
    }

    impl EventElement for CapturingChild {
        fn on_event(&self, event: &ElementEvent) -> EventResult {
            self.events.set(self.events.get() + 1);
            match event {
                ElementEvent::PointerDown(pointer) => EventResult::consumed()
                    .with_pointer_capture(PointerKey::new(pointer.source, pointer.id)),
                ElementEvent::PointerUp(pointer) => EventResult::consumed()
                    .with_pointer_release(PointerKey::new(pointer.source, pointer.id)),
                _ => EventResult::consumed(),
            }
        }
    }

    impl LayoutElement for CapturingChild {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some((Vec2d::default(), Vec2d { x: 100.0, y: 100.0 }))
        }
    }

    impl Drawable for CapturingChild {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for CapturingChild {}

    /// A child that reports the same intrinsic size under every constraint,
    /// standing in for wrapped content such as a code block's text.
    struct FixedSizeChild {
        size: ResolvedSize,
    }

    impl VisitorElement for FixedSizeChild {
        fn debug_name(&self) -> &'static str {
            "FixedSizeChild"
        }
    }

    impl EventElement for FixedSizeChild {}

    impl LayoutElement for FixedSizeChild {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            self.size
        }
    }

    impl Drawable for FixedSizeChild {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for FixedSizeChild {}

    struct CountingChild {
        size: ResolvedSize,
        measures: Rc<Cell<usize>>,
    }

    impl VisitorElement for CountingChild {
        fn debug_name(&self) -> &'static str {
            "CountingChild"
        }
    }

    impl EventElement for CountingChild {}

    impl LayoutElement for CountingChild {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            self.measures.set(self.measures.get() + 1);
            self.size
        }
    }

    impl Drawable for CountingChild {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Rebuildable for CountingChild {}

    struct DrawingChild {
        draws: Rc<Cell<usize>>,
        stable: bool,
        size: ResolvedSize,
    }

    impl VisitorElement for DrawingChild {
        fn debug_name(&self) -> &'static str {
            "DrawingChild"
        }
    }

    impl EventElement for DrawingChild {}

    impl LayoutElement for DrawingChild {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            self.size
        }
    }

    impl Drawable for DrawingChild {
        fn draw(&self, ctx: &BuildContext) {
            self.draws.set(self.draws.get() + 1);
            ctx.canvas.fill_rect(Vec2d::default(), self.size);
        }

        fn is_paint_stable(&self) -> bool {
            self.stable
        }
    }

    impl Rebuildable for DrawingChild {}

    struct PaintCompositionProbe {
        draws: Rc<Cell<usize>>,
        paints: Rc<Cell<usize>>,
        geometry_syncs: Rc<Cell<usize>>,
        translations: Rc<RefCell<Vec<(f32, f32)>>>,
        size: ResolvedSize,
    }

    impl VisitorElement for PaintCompositionProbe {
        fn debug_name(&self) -> &'static str {
            "PaintCompositionProbe"
        }
    }

    impl EventElement for PaintCompositionProbe {}

    impl LayoutElement for PaintCompositionProbe {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            self.size
        }
    }

    impl Drawable for PaintCompositionProbe {
        fn draw(&self, _ctx: &BuildContext) {
            self.draws.set(self.draws.get() + 1);
        }

        fn paint(&self, ctx: &BuildContext) {
            self.paints.set(self.paints.get() + 1);
            ctx.canvas.fill_rect(Vec2d::ZERO, self.size);
        }

        fn sync_paint_geometry(&self, ctx: &BuildContext) {
            self.geometry_syncs.set(self.geometry_syncs.get() + 1);
            self.translations
                .borrow_mut()
                .push(ctx.canvas.get_transform_translation());
        }

        fn is_paint_stable(&self) -> bool {
            true
        }
    }

    impl Rebuildable for PaintCompositionProbe {}

    struct TilePaintProbe {
        paints: Rc<RefCell<Vec<(Option<(f32, f32, f32, f32)>, (f32, f32))>>>,
        size: ResolvedSize,
    }

    impl VisitorElement for TilePaintProbe {
        fn debug_name(&self) -> &'static str {
            "TilePaintProbe"
        }
    }

    impl EventElement for TilePaintProbe {}

    impl LayoutElement for TilePaintProbe {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            self.size
        }
    }

    impl Drawable for TilePaintProbe {
        fn draw(&self, _ctx: &BuildContext) {}

        fn paint(&self, ctx: &BuildContext) {
            self.paints.borrow_mut().push((
                ctx.visible_rect,
                ctx.canvas.get_transform_translation(),
            ));
            ctx.canvas.fill_rect(Vec2d::ZERO, self.size);
        }

        fn is_paint_stable(&self) -> bool {
            true
        }
    }

    impl Rebuildable for TilePaintProbe {}

    struct OrderedChild {
        label: &'static str,
        events: Rc<RefCell<Vec<(&'static str, (f32, f32))>>>,
        stable: bool,
        size: ResolvedSize,
    }

    impl VisitorElement for OrderedChild {
        fn debug_name(&self) -> &'static str {
            "OrderedChild"
        }
    }

    impl EventElement for OrderedChild {}

    impl LayoutElement for OrderedChild {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            self.size
        }
    }

    impl Drawable for OrderedChild {
        fn draw(&self, ctx: &BuildContext) {
            self.events
                .borrow_mut()
                .push((self.label, ctx.canvas.get_transform_translation()));
            ctx.canvas.fill_rect(Vec2d::ZERO, self.size);
        }

        fn paint(&self, ctx: &BuildContext) {
            ctx.canvas.fill_rect(Vec2d::ZERO, self.size);
        }

        fn is_paint_stable(&self) -> bool {
            self.stable
        }
    }

    impl Rebuildable for OrderedChild {}

    struct ImageDrawingChild {
        draws: Rc<Cell<usize>>,
        image_id: u32,
        size: ResolvedSize,
    }

    impl VisitorElement for ImageDrawingChild {
        fn debug_name(&self) -> &'static str {
            "ImageDrawingChild"
        }
    }

    impl EventElement for ImageDrawingChild {}

    impl LayoutElement for ImageDrawingChild {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            self.size
        }
    }

    impl Drawable for ImageDrawingChild {
        fn draw(&self, ctx: &BuildContext) {
            self.draws.set(self.draws.get() + 1);
            ctx.canvas.draw_image(self.image_id, Vec2d::default(), self.size);
        }

        fn is_paint_stable(&self) -> bool {
            true
        }
    }

    impl Rebuildable for ImageDrawingChild {}

    struct DrawingColumn {
        children: Vec<AnyElement>,
        size: ResolvedSize,
    }

    impl VisitorElement for DrawingColumn {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "DrawingColumn"
        }
    }

    impl EventElement for DrawingColumn {}

    impl LayoutElement for DrawingColumn {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            self.size
        }
    }

    impl Drawable for DrawingColumn {
        fn draw(&self, ctx: &BuildContext) {
            for (index, child) in self.children.iter().enumerate() {
                let y = if index == 0 { 1_100.0 } else { 2_148.0 };
                if !ctx.is_rect_visible(0.0, y, self.size.width, 100.0) {
                    continue;
                }

                let mut child_ctx = ctx.clone();
                child_ctx.parent_size = ResolvedSize {
                    width: self.size.width,
                    height: 100.0,
                };
                child_ctx.box_constraint = BoxConstraint {
                    min_width: 0.0,
                    min_height: 0.0,
                    max_width: self.size.width,
                    max_height: 100.0,
                };
                child_ctx.visible_rect = ctx
                    .visible_rect
                    .map(|(x, visible_y, width, height)| (x, visible_y - y, width, height));
                child_ctx.canvas.save();
                child_ctx.canvas.translate(Vec2d { x: 0.0, y });
                child.draw(&child_ctx);
                child_ctx.canvas.restore();
            }
        }

        fn draw_paint_islands(
            &self,
            retained_ctx: &BuildContext,
            live_ctx: &BuildContext,
            draw_stable: &mut dyn FnMut(
                &dyn Element,
                &BuildContext,
                Vec2d,
                Option<ResolvedSize>,
            ),
            draw_dynamic: &mut dyn FnMut(
                &dyn Element,
                &BuildContext,
                Vec2d,
                Option<ResolvedSize>,
            ),
        ) -> bool {
            let mut saw_stable = false;
            let mut saw_dynamic = false;
            for (index, child) in self.children.iter().enumerate() {
                let y = if index == 0 { 0.0 } else { 100.0 };
                if child.is_paint_stable() {
                    saw_stable = true;
                    let mut child_ctx = retained_ctx.clone();
                    child_ctx.parent_size = ResolvedSize {
                        width: self.size.width,
                        height: 100.0,
                    };
                    child_ctx.visible_rect = None;
                    draw_stable(child.as_ref(), &child_ctx, Vec2d { x: 0.0, y }, None);
                } else {
                    saw_dynamic = true;
                    if !live_ctx.is_rect_visible(0.0, y, self.size.width, 100.0) {
                        continue;
                    }
                    let mut child_ctx = live_ctx.clone();
                    child_ctx.parent_size = ResolvedSize {
                        width: self.size.width,
                        height: 100.0,
                    };
                    child_ctx.visible_rect = live_ctx
                        .visible_rect
                        .map(|(x, visible_y, width, height)| {
                            (x, visible_y - y, width, height)
                        });
                    draw_dynamic(child.as_ref(), &child_ctx, Vec2d { x: 0.0, y }, None);
                }
            }
            saw_stable && saw_dynamic
        }

        fn is_paint_stable(&self) -> bool {
            self.children.iter().all(|child| child.is_paint_stable())
        }
    }

    impl Rebuildable for DrawingColumn {}

    struct OrderedColumn {
        children: Vec<AnyElement>,
        size: ResolvedSize,
        row_step: f32,
    }

    impl VisitorElement for OrderedColumn {
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }

        fn debug_name(&self) -> &'static str {
            "OrderedColumn"
        }
    }

    impl EventElement for OrderedColumn {}

    impl LayoutElement for OrderedColumn {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            self.size
        }
    }

    impl Drawable for OrderedColumn {
        fn draw(&self, ctx: &BuildContext) {
            for (index, child) in self.children.iter().enumerate() {
                let y = index as f32 * self.row_step;
                if !ctx.is_rect_visible(0.0, y, self.size.width, 100.0) {
                    continue;
                }

                let mut child_ctx = ctx.clone();
                child_ctx.parent_size = ResolvedSize {
                    width: self.size.width,
                    height: 100.0,
                };
                child_ctx.box_constraint = BoxConstraint {
                    min_width: 0.0,
                    min_height: 0.0,
                    max_width: self.size.width,
                    max_height: 100.0,
                };
                child_ctx.visible_rect = ctx
                    .visible_rect
                    .map(|(x, visible_y, width, height)| (x, visible_y - y, width, height));
                child_ctx.canvas.save();
                child_ctx.canvas.translate(Vec2d { x: 0.0, y });
                child.draw(&child_ctx);
                child_ctx.canvas.restore();
            }
        }

        fn draw_paint_islands(
            &self,
            retained_ctx: &BuildContext,
            live_ctx: &BuildContext,
            draw_stable: &mut dyn FnMut(
                &dyn Element,
                &BuildContext,
                Vec2d,
                Option<ResolvedSize>,
            ),
            draw_dynamic: &mut dyn FnMut(
                &dyn Element,
                &BuildContext,
                Vec2d,
                Option<ResolvedSize>,
            ),
        ) -> bool {
            let mut saw_stable = false;
            let mut saw_dynamic = false;
            for (index, child) in self.children.iter().enumerate() {
                let y = index as f32 * self.row_step;
                if child.is_paint_stable() {
                    saw_stable = true;
                    let mut child_ctx = retained_ctx.clone();
                    child_ctx.parent_size = ResolvedSize {
                        width: self.size.width,
                        height: 100.0,
                    };
                    child_ctx.visible_rect = None;
                    draw_stable(child.as_ref(), &child_ctx, Vec2d { x: 0.0, y }, None);
                } else {
                    saw_dynamic = true;
                    if !live_ctx.is_rect_visible(0.0, y, self.size.width, 100.0) {
                        continue;
                    }
                    let mut child_ctx = live_ctx.clone();
                    child_ctx.parent_size = ResolvedSize {
                        width: self.size.width,
                        height: 100.0,
                    };
                    child_ctx.visible_rect = live_ctx
                        .visible_rect
                        .map(|(x, visible_y, width, height)| {
                            (x, visible_y - y, width, height)
                        });
                    draw_dynamic(child.as_ref(), &child_ctx, Vec2d { x: 0.0, y }, None);
                }
            }
            saw_stable && saw_dynamic
        }

        fn is_paint_stable(&self) -> bool {
            self.children.iter().all(|child| child.is_paint_stable())
        }
    }

    impl Rebuildable for OrderedColumn {}

    fn drawing_scrollable(draws: Rc<Cell<usize>>) -> RawScrollableContainer<AnyElement> {
        drawing_scrollable_with_stability(draws, true)
    }

    fn drawing_scrollable_with_stability(
        draws: Rc<Cell<usize>>,
        stable: bool,
    ) -> RawScrollableContainer<AnyElement> {
        drawing_scrollable_with_size(
            draws,
            stable,
            ResolvedSize {
                width: 100.0,
                height: 400.0,
            },
        )
    }

    fn drawing_scrollable_with_size(
        draws: Rc<Cell<usize>>,
        stable: bool,
        size: ResolvedSize,
    ) -> RawScrollableContainer<AnyElement> {
        let mut state = ScrollState::for_test_at(Vec2d::default());
        state.axis = crate::ScrollAxis::Vertical;

        RawScrollableContainer {
            child: DrawingChild { draws, stable, size }.boxed(),
            ctrl: Rc::new(state),
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            viewport_w: 100.0,
            viewport_h: 100.0,
            vertical_bar_width: 0.0,
            horizontal_bar_height: 0.0,
            bounds: CacheBounds::new(),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
            layout_cache: Default::default(),
            #[cfg(not(feature = "portable-guest"))]
            paint_cache: Default::default(),
        }
    }

    fn drawing_context(visible_rect: Option<(f32, f32, f32, f32)>) -> BuildContext<'static> {
        let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
        let mut context = BuildContext::new(
            aimer_canvas::Canvas::new(inner),
            ResolvedSize {
                width: 100.0,
                height: 100.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        );
        context.box_constraint = BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 100.0,
            max_height: 100.0,
        };
        context.visible_rect = visible_rect;
        context
    }

    #[tokio::test]
    async fn offscreen_scrollable_skips_child_content_before_dispatch() {
        let draws = Rc::new(Cell::new(0));
        let scrollable = drawing_scrollable(draws.clone());
        let mut ctx = drawing_context(Some((0.0, 101.0, 100.0, 20.0)));

        scrollable.draw(&ctx);

        assert_eq!(draws.get(), 0);

        ctx.visible_rect = Some((0.0, 0.0, 100.0, 100.0));
        scrollable.draw(&ctx);
        assert_eq!(draws.get(), 1);
    }

    #[tokio::test]
    async fn stable_scrollable_reuses_paint_until_scale_invalidates_it() {
        aimer_widget::reset_paint_stats();
        let draws = Rc::new(Cell::new(0));
        let scrollable = drawing_scrollable(draws.clone());
        let mut ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));

        scrollable.draw(&ctx);
        assert_eq!(draws.get(), 1, "the first visible frame records the child");
        assert_eq!(
            ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers,
            1,
            "the first visible frame should record one compositor layer"
        );

        ctx.canvas.begin_frame();
        scrollable.ctrl.scroll_offset.set(Vec2d { x: 0.0, y: -20.0 });
        scrollable.draw(&ctx);
        assert_eq!(
            draws.get(),
            1,
            "an offset-only frame should composite the retained layer"
        );
        assert_eq!(
            ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers,
            1,
            "offset-only scrolling should keep the layer as one command"
        );

        ctx.canvas.begin_frame();
        ctx.scale = 2.0;
        scrollable.draw(&ctx);
        assert_eq!(
            draws.get(),
            2,
            "a scale change must record fresh paint commands"
        );
        assert_eq!(
            aimer_widget::take_paint_stats(),
            aimer_widget::PaintStats {
                candidates: 3,
                records: 2,
                replays: 1,
                invalidations: 1,
                fallbacks: 0,
                tile_records: 0,
                tile_replays: 0,
            }
        );
    }

    #[tokio::test]
    async fn offset_only_scroll_replays_paint_but_updates_live_composition_geometry() {
        let draws = Rc::new(Cell::new(0));
        let paints = Rc::new(Cell::new(0));
        let geometry_syncs = Rc::new(Cell::new(0));
        let translations = Rc::new(RefCell::new(Vec::new()));
        let scrollable = {
            let mut state = ScrollState::for_test_at(Vec2d::default());
            state.axis = crate::ScrollAxis::Vertical;
            RawScrollableContainer {
                child: PaintCompositionProbe {
                    draws: draws.clone(),
                    paints: paints.clone(),
                    geometry_syncs: geometry_syncs.clone(),
                    translations: translations.clone(),
                    size: ResolvedSize {
                        width: 100.0,
                        height: 400.0,
                    },
                }
                .boxed(),
                ctrl: Rc::new(state),
                vertical_scroll_bar: None,
                horizontal_scroll_bar: None,
                viewport_w: 100.0,
                viewport_h: 100.0,
                vertical_bar_width: 0.0,
                horizontal_bar_height: 0.0,
                bounds: CacheBounds::new(),
                event_dispatcher: RefCell::new(EventDispatcher::new()),
                layout_cache: Default::default(),
                #[cfg(not(feature = "portable-guest"))]
                paint_cache: Default::default(),
            }
        };
        let ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));

        scrollable.draw(&ctx);
        assert_eq!(paints.get(), 1, "the first frame records local child paint");
        assert_eq!(draws.get(), 0, "stable content must not use the normal draw path");
        assert_eq!(geometry_syncs.get(), 1);

        ctx.canvas.begin_frame();
        scrollable.ctrl.scroll_offset.set(Vec2d { x: 0.0, y: -20.0 });
        scrollable.draw(&ctx);

        assert_eq!(paints.get(), 1, "offset-only scrolling must not repaint the child");
        assert_eq!(draws.get(), 0, "offset-only scrolling must not fall back to direct draw");
        assert_eq!(geometry_syncs.get(), 2, "live geometry still follows composition");
        assert_eq!(*translations.borrow(), vec![(0.0, 0.0), (0.0, -20.0)]);
        assert_eq!(
            ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers,
            1,
            "the translated frame must submit the retained layer"
        );
    }

    #[tokio::test]
    async fn replacing_a_retained_image_invalidates_the_paint_layer() {
        aimer_widget::reset_paint_stats();
        let draws = Rc::new(Cell::new(0));
        let mut state = ScrollState::for_test_at(Vec2d::default());
        state.axis = crate::ScrollAxis::Vertical;
        let scrollable = RawScrollableContainer {
            child: ImageDrawingChild {
                draws: draws.clone(),
                image_id: 7,
                size: ResolvedSize {
                    width: 100.0,
                    height: 400.0,
                },
            }
            .boxed(),
            ctrl: Rc::new(state),
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            viewport_w: 100.0,
            viewport_h: 100.0,
            vertical_bar_width: 0.0,
            horizontal_bar_height: 0.0,
            bounds: CacheBounds::new(),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
            layout_cache: Default::default(),
            #[cfg(not(feature = "portable-guest"))]
            paint_cache: Default::default(),
        };
        let ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));

        ctx.canvas.load_image_with_id(7, &[255, 0, 0, 255], 1, 1);
        scrollable.draw(&ctx);
        assert_eq!(draws.get(), 1);

        ctx.canvas.begin_frame();
        ctx.canvas.load_image_with_id(7, &[0, 255, 0, 255], 1, 1);
        scrollable.draw(&ctx);

        assert_eq!(draws.get(), 2);
        assert_eq!(
            aimer_widget::take_paint_stats(),
            aimer_widget::PaintStats {
                candidates: 2,
                records: 2,
                replays: 0,
                invalidations: 1,
                fallbacks: 0,
                tile_records: 0,
                tile_replays: 0,
            }
        );
    }

    #[tokio::test]
    async fn dirty_stable_content_re_records_the_full_retained_paint() {
        aimer_widget::reset_paint_stats();
        let draws = Rc::new(Cell::new(0));
        let scrollable = drawing_scrollable(draws.clone());
        let ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));

        scrollable.draw(&ctx);
        assert_eq!(draws.get(), 1);

        scrollable.child.mark_needs_rebuild();
        ctx.canvas.begin_frame();
        scrollable.draw(&ctx);

        assert_eq!(draws.get(), 2);
        assert_eq!(
            aimer_widget::take_paint_stats(),
            aimer_widget::PaintStats {
                candidates: 2,
                records: 2,
                replays: 0,
                invalidations: 1,
                fallbacks: 0,
                tile_records: 0,
                tile_replays: 0,
            }
        );
    }

    #[tokio::test]
    async fn dynamic_scrollable_content_stays_on_the_direct_path() {
        aimer_widget::reset_paint_stats();
        let draws = Rc::new(Cell::new(0));
        let scrollable = drawing_scrollable_with_stability(draws.clone(), false);
        let ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));

        scrollable.draw(&ctx);
        assert_eq!(draws.get(), 1);
        assert_eq!(
            ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers,
            0,
            "dynamic content must not be captured into a compositor layer"
        );

        ctx.canvas.begin_frame();
        scrollable.ctrl.scroll_offset.set(Vec2d { x: 0.0, y: -20.0 });
        scrollable.draw(&ctx);
        assert_eq!(
            draws.get(),
            2,
            "dynamic content must be redrawn after an offset change"
        );
        let stats = aimer_widget::take_paint_stats();
        assert_eq!(stats.candidates, 2);
        assert_eq!(stats.fallbacks, 2);
        assert_eq!(stats.records, 0);
        assert_eq!(stats.replays, 0);
    }

    #[tokio::test]
    async fn dynamic_content_before_stable_content_preserves_order_with_direct_fallback() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut state = ScrollState::for_test_at(Vec2d::default());
        state.axis = crate::ScrollAxis::Vertical;
        let scrollable = RawScrollableContainer {
            child: OrderedColumn {
                children: vec![
                    OrderedChild {
                        label: "dynamic",
                        events: events.clone(),
                        stable: false,
                        size: ResolvedSize {
                            width: 100.0,
                            height: 100.0,
                        },
                    }
                    .boxed(),
                    OrderedChild {
                        label: "stable",
                        events: events.clone(),
                        stable: true,
                        size: ResolvedSize {
                            width: 100.0,
                            height: 100.0,
                        },
                    }
                    .boxed(),
                ],
                size: ResolvedSize {
                    width: 100.0,
                    height: 200.0,
                },
                row_step: 100.0,
            }
            .boxed(),
            ctrl: Rc::new(state),
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            viewport_w: 100.0,
            viewport_h: 100.0,
            vertical_bar_width: 0.0,
            horizontal_bar_height: 0.0,
            bounds: CacheBounds::new(),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
            layout_cache: Default::default(),
            #[cfg(not(feature = "portable-guest"))]
            paint_cache: Default::default(),
        };
        let ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));

        scrollable.draw(&ctx);

        assert_eq!(
            events
                .borrow()
                .iter()
                .map(|(label, _)| *label)
                .collect::<Vec<_>>(),
            vec!["dynamic", "stable"]
        );
        assert_eq!(
            ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers,
            0,
            "an interleaved dynamic/stable order must use direct fallback"
        );

        ctx.canvas.begin_frame();
        events.borrow_mut().clear();
        scrollable.ctrl.scroll_offset.set(Vec2d { x: 0.0, y: -20.0 });
        scrollable.draw(&ctx);

        assert_eq!(
            events
                .borrow()
                .iter()
                .map(|(label, _)| *label)
                .collect::<Vec<_>>(),
            vec!["dynamic", "stable"]
        );
        assert_eq!(
            ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers,
            0,
            "fallback order must remain direct after an offset change"
        );
    }

    #[tokio::test]
    async fn dynamic_scrollable_culls_rows_and_preserves_composed_translation() {
        let events = Rc::new(RefCell::new(Vec::<(&'static str, (f32, f32))>::new()));
        let mut state = ScrollState::for_test_at(Vec2d::default());
        state.axis = crate::ScrollAxis::Vertical;
        let scrollable = RawScrollableContainer {
            child: OrderedColumn {
                children: vec![
                    OrderedChild {
                        label: "first",
                        events: events.clone(),
                        stable: false,
                        size: ResolvedSize {
                            width: 100.0,
                            height: 100.0,
                        },
                    }
                    .boxed(),
                    OrderedChild {
                        label: "second",
                        events: events.clone(),
                        stable: false,
                        size: ResolvedSize {
                            width: 100.0,
                            height: 100.0,
                        },
                    }
                    .boxed(),
                ],
                size: ResolvedSize {
                    width: 100.0,
                    height: 2_000.0,
                },
                row_step: 1_000.0,
            }
            .boxed(),
            ctrl: Rc::new(state),
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            viewport_w: 100.0,
            viewport_h: 100.0,
            vertical_bar_width: 0.0,
            horizontal_bar_height: 0.0,
            bounds: CacheBounds::new(),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
            layout_cache: Default::default(),
            #[cfg(not(feature = "portable-guest"))]
            paint_cache: Default::default(),
        };
        let ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));

        scrollable.draw(&ctx);
        assert_eq!(*events.borrow(), vec![("first", (0.0, 0.0))]);

        ctx.canvas.begin_frame();
        events.borrow_mut().clear();
        scrollable.ctrl.scroll_offset.set(Vec2d { x: 0.0, y: -1_000.0 });
        scrollable.draw(&ctx);

        assert_eq!(*events.borrow(), vec![("second", (0.0, 0.0))]);
    }

    #[tokio::test]
    async fn oversized_retained_tiles_record_their_overlap_window() {
        let paints = Rc::new(RefCell::new(Vec::new()));
        let mut state = ScrollState::for_test_at(Vec2d::default());
        state.axis = crate::ScrollAxis::Vertical;
        let scrollable = RawScrollableContainer {
            child: TilePaintProbe {
                paints: paints.clone(),
                size: ResolvedSize {
                    width: 100.0,
                    height: 200_000.0,
                },
            }
            .boxed(),
            ctrl: Rc::new(state),
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            viewport_w: 100.0,
            viewport_h: 100.0,
            vertical_bar_width: 0.0,
            horizontal_bar_height: 0.0,
            bounds: CacheBounds::new(),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
            layout_cache: Default::default(),
            #[cfg(not(feature = "portable-guest"))]
            paint_cache: Default::default(),
        };
        let ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));
        scrollable.ctrl.scroll_offset.set(Vec2d { x: 0.0, y: -1_024.0 });

        scrollable.draw(&ctx);

        let paints = paints.borrow();
        assert_eq!(paints.len(), 2, "the visible window should span two tiles");
        let (visible_rect, translation) = paints[1];
        assert_eq!(translation, (0.0, -1_024.0));
        let (_, visible_y, _, visible_height) =
            visible_rect.expect("the tile paint should receive its expanded window");
        assert_eq!(visible_y, 1_022.0);
        assert_eq!(visible_height, 1_028.0);
    }

    #[tokio::test]
    async fn dynamic_islands_retain_a_static_prefix_and_redraw_only_the_dynamic_suffix() {
        aimer_widget::reset_paint_stats();
        let static_draws = Rc::new(Cell::new(0));
        let dynamic_draws = Rc::new(Cell::new(0));
        let mut state = ScrollState::for_test_at(Vec2d::default());
        state.axis = crate::ScrollAxis::Vertical;
        let scrollable = RawScrollableContainer {
            child: DrawingColumn {
                children: vec![
                    DrawingChild {
                        draws: static_draws.clone(),
                        stable: true,
                        size: ResolvedSize {
                            width: 100.0,
                            height: 100.0,
                        },
                    }
                    .boxed(),
                    DrawingChild {
                        draws: dynamic_draws.clone(),
                        stable: false,
                        size: ResolvedSize {
                            width: 100.0,
                            height: 100.0,
                        },
                    }
                    .boxed(),
                ],
                size: ResolvedSize {
                    width: 100.0,
                    height: 200.0,
                },
            }
            .boxed(),
            ctrl: Rc::new(state),
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            viewport_w: 100.0,
            viewport_h: 100.0,
            vertical_bar_width: 0.0,
            horizontal_bar_height: 0.0,
            bounds: CacheBounds::new(),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
            layout_cache: Default::default(),
            #[cfg(not(feature = "portable-guest"))]
            paint_cache: Default::default(),
        };
        let ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));

        scrollable.draw(&ctx);
        assert_eq!(static_draws.get(), 1);
        assert_eq!(dynamic_draws.get(), 1);
        assert_eq!(
            ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers,
            1,
            "the stable island should be submitted as one retained layer"
        );

        ctx.canvas.begin_frame();
        scrollable.ctrl.scroll_offset.set(Vec2d { x: 0.0, y: -20.0 });
        scrollable.draw(&ctx);

        assert_eq!(static_draws.get(), 1, "the static island must stay cached");
        assert_eq!(dynamic_draws.get(), 2, "the dynamic island must repaint");
        assert_eq!(
            ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers,
            1,
            "offset-only frames should keep one retained static layer"
        );
        assert_eq!(
            aimer_widget::take_paint_stats(),
            aimer_widget::PaintStats {
                candidates: 2,
                records: 1,
                replays: 1,
                invalidations: 0,
                fallbacks: 0,
                tile_records: 0,
                tile_replays: 0,
            }
        );
    }

    #[tokio::test]
    async fn oversized_stable_scrollable_reuses_its_visible_retained_tile() {
        aimer_widget::reset_paint_stats();
        let draws = Rc::new(Cell::new(0));
        let scrollable = drawing_scrollable_with_size(
            draws.clone(),
            true,
            ResolvedSize {
                width: 100.0,
                height: 200_000.0,
            },
        );
        let ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));

        scrollable.draw(&ctx);
        assert_eq!(draws.get(), 1, "the first tile records the child once");
        assert_eq!(
            ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers,
            1,
            "the visible tile should be submitted as one layer"
        );

        ctx.canvas.begin_frame();
        scrollable.ctrl.scroll_offset.set(Vec2d { x: 0.0, y: -20.0 });
        scrollable.draw(&ctx);
        assert_eq!(
            draws.get(),
            1,
            "a small offset within the same tile should not redraw the child"
        );
        assert_eq!(
            ctx.canvas.get_inner_canvas().draw_list().stats().retained_layers,
            1,
            "the cached tile should remain one compositor command"
        );
        let stats = aimer_widget::take_paint_stats();
        assert_eq!(stats.candidates, 2);
        assert_eq!(stats.records, 1);
        assert_eq!(stats.replays, 1);
        assert_eq!(stats.tile_records, 1);
        assert_eq!(stats.tile_replays, 1);
        assert_eq!(stats.fallbacks, 0);
    }

    #[tokio::test]
    async fn dirty_static_content_repaints_only_the_invalidated_retained_tile() {
        let first_draws = Rc::new(Cell::new(0));
        let second_draws = Rc::new(Cell::new(0));
        let first = StatelessElement::wrapper(
            DrawingChild {
                draws: first_draws.clone(),
                stable: true,
                size: ResolvedSize {
                    width: 100.0,
                    height: 100.0,
                },
            }
            .boxed(),
            None,
            "FirstDrawingTile",
        )
        .boxed();
        let second = StatelessElement::wrapper(
            DrawingChild {
                draws: second_draws.clone(),
                stable: true,
                size: ResolvedSize {
                    width: 100.0,
                    height: 100.0,
                },
            }
            .boxed(),
            None,
            "SecondDrawingTile",
        )
        .boxed();
        let mut state = ScrollState::for_test_at(Vec2d::default());
        state.axis = crate::ScrollAxis::Vertical;
        let scrollable = RawScrollableContainer {
            child: DrawingColumn {
                children: vec![first, second],
                size: ResolvedSize {
                    width: 100.0,
                    height: 200_000.0,
                },
            }
            .boxed(),
            ctrl: Rc::new(state),
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            viewport_w: 100.0,
            viewport_h: 100.0,
            vertical_bar_width: 0.0,
            horizontal_bar_height: 0.0,
            bounds: CacheBounds::new(),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
            layout_cache: Default::default(),
            #[cfg(not(feature = "portable-guest"))]
            paint_cache: Default::default(),
        };
        let ctx = drawing_context(Some((0.0, 0.0, 100.0, 100.0)));

        scrollable.draw(&ctx);
        assert_eq!(first_draws.get(), 0);
        assert_eq!(second_draws.get(), 0);

        ctx.canvas.begin_frame();
        scrollable.ctrl.scroll_offset.set(Vec2d { x: 0.0, y: -1_900.0 });
        scrollable.draw(&ctx);
        assert_eq!(first_draws.get(), 1);
        assert_eq!(second_draws.get(), 1);

        scrollable.child.visit_children(&mut |child| {
            if child.debug_name() == "FirstDrawingTile" {
                child.mark_needs_rebuild();
            }
        });

        ctx.canvas.begin_frame();
        scrollable.draw(&ctx);
        assert_eq!(
            first_draws.get(),
            2,
            "the dirty tile should be repainted while its clean neighbor remains cached"
        );
        assert_eq!(
            second_draws.get(),
            1,
            "repainting one tile must not redraw its clean neighbor"
        );
    }

    /// A scrollable along `axis` holding a child of a fixed intrinsic size,
    /// used to observe how the container resolves its own extents.
    fn sized_scrollable(
        axis: crate::ScrollAxis,
        child: ResolvedSize,
    ) -> RawScrollableContainer<AnyElement> {
        let mut state = ScrollState::for_test_at(Vec2d::default());
        state.axis = axis;

        RawScrollableContainer {
            child: FixedSizeChild { size: child }.boxed(),
            ctrl: Rc::new(state),
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            viewport_w: 100.0,
            viewport_h: 100.0,
            vertical_bar_width: 0.0,
            horizontal_bar_height: 0.0,
            bounds: CacheBounds::new(),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
            layout_cache: Default::default(),
            #[cfg(not(feature = "portable-guest"))]
            paint_cache: Default::default(),
        }
    }

    fn counting_scrollable(measures: Rc<Cell<usize>>) -> RawScrollableContainer<AnyElement> {
        let mut state = ScrollState::for_test_at(Vec2d::default());
        state.axis = crate::ScrollAxis::Vertical;

        RawScrollableContainer {
            child: CountingChild {
                size: ResolvedSize {
                    width: 100.0,
                    height: 400.0,
                },
                measures,
            }
            .boxed(),
            ctrl: Rc::new(state),
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            viewport_w: 100.0,
            viewport_h: 100.0,
            vertical_bar_width: 0.0,
            horizontal_bar_height: 0.0,
            bounds: CacheBounds::new(),
            event_dispatcher: RefCell::new(EventDispatcher::new()),
            layout_cache: Default::default(),
            #[cfg(not(feature = "portable-guest"))]
            paint_cache: Default::default(),
        }
    }

    #[tokio::test]
    async fn changing_only_scroll_offset_reuses_content_layout() {
        let measures = Rc::new(Cell::new(0));
        let scrollable = counting_scrollable(measures.clone());
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        let mut ctx = BuildContext::new(
            canvas,
            ResolvedSize {
                width: 100.0,
                height: 100.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        );
        ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 100.0,
            max_height: 100.0,
        };

        assert_eq!(scrollable.content_size(&ctx).height, 400.0);
        assert_eq!(measures.get(), 1);

        scrollable.ctrl.scroll_offset.set(Vec2d { x: 0.0, y: -80.0 });

        assert_eq!(scrollable.content_size(&ctx).height, 400.0);
        assert_eq!(scrollable.computed_size(&ctx).height, 100.0);
        assert_eq!(
            measures.get(),
            1,
            "changing only the scroll offset must not remeasure content"
        );

        let mut changed_ctx = ctx.clone();
        changed_ctx.box_constraint.max_width = 80.0;
        assert_eq!(scrollable.content_size(&changed_ctx).width, 100.0);
        assert_eq!(
            measures.get(),
            2,
            "changing a layout constraint must retire the scroll snapshot"
        );
    }

    /// Regression test: a horizontal scrollable measured under an unbounded
    /// height — a `Column` inside a vertical scroll viewport does exactly that
    /// — must wrap its child's height instead of stretching to the parent's
    /// resolved size, which used to blow a code block up to the full height of
    /// the outer viewport.
    #[tokio::test]
    async fn a_horizontal_scrollable_wraps_its_height_when_the_cross_axis_is_unbounded() {
        let mut scrollable = sized_scrollable(crate::ScrollAxis::Horizontal, ResolvedSize {
            width: 300.0,
            height: 120.0,
        });
        scrollable.horizontal_bar_height = 10.0;

        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        let mut ctx = BuildContext::new(
            canvas,
            ResolvedSize {
                width: 500.0,
                height: 600.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        );
        ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 500.0,
            max_height: f32::MAX,
        };

        assert_eq!(scrollable.viewport_size(&ctx), (500.0, 120.0));
        assert_eq!(scrollable.computed_size(&ctx), ResolvedSize {
            width: 500.0,
            height: 130.0,
        });
    }

    #[tokio::test]
    async fn a_vertical_scrollable_wraps_its_width_when_the_cross_axis_is_unbounded() {
        let mut scrollable = sized_scrollable(crate::ScrollAxis::Vertical, ResolvedSize {
            width: 300.0,
            height: 120.0,
        });
        scrollable.vertical_bar_width = 12.0;

        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        let mut ctx = BuildContext::new(
            canvas,
            ResolvedSize {
                width: 800.0,
                height: 400.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        );
        ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: f32::MAX,
            max_height: 400.0,
        };

        assert_eq!(scrollable.viewport_size(&ctx), (300.0, 400.0));
        assert_eq!(scrollable.computed_size(&ctx), ResolvedSize {
            width: 312.0,
            height: 400.0,
        });
    }

    /// A container laid out over the top-left 100x100 corner, wrapping a child
    /// that captures the pointer it is pressed with. Both elements of a rebuild
    /// share `ctrl`, exactly as the live scroll engine is shared across one.
    fn capturing_scrollable(
        events: Rc<Cell<usize>>,
        ctrl: Rc<ScrollState>,
    ) -> RawScrollableContainer<AnyElement> {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 0.0, 0.0, 100.0, 100.0);

        RawScrollableContainer {
            child: CapturingChild { events }.boxed(),
            ctrl,
            vertical_scroll_bar: None,
            horizontal_scroll_bar: None,
            viewport_w: 100.0,
            viewport_h: 100.0,
            vertical_bar_width: 0.0,
            horizontal_bar_height: 0.0,
            bounds,
            event_dispatcher: RefCell::new(EventDispatcher::new()),
            layout_cache: Default::default(),
            #[cfg(not(feature = "portable-guest"))]
            paint_cache: Default::default(),
        }
    }

    #[tokio::test]
    async fn computed_size_fills_the_parent_constraint_over_the_stored_viewport() {
        let scrollable = capturing_scrollable(
            Rc::new(Cell::new(0)),
            Rc::new(ScrollState::for_test_at(Vec2d::default())),
        );
        let mut scrollable = scrollable;
        scrollable.vertical_bar_width = 12.0;
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        let mut ctx = BuildContext::new(
            canvas,
            ResolvedSize {
                width: 200.0,
                height: 160.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        );
        ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 40.0,
            min_height: 30.0,
            max_width: 200.0,
            max_height: 160.0,
        };

        assert_eq!(scrollable.computed_size(&ctx), ResolvedSize {
            width: 200.0,
            height: 160.0,
        });
    }

    #[tokio::test]
    async fn a_flex_assigned_constraint_shrinks_the_scrollable_viewport() {
        let mut scrollable = capturing_scrollable(
            Rc::new(Cell::new(0)),
            Rc::new(ScrollState::for_test_at(Vec2d::default())),
        );
        scrollable.viewport_w = 800.0;
        scrollable.viewport_h = 600.0;
        scrollable.vertical_bar_width = 12.0;

        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        let mut ctx = BuildContext::new(
            canvas,
            ResolvedSize {
                width: 800.0,
                height: 600.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        );
        ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 320.0,
            max_height: 180.0,
        };

        assert_eq!(scrollable.viewport_size(&ctx), (308.0, 180.0));
        assert_eq!(scrollable.computed_size(&ctx), ResolvedSize {
            width: 320.0,
            height: 180.0,
        });
    }

    #[tokio::test]
    async fn a_retained_scrollable_expands_when_the_parent_constraint_grows() {
        let mut scrollable = capturing_scrollable(
            Rc::new(Cell::new(0)),
            Rc::new(ScrollState::for_test_at(Vec2d::default())),
        );
        scrollable.viewport_w = 320.0;
        scrollable.viewport_h = 180.0;
        scrollable.vertical_bar_width = 12.0;

        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        let mut ctx = BuildContext::new(
            canvas,
            ResolvedSize {
                width: 320.0,
                height: 180.0,
            },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        );
        ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 320.0,
            max_height: 180.0,
        };

        assert_eq!(scrollable.viewport_size(&ctx), (308.0, 180.0));

        ctx.parent_size = ResolvedSize {
            width: 640.0,
            height: 360.0,
        };
        ctx.box_constraint.max_width = 640.0;
        ctx.box_constraint.max_height = 360.0;

        assert_eq!(scrollable.viewport_size(&ctx), (628.0, 360.0));
        assert_eq!(scrollable.computed_size(&ctx), ResolvedSize {
            width: 640.0,
            height: 360.0,
        });
    }

    // A rebuild triggered by the press itself — a `Button` inside the list
    // darkening under the finger — replaces this container. The capture the child
    // took lives in the dispatcher beside the children rather than in them, so
    // the positional walk cannot reach it: without the hand-over,
    // `child_route_allowed` sees no capture, the replacement rejects the release
    // as being outside its viewport, and the child never hears the pointer lift.
    #[test]
    fn a_rebuild_during_a_press_keeps_the_capture_so_a_release_outside_still_lands() {
        let events = Rc::new(Cell::new(0));
        let ctrl = Rc::new(ScrollState::for_test_at(Vec2d::default()));
        let pressed = capturing_scrollable(events.clone(), ctrl.clone());
        let pointer = PointerKey::new(PointerSource::Touch, 2);

        let down = pressed.on_event(&ElementEvent::PointerDown(PointerInfo::touch(
            Vec2d { x: 10.0, y: 10.0 },
            pointer.id,
        )));
        assert_eq!(down.capture_request(), CaptureRequest::Capture(pointer));

        let rebuilt = capturing_scrollable(events.clone(), ctrl);
        // Standing in for the identity transfer reconciliation performs around
        // the hand-over, which is what makes the carried capture resolve against
        // the new subtree.
        rebuilt.child.set_element_id(pressed.child.id());
        rebuilt.adopt_runtime_state_from(&pressed as &dyn Element);

        let up = rebuilt.on_event(&ElementEvent::PointerUp(PointerInfo::touch(
            Vec2d { x: 200.0, y: 200.0 },
            pointer.id,
        )));

        assert_eq!(events.get(), 2, "the child must hear the release it is owed");
        assert_eq!(up.capture_request(), CaptureRequest::Release(pointer));
    }
}
