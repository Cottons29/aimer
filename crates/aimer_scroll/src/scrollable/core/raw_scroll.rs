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
use aimer_widget::components::element::{
    paint_element_was_invalidated, paint_invalidations_are_known, paint_subtree_was_invalidated,
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
    layout: ScrollLayoutKey,
    child_generation: u64,
    rebuild_generation: u64,
    texture_epoch: u64,
    content_width_bits: u32,
    content_height_bits: u32,
}

#[cfg(not(feature = "portable-guest"))]
struct RetainedPaint {
    key: ScrollPaintKey,
    content: Arc<aimer_canvas::RetainedLayerContent>,
    element_ids: Vec<ElementId>,
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
    element_ids: Vec<ElementId>,
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
#[inline]
fn retained_tile_contract_matches(left: ScrollPaintKey, right: ScrollPaintKey) -> bool {
    left.layout.constraint == right.layout.constraint
        && left.layout.parent_size == right.layout.parent_size
        && left.layout.scale_bits == right.layout.scale_bits
        && left.layout.axis == right.layout.axis
        && left.layout.viewport_w == right.layout.viewport_w
        && left.layout.viewport_h == right.layout.viewport_h
        && left.layout.vertical_bar_width == right.layout.vertical_bar_width
        && left.layout.horizontal_bar_height == right.layout.horizontal_bar_height
        && left.layout.layout_generation == right.layout.layout_generation
        && left.texture_epoch == right.texture_epoch
        && left.content_width_bits == right.content_width_bits
        && left.content_height_bits == right.content_height_bits
}

#[cfg(not(feature = "portable-guest"))]
#[inline]
fn retained_paint_contract_matches(left: ScrollPaintKey, right: ScrollPaintKey) -> bool {
    left.layout.constraint == right.layout.constraint
        && left.layout.parent_size == right.layout.parent_size
        && left.layout.scale_bits == right.layout.scale_bits
        && left.layout.axis == right.layout.axis
        && left.layout.viewport_w == right.layout.viewport_w
        && left.layout.viewport_h == right.layout.viewport_h
        && left.layout.vertical_bar_width == right.layout.vertical_bar_width
        && left.layout.horizontal_bar_height == right.layout.horizontal_bar_height
        && left.layout.layout_generation == right.layout.layout_generation
        && left.texture_epoch == right.texture_epoch
        && left.content_width_bits == right.content_width_bits
        && left.content_height_bits == right.content_height_bits
}

#[cfg(not(feature = "portable-guest"))]
#[inline]
fn retained_paint_can_reuse(
    cached: &RetainedPaint,
    key: ScrollPaintKey,
    dynamic_islands: bool,
) -> bool {
    if cached.dynamic_islands != dynamic_islands {
        return false;
    }
    if cached.key == key {
        return true;
    }
    if !retained_paint_contract_matches(cached.key, key)
        || cached.key.layout.tree_generation != key.layout.tree_generation
        || !paint_invalidations_are_known()
    {
        return false;
    }
    cached
        .element_ids
        .iter()
        .all(|element| !paint_element_was_invalidated(*element))
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
    snapshot: RefCell<Option<RetainedPaint>>,
    tiles: RefCell<HashMap<RetainedTileCoordinate, RetainedPaintTile>>,
    tile_key: Cell<Option<ScrollPaintKey>>,
    tile_draws: RefCell<Vec<RetainedTileDraw>>,
}

#[cfg(not(feature = "portable-guest"))]
static NEXT_SCROLL_LAYER_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(not(feature = "portable-guest"))]
impl Default for ScrollPaintCache {
    fn default() -> Self {
        Self {
            layer_id: NEXT_SCROLL_LAYER_ID.fetch_add(1, Ordering::Relaxed),
            snapshot: RefCell::new(None),
            tiles: RefCell::new(HashMap::new()),
            tile_key: Cell::new(None),
            tile_draws: RefCell::new(Vec::new()),
        }
    }
}

#[cfg(not(feature = "portable-guest"))]
impl ScrollPaintCache {
    fn clear(&self) {
        self.snapshot.borrow_mut().take();
        self.tiles.borrow_mut().clear();
        self.tile_key.set(None);
        self.tile_draws.borrow_mut().clear();
    }

    fn clear_tiles(&self) {
        self.tiles.borrow_mut().clear();
        self.tile_key.set(None);
        self.tile_draws.borrow_mut().clear();
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
    fn paint_key(&self, ctx: &BuildContext, content_size: ResolvedSize) -> ScrollPaintKey {
        ScrollPaintKey {
            layout: self.layout_key(ctx),
            child_generation: self.child.subtree_generation(),
            rebuild_generation: rebuild_invalidation_generation(),
            texture_epoch: ctx.canvas.texture_cache_epoch(),
            content_width_bits: content_size.width.to_bits(),
            content_height_bits: content_size.height.to_bits(),
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
    ) {
        let stable = self.child.is_paint_stable();
        #[cfg(debug_assertions)]
        let stable = stable && !aimer_widget::inspector_overlay::is_enabled();
        if !stable {
            if self.draw_child_with_dynamic_islands(ctx, child_ctx, content_size) {
                return;
            }
            self.paint_cache.clear();
            self.child.draw(child_ctx);
            return;
        }

        let key = self.paint_key(ctx, content_size);
        if can_use_retained_layer(content_size) {
            self.paint_cache.clear_tiles();
            if self
                .paint_cache
                .snapshot
                .borrow()
                .as_ref()
                .is_some_and(|cached| retained_paint_can_reuse(cached, key, false))
            {
                let cached = self.paint_cache.snapshot.borrow();
                if let Some(cached) = cached.as_ref() {
                    ctx.canvas.draw_retained_layer(
                        self.paint_cache.layer_id,
                        content_size.width,
                        content_size.height,
                        cached.content.clone(),
                    );
                    return;
                }
            }

            // Drop stale commands before recording a replacement. This makes a
            // style/text/scale/texture invalidation release the old command
            // payload immediately instead of retaining two generations.
            self.paint_cache.snapshot.borrow_mut().take();

            let recording_canvas = ctx.canvas.fork_for_recording();
            let mut recording_ctx = child_ctx.clone();
            recording_ctx.canvas = Canvas::new(&recording_canvas);
            // A retained stream must contain all content. The outer viewport clip
            // still prevents it from reaching the framebuffer when replayed.
            recording_ctx.visible_rect = None;
            aimer_widget::components::element::begin_paint_tracking();
            self.child.draw(&recording_ctx);
            let element_ids = aimer_widget::components::element::take_paint_tracking();

            let recorded = recording_canvas.take_draw_list();
            let Some(commands) = recorded.retained_snapshot() else {
                // Rich text, uploads, and custom pipeline payloads intentionally
                // decline retention because replaying them would allocate or
                // duplicate non-cloneable state. The ordinary draw remains the
                // correct fallback for those trees.
                self.paint_cache.clear();
                self.child.draw(child_ctx);
                return;
            };

            let content = Arc::new(aimer_canvas::RetainedLayerContent::from_snapshot(commands));
            if !content.is_compositor_safe() {
                self.paint_cache.clear();
                self.child.draw(child_ctx);
                return;
            }
            self.paint_cache.snapshot.borrow_mut().replace(RetainedPaint {
                key,
                content: content.clone(),
                element_ids,
                dynamic_islands: false,
            });
            ctx.canvas.draw_retained_layer(
                self.paint_cache.layer_id,
                content_size.width,
                content_size.height,
                content,
            );
            return;
        }

        // A full layer would exceed the memory cap. Keep only the tiles around
        // the cache window, with a small overlap so elements crossing a tile
        // edge are recorded into the tile that owns their visible pixels.
        self.paint_cache.snapshot.borrow_mut().take();
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
    ) -> bool {
        if !can_use_retained_layer(content_size) {
            return false;
        }

        let key = self.paint_key(ctx, content_size);
        let cached_content = self
            .paint_cache
            .snapshot
            .borrow()
            .as_ref()
            .filter(|cached| retained_paint_can_reuse(cached, key, true))
            .map(|cached| (cached.content.clone(), cached.element_ids.clone()));
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
                paint_ctx.canvas = Canvas::new(recording);
                recording.save();
                if let Some(clip) = clip {
                    recording.set_clip(0.0, 0.0, clip.width, clip.height);
                }
                recording.translate(offset.x, offset.y);
                element.draw(&paint_ctx);
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
                    if state.static_content.is_none() {
                        let Some((content, element_ids)) = state.finish_recording() else {
                            state.failed = true;
                            return;
                        };
                        self.paint_cache.snapshot.borrow_mut().replace(RetainedPaint {
                            key,
                            content: content.clone(),
                            element_ids,
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
            self.paint_cache.snapshot.borrow_mut().replace(RetainedPaint {
                key,
                content: content.clone(),
                element_ids,
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

        if let Some(cached_key) = self.paint_cache.tile_key.get() {
            if !retained_tile_contract_matches(cached_key, key) {
                self.paint_cache.tiles.borrow_mut().clear();
            } else if cached_key != key {
                let subtree_generation_changed = cached_key.layout.tree_generation
                    != key.layout.tree_generation
                    || cached_key.child_generation != key.child_generation;
                if subtree_generation_changed || !paint_invalidations_are_known() {
                    self.paint_cache.tiles.borrow_mut().clear();
                } else if paint_subtree_was_invalidated(self.child.id()) {
                    self.paint_cache.tiles.borrow_mut().retain(|_, tile| {
                        !tile
                            .element_ids
                            .iter()
                            .any(|element| paint_element_was_invalidated(*element))
                    });
                }
            }
        }
        self.paint_cache.tile_key.set(Some(key));
        self.paint_cache.tiles.borrow_mut().retain(|coordinate, _tile| {
            coordinate.x >= range.x_start
                && coordinate.x < range.x_end
                && coordinate.y >= range.y_start
                && coordinate.y < range.y_end
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
                        element_ids,
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
        recording_ctx.canvas = Canvas::new(&recording_canvas);
        recording_ctx.visible_rect = Some((
            (rect.x - RETAINED_LAYER_TILE_OVERLAP_PX).max(0.0),
            (rect.y - RETAINED_LAYER_TILE_OVERLAP_PX).max(0.0),
            (rect.width + 2.0 * RETAINED_LAYER_TILE_OVERLAP_PX)
                .min(content_size.width - rect.x + RETAINED_LAYER_TILE_OVERLAP_PX),
            (rect.height + 2.0 * RETAINED_LAYER_TILE_OVERLAP_PX)
                .min(content_size.height - rect.y + RETAINED_LAYER_TILE_OVERLAP_PX),
        ));
        aimer_widget::components::element::begin_paint_tracking();
        self.child.draw(&recording_ctx);
        let element_ids = aimer_widget::components::element::take_paint_tracking();

        let recorded = recording_canvas.take_draw_list();
        let commands = recorded.retained_snapshot()?;
        let content = Arc::new(aimer_canvas::RetainedLayerContent::from_snapshot(commands));
        content
            .is_compositor_safe()
            .then_some((content, element_ids))
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
    use std::cell::Cell;

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
    }

    #[tokio::test]
    async fn dynamic_scrollable_content_stays_on_the_direct_path() {
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
    }

    #[tokio::test]
    async fn dynamic_islands_retain_a_static_prefix_and_redraw_only_the_dynamic_suffix() {
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
    }

    #[tokio::test]
    async fn oversized_stable_scrollable_reuses_its_visible_retained_tile() {
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
