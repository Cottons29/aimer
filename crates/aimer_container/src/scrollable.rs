pub mod constants;
pub mod controller;
pub mod draw_scroll;
pub mod handle_scroll;
pub mod raw_scroll;
pub mod scroll_bar;
pub mod scroll_behavior;

pub use controller::{DragMode, ScrollController};
use controller::VelocityHistory;
pub use scroll_behavior::{ScrollAxis, ScrollBehavior};

use crate::scrollable::raw_scroll::RawScrollableContainer;
pub use crate::scrollable::scroll_bar::*;
use aimer_attribute::CacheBounds;
use aimer_attribute::position::Vec2d;
use aimer_macro::WidgetConstructor;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, Widget};
use std::cell::Cell;

#[derive(WidgetConstructor)]
pub struct Scrollable<W: Widget + 'static> {
    pub child: W,
    #[constructor(default)]
    pub scroll_behavior: ScrollBehavior,
    #[constructor(default)]
    pub axis: ScrollAxis,
    #[constructor(default = Some(ScrollBar::default()))]
    pub vertical_scroll_bar: Option<ScrollBar>,
    #[constructor(default = Some(ScrollBar::default()))]
    pub horizontal_scroll_bar: Option<ScrollBar>,
}

impl<W: Widget> Widget for Scrollable<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
        match self.axis {
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = f32::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = f32::MAX,
        }

        Box::new(RawScrollableContainer {
            child: self.child.to_element(&child_ctx),
            ctrl: ScrollController {
                speed_multiplier: ctx.scale,
                scroll_offset: Cell::new(Vec2d {
                    x: self.scroll_behavior.scroll_offset.x * ctx.scale,
                    y: self.scroll_behavior.scroll_offset.y * ctx.scale,
                }),
                last_pointer_pos: Cell::new(None),
                drag_mode: Cell::new(DragMode::None),
                cached_max_scroll: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
                cached_min_scroll: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
                pointer_velocity: Cell::new(Vec2d {
                    x: self.scroll_behavior.velocity.x * ctx.scale,
                    y: self.scroll_behavior.velocity.y * ctx.scale,
                }),
                last_event_time: Cell::new(None),
                last_frame_time: Cell::new(None),
                v_thumb_rect: Cell::new(None),
                h_thumb_rect: Cell::new(None),
                v_scroll_multiplier: Cell::new(0.0),
                h_scroll_multiplier: Cell::new(0.0),
                last_scale: Cell::new(ctx.scale),
                scroll_behavior: ScrollBehavior {
                    max_scroll: self.scroll_behavior.max_scroll,
                    min_scroll: self.scroll_behavior.min_scroll,
                    velocity: self.scroll_behavior.velocity,
                    scroll_offset: self.scroll_behavior.scroll_offset,
                    bouncy: self.scroll_behavior.bouncy,
                    bouncy_resistance: self.scroll_behavior.bouncy_resistance,
                    bouncy_recovery: self.scroll_behavior.bouncy_recovery,
                    friction: self.scroll_behavior.friction,
                },
                axis: match self.axis {
                    ScrollAxis::Vertical => ScrollAxis::Vertical,
                    ScrollAxis::Horizontal => ScrollAxis::Horizontal,
                },
                cursor_pos: Cell::new(None),
                velocity_history: std::cell::RefCell::new(VelocityHistory::new()),
                cached_viewport: Cell::new((0.0, 0.0)),
                cached_v_track_width: Cell::new(0.0),
                cached_h_track_width: Cell::new(0.0),
            },
            vertical_scroll_bar: self.vertical_scroll_bar.clone(),
            horizontal_scroll_bar: self.horizontal_scroll_bar.clone(),
            window: ctx.window,
            bounds: CacheBounds::with_vec2d(child_ctx.parent_pos),
        })
    }
}
