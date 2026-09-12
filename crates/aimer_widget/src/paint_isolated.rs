//! Framework-owned retained paint for stateful visual subtrees.
//!
//! The module deliberately has a small interface: a caller supplies the live
//! child, its current paint contract, and a canvas. The implementation owns
//! cache validation, local recording, compositor-layer selection, and the
//! direct fallback. Keeping those decisions here prevents every retained
//! caller from growing a slightly different invalidation policy.

use std::cell::RefCell;
use std::sync::Arc;

use aimer_attribute::size::ResolvedSize;
use aimer_cupid::damage_region::DamageRect;
use aimer_canvas::{
    Canvas, RETAINED_LAYER_MAX_BYTES, RETAINED_LAYER_MAX_DIMENSION, RetainedDrawList,
    RetainedLayerContent, next_retained_layer_id,
};

use crate::base::BuildContext;
use crate::components::element::{
    ElementId, element_tree_generation, paint_element_was_invalidated,
    paint_invalidations_are_known, rebuild_invalidation_generation,
};
use crate::Element;
use crate::paint_damage::paint_damage_rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaintContract {
    parent_width: u32,
    parent_height: u32,
    min_width: u32,
    min_height: u32,
    max_width: u32,
    max_height: u32,
    scale: u32,
    width: u32,
    height: u32,
    tree_generation: u64,
    rebuild_generation: u64,
    rebuild_invalidation_generation: u64,
    layout_generation: u64,
    texture_epoch: u64,
}

impl PaintContract {
    #[inline]
    pub(crate) fn new(
        ctx: &BuildContext,
        content_size: ResolvedSize,
        rebuild_generation: u64,
    ) -> Option<Self> {
        let values = [
            ctx.parent_size.width,
            ctx.parent_size.height,
            ctx.box_constraint.min_width,
            ctx.box_constraint.min_height,
            ctx.box_constraint.max_width,
            ctx.box_constraint.max_height,
            ctx.scale,
            content_size.width,
            content_size.height,
        ];
        values
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0)
            .then_some(Self {
                parent_width: ctx.parent_size.width.to_bits(),
                parent_height: ctx.parent_size.height.to_bits(),
                min_width: ctx.box_constraint.min_width.to_bits(),
                min_height: ctx.box_constraint.min_height.to_bits(),
                max_width: ctx.box_constraint.max_width.to_bits(),
                max_height: ctx.box_constraint.max_height.to_bits(),
                scale: ctx.scale.to_bits(),
                width: content_size.width.to_bits(),
                height: content_size.height.to_bits(),
                tree_generation: element_tree_generation(),
                rebuild_generation,
                rebuild_invalidation_generation: rebuild_invalidation_generation(),
                layout_generation: crate::layout_invalidation_generation(),
                texture_epoch: ctx.canvas.texture_cache_epoch(),
            })
    }

    #[inline]
    fn reusable_inputs_match(self, other: Self) -> bool {
        self.parent_width == other.parent_width
            && self.parent_height == other.parent_height
            && self.min_width == other.min_width
            && self.min_height == other.min_height
            && self.max_width == other.max_width
            && self.max_height == other.max_height
            && self.scale == other.scale
            && self.width == other.width
            && self.height == other.height
            && self.tree_generation == other.tree_generation
            && self.rebuild_generation == other.rebuild_generation
            && self.layout_generation == other.layout_generation
            && self.texture_epoch == other.texture_epoch
    }

    #[inline]
    pub(crate) fn width(self) -> f32 {
        f32::from_bits(self.width)
    }

    #[inline]
    pub(crate) fn height(self) -> f32 {
        f32::from_bits(self.height)
    }
}

enum CachedContent {
    Layer(Arc<RetainedLayerContent>),
    Commands(RetainedDrawList),
}

struct CachedPaint {
    key: PaintContract,
    content: CachedContent,
    element_ids: Vec<ElementId>,
}

/// Owns one stateful element's retained visual output.
pub(crate) struct PaintCache {
    layer_id: u64,
    cached: RefCell<Option<CachedPaint>>,
    last_footprint: std::cell::Cell<Option<DamageRect>>,
}

impl Default for PaintCache {
    #[inline]
    fn default() -> Self {
        Self {
            layer_id: next_retained_layer_id(),
            cached: RefCell::new(None),
            last_footprint: std::cell::Cell::new(None),
        }
    }
}

impl PaintCache {
    #[inline]
    pub(crate) fn clear(&self) {
        self.cached.borrow_mut().take();
        self.last_footprint.set(None);
    }

    /// Replays the cache or records the child into a local retained stream.
    ///
    /// `true` means visual commands were emitted into `ctx.canvas`; `false`
    /// means the caller must use the child's ordinary live `draw` path.
    pub(crate) fn paint_or_replay(
        &self,
        ctx: &BuildContext,
        child: &dyn Element,
        key: PaintContract,
    ) -> bool {
        if !child.is_paint_stable() || !child.is_layout_stable() {
            self.clear();
            if !child.is_paint_bounded() {
                crate::mark_paint_damage_full();
            }
            return false;
        }

        if !paint_invalidations_are_known() {
            self.clear();
            if !child.is_paint_bounded() {
                crate::mark_paint_damage_full();
            }
            return false;
        }

        if self
            .cached
            .borrow()
            .as_ref()
            .is_some_and(|cached| Self::can_reuse(cached, key))
        {
            let cached = self.cached.borrow();
            let cached = cached
                .as_ref()
                .expect("cache entry disappeared while replaying retained paint");
            self.note_footprint(footprint(ctx, key));
            Self::replay(self.layer_id, ctx, cached);
            return true;
        }

        let Some(cached) = Self::record(ctx, child, key) else {
            self.clear();
            if !child.is_paint_bounded() {
                crate::mark_paint_damage_full();
            }
            return false;
        };
        self.cached.borrow_mut().replace(cached);
        self.note_footprint(footprint(ctx, key));

        let cached = self.cached.borrow();
        let cached = cached
            .as_ref()
            .expect("newly recorded paint cache entry is missing");
        Self::replay(self.layer_id, ctx, cached);
        true
    }

    fn note_footprint(&self, current: Option<DamageRect>) {
        let previous = self.last_footprint.replace(current);
        if previous == current {
            return;
        }
        if let Some(previous) = previous {
            crate::mark_paint_damage(previous);
        }
        if let Some(current) = current {
            crate::mark_paint_damage(current);
        } else {
            crate::mark_paint_damage_full();
        }
    }

    #[inline]
    fn can_reuse(cached: &CachedPaint, key: PaintContract) -> bool {
        if cached.key == key {
            return true;
        }
        cached.key.reusable_inputs_match(key)
            && paint_invalidations_are_known()
            && cached
                .element_ids
                .iter()
                .all(|element| !paint_element_was_invalidated(*element))
    }

    fn record(ctx: &BuildContext, child: &dyn Element, key: PaintContract) -> Option<CachedPaint> {
        let recording_canvas = ctx.canvas.fork_for_recording();
        let mut recording_ctx = ctx.clone();
        recording_ctx.replace_canvas(Canvas::new(&recording_canvas));
        // The outer canvas owns the current clip. A retained stream must keep
        // complete local content so a later clip change cannot reveal blank
        // pixels that were absent from the first recording.
        recording_ctx.visible_rect = None;

        crate::components::element::begin_paint_tracking();
        child.paint(&recording_ctx);
        let element_ids = crate::components::element::take_paint_tracking();

        let recorded = recording_canvas.take_draw_list();
        let snapshot = recorded.retained_snapshot()?;
        let layer_content = Arc::new(RetainedLayerContent::from_snapshot(snapshot.clone()));
        let content = if layer_content.is_compositor_safe() && can_use_layer(key) {
            CachedContent::Layer(layer_content)
        } else {
            CachedContent::Commands(snapshot)
        };

        Some(CachedPaint {
            key,
            content,
            element_ids,
        })
    }

    #[inline]
    fn replay(layer_id: u64, ctx: &BuildContext, cached: &CachedPaint) {
        match &cached.content {
            CachedContent::Layer(content) => {
                ctx.canvas.draw_retained_layer(
                    layer_id,
                    cached.key.width(),
                    cached.key.height(),
                    content.clone(),
                );
            }
            CachedContent::Commands(commands) => {
                let _ = ctx.canvas.replay_retained(commands);
            }
        }
    }
}

#[inline]
fn can_use_layer(key: PaintContract) -> bool {
    let width = key.width().ceil().max(1.0) as u64;
    let height = key.height().ceil().max(1.0) as u64;
    width <= u64::from(RETAINED_LAYER_MAX_DIMENSION)
        && height <= u64::from(RETAINED_LAYER_MAX_DIMENSION)
        && width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .is_some_and(|bytes| bytes <= RETAINED_LAYER_MAX_BYTES)
}

/// Converts the cache owner's current local box into a conservative device
/// footprint. The small edge pad covers rasterization coverage at antialiased
/// boundaries; custom paint that can bleed farther must remain on the full
/// repaint fallback by leaving `is_paint_stable` disabled.
fn footprint(ctx: &BuildContext, key: PaintContract) -> Option<DamageRect> {
    paint_damage_rect(
        ctx,
        ResolvedSize {
            width: key.width(),
            height: key.height(),
        },
    )
}
