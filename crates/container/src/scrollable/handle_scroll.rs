use chrono::Utc;
use attribute::Float;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use events::element::ElementEvent;
use utils::debug;
use widget::base::BuildContext;
use widget::Element;
use crate::raw_scroll::{DragMode, RawScrollableContainer};
use crate::ScrollAxis;

impl<E: Element> Element for RawScrollableContainer<E> {
    fn on_event(&self, event: &ElementEvent) -> bool {

        if let Some(cursor_pos) = event.get_pointer_pos() {
            self.cursor_pos.set(Some(cursor_pos));
        }

        if let Some(cursor) = self.cursor_pos.get() {
            if !self.bounds.is_inside(cursor.x, cursor.y) {
                self.drag_mode.set(DragMode::None);
                return false;
            }
        }

        let pos = match event {
            ElementEvent::PointerDown(p)
            | ElementEvent::PointerUp(p)
            | ElementEvent::PointerMove(p)
            | ElementEvent::Scroll(p) => *p,
            ElementEvent::Cancel | ElementEvent::CharInput { .. } | ElementEvent::KeyInput { .. } => Vec2d::default(),
        };



        let mode_before = self.drag_mode.get();
        let mut child_consumed = false;

        // Forward events to child manually if we haven't stolen the drag yet
        if mode_before == DragMode::None || mode_before == DragMode::Pending {
            child_consumed = widget::dispatch_event(&self.child, pos, event);
        } else if matches!(event, ElementEvent::PointerUp(_) | ElementEvent::Cancel) {
            // Ensure child gets cancel if we are active
            let _ = widget::dispatch_event(&self.child, pos, &ElementEvent::Cancel);
        }

        let we_consumed = match event {
            ElementEvent::Scroll(delta) => {
                let offset = self.scroll_offset.get();
                let clamped = self.clamp_offset(offset);

                // For MacOS trackpads/Natural scroll, we want to treat scroll events
                // more like velocity inputs to allow for smooth interpolation and momentum.
                let mut scroll_delta = match self.axis {
                    ScrollAxis::Vertical => Vec2d { x: 0.0, y: delta.y * 1.1 },
                    ScrollAxis::Horizontal => Vec2d { x: delta.x * 1.1, y: 0.0 },
                };

                // Apply bouncy resistance if we're out of bounds
                // This ensures the injected velocity reflects the resistance immediately
                if self.scroll_behavior.bouncy {
                    match self.axis {
                        ScrollAxis::Vertical => {
                            if offset.y > clamped.y || offset.y < clamped.y {
                                let oob_dist = (offset.y - clamped.y).abs();
                                let viewport_h = self.cached_max_scroll.get().y.max(100.0);
                                let resistance = (1.0 - (oob_dist / viewport_h).min(0.75)).powi(2) * 0.3;
                                scroll_delta.y *= resistance;
                            }
                        }
                        ScrollAxis::Horizontal => {
                            if offset.x > clamped.x || offset.x < clamped.x {
                                let oob_dist = (offset.x - clamped.x).abs();
                                let viewport_w = self.cached_max_scroll.get().x.max(100.0);
                                let resistance = (1.0 - (oob_dist / viewport_w).min(0.75)).powi(2) * 0.3;
                                scroll_delta.x *= resistance;
                            }
                        }
                    }
                }

                // If not bouncy, we clamp the delta to prevent over-scrolling via high-velocity scroll events
                if !self.scroll_behavior.bouncy {
                    if (offset.y <= clamped.y && scroll_delta.y < 0.0)
                        || (offset.y >= clamped.y && scroll_delta.y > 0.0)
                    {
                        scroll_delta.y = 0.0;
                    }
                    if (offset.x <= clamped.x && scroll_delta.x < 0.0)
                        || (offset.x >= clamped.x && scroll_delta.x > 0.0)
                    {
                        scroll_delta.x = 0.0;
                    }
                }

                // Inject some velocity so that high-frequency scroll events
                // integrate into the momentum system for a smoother "glide".
                let now = Utc::now();
                let dt = self
                    .last_event_time
                    .get()
                    .map(|t| (now - t).num_microseconds().unwrap_or(0) as f64 / 1_000_000.0)
                    .map(|dt| dt as crate::scrollable::raw_scroll::Float)
                    .unwrap_or(1.0 / 60.0)
                    .max(0.005); // 5ms floor
                self.last_event_time.set(Some(now));

                let frame_ref = 1.0 / 60.0;
                let mut v = self.pointer_velocity.get();

                let mut target_vx = (scroll_delta.x / dt) * frame_ref;
                let mut target_vy = (scroll_delta.y / dt) * frame_ref;

                let max_scroll_v = 5000.0 * self.last_scale.get();
                target_vx = target_vx.clamp(-max_scroll_v, max_scroll_v);
                target_vy = target_vy.clamp(-max_scroll_v, max_scroll_v);

                // For MacOS trackpads, high-frequency events need smoother blending.
                // We use a stronger blend to filter noise.
                v.x = v.x * 0.8 + target_vx * 0.2;
                v.y = v.y * 0.8 + target_vy * 0.2;

                self.pointer_velocity.set(v);

                // Update offset immediately for responsiveness, but ONLY if we aren't
                // already moving very fast (let draw handle fast motion).
                // Actually, to fix shaking, we must ensure we don't have conflicting sources of truth.
                // Let's apply a fraction of the delta here, and let the velocity handle the rest.
                // Or simply let draw() handle it ALL if we request redraw.
                // Redraw is generally fast enough (60-120fps).

                self.window.request_redraw();
                true
            }
            ElementEvent::PointerDown(p) => {
                let mut mode = DragMode::Pending; // 4 = pending content drag
                if let Some((x, y, w, h)) = self.v_thumb_rect.get() {
                    if p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h {
                        mode = DragMode::VerticalScrollbar;
                    }
                }
                if mode == DragMode::Pending {
                    if let Some((x, y, w, h)) = self.h_thumb_rect.get() {
                        if p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h {
                            mode = DragMode::HorizontalScrollbar;
                        }
                    }
                }

                // Stop any existing momentum
                self.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });

                self.drag_mode.set(mode);
                self.last_pointer_pos.set(Some(*p));
                false
            }
            ElementEvent::PointerMove(p) => {
                let mut mode = self.drag_mode.get();

                // Touch slop (drag threshold) check
                if mode == DragMode::Pending {
                    if let Some(start) = self.last_pointer_pos.get() {
                        let dx = p.x - start.x;
                        let dy = p.y - start.y;

                        // Point 5: DPI Awareness
                        let threshold = 10.0 * self.last_scale.get();
                        let exceeds_threshold = match self.axis {
                            ScrollAxis::Vertical => dy.abs() > threshold && dy.abs() > dx.abs(),
                            ScrollAxis::Horizontal => dx.abs() > threshold && dx.abs() > dy.abs(),
                        };

                        if exceeds_threshold {
                            mode = DragMode::Content;
                            self.drag_mode.set(DragMode::Content);

                            // Adjust last_pointer_pos so we don't 'lose' the first 10px of movement
                            // We set it to where the threshold was crossed.
                            let mut adjusted_start = start;
                            match self.axis {
                                ScrollAxis::Vertical => {
                                    if dy > 0.0 {
                                        adjusted_start.y += threshold;
                                    } else {
                                        adjusted_start.y -= threshold;
                                    }
                                }
                                ScrollAxis::Horizontal => {
                                    if dx > 0.0 {
                                        adjusted_start.x += threshold;
                                    } else {
                                        adjusted_start.x -= threshold;
                                    }
                                }
                            }
                            self.last_pointer_pos.set(Some(adjusted_start));

                            // Steal gesture: Send cancel to child so it releases pressed states
                            let _ = widget::dispatch_event(&self.child, *p, &ElementEvent::Cancel);
                        } else {
                            // Still within touch slop or moving in wrong axis, don't scroll yet
                            return child_consumed;
                        }
                    }
                }

                if mode != DragMode::None && mode != DragMode::Pending {
                    if let Some(last) = self.last_pointer_pos.get() {
                        let speed_multiplier = self.speed_multiplier;
                        let dx = (p.x - last.x) * speed_multiplier as Float;

                        let dy = (p.y - last.y) * speed_multiplier as Float;
                        // debug!("PointerMove: y={} | last_y={}", p.y, last.y);

                        // Track velocity based on the current scroll axis and drag mode
                        let mut new_velocity = match mode {
                            DragMode::Content => match self.axis {
                                ScrollAxis::Vertical => Vec2d { x: 0.0, y: dy },
                                ScrollAxis::Horizontal => Vec2d { x: dx, y: 0.0 },
                            },
                            _ => Vec2d { x: 0.0, y: 0.0 }, // No momentum for scrollbar drags
                        };

                        // Time-based velocity tracking
                        let now = Utc::now();
                        let dt = self
                            .last_event_time
                            .get()
                            .map(|t| (now - t).num_microseconds().unwrap_or(0) as f64 / 1_000_000.0)
                            .map(|dt| dt as crate::scrollable::raw_scroll::Float)
                            .unwrap_or(1.0 / 60.0)
                            .max(0.001); // avoid division by zero
                        self.last_event_time.set(Some(now));

                        let frame_ref = 1.0 / 60.0;
                        new_velocity.x = (new_velocity.x / dt) * frame_ref;
                        new_velocity.y = (new_velocity.y / dt) * frame_ref;

                        // Apply a gain/sensitivity boost to feel faster on touch
                        let sensitivity_gain = 1.25 as crate::scrollable::raw_scroll::Float;
                        new_velocity.x *= sensitivity_gain;
                        new_velocity.y *= sensitivity_gain;

                        // Point 2: Time-weighted moving average for velocity blending
                        // Makes it robust against irregular event timing
                        let old_velocity = self.pointer_velocity.get();
                        let blend_factor = (dt / 0.05).min(1.0); // 50ms window for full replacement
                        let blend_new = (0.6 * (1.0 - blend_factor) + blend_factor).min(1.0);
                        let blend_old = 1.0 - blend_new;

                        new_velocity.x = old_velocity.x * blend_old + new_velocity.x * blend_new;
                        new_velocity.y = old_velocity.y * blend_old + new_velocity.y * blend_new;

                        self.pointer_velocity.set(new_velocity);

                        let mut offset = self.scroll_offset.get();
                        let clamped = self.clamp_offset(offset);

                        match mode {
                            DragMode::Content => match self.axis {
                                ScrollAxis::Vertical => {
                                    let mut actual_dy = dy;
                                    // Point 3: Non-linear rubber banding
                                    if offset.y > clamped.y || offset.y < clamped.y {
                                        let oob_dist = (offset.y - clamped.y).abs();
                                        // Viewport-relative quadratic resistance
                                        let viewport_h = self.cached_max_scroll.get().y.max(100.0);
                                        let resistance = (1.0 - (oob_dist / viewport_h).min(0.75)).powi(2) * 0.3;
                                        actual_dy *= resistance;
                                    }
                                    offset.y += actual_dy;
                                }
                                ScrollAxis::Horizontal => {
                                    let mut actual_dx = dx;
                                    if offset.x > clamped.x || offset.x < clamped.x {
                                        let oob_dist = (offset.x - clamped.x).abs();
                                        let viewport_w = self.cached_max_scroll.get().x.max(100.0);
                                        let resistance = (1.0 - (oob_dist / viewport_w).min(0.75)).powi(2) * 0.3;
                                        actual_dx *= resistance;
                                    }
                                    offset.x += actual_dx;
                                }
                            },
                            DragMode::VerticalScrollbar => {
                                // Point 7: Smooth scrollbar interaction
                                let target_y = offset.y - dy * self.v_scroll_multiplier.get();
                                offset.y = offset.y * 0.4 + target_y * 0.6;
                            }
                            DragMode::HorizontalScrollbar => {
                                let target_x = offset.x - dx * self.h_scroll_multiplier.get();
                                offset.x = offset.x * 0.4 + target_x * 0.6;
                            }
                            _ => {}
                        }

                        if !self.scroll_behavior.bouncy {
                            offset = self.clamp_offset(offset);
                        }
                        self.scroll_offset.set(offset);
                    }
                    self.last_pointer_pos.set(Some(*p));
                    self.window.request_redraw();
                    return true;
                }
                false
            }
            ElementEvent::CharInput { .. } | ElementEvent::KeyInput { .. } => child_consumed,
            ElementEvent::PointerUp(_) | ElementEvent::Cancel => {
                // If the user pauses before lifting, momentum should be cleared
                let now = Utc::now();
                if let Some(last_time) = self.last_event_time.get() {
                    let elapsed = (now - last_time).num_milliseconds();
                    if elapsed > 100 {
                        self.pointer_velocity.set(Vec2d::default());
                    }
                }

                self.drag_mode.set(DragMode::None);
                self.last_pointer_pos.set(None);
                self.window.request_redraw();
                false
            }
        };

        child_consumed || we_consumed
    }

    fn event_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Do nothing. We intercept and manage child event dispatch manually in `on_event`
        // to properly handle touch slop and scroll stealing.
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ResolvedSize { width: ctx.box_constraint.max_width, height: ctx.box_constraint.max_height }
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let mut child_ctx = ctx.clone();
        match self.axis {
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = crate::scrollable::raw_scroll::Float::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = crate::scrollable::raw_scroll::Float::MAX,
        }
        self.child.computed_size(&child_ctx)
    }
}