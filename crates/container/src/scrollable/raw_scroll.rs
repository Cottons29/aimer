use crate::scrollable::controller::ScrollController;
use crate::scrollable::scroll_bar::ScrollBar;
use crate::scrollable::ScrollAxis;
use attribute::dimension::Dimension;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use attribute::{Bounds, CacheBounds};
use canvas::CanvasRendering;
use events::element::ElementEvent;
use utils::debug;
use widget::base::*;
use widget::{Drawable, Element};
use winit::window::Window;

pub use crate::scrollable::controller::DragMode;

pub struct RawScrollableContainer<E: Element> {
    pub(crate) child: E,
    pub(crate) ctrl: ScrollController,
    pub(crate) vertical_scroll_bar: Option<ScrollBar>,
    pub(crate) horizontal_scroll_bar: Option<ScrollBar>,
    pub(crate) window: &'static Window,
    pub(crate) bounds: CacheBounds,
}

impl<E: Element> RawScrollableContainer<E> {
    /// Compute the viewport size from the build context constraints.
    pub(crate) fn viewport_size(&self, ctx: &BuildContext) -> (f32, f32) {
        (ctx.box_constraint.max_width, ctx.box_constraint.max_height)
    }

    pub(crate) fn draw_scrollbar(
        &self,
        ctx: &BuildContext,
        scroll_bar: &ScrollBar,
        viewport_w: f32,
        viewport_h: f32,
        is_vertical: bool,
    ) {
        let scale = ctx.scale;
        let offset = self.ctrl.visual_offset(self.ctrl.scroll_offset.get());

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
            let resolve_btn_h = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> f32 {
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
            let resolve_btn_w = |btn: &crate::scrollable::scroll_bar::ScrollButton| -> f32 {
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
            self.ctrl.v_scroll_multiplier.set(multiplier);
        } else {
            self.ctrl.h_scroll_multiplier.set(multiplier);
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
            [0.0; 4],
        );

        // Draw up/left button
        if let Some(ref btn) = scroll_bar.up_button {
            let btn_color: Color = btn.color.into();
            let (bw, bh) = if is_vertical { (track_width, button_h.0) } else { (button_h.0, track_width) };
            ctx.canvas.fill_color_rect(
                Vec2d { x: 0.0, y: 0.0 },
                ResolvedSize { width: bw, height: bh },
                btn_color,
                [0.0; 4],
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
                .fill_color_rect(Vec2d { x: bx, y: by }, ResolvedSize { width: bw, height: bh }, btn_color, [0.0; 4]);
        }

        // Draw thumb
        let thumb_color: Color = scroll_bar.thumb.color.into();
        let thumb_x_offset = (track_width - thumb_width) / 2.0;
        let (tx, ty, tw, th) = if is_vertical {
            self.ctrl.v_thumb_rect.set(Some((
                viewport_w - track_width + thumb_x_offset,
                thumb_offset,
                thumb_width,
                thumb_length,
            )));
            (thumb_x_offset, thumb_offset, thumb_width, thumb_length)
        } else {
            self.ctrl.h_thumb_rect.set(Some((
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
            [thumb_radius as f32; 4],
        );

        ctx.canvas.restore();
    }
}
