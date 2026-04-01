use crate::scrollable::scroll_behavior::ScrollBehavior;
use crate::scrollable::ScrollAxis;
use attribute::position::Vec2d;
use chrono::{DateTime, Utc};
use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragMode {
    None = 0,
    Content = 1,
    VerticalScrollbar = 2,
    HorizontalScrollbar = 3,
    Pending = 4,
}

pub struct ScrollController {
    pub(crate) scroll_behavior: ScrollBehavior,
    pub(crate) axis: ScrollAxis,
    pub(crate) scroll_offset: Cell<Vec2d>,
    pub(crate) last_pointer_pos: Cell<Option<Vec2d>>,
    pub(crate) drag_mode: Cell<DragMode>,
    pub(crate) cached_max_scroll: Cell<Vec2d>,
    pub(crate) cached_min_scroll: Cell<Vec2d>,
    pub(crate) pointer_velocity: Cell<Vec2d>,
    pub(crate) last_event_time: Cell<Option<DateTime<Utc>>>,
    pub(crate) last_frame_time: Cell<Option<DateTime<Utc>>>,
    pub(crate) v_thumb_rect: Cell<Option<(f32, f32, f32, f32)>>,
    pub(crate) h_thumb_rect: Cell<Option<(f32, f32, f32, f32)>>,
    pub(crate) v_scroll_multiplier: Cell<f32>,
    pub(crate) h_scroll_multiplier: Cell<f32>,
    pub(crate) last_scale: Cell<f32>,
    pub(crate) speed_multiplier: f32,
    pub(crate) cursor_pos: Cell<Option<Vec2d>>,
}

impl ScrollController {
    /// Clamp the scroll offset within the allowed range.
    /// scroll_offset is negative (content moves up), so min_scroll <= offset <= 0 typically.
    pub(crate) fn clamp_offset(&self, mut offset: Vec2d) -> Vec2d {
        let min = self.cached_min_scroll.get();
        let max = self.cached_max_scroll.get();
        offset.x = offset.x.max(-max.x).min(-min.x);
        offset.y = offset.y.max(-max.y).min(-min.y);
        offset
    }

    #[inline(always)]
    fn apply_bouncy(value: f32, min: f32, max: f32, resistance: f32) -> f32 {
        if value < min {
            let diff = min - value;
            min - diff.powf(0.75) * (resistance * 2.0)
        } else if value > max {
            let diff = value - max;
            max + diff.powf(0.75) * (resistance * 2.0)
        } else {
            value
        }
    }

    pub(crate) fn visual_offset(&self, offset: Vec2d) -> Vec2d {
        let min = self.cached_min_scroll.get();
        let max = self.cached_max_scroll.get();

        let min_x = -min.x;
        let max_x = -max.x;
        let min_y = -min.y;
        let max_y = -max.y;

        if self.scroll_behavior.bouncy {
            let resistance = self.scroll_behavior.bouncy_resistance as f32;

            (
                Self::apply_bouncy(offset.x, max_x, min_x, resistance),
                Self::apply_bouncy(offset.y, max_y, min_y, resistance),
            )
                .into()
        } else {
            (offset.x.clamp(max_x, min_x), offset.y.clamp(max_y, min_y)).into()
        }
    }

    /// Check if a point is inside the vertical thumb rect.
    pub(crate) fn hit_test_v_thumb(&self, p: Vec2d) -> bool {
        if let Some((x, y, w, h)) = self.v_thumb_rect.get() {
            p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h
        } else {
            false
        }
    }

    /// Check if a point is inside the horizontal thumb rect.
    pub(crate) fn hit_test_h_thumb(&self, p: Vec2d) -> bool {
        if let Some((x, y, w, h)) = self.h_thumb_rect.get() {
            p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h
        } else {
            false
        }
    }

    /// Update momentum, spring-back, and friction during the draw phase (when not dragging).
    /// Returns the updated offset and whether a redraw is needed.
    pub(crate) fn update_momentum(&self, mut offset: Vec2d) -> (Vec2d, bool) {
        let clamped = self.clamp_offset(offset);
        let mut velocity = self.pointer_velocity.get();
        let mut needs_redraw = false;

        let now = Utc::now();
        let dt = self
            .last_frame_time
            .get()
            .map(|t| (now - t).num_microseconds().unwrap_or(0) as f32 / 1_000_000.0)
            .unwrap_or(1.0 / 120.0)
            .min(0.05);
        self.last_frame_time.set(Some(now));

        let frame_ratio = dt / (1.0 / 120.0);

        if velocity.x.abs() > 0.01 || velocity.y.abs() > 0.01 {
            #[cfg(target_os = "ios")]
            let oob_damping_base: f32 = 0.15;
            #[cfg(not(target_os = "ios"))]
            let oob_damping_base: f32 = 0.4;
            if offset.x != clamped.x {
                let damping = oob_damping_base.powf(frame_ratio);
                velocity.x *= damping;
                if (offset.x > clamped.x && velocity.x > 0.0) || (offset.x < clamped.x && velocity.x < 0.0) {
                    velocity.x *= 0.5;
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

        // Spring back if bouncy is enabled
        if self.scroll_behavior.bouncy && (offset.x != clamped.x || offset.y != clamped.y) {
            let dx = clamped.x - offset.x;
            let dy = clamped.y - offset.y;

            let base_spring = 1.0 - (1.0 - self.scroll_behavior.bouncy_recovery).powf(frame_ratio);
            let spring_factor = base_spring.sqrt();

            offset.x += dx * spring_factor;
            offset.y += dy * spring_factor;

            let mut v = self.pointer_velocity.get();
            v.x *= 0.7_f32.powf(frame_ratio);
            v.y *= 0.7_f32.powf(frame_ratio);
            self.pointer_velocity.set(v);

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

        (offset, needs_redraw)
    }
}