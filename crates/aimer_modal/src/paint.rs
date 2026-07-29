use std::cell::RefCell;

use aimer_attribute::BoxConstraint;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_widget::base::BuildContext;
use aimer_widget::{AnyElement, Drawable};

/// Paints overlay content at `offset` inside the host viewport.
///
/// `offset` is relative to `ctx.parent_pos`, `visuals` carries the current
/// `(opacity, scale)` pair of the entry timeline, and `child_bounds` receives
/// the absolute rectangle used for hit testing the content against a barrier
/// press.
pub(crate) fn paint_overlay_content(
    ctx: &BuildContext,
    child: &AnyElement,
    child_size: ResolvedSize,
    offset: Vec2d,
    visuals: (f32, f32),
    child_bounds: &RefCell<Option<(Vec2d, Vec2d)>>,
) {
    let (opacity, scale) = visuals;
    let origin = Vec2d {
        x: ctx.parent_pos.x + offset.x,
        y: ctx.parent_pos.y + offset.y,
    };
    *child_bounds.borrow_mut() = Some((
        origin,
        Vec2d {
            x: origin.x + child_size.width,
            y: origin.y + child_size.height,
        },
    ));

    let mut child_ctx = ctx.clone();
    child_ctx.parent_size = child_size;
    child_ctx.parent_pos = origin;
    child_ctx.box_constraint = BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: child_size.width,
        max_height: child_size.height,
    };
    child_ctx.visible_rect = ctx
        .visible_rect
        .map(|(x, y, width, height)| (x - offset.x, y - offset.y, width, height));

    let center = Vec2d {
        x: offset.x + child_size.width / 2.0,
        y: offset.y + child_size.height / 2.0,
    };
    ctx.canvas.save();
    ctx.canvas.translate(center);
    ctx.canvas.scale(scale, scale);
    ctx.canvas.translate(Vec2d {
        x: -child_size.width / 2.0,
        y: -child_size.height / 2.0,
    });
    ctx.canvas.set_alpha(opacity);
    child.draw(&child_ctx);
    ctx.canvas.restore_alpha();
    ctx.canvas.restore();
}

/// Returns whether `position` falls inside the last painted content rectangle.
pub(crate) fn contains(child_bounds: &RefCell<Option<(Vec2d, Vec2d)>>, position: Vec2d) -> bool {
    child_bounds.borrow().is_some_and(|(start, end)| {
        position.x >= start.x && position.x <= end.x && position.y >= start.y && position.y <= end.y
    })
}
