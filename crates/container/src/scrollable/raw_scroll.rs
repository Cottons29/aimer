use crate::scrollable::scroll_bar::ScrollBar;
use crate::scrollable::{ScrollAxis, ScrollBehavior};
use attribute::dimension::Dimension;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use canvas::CanvasRendering;
use chrono::{DateTime, Utc};
use events::element::ElementEvent;
use std::cell::Cell;
use widget::base::*;
use widget::{Drawable, Element};
use winit::window::Window;
use utils::debug;

#[cfg(not(target_arch = "wasm32"))]
type FLOAT = f32;
#[cfg(target_arch = "wasm32")]
type FLOAT = f64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragMode {
    None = 0,
    Content = 1,
    VerticalScrollbar = 2,
    HorizontalScrollbar = 3,
    Pending = 4,
}

pub struct RawScrollableContainer<E: Element> {
    pub(crate) child: E,
    pub(crate) scroll_behavior: ScrollBehavior,
    pub(crate) axis: ScrollAxis,
    pub(crate) vertical_scroll_bar: Option<ScrollBar>,
    pub(crate) horizontal_scroll_bar: Option<ScrollBar>,
    pub(crate) scroll_offset: Cell<Vec2d>,
    pub(crate) last_pointer_pos: Cell<Option<Vec2d>>,
    pub(crate) drag_mode: Cell<DragMode>, // 0=none, 1=content, 2=v_scrollbar, 3=h_scrollbar 4=pending
    pub(crate) cached_max_scroll: Cell<Vec2d>,
    pub(crate) cached_min_scroll: Cell<Vec2d>,
    pub(crate) pointer_velocity: Cell<Vec2d>,
    pub(crate) last_event_time: Cell<Option<DateTime<Utc>>>,
    pub(crate) last_frame_time: Cell<Option<DateTime<Utc>>>,
    pub(crate) v_thumb_rect: Cell<Option<(FLOAT, FLOAT, FLOAT, FLOAT)>>, // (x, y, w, h)
    pub(crate) h_thumb_rect: Cell<Option<(FLOAT, FLOAT, FLOAT, FLOAT)>>, // (x, y, w, h)
    pub(crate) v_scroll_multiplier: Cell<FLOAT>,
    pub(crate) h_scroll_multiplier: Cell<FLOAT>,
    pub(crate) window: &'static Window,
    pub(crate) speed_multiplier: f32
}

impl<E: Element> RawScrollableContainer<E> {
    /// Compute the viewport size from the build context constraints.
    fn viewport_size(&self, ctx: &BuildContext) -> (FLOAT, FLOAT) {
        (ctx.box_constraint.max_width, ctx.box_constraint.max_height)
    }

    /// Clamp the scroll offset within the allowed range.
    /// scroll_offset is negative (content moves up), so min_scroll <= offset <= 0 typically.
    fn clamp_offset(&self, mut offset: Vec2d) -> Vec2d {
        let min = self.cached_min_scroll.get();
        let max = self.cached_max_scroll.get();
        offset.x = offset.x.max(-max.x).min(-min.x);
        offset.y = offset.y.max(-max.y).min(-min.y);
        offset
    }

    #[inline(always)]
    fn apply_bouncy(value: FLOAT, min: FLOAT, max: FLOAT, resistance: FLOAT) -> FLOAT {
        if value < min {
            min - (min - value) * resistance
        } else if value > max {
            max + (value - max) * resistance
        } else {
            value
        }
    }

    fn visual_offset(&self, offset: Vec2d) -> Vec2d {
        let min = self.cached_min_scroll.get();
        let max = self.cached_max_scroll.get();

        let min_x = -min.x;
        let max_x = -max.x;
        let min_y = -min.y;
        let max_y = -max.y;

        if self.scroll_behavior.bouncy {
            let resistance = self.scroll_behavior.bouncy_resistance as FLOAT;

            (
                Self::apply_bouncy(offset.x, max_x, min_x, resistance),
                Self::apply_bouncy(offset.y, max_y, min_y, resistance),
            )
                .into()
        } else {
            (offset.x.clamp(max_x, min_x), offset.y.clamp(max_y, min_y)).into()
        }
    }


    fn draw_scrollbar(
        &self,
        ctx: &BuildContext,
        scroll_bar: &ScrollBar,
        viewport_w: FLOAT,
        viewport_h: FLOAT,
        is_vertical: bool,
    ) {
        let scale = ctx.scale;
        let offset = self.visual_offset(self.scroll_offset.get());

        let track_width = match scroll_bar.track.width {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => {
                if is_vertical {
                    viewport_w * (p / 100.0)
                } else {
                    viewport_h * (p / 100.0)
                }
            }
            Dimension::Auto => 12.0 * scale,
        };

        let thumb_width = match scroll_bar.thumb.width {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => track_width * (p / 100.0),
            Dimension::Auto => (track_width * 0.6).max(4.0),
        };

        let (track_length, content_extent, scroll_pos) = if is_vertical {
            let content_size = self.content_size(ctx);
            (viewport_h, content_size.height, -offset.y)
        } else {
            let content_size = self.content_size(ctx);
            (viewport_w, content_size.width, -offset.x)
        };

        let button_h = if is_vertical {
            let resolve_btn_h = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> FLOAT {
                match btn.height {
                    Dimension::Px(v) => v * scale,
                    Dimension::Percent(p) => track_length * (p / 100.0),
                    Dimension::Auto => track_width,
                }
            };
            let up_h = scroll_bar
                .up_button
                .as_ref()
                .map(|b| resolve_btn_h(b))
                .unwrap_or(0.0);
            let down_h = scroll_bar
                .down_button
                .as_ref()
                .map(|b| resolve_btn_h(b))
                .unwrap_or(0.0);
            (up_h, down_h)
        } else {
            let resolve_btn_w = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> FLOAT {
                match btn.width {
                    Dimension::Px(v) => v * scale,
                    Dimension::Percent(p) => track_length * (p / 100.0),
                    Dimension::Auto => track_width,
                }
            };
            let left_w = scroll_bar
                .up_button
                .as_ref()
                .map(|b| resolve_btn_w(b))
                .unwrap_or(0.0);
            let right_w = scroll_bar
                .down_button
                .as_ref()
                .map(|b| resolve_btn_w(b))
                .unwrap_or(0.0);
            (left_w, right_w)
        };

        let usable_track = (track_length - button_h.0 - button_h.1).max(0.0);
        let thumb_ratio = if content_extent > 0.0 { (track_length / content_extent).min(1.0) } else { 1.0 };
        let thumb_length = (usable_track * thumb_ratio).max(20.0 * scale);
        let max_thumb_move = (usable_track - thumb_length).max(0.0);
        let max_scroll = (content_extent - track_length).max(0.0);
        let multiplier = if max_thumb_move > 0.0 { max_scroll / max_thumb_move } else { 0.0 };
        if is_vertical {
            self.v_scroll_multiplier.set(multiplier);
        } else {
            self.h_scroll_multiplier.set(multiplier);
        }

        let scroll_ratio = if max_scroll > 0.0 { scroll_pos / max_scroll } else { 0.0 };
        let thumb_offset = button_h.0 + scroll_ratio * max_thumb_move;

        let thumb_radius = match scroll_bar.thumb.radius {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => thumb_width * (p / 100.0),
            Dimension::Auto => thumb_width / 2.0,
        };

        ctx.canvas.save();

        // Position the scrollbar at the edge of the viewport
        if is_vertical {
            ctx.canvas
                .translate(Vec2d { x: (viewport_w - track_width).round(), y: 0.0 });
        } else {
            ctx.canvas
                .translate(Vec2d { x: 0.0, y: (viewport_h - track_width).round() });
        }

        // Draw track
        let track_color: Color = scroll_bar.track.color.into();
        let (track_w, track_h) = if is_vertical { (track_width, track_length) } else { (track_length, track_width) };
        ctx.canvas.fill_color_rect(
            Vec2d { x: 0.0, y: 0.0 },
            ResolvedSize { width: track_w, height: track_h },
            track_color,
            0.0,
        );

        // Draw up/left button
        if let Some(ref btn) = scroll_bar.up_button {
            let btn_color: Color = btn.color.into();
            let (bw, bh) = if is_vertical { (track_width, button_h.0) } else { (button_h.0, track_width) };
            ctx.canvas.fill_color_rect(
                Vec2d { x: 0.0, y: 0.0 },
                ResolvedSize { width: bw, height: bh },
                btn_color,
                0.0,
            );
        }

        // Draw down/right button
        if let Some(ref btn) = scroll_bar.down_button {
            let btn_color: Color = btn.color.into();
            let (bx, by, bw, bh) = if is_vertical {
                (0.0, track_length - button_h.1, track_width, button_h.1)
            } else {
                (track_length - button_h.1, 0.0, button_h.1, track_width)
            };
            ctx.canvas
                .fill_color_rect(Vec2d { x: bx, y: by }, ResolvedSize { width: bw, height: bh }, btn_color, 0.0);
        }

        // Draw thumb
        let thumb_color: Color = scroll_bar.thumb.color.into();
        let thumb_x_offset = (track_width - thumb_width) / 2.0;
        let (tx, ty, tw, th) = if is_vertical {
            self.v_thumb_rect.set(Some((
                viewport_w - track_width + thumb_x_offset,
                thumb_offset,
                thumb_width,
                thumb_length,
            )));
            (thumb_x_offset, thumb_offset, thumb_width, thumb_length)
        } else {
            self.h_thumb_rect.set(Some((
                thumb_offset,
                viewport_h - track_width + thumb_x_offset,
                thumb_length,
                thumb_width,
            )));
            (thumb_offset, thumb_x_offset, thumb_length, thumb_width)
        };

        ctx.canvas.fill_color_rect(
            Vec2d { x: tx, y: ty },
            ResolvedSize { width: tw, height: th },
            thumb_color,
            thumb_radius as f32,
        );

        ctx.canvas.restore();
    }
}

impl<E: Element> Drawable for RawScrollableContainer<E> {
    fn draw(&self, ctx: &BuildContext) {
        let (raw_viewport_w, raw_viewport_h) = self.viewport_size(ctx);
        // Cap viewport size to avoid precision issues with Float::MAX in shaders/transforms
        let max_dim = 1e7 as FLOAT;
        let viewport_w = raw_viewport_w.min(max_dim);
        let viewport_h = raw_viewport_h.min(max_dim);

        let content_size = self.content_size(ctx);
        let max_x = (content_size.width - viewport_w).max(0.0);
        let max_y = (content_size.height - viewport_h).max(0.0);

        let mut final_max = Vec2d { x: max_x, y: max_y };
        let user_max = self.scroll_behavior.max_scroll;
        if user_max.x != FLOAT::MAX {
            final_max.x = final_max.x.max(user_max.x * ctx.scale);
        }
        if user_max.y != FLOAT::MAX {
            final_max.y = final_max.y.max(user_max.y * ctx.scale);
        }

        self.cached_max_scroll.set(final_max);

        let user_min = self.scroll_behavior.min_scroll;
        self.cached_min_scroll
            .set(Vec2d { x: user_min.x * ctx.scale, y: user_min.y * ctx.scale });

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
                .map(|t| (now - t).num_microseconds().unwrap_or(0) as f64 / 1_000_000.0)
                .map(|dt| dt as FLOAT)
                .unwrap_or(1.0 / 120.0)
                .min(0.05); // cap at 50ms to avoid huge jumps after stalls
            self.last_frame_time.set(Some(now));

            let frame_ratio = dt / (1.0 / 120.0);

            if velocity.x.abs() > 0.01 || velocity.y.abs() > 0.01 {
                // Gradually reduce velocity when out of bounds (smooth deceleration)
                #[cfg(target_os = "ios")]
                let oob_damping_base: FLOAT = 0.15;
                #[cfg(not(target_os = "ios"))]
                let oob_damping_base: FLOAT = 0.4;
                if offset.x != clamped.x {
                    let damping = oob_damping_base.powf(frame_ratio);
                    velocity.x *= damping;
                }
                if offset.y != clamped.y {
                    let damping = oob_damping_base.powf(frame_ratio);
                    velocity.y *= damping;
                }

                offset.x += velocity.x * frame_ratio;
                offset.y += velocity.y * frame_ratio;
                let friction_factor = (self.scroll_behavior.friction as FLOAT).powf(frame_ratio);
                velocity.x *= friction_factor;
                velocity.y *= friction_factor;
                self.pointer_velocity.set(velocity);
                needs_redraw = true;
            } else if velocity.x != 0.0 || velocity.y != 0.0 {
                self.pointer_velocity.set(Vec2d { x: 0.0, y: 0.0 });
            }

            // Spring back if bouncy is enabled (time-based for consistent behavior across frame rates)
            if self.scroll_behavior.bouncy && (offset.x != clamped.x || offset.y != clamped.y) {
                let spring_factor = 1.0 - (1.0 - self.scroll_behavior.bouncy_recovery as FLOAT).powf(frame_ratio);
                offset.x += (clamped.x - offset.x) * spring_factor;
                offset.y += (clamped.y - offset.y) * spring_factor;

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
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = FLOAT::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = FLOAT::MAX,
        }
        child_ctx.visible_rect = Some((-offset_x as FLOAT, -offset_y as FLOAT, viewport_w, viewport_h));

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
            if let Some(ref vbar) = self.vertical_scroll_bar {
                if matches!(self.axis, ScrollAxis::Vertical) {
                    self.draw_scrollbar(ctx, vbar, viewport_w, viewport_h, true);
                }
            }
            if let Some(ref hbar) = self.horizontal_scroll_bar {
                if matches!(self.axis, ScrollAxis::Horizontal) {
                    self.draw_scrollbar(ctx, hbar, viewport_w, viewport_h, false);
                }
            }
        }
        ctx.canvas.clear_clip();
        ctx.canvas.restore();
    }
}

impl<E: Element> Element for RawScrollableContainer<E> {
    fn on_event(&self, event: &ElementEvent) -> bool {
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

                let mut offset = self.scroll_offset.get();
                match self.axis {
                    ScrollAxis::Vertical => {
                        offset.y += delta.y;
                    }
                    ScrollAxis::Horizontal => {
                        offset.x += delta.x;
                    }
                }
                if !self.scroll_behavior.bouncy {
                    offset = self.clamp_offset(offset);
                }
                self.scroll_offset.set(offset);
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

                        let threshold = 10.0;
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
                                    if dy > 0.0 { adjusted_start.y += threshold; } else { adjusted_start.y -= threshold; }
                                }
                                ScrollAxis::Horizontal => {
                                    if dx > 0.0 { adjusted_start.x += threshold; } else { adjusted_start.x -= threshold; }
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
                        let dx = p.x - last.x * speed_multiplier;

                        let dy = (p.y - last.y) * speed_multiplier;
                        debug!("PointerMove: y={} | last_y={}", p.y, last.y);

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
                            .map(|dt| dt as FLOAT)
                            .unwrap_or(1.0 / 60.0)
                            .max(0.001); // avoid division by zero
                        self.last_event_time.set(Some(now));

                        let frame_ref = 1.0 / 60.0;
                        new_velocity.x = (new_velocity.x / dt) * frame_ref;
                        new_velocity.y = (new_velocity.y / dt) * frame_ref;

                        // Apply a gain/sensitivity boost to feel faster on touch
                        let sensitivity_gain = 1.25 as FLOAT;
                        new_velocity.x *= sensitivity_gain;
                        new_velocity.y *= sensitivity_gain;

                        // Smooth acceleration: blend with previous velocity 
                        // Reduced history weight (0.4) so the final flick dominates the estimation
                        let old_velocity = self.pointer_velocity.get();
                        let (blend_old, blend_new) = (0.4 as FLOAT, 0.6 as FLOAT);
                        new_velocity.x = old_velocity.x * blend_old + new_velocity.x * blend_new;
                        new_velocity.y = old_velocity.y * blend_old + new_velocity.y * blend_new;

                        self.pointer_velocity.set(new_velocity);

                        let mut offset = self.scroll_offset.get();
                        let clamped = self.clamp_offset(offset);

                        match mode {
                            DragMode::Content => match self.axis {
                                ScrollAxis::Vertical => {
                                    let mut actual_dy = dy;
                                    // Non-linear rubber banding feels more natural
                                    if offset.y > clamped.y || offset.y < clamped.y {
                                         // Simplify for now: use existing 0.3 but could be viewport-relative
                                        actual_dy *= 0.3;
                                    }
                                    offset.y += actual_dy;
                                }
                                ScrollAxis::Horizontal => {
                                    let mut actual_dx = dx;
                                    if offset.x > clamped.x || offset.x < clamped.x {
                                        actual_dx *= 0.3;
                                    }
                                    offset.x += actual_dx;
                                }
                            },
                            DragMode::VerticalScrollbar => {
                                offset.y -= dy * self.v_scroll_multiplier.get();
                            }
                            DragMode::HorizontalScrollbar => {
                                offset.x -= dx * self.h_scroll_multiplier.get();
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
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = FLOAT::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = FLOAT::MAX,
        }
        self.child.computed_size(&child_ctx)
    }
}
