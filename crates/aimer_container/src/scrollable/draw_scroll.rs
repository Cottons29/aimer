use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_widget::base::BuildContext;
use aimer_widget::{Drawable, Element, LayoutElement};

use crate::ScrollAxis;
use crate::raw_scroll::{DragMode, RawScrollableContainer};

impl<E: Element> Drawable for RawScrollableContainer<E> {
    fn draw(&self, ctx: &BuildContext) {
        // println!("Scrollable drawing child: {})", self.child.debug_name() );

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
        self.ctrl
            .cached_content_size
            .set(content_size);
        let transform = ctx
            .canvas
            .get_transform_translation();
        let max_x = (content_size.width - viewport_w).max(0.0);
        let max_y = (content_size.height - viewport_h).max(0.0);

        self.bounds
            .save(ctx.scale, transform.0, transform.1, viewport_w, viewport_h);
        self.ctrl
            .cached_viewport
            .set((viewport_w, viewport_h));
        self.ctrl
            .cursor_pos
            .set(Some(ctx.cursor_pos));

        let mut final_max = Vec2d { x: max_x, y: max_y };
        let user_max = self
            .ctrl
            .scroll_behavior
            .max_scroll;
        if user_max.x != f32::MAX {
            final_max.x = final_max
                .x
                .max(user_max.x * ctx.scale);
        }
        if user_max.y != f32::MAX {
            final_max.y = final_max
                .y
                .max(user_max.y * ctx.scale);
        }

        self.ctrl
            .cached_max_scroll
            .set(final_max);

        let user_min = self
            .ctrl
            .scroll_behavior
            .min_scroll;
        self.ctrl
            .cached_min_scroll
            .set(Vec2d { x: user_min.x * ctx.scale, y: user_min.y * ctx.scale });

        self.ctrl
            .last_scale
            .set(ctx.scale);

        let mut offset = self
            .ctrl
            .scroll_offset
            .get();

        if self
            .ctrl
            .drag_mode
            .get()
            == DragMode::None
        {
            // let vel = self.ctrl.pointer_velocity.get();
            // let vel_mag = (vel.x * vel.x + vel.y * vel.y).sqrt();
            // if vel_mag > VELOCITY_EPSILON {
            //     info!("[scroll] DRAW momentum vel_mag={:.2} offset=({:.1},{:.1})",
            // vel_mag, offset.x, offset.y); }
            let (new_offset, needs_redraw) = self
                .ctrl
                .update_momentum(offset);
            offset = new_offset;

            if needs_redraw {
                aimer_events::window::request_animation_frame();
            } else {
                // Not dragging and momentum/fling/spring-back have fully settled:
                // this is where a scroll session ends. `end_scroll` is edge-
                // triggered, so it fires the callback only once (on the actual
                // scrolling → idle transition) and is a no-op on later idle frames.
                self.ctrl
                    .end_scroll();
            }
        }

        self.ctrl
            .scroll_offset
            .set(offset);

        // Level-triggered per-frame notification: fires `on_scroll` only when the
        // logical offset actually moved since the last frame (epsilon-guarded), so
        // it covers drags, wheel/keyboard, momentum, spring-back and programmatic
        // scrolls without emitting on idle frames.
        self.ctrl
            .notify_scroll();

        // Write-back: persist the live position so a full teardown/re-create can
        // restore it (see `scroll_storage`). Stored in logical (unscaled) pixels
        // to survive a scale change. Only when the user opted in via `storage_key`.
        crate::scrollable::scroll_storage::save_offset(
            &self
                .ctrl
                .storage_key,
            Vec2d { x: offset.x / ctx.scale, y: offset.y / ctx.scale },
        );

        let offset = self
            .ctrl
            .visual_offset(offset);

        // Clip to viewport
        ctx.canvas.save();
        ctx.canvas.set_clip(
            Vec2d { x: 0.0, y: 0.0 },
            ResolvedSize { width: viewport_w.round(), height: viewport_h.round() },
        );

        // Translate by scroll offset.  Round to device pixels at every scale
        // so that child widget edges always land on exact device-pixel
        // boundaries — without this, a fractional offset combined with the
        // flex layout's child positions produces sub-pixel seams that the GPU
        // anti-aliases into a visible white line between adjacent items.
        let scale = ctx.scale.max(1.0);
        let offset_x = (offset.x * scale).round() / scale;
        let offset_y = (offset.y * scale).round() / scale;

        ctx.canvas
            .translate(Vec2d { x: offset_x, y: offset_y });

        let mut child_ctx = ctx.clone();
        match self.ctrl.axis {
            ScrollAxis::Vertical => {
                child_ctx
                    .box_constraint
                    .max_height = f32::MAX
            }
            ScrollAxis::Horizontal => {
                child_ctx
                    .box_constraint
                    .max_width = f32::MAX
            }
        }
        child_ctx.visible_rect = Some((-offset_x, -offset_y, viewport_w, viewport_h));

        // Draw child content
        self.child
            .draw(&child_ctx);

        // Restore before drawing scrollbars (they should not be offset by scroll)
        ctx.canvas
            .clear_clip();
        ctx.canvas.restore();

        // Draw scrollbars on top, clipped to viewport
        ctx.canvas.save();
        ctx.canvas.set_clip(
            Vec2d { x: 0.0, y: 0.0 },
            ResolvedSize { width: viewport_w.round(), height: viewport_h.round() },
        );
        {
            if let Some(ref vertical_bar) = self.vertical_scroll_bar
                && matches!(self.ctrl.axis, ScrollAxis::Vertical)
            {
                self.draw_scrollbar(ctx, vertical_bar, viewport_w, viewport_h, true);
            }
            if let Some(ref horizontal_bar) = self.horizontal_scroll_bar
                && matches!(self.ctrl.axis, ScrollAxis::Horizontal)
            {
                self.draw_scrollbar(ctx, horizontal_bar, viewport_w, viewport_h, false);
            }
        }
        ctx.canvas
            .clear_clip();
        ctx.canvas.restore();
    }
}
