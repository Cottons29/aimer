use crate::ScrollAxis;
use crate::raw_scroll::{DragMode, RawScrollableContainer};
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use chrono::Utc;
use events::element::ElementEvent;
use utils::debug;
use widget::Element;
use widget::base::BuildContext;

impl<E: Element> Element for RawScrollableContainer<E> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        if let Some(cursor_pos) = event.get_pointer_pos() {
            self.ctrl.cursor_pos.set(Some(cursor_pos));
        }

        if let Some(cursor) = self.ctrl.cursor_pos.get() {
            if !self.bounds.is_inside(cursor.x, cursor.y) {
                self.ctrl.drag_mode.set(DragMode::None);
                return false;
            }
        }

        let pos = match event {
            ElementEvent::PointerDown(p) | ElementEvent::PointerUp(p) | ElementEvent::PointerMove(p) | ElementEvent::Scroll{delta: p, ..} => *p,
            ElementEvent::Cancel | ElementEvent::CharInput { .. } | ElementEvent::KeyInput { .. } => Vec2d::default(),
        };

        let mode_before = self.ctrl.drag_mode.get();
        let mut child_consumed = false;

        if mode_before == DragMode::None || mode_before == DragMode::Pending {
            child_consumed = widget::dispatch_event(&self.child, pos, event);
        } else if matches!(event, ElementEvent::PointerUp(_) | ElementEvent::Cancel) {
            let _ = widget::dispatch_event(&self.child, pos, &ElementEvent::Cancel);
        }

        let we_consumed = match event {
            ElementEvent::Scroll{delta, phase} => {
                let mut offset = self.ctrl.scroll_offset.get();
                let clamped = self.ctrl.clamp_offset(offset);

                let mut scroll_delta = match self.ctrl.axis {
                    ScrollAxis::Vertical => Vec2d { x: 0.0, y: delta.y },
                    ScrollAxis::Horizontal => Vec2d { x: delta.x, y: 0.0 },
                };

                if self.ctrl.scroll_behavior.bouncy {
                    match self.ctrl.axis {
                        ScrollAxis::Vertical => {
                            if offset.y > clamped.y || offset.y < clamped.y {
                                let oob_dist = (offset.y - clamped.y).abs();
                                let viewport_h = self.ctrl.cached_max_scroll.get().y.max(100.0);
                                let resistance = (1.0 - (oob_dist / viewport_h).min(0.75)).powi(2) * 0.3;
                                scroll_delta.y *= resistance;
                            }
                        }
                        ScrollAxis::Horizontal => {
                            if offset.x > clamped.x || offset.x < clamped.x {
                                let oob_dist = (offset.x - clamped.x).abs();
                                let viewport_w = self.ctrl.cached_max_scroll.get().x.max(100.0);
                                let resistance = (1.0 - (oob_dist / viewport_w).min(0.75)).powi(2) * 0.3;
                                scroll_delta.x *= resistance;
                            }
                        }
                    }
                }

                if !self.ctrl.scroll_behavior.bouncy {
                    if (offset.y <= clamped.y && scroll_delta.y < 0.0) || (offset.y >= clamped.y && scroll_delta.y > 0.0) {
                        scroll_delta.y = 0.0;
                    }
                    if (offset.x <= clamped.x && scroll_delta.x < 0.0) || (offset.x >= clamped.x && scroll_delta.x > 0.0) {
                        scroll_delta.x = 0.0;
                    }
                }

                offset.x += scroll_delta.x;
                offset.y += scroll_delta.y;
                self.ctrl.scroll_offset.set(offset);

                let now = Utc::now();
                let dt = self
                    .ctrl
                    .last_event_time
                    .get()
                    .map(|t| (now - t).num_microseconds().unwrap_or(0) as f32 / 1_000_000.0)
                    // .map(|dt| dt as crate::scrollable::raw_scroll::Float)
                    .unwrap_or(1.0 / 120.0)
                    .max(0.005);
                self.ctrl.last_event_time.set(Some(now));

                let frame_ref = 1.0 / 120.0;
                let mut v = self.ctrl.pointer_velocity.get();

                let mut target_vx = (scroll_delta.x / dt) * frame_ref;
                let mut target_vy = (scroll_delta.y / dt) * frame_ref;

                
                
                if self.ctrl.scroll_behavior.bouncy {
                    match self.ctrl.axis {
                        ScrollAxis::Vertical => {
                            if (offset.y > clamped.y && scroll_delta.y > 0.0) || (offset.y < clamped.y && scroll_delta.y < 0.0) {
                                target_vy *= 0.5;
                            }
                        }
                        ScrollAxis::Horizontal => {
                            if (offset.x > clamped.x && scroll_delta.x > 0.0) || (offset.x < clamped.x && scroll_delta.x < 0.0) {
                                target_vx *= 0.5;
                            }
                        }
                    }
                }

                let max_scroll_v = 15000.0 * self.ctrl.last_scale.get();
                target_vx = target_vx.clamp(-max_scroll_v, max_scroll_v);
                target_vy = target_vy.clamp(-max_scroll_v, max_scroll_v);

                v.x = v.x * 0.7 + target_vx * 0.8;
                v.y = v.y * 0.7 + target_vy* 0.8;

                self.ctrl.pointer_velocity.set(v);

                self.window.request_redraw();
                true
            }
            ElementEvent::PointerDown(p) => {
                let mut mode = DragMode::Pending;
                if self.ctrl.hit_test_v_thumb(*p) {
                    mode = DragMode::VerticalScrollbar;
                }
                if mode == DragMode::Pending && self.ctrl.hit_test_h_thumb(*p) {
                    mode = DragMode::HorizontalScrollbar;
                }

                self.ctrl.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });

                self.ctrl.drag_mode.set(mode);
                self.ctrl.last_pointer_pos.set(Some(*p));
                false
            }
            ElementEvent::PointerMove(p) => {
                let mut mode = self.ctrl.drag_mode.get();

                if mode == DragMode::Pending {
                    if let Some(start) = self.ctrl.last_pointer_pos.get() {
                        let dx = p.x - start.x;
                        let dy = p.y - start.y;

                        let threshold = 10.0 * self.ctrl.last_scale.get();
                        let exceeds_threshold = match self.ctrl.axis {
                            ScrollAxis::Vertical => dy.abs() > threshold && dy.abs() > dx.abs(),
                            ScrollAxis::Horizontal => dx.abs() > threshold && dx.abs() > dy.abs(),
                        };

                        if exceeds_threshold {
                            mode = DragMode::Content;
                            self.ctrl.drag_mode.set(DragMode::Content);

                            let mut adjusted_start = start;
                            match self.ctrl.axis {
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
                            self.ctrl.last_pointer_pos.set(Some(adjusted_start));

                            let _ = widget::dispatch_event(&self.child, *p, &ElementEvent::Cancel);
                        } else {
                            return child_consumed;
                        }
                    }
                }

                if mode != DragMode::None && mode != DragMode::Pending {
                    if let Some(last) = self.ctrl.last_pointer_pos.get() {
                        let speed_multiplier = self.ctrl.speed_multiplier;
                        let dx = (p.x - last.x) * speed_multiplier;

                        let dy = (p.y - last.y) * speed_multiplier;

                        let mut new_velocity = match mode {
                            DragMode::Content => match self.ctrl.axis {
                                ScrollAxis::Vertical => Vec2d { x: 0.0, y: dy },
                                ScrollAxis::Horizontal => Vec2d { x: dx, y: 0.0 },
                            },
                            _ => Vec2d { x: 0.0, y: 0.0 },
                        };

                        let now = Utc::now();
                        let dt = self.ctrl
                            .last_event_time
                            .get()
                            .map(|t| (now - t).num_microseconds().unwrap_or(0) as f32 / 1_000_000.0)
                            .unwrap_or(1.0 / 60.0)
                            .max(0.001);
                        self.ctrl.last_event_time.set(Some(now));

                        let frame_ref = 1.0 / 60.0;
                        new_velocity.x = (new_velocity.x / dt) * frame_ref;
                        new_velocity.y = (new_velocity.y / dt) * frame_ref;

                        let sensitivity_gain = 1.0;
                        new_velocity.x *= sensitivity_gain;
                        new_velocity.y *= sensitivity_gain;

                        let old_velocity = self.ctrl.pointer_velocity.get();
                        let blend_factor = (dt / 0.1).min(1.0);
                        let blend_new = (0.4 * (1.0 - blend_factor) + blend_factor).min(1.0);
                        let blend_old = 1.0 - blend_new;

                        new_velocity.x = old_velocity.x * blend_old + new_velocity.x * blend_new;
                        new_velocity.y = old_velocity.y * blend_old + new_velocity.y * blend_new;

                        self.ctrl.pointer_velocity.set(new_velocity);

                        let mut offset = self.ctrl.scroll_offset.get();
                        let clamped = self.ctrl.clamp_offset(offset);

                        match mode {
                            DragMode::Content => match self.ctrl.axis {
                                ScrollAxis::Vertical => {
                                    let mut actual_dy = dy;

                                    if offset.y > clamped.y || offset.y < clamped.y {
                                        let oob_dist = (offset.y - clamped.y).abs();

                                        let viewport_h = self.ctrl.cached_max_scroll.get().y.max(100.0);
                                        let resistance = (1.0 - (oob_dist / viewport_h).min(0.75)).powi(2) * 0.3;
                                        actual_dy *= resistance;
                                    }
                                    offset.y += actual_dy;
                                }
                                ScrollAxis::Horizontal => {
                                    let mut actual_dx = dx;
                                    if offset.x > clamped.x || offset.x < clamped.x {
                                        let oob_dist = (offset.x - clamped.x).abs();
                                        let viewport_w = self.ctrl.cached_max_scroll.get().x.max(100.0);
                                        let resistance = (1.0 - (oob_dist / viewport_w).min(0.75)).powi(2) * 0.3;
                                        actual_dx *= resistance;
                                    }
                                    offset.x += actual_dx;
                                }
                            },
                            DragMode::VerticalScrollbar => {
                                let target_y = offset.y - dy * self.ctrl.v_scroll_multiplier.get();
                                offset.y = offset.y * 0.4 + target_y * 0.6;
                            }
                            DragMode::HorizontalScrollbar => {
                                let target_x = offset.x - dx * self.ctrl.h_scroll_multiplier.get();
                                offset.x = offset.x * 0.4 + target_x * 0.6;
                            }
                            _ => {}
                        }

                        if !self.ctrl.scroll_behavior.bouncy {
                            offset = self.ctrl.clamp_offset(offset);
                        }
                        self.ctrl.scroll_offset.set(offset);
                    }
                    self.ctrl.last_pointer_pos.set(Some(*p));
                    self.window.request_redraw();
                    return true;
                }
                false
            }
            ElementEvent::CharInput { .. } | ElementEvent::KeyInput { .. } => child_consumed,
            ElementEvent::PointerUp(_) | ElementEvent::Cancel => {
                let now = Utc::now();
                if let Some(last_time) = self.ctrl.last_event_time.get() {
                    let elapsed = (now - last_time).num_milliseconds();
                    if elapsed > 100 {
                        self.ctrl.pointer_velocity.set(Vec2d::default());
                    }
                }

                self.ctrl.drag_mode.set(DragMode::None);
                self.ctrl.last_pointer_pos.set(None);
                self.window.request_redraw();
                false
            }
        };

        child_consumed || we_consumed
    }

    fn event_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {}

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ResolvedSize { width: ctx.box_constraint.max_width, height: ctx.box_constraint.max_height }
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let mut child_ctx = ctx.clone();
        match self.ctrl.axis {
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = f32::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = f32::MAX,
        }
        self.child.computed_size(&child_ctx)
    }
}
