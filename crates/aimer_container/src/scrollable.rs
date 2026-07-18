pub mod constants;
pub mod controller;
pub mod draw_scroll;
pub mod handle_scroll;
pub mod raw_scroll;
pub mod scroll_bar;
pub mod scroll_behavior;
pub mod scroll_storage;

use std::cell::Cell;
use std::rc::Rc;

use aimer_attribute::CacheBounds;
use aimer_attribute::position::Vec2d;
#[allow(unused)]
use aimer_macro::key;
use aimer_utils::callback::Callback;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, Key, RequiredChild, Widget};
pub use controller::{DragMode, ScrollController};
use controller::{ScrollState, VelocityHistory};
pub use scroll_behavior::{ScrollAxis, ScrollBehavior};

use crate::scrollable::raw_scroll::RawScrollableContainer;
pub use crate::scrollable::scroll_bar::*;

pub struct Scrollable<W = RequiredChild> {
    pub child: W,
    pub scroll_behavior: ScrollBehavior,
    pub axis: ScrollAxis,
    pub vertical_scroll_bar: Option<ScrollBar>,
    pub horizontal_scroll_bar: Option<ScrollBar>,
    /// Opt-in `PageStorage`-style identity. When set, the live scroll offset is
    /// saved under this key and restored if the `Scrollable` is fully torn down
    /// and later re-created (e.g. a swapped tab). `None` = not remembered
    /// across teardown (rebuild/resize is still preserved via
    /// reconciliation).
    pub key: Key,
    /// Optional app-held [`ScrollController`] for programmatic control. When
    /// `Some`, the app can read the live position and drive it with
    /// [`ScrollController::jump_to`] / [`ScrollController::animate_to`]; the
    /// controller shares this scrollable's state and survives rebuilds. `None`
    /// keeps the zero-cost default (internally managed) behaviour.
    pub controller: Option<ScrollController>,
}

impl Default for Scrollable {
    fn default() -> Self {
        Self::new()
    }
}

impl Scrollable {
    pub fn new() -> Self {
        Self {
            child: RequiredChild,
            scroll_behavior: ScrollBehavior::default(),
            axis: ScrollAxis::default(),
            vertical_scroll_bar: Some(ScrollBar::default()),
            horizontal_scroll_bar: Some(ScrollBar::default()),
            key: key!(),
            controller: None,
        }
    }

    pub fn with_child<W: Widget>(child: W) -> Scrollable<W> {
        Scrollable {
            child,
            scroll_behavior: ScrollBehavior::default(),
            axis: ScrollAxis::default(),
            vertical_scroll_bar: Some(ScrollBar::default()),
            horizontal_scroll_bar: Some(ScrollBar::default()),
            key: key!(),
            controller: None,
        }
    }

    pub fn scroll_behavior(mut self, scroll_behavior: ScrollBehavior) -> Self {
        self.scroll_behavior = scroll_behavior;
        self
    }

    pub fn axis(mut self, axis: ScrollAxis) -> Self {
        self.axis = axis;
        self
    }

    pub fn vertical_scroll_bar(mut self, scroll_bar: Option<ScrollBar>) -> Self {
        self.vertical_scroll_bar = scroll_bar;
        self
    }

    pub fn horizontal_scroll_bar(mut self, scroll_bar: Option<ScrollBar>) -> Self {
        self.horizontal_scroll_bar = scroll_bar;
        self
    }

    pub fn key(mut self, key: Key) -> Self {
        self.key = key;
        self
    }

    pub fn controller(mut self, controller: ScrollController) -> Self {
        self.controller = Some(controller);
        self
    }

    pub fn child<W: Widget>(self, child: W) -> Scrollable<W> {
        Scrollable {
            child,
            scroll_behavior: self.scroll_behavior,
            axis: self.axis,
            key: self.key,
            controller: self.controller,
            vertical_scroll_bar: self.vertical_scroll_bar,
            horizontal_scroll_bar: self.horizontal_scroll_bar,
        }
    }
}

impl<W: Widget> Widget for Scrollable<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let mut child_ctx = ctx.clone();
        child_ctx
            .box_constraint
            .max_width = ctx
            .box_constraint
            .max_width;
        child_ctx
            .box_constraint
            .max_height = ctx
            .box_constraint
            .max_height;
        match self.axis {
            ScrollAxis::Vertical => {
                child_ctx
                    .box_constraint
                    .max_height = f32::MAX
            }
            ScrollAxis::Horizontal => {
                child_ctx
                    .box_constraint
                    .max_width = f32::MAX
            }
        }

        // Seed the initial offset: prefer a previously stored position (survives a
        // full teardown) keyed by `storage_key`; otherwise fall back to the declared
        // `scroll_behavior.scroll_offset`. Stored offsets are logical (unscaled), so
        // re-apply `ctx.scale` here just like the declared offset below.
        let mut initial_offset = scroll_storage::read_offset(&self.key)
            .map(|logical| Vec2d { x: logical.x * ctx.scale, y: logical.y * ctx.scale })
            .unwrap_or(Vec2d {
                x: self
                    .scroll_behavior
                    .scroll_offset
                    .x
                    * ctx.scale,
                y: self
                    .scroll_behavior
                    .scroll_offset
                    .y
                    * ctx.scale,
            });

        // If an app-supplied controller is already attached (i.e. this is a
        // rebuild), it is the source of truth for the live position — seed the
        // fresh state from it so the viewport stays put. Its `offset()` is
        // logical (positive toward the content end); convert to the internal
        // scaled/negated convention.
        if let Some(ctrl) = &self.controller
            && ctrl.is_attached()
        {
            let logical = ctrl.offset();
            initial_offset = Vec2d { x: -logical.x * ctx.scale, y: -logical.y * ctx.scale };
        }

        let child = self
            .child
            .to_element(&child_ctx);
        let state = Rc::new(ScrollState {
            speed_multiplier: ctx.scale,
            scroll_offset: Cell::new(initial_offset),
            storage_key: self
                .key
                .clone(),
            last_pointer_pos: Cell::new(None),
            drag_mode: Cell::new(DragMode::None),
            cached_max_scroll: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            cached_min_scroll: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            pointer_velocity: Cell::new(Vec2d {
                x: self
                    .scroll_behavior
                    .velocity
                    .x
                    * ctx.scale,
                y: self
                    .scroll_behavior
                    .velocity
                    .y
                    * ctx.scale,
            }),
            last_event_time: Cell::new(None),
            last_frame_time: Cell::new(None),
            v_thumb_rect: Cell::new(None),
            h_thumb_rect: Cell::new(None),
            v_scroll_multiplier: Cell::new(0.0),
            h_scroll_multiplier: Cell::new(0.0),
            last_scale: Cell::new(ctx.scale),
            scroll_behavior: self.scroll_behavior,
            axis: self.axis,
            cursor_pos: Cell::new(None),
            velocity_history: std::cell::RefCell::new(VelocityHistory::new()),
            cached_viewport: Cell::new((0.0, 0.0)),
            cached_v_track_width: Cell::new(0.0),
            cached_h_track_width: Cell::new(0.0),
            cached_content_size: Cell::new(Default::default()),
            fling_start_time: Cell::new(None),
            fling_start_offset: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            fling_target_offset: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            fling_duration: Cell::new(0.0),
            anim_curve: Cell::new(None),
            active_touch_id: Cell::new(None),
            spring_velocity: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            momentum_start_time: Cell::new(None),
            vel_accum: Cell::new(Vec2d { x: 0.0, y: 0.0 }),
            vel_sample_time: Cell::new(None),
            is_scrolling: Cell::new(false),
            // Left empty here; `attach` (below) re-shares any app-registered
            // scroll-lifecycle callbacks from the controller into this state.
            on_scroll_start: std::cell::RefCell::new(Callback::default()),
            on_scroll_end: std::cell::RefCell::new(Callback::default()),
            on_scroll: std::cell::RefCell::new(Callback::default()),
            last_reported_offset: Cell::new(None),
        });

        // Share the freshly built state with the app's controller (if any) so
        // `jump_to` / `animate_to` / `offset` operate on this live scrollable.
        if let Some(ctrl) = &self.controller {
            ctrl.attach(state.clone());
        }

        Box::new(RawScrollableContainer {
            child,
            ctrl: state,
            vertical_scroll_bar: self.vertical_scroll_bar,
            horizontal_scroll_bar: self.horizontal_scroll_bar,
            bounds: CacheBounds::with_vec2d(child_ctx.parent_pos),
        })
    }
}
