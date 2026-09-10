//! Animation-specific damage policies.

use std::cell::Cell;

use aimer_widget::base::BuildContext;
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
    if !visual_value.is_finite() || !child.is_paint_stable() || !child.is_layout_stable() {
        tracker.mark_full();
    } else {
        tracker.mark_current_bounds(ctx, child.content_size(ctx), visual_changed);
    }
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
