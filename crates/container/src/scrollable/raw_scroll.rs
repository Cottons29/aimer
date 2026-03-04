use std::cell::Cell;
use chrono::{DateTime, Utc};
use attribute::dimension::Dimension;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use widget::base::*;
use widget::{Drawable, Element};
use widget::components::element::ElementEvent;

#[cfg(not(target_arch = "wasm32"))]
use skia_safe::{Color as SkColor, Paint, Rect, paint::Style, RRect};
use winit::window::Window;
use utils::debug;
use crate::scrollable::scroll_bar::ScrollBar;
use crate::scrollable::{ScrollAxis, ScrollBehavior};

#[cfg(not(target_arch = "wasm32"))]
type FLOAT = f32;
#[cfg(target_arch = "wasm32")]
type FLOAT = f64;

pub struct RawScrollableContainer {
    pub(crate) child: Box<dyn Element>,
    pub(crate) scroll_behavior: ScrollBehavior,
    pub(crate) axis: ScrollAxis,
    pub(crate) vertical_scroll_bar: Option<ScrollBar>,
    pub(crate) horizontal_scroll_bar: Option<ScrollBar>,
    pub(crate) scroll_offset: Cell<Vec2d>,
    pub(crate) last_pointer_pos: Cell<Option<Vec2d>>,
    pub(crate) drag_mode: Cell<u8>, // 0=none, 1=content, 2=v_scrollbar, 3=h_scrollbar
    pub(crate) cached_max_scroll: Cell<Vec2d>,
    pub(crate) cached_min_scroll: Cell<Vec2d>,
    pub(crate) pointer_velocity: Cell<Vec2d>,
    pub(crate) last_event_time: Cell<Option<DateTime<Utc>>>,
    pub(crate) last_frame_time: Cell<Option<DateTime<Utc>>>,
    pub(crate) v_thumb_rect: Cell<Option<(FLOAT, FLOAT, FLOAT, FLOAT)>>, // (x, y, w, h)
    pub(crate) h_thumb_rect: Cell<Option<(FLOAT, FLOAT, FLOAT, FLOAT)>>, // (x, y, w, h)
    pub(crate) v_scroll_multiplier: Cell<FLOAT>,
    pub(crate) h_scroll_multiplier: Cell<FLOAT>,
    pub(crate) window: &'static Window
}

impl RawScrollableContainer {
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

    fn visual_offset(&self, mut offset: Vec2d) -> Vec2d {
        let min = self.cached_min_scroll.get();
        let max = self.cached_max_scroll.get();
        
        if self.scroll_behavior.bouncy {
            let resistance = self.scroll_behavior.bouncy_resistance as FLOAT;
            // Apply resistance if out of bounds
            if offset.x < -max.x {
                offset.x = -max.x - ((-max.x - offset.x) * resistance);
            } else if offset.x > -min.x {
                offset.x = -min.x + ((offset.x - -min.x) * resistance);
            }
            if offset.y < -max.y {
                offset.y = -max.y - ((-max.y - offset.y) * resistance);
            } else if offset.y > -min.y {
                offset.y = -min.y + ((offset.y - -min.y) * resistance);
            }
        } else {
            offset.x = offset.x.max(-max.x).min(-min.x);
            offset.y = offset.y.max(-max.y).min(-min.y);
        }
        offset
    }

    /// Draw a single scrollbar (vertical or horizontal).
    #[cfg(not(target_arch = "wasm32"))]
    fn draw_scrollbar_native(
        &self,
        ctx: &BuildContext,
        scroll_bar: &ScrollBar,
        viewport_w: FLOAT,
        viewport_h: FLOAT,
        is_vertical: bool,
    ) {
        let scale = ctx.scale;
        let offset = self.visual_offset(self.scroll_offset.get());

        // Resolve track width
        let track_width = match scroll_bar.track.width {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => {
                if is_vertical { viewport_w * (p / 100.0) } else { viewport_h * (p / 100.0) }
            }
            Dimension::Auto => 12.0 * scale,
        };

        // Resolve thumb width
        let thumb_width = match scroll_bar.thumb.width {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => track_width * (p / 100.0),
            Dimension::Auto => (track_width * 0.6).max(4.0),
        };

        // Compute content extent and thumb size/position
        let (track_length, content_extent, scroll_pos) = if is_vertical {
            let content_size = self.content_size(ctx);
            (viewport_h, content_size.height, -offset.y)
        } else {
            let content_size = self.content_size(ctx);
            (viewport_w, content_size.width, -offset.x)
        };

        // Button heights (if present)
        let button_h = if is_vertical {
            let resolve_btn_h = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> FLOAT {
                match btn.height {
                    Dimension::Px(v) => v * scale,
                    Dimension::Percent(p) => track_length * (p / 100.0),
                    Dimension::Auto => track_width,
                }
            };
            let up_h = scroll_bar.up_button.as_ref().map(|b| resolve_btn_h(b)).unwrap_or(0.0);
            let down_h = scroll_bar.down_button.as_ref().map(|b| resolve_btn_h(b)).unwrap_or(0.0);
            (up_h, down_h)
        } else {
            let resolve_btn_w = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> FLOAT {
                match btn.width {
                    Dimension::Px(v) => v * scale,
                    Dimension::Percent(p) => track_length * (p / 100.0),
                    Dimension::Auto => track_width,
                }
            };
            let left_w = scroll_bar.up_button.as_ref().map(|b| resolve_btn_w(b)).unwrap_or(0.0);
            let right_w = scroll_bar.down_button.as_ref().map(|b| resolve_btn_w(b)).unwrap_or(0.0);
            (left_w, right_w)
        };

        let usable_track = (track_length - button_h.0 - button_h.1).max(0.0);
        let thumb_ratio = if content_extent > 0.0 {
            (track_length / content_extent).min(1.0)
        } else {
            1.0
        };
        let thumb_length = (usable_track * thumb_ratio).max(20.0 * scale);
        let max_thumb_move = (usable_track - thumb_length).max(0.0);
        let max_scroll = (content_extent - track_length).max(0.0);
        let multiplier = if max_thumb_move > 0.0 { max_scroll / max_thumb_move } else { 0.0 };
        if is_vertical {
            self.v_scroll_multiplier.set(multiplier);
        } else {
            self.h_scroll_multiplier.set(multiplier);
        }
        
        let scroll_ratio = if max_scroll > 0.0 {
            scroll_pos / max_scroll
        } else {
            0.0
        };
        let thumb_offset = button_h.0 + scroll_ratio * max_thumb_move;

        let thumb_radius = match scroll_bar.thumb.radius {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => thumb_width * (p / 100.0),
            Dimension::Auto => thumb_width / 2.0,
        };

        ctx.canvas.save();

        // Position the scrollbar at the edge of the viewport
        if is_vertical {
            ctx.canvas.translate(((viewport_w - track_width).round(), 0.0));
        } else {
            ctx.canvas.translate((0.0, (viewport_h - track_width).round()));
        }

        // Draw track
        let track_color: Color = scroll_bar.track.color.into();
        let mut track_paint = Paint::default();
        track_paint.set_anti_alias(true);
        track_paint.set_color(SkColor::from(track_color));
        track_paint.set_style(Style::Fill);

        let track_rect = if is_vertical {
            Rect::from_xywh(0.0, 0.0, track_width, track_length)
        } else {
            Rect::from_xywh(0.0, 0.0, track_length, track_width)
        };
        ctx.canvas.draw_rect(track_rect, &track_paint);

        // Draw up/left button
        if let Some(ref btn) = scroll_bar.up_button {
            let btn_color: Color = btn.color.into();
            let mut btn_paint = Paint::default();
            btn_paint.set_anti_alias(true);
            btn_paint.set_color(SkColor::from(btn_color));
            btn_paint.set_style(Style::Fill);

            let btn_rect = if is_vertical {
                Rect::from_xywh(0.0, 0.0, track_width, button_h.0)
            } else {
                Rect::from_xywh(0.0, 0.0, button_h.0, track_width)
            };
            ctx.canvas.draw_rect(btn_rect, &btn_paint);
        }

        // Draw down/right button
        if let Some(ref btn) = scroll_bar.down_button {
            let btn_color: Color = btn.color.into();
            let mut btn_paint = Paint::default();
            btn_paint.set_anti_alias(true);
            btn_paint.set_color(SkColor::from(btn_color));
            btn_paint.set_style(Style::Fill);

            let btn_rect = if is_vertical {
                Rect::from_xywh(0.0, track_length - button_h.1, track_width, button_h.1)
            } else {
                Rect::from_xywh(track_length - button_h.1, 0.0, button_h.1, track_width)
            };
            ctx.canvas.draw_rect(btn_rect, &btn_paint);
        }

        // Draw thumb
        let thumb_color: Color = scroll_bar.thumb.color.into();
        let mut thumb_paint = Paint::default();
        thumb_paint.set_anti_alias(true);
        thumb_paint.set_color(SkColor::from(thumb_color));
        thumb_paint.set_style(Style::Fill);

        let thumb_x_offset = (track_width - thumb_width) / 2.0;
        let thumb_rect = if is_vertical {
            let tr = Rect::from_xywh(thumb_x_offset, thumb_offset, thumb_width, thumb_length);
            self.v_thumb_rect.set(Some((viewport_w - track_width + thumb_x_offset, thumb_offset, thumb_width, thumb_length)));
            tr
        } else {
            let tr = Rect::from_xywh(thumb_offset, thumb_x_offset, thumb_length, thumb_width);
            self.h_thumb_rect.set(Some((thumb_offset, viewport_h - track_width + thumb_x_offset, thumb_length, thumb_width)));
            tr
        };

        if thumb_radius > 0.0 {
            let rrect = RRect::new_rect_xy(thumb_rect, thumb_radius, thumb_radius);
            ctx.canvas.draw_rrect(rrect, &thumb_paint);
        } else {
            ctx.canvas.draw_rect(thumb_rect, &thumb_paint);
        }

        ctx.canvas.restore();
    }

    /// Draw a single scrollbar (vertical or horizontal) for wasm.
    #[cfg(target_arch = "wasm32")]
    fn draw_scrollbar_wasm(
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
                if is_vertical { viewport_w * (p / 100.0) } else { viewport_h * (p / 100.0) }
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
            let up_h = scroll_bar.up_button.as_ref().map(|b| resolve_btn_h(b)).unwrap_or(0.0);
            let down_h = scroll_bar.down_button.as_ref().map(|b| resolve_btn_h(b)).unwrap_or(0.0);
            (up_h, down_h)
        } else {
            let resolve_btn_w = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> FLOAT {
                match btn.width {
                    Dimension::Px(v) => v * scale,
                    Dimension::Percent(p) => track_length * (p / 100.0),
                    Dimension::Auto => track_width,
                }
            };
            let left_w = scroll_bar.up_button.as_ref().map(|b| resolve_btn_w(b)).unwrap_or(0.0);
            let right_w = scroll_bar.down_button.as_ref().map(|b| resolve_btn_w(b)).unwrap_or(0.0);
            (left_w, right_w)
        };

        let usable_track = (track_length - button_h.0 - button_h.1).max(0.0);
        let thumb_ratio = if content_extent > 0.0 {
            (track_length / content_extent).min(1.0)
        } else {
            1.0
        };
        let thumb_length = (usable_track * thumb_ratio).max(20.0 * scale);
        let max_thumb_move = (usable_track - thumb_length).max(0.0);
        let max_scroll = (content_extent - track_length).max(0.0);
        let multiplier = if max_thumb_move > 0.0 { max_scroll / max_thumb_move } else { 0.0 };
        if is_vertical {
            self.v_scroll_multiplier.set(multiplier);
        } else {
            self.h_scroll_multiplier.set(multiplier);
        }
        
        let scroll_ratio = if max_scroll > 0.0 {
            scroll_pos / max_scroll
        } else {
            0.0
        };
        let thumb_offset = button_h.0 + scroll_ratio * max_thumb_move;

        let thumb_radius = match scroll_bar.thumb.radius {
            Dimension::Px(v) => v * scale,
            Dimension::Percent(p) => thumb_width * (p / 100.0),
            Dimension::Auto => thumb_width / 2.0,
        };

        let _ = ctx.canvas.save();

        if is_vertical {
            let _ = ctx.canvas.translate((viewport_w - track_width).round() as f64, 0.0);
        } else {
            let _ = ctx.canvas.translate(0.0, (viewport_h - track_width).round() as f64);
        }

        // Draw track
        let track_color: Color = scroll_bar.track.color.into();
        ctx.canvas.set_fill_style_str(&track_color.to_css_color());
        if is_vertical {
            ctx.canvas.fill_rect(0.0, 0.0, track_width, track_length);
        } else {
            ctx.canvas.fill_rect(0.0, 0.0, track_length, track_width);
        }

        // Draw up/left button
        if let Some(ref btn) = scroll_bar.up_button {
            let btn_color: Color = btn.color.into();
            ctx.canvas.set_fill_style_str(&btn_color.to_css_color());
            if is_vertical {
                ctx.canvas.fill_rect(0.0, 0.0, track_width, button_h.0);
            } else {
                ctx.canvas.fill_rect(0.0, 0.0, button_h.0, track_width);
            }
        }

        // Draw down/right button
        if let Some(ref btn) = scroll_bar.down_button {
            let btn_color: Color = btn.color.into();
            ctx.canvas.set_fill_style_str(&btn_color.to_css_color());
            if is_vertical {
                ctx.canvas.fill_rect(0.0, track_length - button_h.1, track_width, button_h.1);
            } else {
                ctx.canvas.fill_rect(track_length - button_h.1, 0.0, button_h.1, track_width);
            }
        }

        // Draw thumb
        let thumb_color: Color = scroll_bar.thumb.color.into();
        ctx.canvas.set_fill_style_str(&thumb_color.to_css_color());
        let thumb_x_offset = (track_width - thumb_width) / 2.0;
        
        if is_vertical {
            self.v_thumb_rect.set(Some((viewport_w - track_width + thumb_x_offset, thumb_offset, thumb_width, thumb_length)));
        } else {
            self.h_thumb_rect.set(Some((thumb_offset, viewport_h - track_width + thumb_x_offset, thumb_length, thumb_width)));
        }

        if thumb_radius > 0.0 {
            ctx.canvas.begin_path();
            if is_vertical {
                let _ = ctx.canvas.round_rect_with_f64(
                    thumb_x_offset, thumb_offset, thumb_width, thumb_length, thumb_radius,
                );
            } else {
                let _ = ctx.canvas.round_rect_with_f64(
                    thumb_offset, thumb_x_offset, thumb_length, thumb_width, thumb_radius,
                );
            }
            ctx.canvas.fill();
        } else if is_vertical {
            ctx.canvas.fill_rect(thumb_x_offset, thumb_offset, thumb_width, thumb_length);
        } else {
            ctx.canvas.fill_rect(thumb_offset, thumb_x_offset, thumb_length, thumb_width);
        }

        ctx.canvas.restore();
    }
}

impl Drawable for RawScrollableContainer {
    fn draw(&self, ctx: &BuildContext) {
        let (viewport_w, viewport_h) = self.viewport_size(ctx);
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
        self.cached_min_scroll.set(Vec2d {
            x: user_min.x * ctx.scale,
            y: user_min.y * ctx.scale,
        });

        let mut offset = self.scroll_offset.get();

        if self.drag_mode.get() == 0 {
            let clamped = self.clamp_offset(offset);
            let mut velocity = self.pointer_velocity.get();
            let mut needs_redraw = false;

            // Time-based momentum scrolling
            let now = Utc::now();
            let dt = self.last_frame_time.get()
                .map(|t| (now - t).num_microseconds().unwrap_or(0) as f64 / 1_000_000.0)
                .map(|dt| dt as FLOAT)
                .unwrap_or(1.0 / 60.0)
                .min(0.05); // cap at 50ms to avoid huge jumps after stalls
            self.last_frame_time.set(Some(now));

            // Normalize to 16.67ms reference frame (60fps)
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
                self.window.request_redraw();
            }
        }

        self.scroll_offset.set(offset);
        let offset = self.visual_offset(offset);

        // Clip to viewport
        #[cfg(not(target_arch = "wasm32"))]
        {
            ctx.canvas.save();
            ctx.canvas.clip_rect(
                Rect::from_xywh(0.0, 0.0, viewport_w.round(), viewport_h.round()),
                skia_safe::ClipOp::Intersect,
                false,
            );
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx.canvas.save();
            ctx.canvas.begin_path();
            ctx.canvas.rect(0.0, 0.0, viewport_w.round() as f64, viewport_h.round() as f64);
            ctx.canvas.clip();
        }

        // Translate by scroll offset
        // On high-DPI displays (e.g. iOS retina), avoid rounding to preserve smooth sub-pixel scrolling
        let (offset_x, offset_y) = if ctx.scale > 1.5 {
            (offset.x, offset.y)
        } else {
            (offset.x.round(), offset.y.round())
        };

        #[cfg(not(target_arch = "wasm32"))]
        ctx.canvas.translate((offset_x, offset_y));
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx.canvas.translate(offset_x as f64, offset_y as f64);
        }

        let mut child_ctx = ctx.clone();
        match self.axis {
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = FLOAT::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = FLOAT::MAX,
        }

        // Draw child content
        self.child.draw(&child_ctx);

        // Restore before drawing scrollbars (they should not be offset by scroll)
        #[cfg(not(target_arch = "wasm32"))]
        ctx.canvas.restore();
        #[cfg(target_arch = "wasm32")]
        ctx.canvas.restore();

        // Draw scrollbars on top, clipped to viewport
        #[cfg(not(target_arch = "wasm32"))]
        {
            ctx.canvas.save();
            ctx.canvas.clip_rect(
                Rect::from_xywh(0.0, 0.0, viewport_w.round(), viewport_h.round()),
                skia_safe::ClipOp::Intersect,
                false,
            );
            if let Some(ref vbar) = self.vertical_scroll_bar {
                if matches!(self.axis, ScrollAxis::Vertical) {
                    self.draw_scrollbar_native(ctx, vbar, viewport_w, viewport_h, true);
                }
            }
            if let Some(ref hbar) = self.horizontal_scroll_bar {
                if matches!(self.axis, ScrollAxis::Horizontal) {
                    self.draw_scrollbar_native(ctx, hbar, viewport_w, viewport_h, false);
                }
            }
            ctx.canvas.restore();
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx.canvas.save();
            ctx.canvas.begin_path();
            ctx.canvas.rect(0.0, 0.0, viewport_w, viewport_h);
            ctx.canvas.clip();
            if let Some(ref vbar) = self.vertical_scroll_bar {
                if matches!(self.axis, ScrollAxis::Vertical) {
                    self.draw_scrollbar_wasm(ctx, vbar, viewport_w, viewport_h, true);
                }
            }
            if let Some(ref hbar) = self.horizontal_scroll_bar {
                if matches!(self.axis, ScrollAxis::Horizontal) {
                    self.draw_scrollbar_wasm(ctx, hbar, viewport_w, viewport_h, false);
                }
            }
            ctx.canvas.restore();
        }
    }
}

impl Element for RawScrollableContainer {


    fn on_event(&self, event: &ElementEvent) -> bool {
        let pos = match event {
            ElementEvent::PointerDown(p) | ElementEvent::PointerUp(p) | ElementEvent::PointerMove(p) | ElementEvent::Scroll(p) => *p,
            ElementEvent::Cancel => Vec2d::default(),
        };

        let mode_before = self.drag_mode.get();
        let mut child_consumed = false;

        // Forward events to child manually if we haven't stolen the drag yet
        if mode_before == 0 || mode_before == 4 {
            child_consumed = widget::dispatch_event(self.child.as_ref(), pos, event);
        } else if matches!(event, ElementEvent::PointerUp(_) | ElementEvent::Cancel) {
            // Ensure child gets cancel if we are active
            let _ = widget::dispatch_event(self.child.as_ref(), pos, &ElementEvent::Cancel);
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
                let mut mode = 4; // 4 = pending content drag
                if let Some((x, y, w, h)) = self.v_thumb_rect.get() {
                    if p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h {
                        mode = 2;
                    }
                }
                if mode == 4 {
                    if let Some((x, y, w, h)) = self.h_thumb_rect.get() {
                        if p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h {
                            mode = 3;
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
                if mode == 4 {
                    if let Some(start) = self.last_pointer_pos.get() {
                        let dx = p.x - start.x;
                        let dy = p.y - start.y;
                        
                        let exceeds_threshold = match self.axis {
                            ScrollAxis::Vertical => dy.abs() > 10.0 && dy.abs() > dx.abs(),
                            ScrollAxis::Horizontal => dx.abs() > 10.0 && dx.abs() > dy.abs(),
                        };
                        
                        if exceeds_threshold {
                            mode = 1;
                            self.drag_mode.set(1);
                            // Update last_pointer_pos to current pos so the initial drag
                            // doesn't cause a large delta and artificially spike velocity
                            self.last_pointer_pos.set(Some(*p));
                            
                            // Steal gesture: Send cancel to child so it releases pressed states
                            let _ = widget::dispatch_event(self.child.as_ref(), *p, &ElementEvent::Cancel);
                        } else {
                            // Still within touch slop or moving in wrong axis, don't scroll yet
                            return child_consumed; 
                        }
                    }
                }
                
                if mode != 0 && mode != 4 {
                    if let Some(last) = self.last_pointer_pos.get() {
                        let dx = p.x - last.x;
                        let dy = p.y - last.y;
                        
                        // Track velocity based on the current scroll axis and drag mode
                        let mut new_velocity = match mode {
                            1 => match self.axis {
                                ScrollAxis::Vertical => Vec2d { x: 0.0, y: dy },
                                ScrollAxis::Horizontal => Vec2d { x: dx, y: 0.0 },
                            },
                            _ => Vec2d { x: 0.0, y: 0.0 }, // No momentum for scrollbar drags
                        };
                        
                        // Time-based velocity tracking
                        let now = Utc::now();
                        let dt = self.last_event_time.get()
                            .map(|t| (now - t).num_microseconds().unwrap_or(0) as f64 / 1_000_000.0)
                            .map(|dt| dt as FLOAT)
                            .unwrap_or(1.0 / 120.0)
                            .max(0.001); // avoid division by zero
                        self.last_event_time.set(Some(now));

                        let frame_ref = 1.0 / 120.0;
                        new_velocity.x = (new_velocity.x / dt) * frame_ref;
                        new_velocity.y = (new_velocity.y / dt) * frame_ref;
                        
                        // Smooth acceleration: blend with previous velocity (lighter smoothing for responsive touch)
                        let old_velocity = self.pointer_velocity.get();
                        #[cfg(target_os = "ios")]
                        let (blend_old, blend_new) = (0.15 as FLOAT, 0.85 as FLOAT);
                        #[cfg(not(target_os = "ios"))]
                        let (blend_old, blend_new) = (0.3 as FLOAT, 0.7 as FLOAT);
                        new_velocity.x = old_velocity.x * blend_old + new_velocity.x * blend_new;
                        new_velocity.y = old_velocity.y * blend_old + new_velocity.y * blend_new;
                        
                        self.pointer_velocity.set(new_velocity);
                        
                        let mut offset = self.scroll_offset.get();
                        let clamped = self.clamp_offset(offset);
                        
                        match mode {
                            1 => {
                                match self.axis {
                                    ScrollAxis::Vertical => {
                                        let mut actual_dy = dy;
                                        if offset.y != clamped.y {
                                            #[cfg(target_os = "ios")]
                                            { actual_dy *= 0.25; }
                                            #[cfg(not(target_os = "ios"))]
                                            { actual_dy *= 0.3; }
                                        }
                                        offset.y += actual_dy;
                                    }
                                    ScrollAxis::Horizontal => {
                                        let mut actual_dx = dx;
                                        if offset.x != clamped.x {
                                            #[cfg(target_os = "ios")]
                                            { actual_dx *= 0.25; }
                                            #[cfg(not(target_os = "ios"))]
                                            { actual_dx *= 0.3; }
                                        }
                                        offset.x += actual_dx;
                                    }
                                }
                            }
                            2 => {
                                // Scrollbar thumb drag moves thumb down, which means content must move up (-y)
                                // We multiply thumb movement by the calculated multiplier.
                                offset.y -= dy * self.v_scroll_multiplier.get();
                            }
                            3 => {
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
            ElementEvent::PointerUp(_) | ElementEvent::Cancel => {
                // On iOS, apply final velocity smoothing to filter out erratic
                // last-touch spikes that cause jarring flings
                #[cfg(target_os = "ios")]
                {
                    let mut v = self.pointer_velocity.get();
                    // Clamp maximum fling velocity (pixels per frame at 60fps)
                    let max_fling: FLOAT = 40.0;
                    v.x = v.x.clamp(-max_fling, max_fling);
                    v.y = v.y.clamp(-max_fling, max_fling);
                    // Dampen small residual velocities to avoid micro-drifts
                    if v.x.abs() < 0.5 { v.x = 0.0; }
                    if v.y.abs() < 0.5 { v.y = 0.0; }
                    self.pointer_velocity.set(v);
                }
                self.drag_mode.set(0);
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
        ResolvedSize {
            width: ctx.box_constraint.max_width,
            height: ctx.box_constraint.max_height,
        }
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
