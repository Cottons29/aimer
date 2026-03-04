pub mod raw_scroll;
pub mod scroll_bar;

use std::cell::Cell;
use attribute::position::Vec2d;
use constructor::Constructor;
use widget::base::BuildContext;
use widget::{Element, Widget};

use crate::scrollable::raw_scroll::RawScrollableContainer;
pub use crate::scrollable::scroll_bar::*;


#[cfg(target_arch = "wasm32")]
type Float = f64;
#[cfg(not(target_arch = "wasm32"))]
type Float = f32;

#[derive(Constructor)]
pub struct Scrollable<W: Widget> {
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
        match self.axis {
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = Float::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = Float::MAX,
        }
        let child = self.child.to_element(&child_ctx);
        Box::new(RawScrollableContainer {
            child,
            scroll_offset: Cell::new(Vec2d {
                x: self.scroll_behavior.scroll_offset.x * ctx.scale,
                y: self.scroll_behavior.scroll_offset.y * ctx.scale,
            }),
            last_pointer_pos: Cell::new(None),
            drag_mode: Cell::new(0),
            cached_max_scroll: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            cached_min_scroll: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            pointer_velocity: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            last_event_time: Cell::new(None),
            last_frame_time: Cell::new(None),
            v_thumb_rect: Cell::new(None),
            h_thumb_rect: Cell::new(None),
            v_scroll_multiplier: Cell::new(0.0),
            h_scroll_multiplier: Cell::new(0.0),
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
            vertical_scroll_bar: self.vertical_scroll_bar.clone(),
            horizontal_scroll_bar: self.horizontal_scroll_bar.clone(),
            window: ctx.window,
        })
    }
}


#[derive(Constructor)]
pub struct ScrollBehavior {
    pub max_scroll: Vec2d,
    pub min_scroll: Vec2d,
    pub velocity: Vec2d,
    pub scroll_offset: Vec2d,
    #[constructor(default = true)]
    pub bouncy: bool,
    #[constructor(default = 0.6)]
    pub bouncy_resistance: Float,
    #[constructor(default = 0.15)]
    pub bouncy_recovery: Float,
    #[constructor(default = 0.95)]
    pub friction: Float,
}

impl Default for ScrollBehavior {
    fn default() -> Self {
        #[cfg(target_os = "ios")]
        let defaults = (0.55, 0.13, 0.99);
        #[cfg(not(target_os = "ios"))]
        let defaults = (0.6, 0.15, 0.95);

        Self {
            max_scroll: Vec2d { x: Float::MAX, y: Float::MAX },
            min_scroll: Vec2d { x: 0.0, y: 0.0 },
            velocity: Vec2d { x: 0.0, y: 0.0 },
            scroll_offset: Vec2d { x: 0.0, y: 0.0 },
            bouncy: true,
            bouncy_resistance: defaults.0,
            bouncy_recovery: defaults.1,
            friction: defaults.2,
        }
    }
}

#[derive(Default)]
pub enum ScrollAxis {
    #[default]
    Vertical,
    Horizontal,
}




