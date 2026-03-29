use crate::ScrollAxis;
use crate::raw_scroll::{DragMode, RawScrollableContainer};
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use chrono::Utc;
use widget::base::BuildContext;
use widget::{Drawable, Element};

impl<E: Element> Drawable for RawScrollableContainer<E> {
    fn draw(&self, ctx: &BuildContext) {
        let (raw_viewport_w, raw_viewport_h) = self.viewport_size(ctx);
        // debug!("View port size: {:?} x {:?}", raw_viewport_w, raw_viewport_h);
        // Cap viewport size to avoid precision issues with f32::MAX in shaders/transforms
        let max_dim = 1e7_f32;
        let viewport_w = raw_viewport_w.min(max_dim);
        let viewport_h = raw_viewport_h.min(max_dim);
        let content_size = self.content_size(ctx);
        let transform = ctx.canvas.get_transform_translation();
        let max_x = (content_size.width - viewport_w).max(0.0);
        let max_y = (content_size.height - viewport_h).max(0.0);

        self.bounds.save(ctx.scale, transform.0, transform.1 , viewport_w, viewport_h);
        self.cursor_pos.set(Some(ctx.cursor_pos));

        let mut final_max = Vec2d { x: max_x, y: max_y };
        let user_max = self.scroll_behavior.max_scroll;
        if user_max.x != f32::MAX {
            final_max.x = final_max.x.max(user_max.x * ctx.scale);
        }
        if user_max.y != f32::MAX {
            final_max.y = final_max.y.max(user_max.y * ctx.scale);
        }

        self.cached_max_scroll.set(final_max);

        let user_min = self.scroll_behavior.min_scroll;
        self.cached_min_scroll
            .set(Vec2d { x: user_min.x * ctx.scale, y: user_min.y * ctx.scale });

        self.last_scale.set(ctx.scale);

        let mut offset = self.scroll_offset.get();

        if self.drag_mode.get() == DragMode::None {
            let clamped = self.clamp_offset(offset);
            let mut velocity = self.pointer_velocity.get();
            let mut needs_redraw = false;

            // Time-based momentum scrolling
            let now = Utc::now();
            let dt = self
                .last_frame_time
                .get()
                .map(|t| (now - t).num_microseconds().unwrap_or(0) as f32 / 1_000_000.0)
                .unwrap_or(1.0 / 120.0)
                .min(0.05); // cap at 50ms to avoid huge jumps after stalls
            self.last_frame_time.set(Some(now));

            let frame_ratio = dt / (1.0 / 120.0);

            if velocity.x.abs() > 0.01 || velocity.y.abs() > 0.01 {
                // Gradually reduce velocity when out of bounds (smooth deceleration)
                #[cfg(target_os = "ios")]
                let oob_damping_base: f32 = 0.15;
                #[cfg(not(target_os = "ios"))]
                let oob_damping_base: f32 = 0.4;
                if offset.x != clamped.x {
                    let damping = oob_damping_base.powf(frame_ratio);
                    velocity.x *= damping;

                    // If we are moving towards the boundary, reflect some velocity or damp it even more
                    // Point 4: Overscroll Velocity Reflection
                    if (offset.x > clamped.x && velocity.x > 0.0) || (offset.x < clamped.x && velocity.x < 0.0) {
                        velocity.x *= 0.5; // Stronger damping when pulling away from bounds
                    }
                }
                if offset.y != clamped.y {
                    let damping = oob_damping_base.powf(frame_ratio);
                    velocity.y *= damping;

                    if (offset.y > clamped.y && velocity.y > 0.0) || (offset.y < clamped.y && velocity.y < 0.0) {
                        velocity.y *= 0.5;
                    }
                }

                offset.x += velocity.x * frame_ratio;
                offset.y += velocity.y * frame_ratio;
                let friction_factor = self.scroll_behavior.friction.powf(frame_ratio);
                velocity.x *= friction_factor;
                velocity.y *= friction_factor;
                self.pointer_velocity.set(velocity);
                needs_redraw = true;
            } else if velocity.x != 0.0 || velocity.y != 0.0 {
                self.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
            }

            // Spring back if bouncy is enabled (time-based for consistent behavior across frame rates)
            if self.scroll_behavior.bouncy && (offset.x != clamped.x || offset.y != clamped.y) {
                // Point 4: Harmonic-like spring back
                let dx = clamped.x - offset.x;
                let dy = clamped.y - offset.y;

                // Chrome-like bounce: use a slightly more "elastic" spring factor
                // We use a square-root easing for the spring factor to make it feel more "Chrome-like" (snappy at first, then smooth)
                let base_spring = 1.0 - (1.0 - self.scroll_behavior.bouncy_recovery).powf(frame_ratio);
                let spring_factor = base_spring.sqrt();

                offset.x += dx * spring_factor;
                offset.y += dy * spring_factor;

                // Damp velocity during spring back to avoid oscillation
                // Chrome's bounce is very damped.
                let mut v = self.pointer_velocity.get();
                v.x *= 0.7_f32.powf(frame_ratio);
                v.y *= 0.7_f32.powf(frame_ratio);
                self.pointer_velocity.set(v);

                // Snap if close enough (sub-pixel threshold)
                if (offset.x - clamped.x).abs() < 0.25 {
                    offset.x = clamped.x;
                    let mut v = self.pointer_velocity.get();
                    v.x = 0.0;
                    self.pointer_velocity.set(v);
                }
                if (offset.y - clamped.y).abs() < 0.25 {
                    offset.y = clamped.y;
                    let mut v = self.pointer_velocity.get();
                    v.y = 0.0;
                    self.pointer_velocity.set(v);
                }
                needs_redraw = true;
            } else if !self.scroll_behavior.bouncy {
                offset = clamped;
            }

            if needs_redraw {
                #[cfg(target_os = "ios")]
                {
                    let window = self.window;
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        window.request_redraw();
                    });
                }
                #[cfg(not(target_os = "ios"))]
                {
                    self.window.request_redraw();
                }
            }
        }

        self.scroll_offset.set(offset);
        let offset = self.visual_offset(offset);

        // Clip to viewport
        ctx.canvas.save();
        ctx.canvas
            .set_clip(Vec2d { x: 0.0, y: 0.0 }, ResolvedSize { width: viewport_w.round(), height: viewport_h.round() });

        // Translate by scroll offset
        // On high-DPI displays (e.g. iOS retina), avoid rounding to preserve smooth sub-pixel scrolling
        let (offset_x, offset_y) =
            if ctx.scale > 1.5 { (offset.x, offset.y) } else { (offset.x.round(), offset.y.round()) };

        ctx.canvas.translate(Vec2d { x: offset_x, y: offset_y });

        let mut child_ctx = ctx.clone();
        match self.axis {
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = f32::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = f32::MAX,
        }
        child_ctx.visible_rect = Some((-offset_x , -offset_y , viewport_w, viewport_h));

        // Draw child content
        self.child.draw(&child_ctx);

        // Restore before drawing scrollbars (they should not be offset by scroll)
        ctx.canvas.clear_clip();
        ctx.canvas.restore();

        // Draw scrollbars on top, clipped to viewport
        ctx.canvas.save();
        ctx.canvas
            .set_clip(Vec2d { x: 0.0, y: 0.0 }, ResolvedSize { width: viewport_w.round(), height: viewport_h.round() });
        {
            if let Some(ref vertical_bar) = self.vertical_scroll_bar {
                if matches!(self.axis, ScrollAxis::Vertical) {
                    self.draw_scrollbar(ctx, vertical_bar, viewport_w, viewport_h, true);
                }
            }
            if let Some(ref horizontal_bar) = self.horizontal_scroll_bar {
                if matches!(self.axis, ScrollAxis::Horizontal) {
                    self.draw_scrollbar(ctx, horizontal_bar, viewport_w, viewport_h, false);
                }
            }
        }
        ctx.canvas.clear_clip();
        ctx.canvas.restore();
    }
}
