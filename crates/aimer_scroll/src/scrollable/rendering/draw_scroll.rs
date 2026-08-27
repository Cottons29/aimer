use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_widget::base::BuildContext;
use aimer_widget::{Drawable, Element, LayoutElement};

use crate::ScrollAxis;
use crate::raw_scroll::{DragMode, RawScrollableContainer};
use crate::scrollable::cache_extent::cache_rect;
use crate::scrollable::recovery_end::finish_overscroll_recovery;

fn snap_scroll_offset(offset: Vec2d) -> Vec2d {
    Vec2d {
        x: offset.x.round(),
        y: offset.y.round(),
    }
}

/// How far the visible content moved since the previous drawn frame, in the
/// child's coordinates.
///
/// The content is translated by the negated scroll offset, so the visible
/// rectangle travels opposite to it. `None` — the first frame — is no travel:
/// there is nothing yet to measure against, and a viewport that has never
/// moved has no direction to lead toward.
fn visible_travel(previous: Option<Vec2d>, offset: Vec2d) -> Vec2d {
    match previous {
        Some(previous) => Vec2d {
            x: previous.x - offset.x,
            y: previous.y - offset.y,
        },
        None => Vec2d::ZERO,
    }
}

impl<E: Element> Drawable for RawScrollableContainer<E> {
    fn draw(&self, ctx: &BuildContext) {
        // println!("Scrollable drawing child: {})", self.child.debug_name() );

        let scrolling_before_draw = self.ctrl.is_scrolling.get();

        let (raw_viewport_w, raw_viewport_h) = self.viewport_size(ctx);
        // debug!("View port size: {:?} x {:?}", raw_viewport_w, raw_viewport_h);
        // Cap viewport size to avoid precision issues with f32::MAX in
        // shaders/transforms
        let max_dim = 1e7_f32;
        let viewport_w = raw_viewport_w.min(max_dim);
        let viewport_h = raw_viewport_h.min(max_dim);
        let content_size = self.content_size(ctx);
        // Cache content size for the rest of this frame (scrollbar drawing reads
        // it) to avoid recomputing the child layout multiple times per draw.
        self.ctrl.cached_content_size.set(content_size);
        let transform = ctx.canvas.get_transform_translation();
        let layout_size = self.layout_size(ctx);
        let max_x = (content_size.width - viewport_w).max(0.0);
        let max_y = (content_size.height - viewport_h).max(0.0);

        self.bounds.save(
            ctx.scale,
            transform.0,
            transform.1,
            layout_size.width,
            layout_size.height,
        );
        self.ctrl.cached_viewport.set((viewport_w, viewport_h));
        self.ctrl.cursor_pos.set(Some(ctx.cursor_pos));

        let mut final_max = Vec2d { x: max_x, y: max_y };
        let user_max = self.ctrl.scroll_behavior.max_scroll;
        if user_max.x != f32::MAX {
            final_max.x = final_max.x.max(user_max.x * ctx.scale);
        }
        if user_max.y != f32::MAX {
            final_max.y = final_max.y.max(user_max.y * ctx.scale);
        }

        self.ctrl.cached_max_scroll.set(final_max);

        let user_min = self.ctrl.scroll_behavior.min_scroll;
        self.ctrl.cached_min_scroll.set(Vec2d {
            x: user_min.x * ctx.scale,
            y: user_min.y * ctx.scale,
        });

        self.ctrl.last_scale.set(ctx.scale);

        let mut offset = self.ctrl.scroll_offset.get();

        if self.ctrl.drag_mode.get() == DragMode::None {
            // let vel = self.ctrl.pointer_velocity.get();
            // let vel_mag = (vel.x * vel.x + vel.y * vel.y).sqrt();
            // if vel_mag > VELOCITY_EPSILON {
            //     info!("[scroll] DRAW momentum vel_mag={:.2} offset=({:.1},{:.1})",
            // vel_mag, offset.x, offset.y); }
            let (new_offset, needs_redraw) = self.ctrl.update_momentum(offset);
            offset = new_offset;

            // A platform that cannot report a finger lift keeps its gesture
            // open through the browser's own momentum tail. Landing the
            // recovered edge is the last moment that gesture is still about
            // what the user did, so it is terminated here and the distance it
            // still owes is dropped — otherwise the queued tail stretches the
            // edge a second time. A no-op everywhere the platform reports the
            // lift itself.
            finish_overscroll_recovery(&self.ctrl, offset);

            if needs_redraw {
                aimer_events::window::request_animation_frame();
            } else {
                // Not dragging and momentum/fling/spring-back have fully settled:
                // this is where a scroll session ends. `end_scroll` is edge-
                // triggered, so it fires the callback only once (on the actual
                // scrolling → idle transition) and is a no-op on later idle frames.
                self.ctrl.end_scroll();
            }
        }

        self.ctrl.scroll_offset.set(offset);

        // Level-triggered per-frame notification: fires `on_scroll` only when the
        // logical offset actually moved since the last frame (epsilon-guarded), so
        // it covers drags, wheel/keyboard, momentum, spring-back and programmatic
        // scrolls without emitting on idle frames.
        let moved = self.ctrl.notify_scroll();
        self.ctrl.record_draw_frame(scrolling_before_draw, moved);

        // Write-back: persist the live position so a full teardown/re-create can
        // restore it (see `scroll_storage`). Stored in logical (unscaled) pixels
        // to survive a scale change. Only when the user opted in via `storage_key`.
        if self.ctrl.remember_scroll_offset {
            crate::scrollable::scroll_storage::save_offset(
                &self.ctrl.storage_key,
                Vec2d {
                    x: offset.x / ctx.scale,
                    y: offset.y / ctx.scale,
                },
            );
        }

        let offset = self.ctrl.visual_offset(offset);

        // Clip to viewport
        ctx.canvas.save();
        ctx.canvas.set_clip(
            Vec2d { x: 0.0, y: 0.0 },
            ResolvedSize {
                width: viewport_w.round(),
                height: viewport_h.round(),
            },
        );

        // Scroll offsets are already scaled into physical canvas coordinates.
        // Snap them directly so text keeps a stable rasterization phase while
        // moving and adjacent child edges cannot develop sub-pixel seams.
        let snapped_offset = snap_scroll_offset(offset);
        let offset_x = snapped_offset.x;
        let offset_y = snapped_offset.y;

        ctx.canvas.translate(Vec2d {
            x: offset_x,
            y: offset_y,
        });

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
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = f32::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = f32::MAX,
        }
        // The child is asked to materialize more than fits on screen. Handing
        // down the exact viewport makes every consumer of `visible_rect` cull
        // to it, which puts a line's whole cost — build, layout, shaping,
        // highlighting, glyph rasterization — on the single frame its edge
        // crosses the boundary, and that frame is the pause the user feels.
        // The extra content is still clipped on the GPU by the viewport clip
        // set above, so it costs nothing to draw.
        let travel = visible_travel(self.ctrl.last_drawn_offset.get(), snapped_offset);
        self.ctrl.last_drawn_offset.set(Some(snapped_offset));
        child_ctx.visible_rect = Some(cache_rect(
            self.ctrl.axis,
            Vec2d {
                x: -offset_x,
                y: -offset_y,
            },
            (viewport_w, viewport_h),
            travel,
        ));

        // The viewport clip makes the content rectangle a known paint bound.
        // Check it before entering the erased child so an off-screen scrollable
        // still updates its physics and bounds without walking its content.
        if ctx.is_rect_visible(0.0, 0.0, viewport_w, viewport_h) {
            #[cfg(not(feature = "portable-guest"))]
            self.draw_child_with_retained_paint(ctx, &child_ctx, content_size);
            #[cfg(feature = "portable-guest")]
            self.child.draw(&child_ctx);
        }

        // Restore before drawing scrollbars (they are separate in-flow children).
        ctx.canvas.clear_clip();
        ctx.canvas.restore();

        if let Some(vertical_bar) = &self.vertical_scroll_bar
            && matches!(self.ctrl.axis, ScrollAxis::Vertical)
        {
            let mut bar_ctx = ctx.clone();
            bar_ctx.box_constraint.max_width = self.vertical_bar_width;
            bar_ctx.box_constraint.max_height = viewport_h;
            bar_ctx.parent_size = ResolvedSize {
                width: self.vertical_bar_width,
                height: viewport_h,
            };
            ctx.canvas.save();
            ctx.canvas.translate(Vec2d {
                x: viewport_w,
                y: 0.0,
            });
            vertical_bar.draw(&bar_ctx);
            ctx.canvas.restore();
        }
        if let Some(horizontal_bar) = &self.horizontal_scroll_bar
            && matches!(self.ctrl.axis, ScrollAxis::Horizontal)
        {
            let mut bar_ctx = ctx.clone();
            bar_ctx.box_constraint.max_width = viewport_w;
            bar_ctx.box_constraint.max_height = self.horizontal_bar_height;
            bar_ctx.parent_size = ResolvedSize {
                width: viewport_w,
                height: self.horizontal_bar_height,
            };
            ctx.canvas.save();
            ctx.canvas.translate(Vec2d {
                x: 0.0,
                y: viewport_h,
            });
            horizontal_bar.draw(&bar_ctx);
            ctx.canvas.restore();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_frame_has_no_direction_to_lead_toward() {
        let travel = visible_travel(None, Vec2d { x: 3.0, y: 40.0 });

        assert_eq!(travel, Vec2d::ZERO);
    }

    #[test]
    fn scrolling_toward_the_content_end_moves_the_visible_rect_forward() {
        // Scrolling down translates the content up, i.e. to a more negative
        // offset, while the visible rectangle moves down the content.
        let travel = visible_travel(
            Some(Vec2d { x: 0.0, y: -100.0 }),
            Vec2d { x: 0.0, y: -140.0 },
        );

        assert_eq!(travel.y, 40.0);
    }

    #[test]
    fn scroll_translation_snaps_scaled_offsets_to_physical_pixels() {
        let offset = Vec2d {
            x: -10.49,
            y: -20.51,
        };

        let snapped = snap_scroll_offset(offset);

        assert_eq!(snapped.x, -10.0);
        assert_eq!(snapped.y, -21.0);
        assert_eq!(snapped.x.fract(), 0.0);
        assert_eq!(snapped.y.fract(), 0.0);
    }
}
