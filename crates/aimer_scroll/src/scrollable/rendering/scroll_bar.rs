use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_widget::base::{BuildContext, Color, Colors};
use aimer_widget::{AnyElement, Drawable, Element, EventElement, LayoutElement, Rebuildable,
    StatefulElement, StatefulWidget, State, StateUpdater, VisitorElement, Widget};

use crate::scrollable::constants::{SCROLLBAR_HIDE_DURATION_MS, SCROLLBAR_SHOW_DURATION_MS};
use crate::scrollable::controller::{DragMode, ScrollState};
use crate::ScrollAxis;

#[derive(Clone, Copy)]
pub struct ScrollTrack {
    pub width: Dimension,
    pub color: Colors,
    pub hover_color: Colors,
}

impl Default for ScrollTrack {
    fn default() -> Self {
        Self {
            width: Dimension::Auto,
            color: Colors::Transparent,
            hover_color: Colors::Transparent,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ScrollThumb {
    pub width: Dimension,
    pub radius: Dimension,
    pub color: Colors,
    pub hover_color: Colors,
    pub active_color: Colors,
}

impl Default for ScrollThumb {
    fn default() -> Self {
        Self {
            width: Dimension::Auto,
            radius: Dimension::Px(4.0),
            color: Colors::Rgba(150, 150, 150, 150),
            hover_color: Colors::Rgba(100, 100, 100, 200),
            active_color: Colors::Rgba(80, 80, 80, 255),
        }
    }
}

#[derive(Clone, Copy)]
pub struct ScrollButton {
    pub width: Dimension,
    pub height: Dimension,

    pub color: Colors,
    pub hover_color: Colors,
    pub active_color: Colors,
}

#[derive(Clone, Default, aimer_macro::PortableWidget)]
#[portable_widget(id = "aimer_scroll::scrollable::ScrollBar", schema_only)]
pub struct ScrollBar {
    #[portable_skip]
    pub track: ScrollTrack,
    #[portable_skip]
    pub thumb: ScrollThumb,
    #[portable_skip]
    pub up_button: Option<ScrollButton>,
    #[portable_skip]
    pub down_button: Option<ScrollButton>,
    #[portable_skip]
    ctrl: Option<Rc<ScrollState>>,
    #[portable_skip]
    axis: ScrollAxis,
}

impl ScrollBar {
    #[inline]
    pub(crate) fn for_scrollable(mut self, ctrl: Rc<ScrollState>, axis: ScrollAxis) -> Self {
        self.ctrl = Some(ctrl);
        self.axis = axis;
        self
    }
}

/// Persistent state for a [`ScrollBar`].
///
/// The runtime opacity belongs to this state rather than to the scrollable's
/// paint pass, so rebuilding the parent does not reset a scrollbar midway
/// through its show or hide transition.
pub struct ScrollBarState {
    config: ScrollBar,
    runtime: Rc<ScrollBarRuntime>,
}

impl StatefulWidget for ScrollBar {
    type State = ScrollBarState;

    fn create_state(self) -> Self::State {
        ScrollBarState {
            config: self,
            runtime: Rc::new(ScrollBarRuntime::default()),
        }
    }
}

impl Widget for ScrollBar {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "ScrollBar", None)
            .0
            .boxed()
    }
}

impl State<ScrollBar> for ScrollBarState {
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

    fn adopt_config_from(&mut self, new: Self) {
        self.config = new.config;
    }

    fn build(&self, _ctx: &BuildContext) -> impl Widget {
        RawScrollBar {
            config: self.config.clone(),
            ctrl: self.config.ctrl.clone(),
            runtime: self.runtime.clone(),
        }
    }
}

#[derive(Default)]
struct ScrollBarRuntime {
    alpha: Cell<f32>,
    last_activity: Cell<Option<aimer_utils::AnimInstant>>,
    last_offset: Cell<Option<Vec2d>>,
    showing: Cell<bool>,
    transition_start: Cell<Option<aimer_utils::AnimInstant>>,
    transition_from: Cell<f32>,
}

impl ScrollBarRuntime {
    fn update(&self, offset: Vec2d, active: bool, now: aimer_utils::AnimInstant) -> f32 {
        let moved = self
            .last_offset
            .replace(Some(offset))
            .is_some_and(|previous| {
                (previous.x - offset.x).abs() > 0.01 || (previous.y - offset.y).abs() > 0.01
            });

        let activity = active || moved;
        if activity {
            self.last_activity.set(Some(now));
        }

        let mut alpha = self.advance_transition(now);
        if activity {
            if !self.showing.get() {
                self.begin_transition(now, true, alpha);
                alpha = self.advance_transition(now);
            }
        } else if self.showing.get()
            && self
                .last_activity
                .get()
                .is_some_and(|last| {
                    now.duration_since(last) > Duration::from_millis(SCROLLBAR_SHOW_DURATION_MS)
                })
        {
            self.begin_transition(now, false, alpha);
            alpha = self.advance_transition(now);
        }

        self.alpha.set(alpha);
        if self.transition_start.get().is_some() {
            aimer_events::window::request_animation_frame();
        }
        alpha
    }

    fn begin_transition(&self, now: aimer_utils::AnimInstant, showing: bool, from: f32) {
        self.showing.set(showing);
        self.transition_from.set(from);
        self.transition_start.set(Some(now));
    }

    fn advance_transition(&self, now: aimer_utils::AnimInstant) -> f32 {
        let Some(start) = self.transition_start.get() else {
            return self.alpha.get();
        };

        let duration = if self.showing.get() {
            Duration::from_millis(SCROLLBAR_SHOW_DURATION_MS)
        } else {
            Duration::from_millis(SCROLLBAR_HIDE_DURATION_MS)
        };
        let progress = (now.duration_since(start).as_secs_f32() / duration.as_secs_f32())
            .clamp(0.0, 1.0);
        let from = self.transition_from.get();
        let target = if self.showing.get() { 1.0 } else { 0.0 };
        let alpha = from + (target - from) * progress;

        if progress >= 1.0 {
            self.transition_start.set(None);
            self.alpha.set(if self.showing.get() { 1.0 } else { 0.0 });
        }
        alpha
    }

    fn hide(&self) {
        self.alpha.set(0.0);
        self.last_activity.set(None);
        self.showing.set(false);
        self.transition_start.set(None);
        self.transition_from.set(0.0);
    }
}

pub(crate) fn reserved_viewport(
    axis: ScrollAxis,
    width: f32,
    height: f32,
    bar_extent: f32,
) -> (f32, f32) {
    match axis {
        ScrollAxis::Vertical => ((width - bar_extent).max(0.0), height),
        ScrollAxis::Horizontal => (width, (height - bar_extent).max(0.0)),
    }
}

pub(crate) fn track_width(scroll_bar: &ScrollBar, cross_extent: f32, scale: f32) -> f32 {
    match scroll_bar.track.width {
        Dimension::Px(value) => value * scale,
        Dimension::Percent(percent) => cross_extent * (percent / 100.0),
        Dimension::Auto => {
            #[cfg(any(target_os = "android", target_os = "ios"))]
            {
                6.0 * scale
            }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                12.0 * scale
            }
        }
    }
}

struct RawScrollBar {
    config: ScrollBar,
    ctrl: Option<Rc<ScrollState>>,
    runtime: Rc<ScrollBarRuntime>,
}

impl Widget for RawScrollBar {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        Element::boxed(self)
    }
}

impl aimer_widget::PortableWidget for RawScrollBar {}

impl EventElement for RawScrollBar {}
impl Rebuildable for RawScrollBar {}

impl VisitorElement for RawScrollBar {
    fn debug_name(&self) -> &'static str {
        "RawScrollBar"
    }
}

impl LayoutElement for RawScrollBar {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        ResolvedSize {
            width: ctx.box_constraint.max_width,
            height: ctx.box_constraint.max_height,
        }
    }
}

impl Drawable for RawScrollBar {
    fn draw(&self, ctx: &BuildContext) {
        let Some(ctrl) = self.ctrl.as_ref() else {
            return;
        };
        draw_scrollbar(ctx, ctrl, &self.config, self.runtime.as_ref());
    }
}

fn draw_scrollbar(
    ctx: &BuildContext,
    ctrl: &ScrollState,
    scroll_bar: &ScrollBar,
    runtime: &ScrollBarRuntime,
) {
    let (viewport_w, viewport_h) = ctrl.cached_viewport.get();
    let offset = ctrl.visual_offset(ctrl.scroll_offset.get());
    let is_vertical = matches!(ctrl.axis, ScrollAxis::Vertical);
    let scale = ctx.scale;
    let track_width = track_width(scroll_bar, if is_vertical { viewport_w } else { viewport_h }, scale);
    let content_size = ctrl.cached_content_size.get();
    let (track_length, content_extent, scroll_pos) = if is_vertical {
        (viewport_h, content_size.height, -offset.y)
    } else {
        (viewport_w, content_size.width, -offset.x)
    };
    if content_extent <= track_length + 0.01 {
        runtime.hide();
        if is_vertical {
            ctrl.v_thumb_rect.set(None);
        } else {
            ctrl.h_thumb_rect.set(None);
        }
        return;
    }

    let alpha = runtime.update(
        ctrl.scroll_offset.get(),
        ctrl.is_scrolling.get(),
        aimer_utils::AnimInstant::now(),
    );
    if alpha <= 0.0 {
        return;
    }
    if is_vertical {
        ctrl.cached_v_track_width.set(track_width);
    } else {
        ctrl.cached_h_track_width.set(track_width);
    }

    let thumb_width = match scroll_bar.thumb.width {
        Dimension::Px(value) => value * scale,
        Dimension::Percent(percent) => track_width * (percent / 100.0),
        Dimension::Auto => (track_width * 0.6).max(4.0),
    };
    let button_extent = if is_vertical {
        let resolve = |button: &ScrollButton| match button.height {
            Dimension::Px(value) => value * scale,
            Dimension::Percent(percent) => track_length * (percent / 100.0),
            Dimension::Auto => track_width,
        };
        (
            scroll_bar.up_button.as_ref().map(resolve).unwrap_or(0.0),
            scroll_bar.down_button.as_ref().map(resolve).unwrap_or(0.0),
        )
    } else {
        let resolve = |button: &ScrollButton| match button.width {
            Dimension::Px(value) => value * scale,
            Dimension::Percent(percent) => track_length * (percent / 100.0),
            Dimension::Auto => track_width,
        };
        (
            scroll_bar.up_button.as_ref().map(resolve).unwrap_or(0.0),
            scroll_bar.down_button.as_ref().map(resolve).unwrap_or(0.0),
        )
    };
    let usable_track = (track_length - button_extent.0 - button_extent.1).max(0.0);
    let thumb_ratio = (track_length / content_extent).min(1.0);
    let thumb_length = (usable_track * thumb_ratio).max(20.0 * scale);
    let max_thumb_move = (usable_track - thumb_length).max(0.0);
    let max_scroll = (content_extent - track_length).max(0.0);
    let multiplier = if max_thumb_move > 0.0 {
        max_scroll / max_thumb_move
    } else {
        0.0
    };
    if is_vertical {
        ctrl.v_scroll_multiplier.set(multiplier);
    } else {
        ctrl.h_scroll_multiplier.set(multiplier);
    }
    let scroll_ratio = if max_scroll > 0.0 {
        scroll_pos.clamp(0.0, max_scroll) / max_scroll
    } else {
        0.0
    };
    let thumb_offset = button_extent.0 + scroll_ratio * max_thumb_move;
    let thumb_radius = match scroll_bar.thumb.radius {
        Dimension::Px(value) => value * scale,
        Dimension::Percent(percent) => thumb_width * (percent / 100.0),
        Dimension::Auto => thumb_width / 2.0,
    };

    ctx.canvas.save();
    ctx.canvas.set_alpha(alpha);
    let is_active = if is_vertical {
        ctrl.drag_mode.get() == DragMode::VerticalScrollbar
    } else {
        ctrl.drag_mode.get() == DragMode::HorizontalScrollbar
    };
    let is_hover = ctrl.cursor_pos.get().is_some_and(|cursor| {
        if is_vertical {
            ctrl.hit_test_v_thumb(cursor)
        } else {
            ctrl.hit_test_h_thumb(cursor)
        }
    });
    let track_color: Color = if is_hover {
        scroll_bar.track.hover_color.into()
    } else {
        scroll_bar.track.color.into()
    };
    let (track_w, track_h) = if is_vertical {
        (track_width, track_length)
    } else {
        (track_length, track_width)
    };
    ctx.canvas.fill_color_rect(
        Vec2d::ZERO,
        ResolvedSize {
            width: track_w,
            height: track_h,
        },
        track_color,
        [0.0; 4],
    );

    if let Some(button) = scroll_bar.up_button.as_ref() {
        let color: Color = button.color.into();
        let (width, height) = if is_vertical {
            (track_width, button_extent.0)
        } else {
            (button_extent.0, track_width)
        };
        ctx.canvas.fill_color_rect(
            Vec2d::ZERO,
            ResolvedSize { width, height },
            color,
            [0.0; 4],
        );
    }
    if let Some(button) = scroll_bar.down_button.as_ref() {
        let color: Color = button.color.into();
        let (pos, width, height) = if is_vertical {
            (
                Vec2d {
                    x: 0.0,
                    y: track_length - button_extent.1,
                },
                track_width,
                button_extent.1,
            )
        } else {
            (
                Vec2d {
                    x: track_length - button_extent.1,
                    y: 0.0,
                },
                button_extent.1,
                track_width,
            )
        };
        ctx.canvas.fill_color_rect(
            pos,
            ResolvedSize { width, height },
            color,
            [0.0; 4],
        );
    }

    let thumb_color: Color = if is_active {
        scroll_bar.thumb.active_color.into()
    } else if is_hover {
        scroll_bar.thumb.hover_color.into()
    } else {
        scroll_bar.thumb.color.into()
    };
    let thumb_cross_offset = (track_width - thumb_width) / 2.0;
    let (thumb_pos, thumb_size, rect) = if is_vertical {
        let rect = (viewport_w, thumb_offset, thumb_width, thumb_length);
        (
            Vec2d {
                x: thumb_cross_offset,
                y: thumb_offset,
            },
            ResolvedSize {
                width: thumb_width,
                height: thumb_length,
            },
            rect,
        )
    } else {
        let rect = (thumb_offset, viewport_h, thumb_length, thumb_width);
        (
            Vec2d {
                x: thumb_offset,
                y: thumb_cross_offset,
            },
            ResolvedSize {
                width: thumb_length,
                height: thumb_width,
            },
            rect,
        )
    };
    if is_vertical {
        ctrl.v_thumb_rect.set(Some(rect));
    } else {
        ctrl.h_thumb_rect.set(Some(rect));
    }
    ctx.canvas.fill_color_rect(thumb_pos, thumb_size, thumb_color, [thumb_radius; 4]);
    ctx.canvas.restore_alpha();
    ctx.canvas.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts_stateful_widget<W: StatefulWidget>() {}

    #[test]
    fn scrollbar_is_a_stateful_widget() {
        accepts_stateful_widget::<ScrollBar>();
        let _ = ScrollBar::default().create_state();
    }

    #[test]
    fn scrolling_scrollbar_fades_in_instead_of_jumping_to_full_opacity() {
        let runtime = ScrollBarRuntime::default();
        let start = aimer_utils::AnimInstant::now();

        assert_eq!(runtime.update(Vec2d::ZERO, true, start), 0.0);

        let halfway = runtime.update(
            Vec2d::ZERO,
            true,
            start + Duration::from_millis(SCROLLBAR_SHOW_DURATION_MS / 2),
        );
        assert!(halfway > 0.0 && halfway < 1.0);
        assert_eq!(
            runtime.update(
                Vec2d::ZERO,
                true,
                start + Duration::from_millis(SCROLLBAR_SHOW_DURATION_MS),
            ),
            1.0
        );
    }

    #[test]
    fn an_idle_scrollbar_fades_out_after_the_show_delay() {
        let runtime = ScrollBarRuntime::default();
        let start = aimer_utils::AnimInstant::now();
        let fully_visible = start + Duration::from_millis(SCROLLBAR_SHOW_DURATION_MS);

        assert_eq!(runtime.update(Vec2d::ZERO, true, start), 0.0);
        assert_eq!(runtime.update(Vec2d::ZERO, true, fully_visible), 1.0);

        let fade_start = fully_visible
            + Duration::from_millis(SCROLLBAR_SHOW_DURATION_MS + 1);
        assert_eq!(runtime.update(Vec2d::ZERO, false, fade_start), 1.0);
        assert_eq!(
            runtime.update(
                Vec2d::ZERO,
                false,
                fade_start + Duration::from_millis(SCROLLBAR_HIDE_DURATION_MS / 2),
            ),
            0.5
        );
        assert_eq!(
            runtime.update(
                Vec2d::ZERO,
                false,
                fade_start + Duration::from_millis(SCROLLBAR_HIDE_DURATION_MS),
            ),
            0.0
        );
    }

    #[test]
    fn vertical_scrollbar_reserves_width_from_the_content_viewport() {
        assert_eq!(
            reserved_viewport(ScrollAxis::Vertical, 320.0, 500.0, 12.0),
            (308.0, 500.0)
        );
    }

    #[test]
    fn horizontal_scrollbar_reserves_height_from_the_content_viewport() {
        assert_eq!(
            reserved_viewport(ScrollAxis::Horizontal, 320.0, 500.0, 12.0),
            (320.0, 488.0)
        );
    }
}
