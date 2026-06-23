use crate::raw_scroll::{DragMode, RawScrollableContainer};
use crate::scrollable::constants::*;
use crate::ScrollAxis;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_events::element::ElementEvent;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, EventElement, LayoutElement, VisitorElement};
use web_time::Instant;

impl<E: Element> EventElement for RawScrollableContainer<E> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        if let Some(cursor_pos) = event.get_pointer_pos() {
            self.ctrl.cursor_pos.set(Some(cursor_pos));
        }
        let Some(cursor) = self.ctrl.cursor_pos.get() else {
            return false;
        };

        if !self.bounds.is_inside(cursor.x, cursor.y) {
            self.ctrl.drag_mode.set(DragMode::None);
            return false;
        }

        let pos = match event {
            ElementEvent::PointerDown(p) | ElementEvent::PointerUp(p) | ElementEvent::PointerMove(p) | ElementEvent::Scroll{delta: p, ..} => *p,
            ElementEvent::Cancel | ElementEvent::CharInput { .. } | ElementEvent::KeyInput { .. } => Vec2d::default(),
        };

        let mode_before = self.ctrl.drag_mode.get();
        let mut child_consumed = false;

        if mode_before == DragMode::None || mode_before == DragMode::Pending {
            child_consumed = aimer_widget::dispatch_event(&self.child, pos, event);
        } else if matches!(event, ElementEvent::PointerUp(_) | ElementEvent::Cancel) {
            let _ = aimer_widget::dispatch_event(&self.child, pos, &ElementEvent::Cancel);
        }

        let we_consumed = match event {
            ElementEvent::Scroll{delta, ..} => {
                let mut offset = self.ctrl.scroll_offset.get();
                let clamped = self.ctrl.clamp_offset(offset);

                let mut scroll_delta = match self.ctrl.axis {
                    ScrollAxis::Vertical => Vec2d { x: 0.0, y: delta.y },
                    ScrollAxis::Horizontal => Vec2d { x: delta.x, y: 0.0 },
                };

                if self.ctrl.scroll_behavior.bouncy {
                    match self.ctrl.axis {
                        ScrollAxis::Vertical => {
                            if offset.y != clamped.y {
                                let oob_dist = (offset.y - clamped.y).abs();
                                let viewport_h = self.ctrl.cached_max_scroll.get().y.max(MIN_VIEWPORT);
                                let resistance = (1.0 - (oob_dist / viewport_h).min(OOB_RESISTANCE_CLAMP)).powi(2) * OOB_RESISTANCE_SCALE;
                                scroll_delta.y *= resistance;
                            }
                        }
                        ScrollAxis::Horizontal => {
                            if offset.x != clamped.x {
                                let oob_dist = (offset.x - clamped.x).abs();
                                let viewport_w = self.ctrl.cached_max_scroll.get().x.max(MIN_VIEWPORT);
                                let resistance = (1.0 - (oob_dist / viewport_w).min(OOB_RESISTANCE_CLAMP)).powi(2) * OOB_RESISTANCE_SCALE;
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

                let now = Instant::now();
                let dt = self
                    .ctrl
                    .last_event_time
                    .get()
                    .map(|t| now.duration_since(t).as_secs_f32())
                    .unwrap_or(FRAME_REF_60)
                    .max(MIN_EVENT_DT);
                self.ctrl.last_event_time.set(Some(now));

                let frame_ref = FRAME_REF_60;
                let mut v = self.ctrl.pointer_velocity.get();

                let mut target_vx = (scroll_delta.x / dt) * frame_ref;
                let mut target_vy = (scroll_delta.y / dt) * frame_ref;

                
                
                if self.ctrl.scroll_behavior.bouncy {
                    match self.ctrl.axis {
                        ScrollAxis::Vertical => {
                            if (offset.y > clamped.y && scroll_delta.y > 0.0) || (offset.y < clamped.y && scroll_delta.y < 0.0) {
                                target_vy *= OOB_OVERSHOOT_DAMPING;
                            }
                        }
                        ScrollAxis::Horizontal => {
                            if (offset.x > clamped.x && scroll_delta.x > 0.0) || (offset.x < clamped.x && scroll_delta.x < 0.0) {
                                target_vx *= OOB_OVERSHOOT_DAMPING;
                            }
                        }
                    }
                }

                let max_scroll_v = MAX_SCROLL_VELOCITY * self.ctrl.last_scale.get();
                target_vx = target_vx.clamp(-max_scroll_v, max_scroll_v);
                target_vy = target_vy.clamp(-max_scroll_v, max_scroll_v);

                v.x = v.x * WHEEL_BLEND_OLD + target_vx * WHEEL_BLEND_NEW;
                v.y = v.y * WHEEL_BLEND_OLD + target_vy * WHEEL_BLEND_NEW;

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
                #[allow(clippy::collapsible_if)]
                if mode  == DragMode::Pending {
                    if let Some(start) = self.ctrl.last_pointer_pos.get() {
                        let dx = p.x - start.x;
                        let dy = p.y - start.y;

                        let threshold = DRAG_START_THRESHOLD_DP * self.ctrl.last_scale.get();
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

                            let _ = aimer_widget::dispatch_event(&self.child, *p, &ElementEvent::Cancel);
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

                        let now = Instant::now();
                        let dt = self.ctrl
                            .last_event_time
                            .get()
                            .map(|t| now.duration_since(t).as_secs_f32())
                            .unwrap_or(FRAME_REF_120)
                            .max(MIN_MOVE_DT);
                        self.ctrl.last_event_time.set(Some(now));

                        let frame_ref = FRAME_REF_120;
                        new_velocity.x = (new_velocity.x / dt) * frame_ref;
                        new_velocity.y = (new_velocity.y / dt) * frame_ref;

                        let sensitivity_gain = 1.0;
                        new_velocity.x *= sensitivity_gain;
                        new_velocity.y *= sensitivity_gain;

                        let old_velocity = self.ctrl.pointer_velocity.get();
                        let blend_factor = (dt / DRAG_BLEND_WINDOW).min(1.0);
                        let blend_new = (DRAG_BLEND_BASE * (1.0 - blend_factor) + blend_factor).min(1.0);
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

                                    if offset.y != clamped.y {
                                        let oob_dist = (offset.y - clamped.y).abs();

                                        let viewport_h = self.ctrl.cached_max_scroll.get().y.max(MIN_VIEWPORT);
                                        let resistance = (1.0 - (oob_dist / viewport_h).min(OOB_RESISTANCE_CLAMP)).powi(2) * OOB_RESISTANCE_SCALE;
                                        actual_dy *= resistance;
                                    }
                                    offset.y += actual_dy;
                                }
                                ScrollAxis::Horizontal => {
                                    let mut actual_dx = dx;
                                    if offset.x != clamped.x {
                                        let oob_dist = (offset.x - clamped.x).abs();
                                        let viewport_w = self.ctrl.cached_max_scroll.get().x.max(MIN_VIEWPORT);
                                        let resistance = (1.0 - (oob_dist / viewport_w).min(OOB_RESISTANCE_CLAMP)).powi(2) * OOB_RESISTANCE_SCALE;
                                        actual_dx *= resistance;
                                    }
                                    offset.x += actual_dx;
                                }
                            },
                            DragMode::VerticalScrollbar => {
                                let target_y = offset.y - dy * self.ctrl.v_scroll_multiplier.get();
                                offset.y = offset.y * SCROLLBAR_DRAG_SMOOTH_OLD + target_y * SCROLLBAR_DRAG_SMOOTH_NEW;
                            }
                            DragMode::HorizontalScrollbar => {
                                let target_x = offset.x - dx * self.ctrl.h_scroll_multiplier.get();
                                offset.x = offset.x * SCROLLBAR_DRAG_SMOOTH_OLD + target_x * SCROLLBAR_DRAG_SMOOTH_NEW;
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
                let now = Instant::now();
                if let Some(last_time) = self.ctrl.last_event_time.get() {
                    let elapsed = now.duration_since(last_time).as_millis();
                    if elapsed > VELOCITY_RESET_IDLE_MS {
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


}

impl<E: Element> VisitorElement for RawScrollableContainer<E> {
    fn debug_name(&self) -> &'static str {
        "RawScrollableContainer"
    }
}

impl<E: Element> LayoutElement for RawScrollableContainer<E> {
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
