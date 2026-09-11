//! Animation-specific damage policies.

use std::cell::Cell;

use aimer_widget::base::{BuildContext, ResolvedSize};
use aimer_widget::{Element, PaintDamageTracker};

/// Samples one animation value without allocating or comparing floating-point
/// values through an equality operation that treats `NaN` inconsistently.
#[inline]
pub(crate) fn sample_changed(last: &Cell<Option<u32>>, value: f32) -> bool {
    let bits = value.to_bits();
    last.replace(Some(bits)) != Some(bits)
}

/// Marks a bounded transform/paint animation using the child's current
/// transformed footprint. Unknown paint or layout remains a full-frame
/// fallback because a local rectangle cannot prove that uncovered pixels are
/// safe to preserve.
#[inline]
pub(crate) fn mark_bounded_animation_damage(
    tracker: &PaintDamageTracker,
    ctx: &BuildContext,
    child: &dyn Element,
    visual_value: f32,
    visual_changed: bool,
) {
    if !visual_value.is_finite() {
        tracker.mark_full();
    } else {
        mark_bounded_child_damage(tracker, ctx, child, visual_changed);
    }
}

/// Marks a live animation whose output is known to remain inside the child's
/// current layout rectangle. Unlike retained paint stability, this contract
/// permits the child to rebuild, load asynchronously, or change size between
/// frames; [`PaintDamageTracker`] unions the old and current rectangles.
#[inline]
pub(crate) fn mark_bounded_child_damage(
    tracker: &PaintDamageTracker,
    ctx: &BuildContext,
    child: &dyn Element,
    visual_changed: bool,
) {
    if !child.is_paint_bounded() {
        tracker.mark_full();
        return;
    }

    tracker.mark_current_bounds(ctx, child.content_size(ctx), visual_changed);
}

/// Marks an [`AnimatedSwitcher`] cross-fade. Both children share the
/// switcher's local origin, so the maximum width and height cover the union of
/// their rectangles while preserving the tracker's previous-frame cleanup.
#[inline]
pub(crate) fn mark_bounded_crossfade_damage(
    tracker: &PaintDamageTracker,
    ctx: &BuildContext,
    current: &dyn Element,
    old: Option<&dyn Element>,
    visual_changed: bool,
) {
    if !current.is_paint_bounded()
        || old.is_some_and(|child| !child.is_paint_bounded())
    {
        tracker.mark_full();
        return;
    }

    let current_size = current.content_size(ctx);
    let old_size = old.map(|child| child.content_size(ctx));
    if !is_valid_size(current_size) || old_size.is_some_and(|size| !is_valid_size(size)) {
        tracker.mark_full();
        return;
    }
    let bounds = old_size
        .map(|old_size| ResolvedSize {
            width: current_size.width.max(old_size.width),
            height: current_size.height.max(old_size.height),
        })
        .unwrap_or(current_size);

    tracker.mark_current_bounds(ctx, bounds, visual_changed);
}

#[inline]
fn is_valid_size(size: ResolvedSize) -> bool {
    size.width.is_finite()
        && size.height.is_finite()
        && size.width >= 0.0
        && size.height >= 0.0
}

/// Marks an animation whose output is rebuilt or otherwise not bounded by the
/// retained child's local box.
#[inline]
pub(crate) fn mark_dynamic_animation_damage(
    tracker: &PaintDamageTracker,
    output_changed: bool,
) {
    if output_changed {
        tracker.mark_full();
    }
}
