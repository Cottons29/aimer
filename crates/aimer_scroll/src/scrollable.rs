pub mod constants;
pub mod controller;
pub mod draw_scroll;
pub mod handle_scroll;
pub mod raw_scroll;
pub mod scroll_bar;
pub mod scroll_behavior;
pub mod scroll_storage;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use aimer_attribute::CacheBounds;
use aimer_attribute::position::Vec2d;
#[allow(unused)]
use aimer_macro::key;
use aimer_utils::callback::Callback;
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, AnyWidget, Element, Key, RequiredChild, State, StateUpdater, StatefulElement,
    StatefulWidget, Widget,
};
pub use controller::{DragMode, ScrollController};
use controller::{ScrollState, VelocityHistory};
pub use scroll_behavior::{ScrollAxis, ScrollBehavior};

use crate::scrollable::raw_scroll::RawScrollableContainer;
pub use crate::scrollable::scroll_bar::*;

/// A single-child viewport that scrolls overflowing content along one axis.
///
/// The child receives an unbounded constraint on the selected [`ScrollAxis`]
/// and the viewport clips and translates that content according to
/// [`ScrollBehavior`]. The default axis is vertical, both scroll bars are
/// enabled, a fresh storage [`Key`] is generated, and no external
/// [`ScrollController`] is attached.
///
/// Attach a child with [`Scrollable::child`] to retain its concrete type, or
/// with [`Scrollable::box_child`] when branches need a shared erased type.
///
/// # Example
///
/// ```rust
/// use aimer_container::SizedBox;
/// use aimer_flex::Column;
/// use aimer_scroll::{ScrollAxis, Scrollable};
///
/// let viewport = Scrollable::new().axis(ScrollAxis::Vertical)
///                                 .vertical_scroll_bar(None)
///                                 .child(Column::new().children([SizedBox::new().height(200),
///                                                                SizedBox::new().height(200)]));
/// ```
pub struct Scrollable<W = RequiredChild> {
    pub child: Rc<W>,
    pub scroll_behavior: ScrollBehavior,
    pub axis: ScrollAxis,
    pub vertical_scroll_bar: Option<ScrollBar>,
    pub horizontal_scroll_bar: Option<ScrollBar>,
    /// Opt-in `PageStorage`-style identity. When set, the live scroll offset is
    /// saved under this key and restored if the `Scrollable` is fully torn down
    /// and later recreated (e.g. a swapped tab). `None` = not remembered
    /// across teardown (rebuild/resize is still preserved via
    /// reconciliation).
    pub key: Key,
    /// Optional app-held [`ScrollController`] for programmatic control. When
    /// `Some`, the app can read the live position and drive it with
    /// [`ScrollController::jump_to`] / [`ScrollController::animate_to`]; the
    /// controller shares this scrollable's state and survives rebuilds. `None`
    /// keeps the zero-cost default (internally managed) behavior.
    pub controller: Option<ScrollController>,
}

impl Default for Scrollable {
    fn default() -> Self {
        Self::new()
    }
}

impl Scrollable {
    /// Creates a vertical scrollable with default scroll bars and no
    /// controller.
    ///
    /// Finish the builder with [`Scrollable::child`] or
    /// [`Scrollable::box_child`].
    #[inline]
    pub fn new() -> Self {
        Self {
            child: Rc::new(RequiredChild),
            scroll_behavior: ScrollBehavior::default(),
            axis: ScrollAxis::default(),
            vertical_scroll_bar: Some(ScrollBar::default()),
            horizontal_scroll_bar: Some(ScrollBar::default()),
            key: key!(),
            controller: None,
        }
    }

    /// Creates a scrollable with its required child already attached.
    ///
    /// This is equivalent to [`Scrollable::new`] followed by
    /// [`Scrollable::child`]: it uses the default vertical axis and behavior,
    /// enables both default scroll bars, generates a storage key, and has no
    /// external controller.
    #[inline]
    pub fn with_child<W: Widget>(child: W) -> Scrollable<W> {
        Scrollable {
            child: Rc::new(child),
            scroll_behavior: ScrollBehavior::default(),
            axis: ScrollAxis::default(),
            vertical_scroll_bar: Some(ScrollBar::default()),
            horizontal_scroll_bar: Some(ScrollBar::default()),
            key: Key::unique(),
            controller: None,
        }
    }

    /// Replaces the scrolling physics and initial offset configuration.
    ///
    /// The default is [`ScrollBehavior::default`]. Offsets are interpreted in
    /// logical pixels and scaled for the current build context.
    #[inline]
    pub fn scroll_behavior(mut self, scroll_behavior: ScrollBehavior) -> Self {
        self.scroll_behavior = scroll_behavior;
        self
    }

    /// Sets the single axis along which content may overflow and scroll.
    ///
    /// The default is [`ScrollAxis::Vertical`]. The child is made unbounded
    /// only on this axis and remains constrained on the cross axis.
    #[inline]
    pub fn axis(mut self, axis: ScrollAxis) -> Self {
        self.axis = axis;
        self
    }

    /// Replaces the vertical scroll-bar configuration.
    ///
    /// A default bar is enabled initially. Pass `None` to hide it. This setting
    /// controls presentation and does not change [`Scrollable::axis`].
    #[inline]
    pub fn vertical_scroll_bar(mut self, scroll_bar: Option<ScrollBar>) -> Self {
        self.vertical_scroll_bar = scroll_bar;
        self
    }

    /// Replaces the horizontal scroll-bar configuration.
    ///
    /// A default bar is enabled initially. Pass `None` to hide it. This setting
    /// controls presentation and does not change [`Scrollable::axis`].
    #[inline]
    pub fn horizontal_scroll_bar(mut self, scroll_bar: Option<ScrollBar>) -> Self {
        self.horizontal_scroll_bar = scroll_bar;
        self
    }

    /// Replaces the identity used to persist and restore the logical offset.
    ///
    /// [`Scrollable::new`] generates a fresh key. Reusing a stable key allows a
    /// torn-down viewport to recover its stored position.
    #[inline]
    pub fn key(mut self, key: Key) -> Self {
        self.key = key;
        self
    }

    /// Attaches an application-owned controller for reading or driving offset.
    ///
    /// By default no controller is attached. The supplied controller replaces
    /// any previous one and its state survives widget rebuilds.
    #[inline]
    pub fn controller(mut self, controller: ScrollController) -> Self {
        self.controller = Some(controller);
        self
    }

    /// Attaches the required child and completes this builder.
    ///
    /// This terminal operation preserves all scrolling configuration and the
    /// child's concrete type. Use [`Scrollable::box_child`] when different
    /// branches need one erased return type.
    #[inline]
    pub fn child<W: Widget>(self, child: W) -> Scrollable<W> {
        Scrollable {
            child: Rc::new(child),
            scroll_behavior: self.scroll_behavior,
            axis: self.axis,
            key: self.key,
            controller: self.controller,
            vertical_scroll_bar: self.vertical_scroll_bar,
            horizontal_scroll_bar: self.horizontal_scroll_bar,
        }
    }

    /// Attaches `child` and erases the resulting widget's concrete type.
    ///
    /// This is equivalent to calling [`Scrollable::child`] followed by
    /// [`Widget::boxed`]. Use it when different branches must return one
    /// [`AnyWidget`] type.
    #[inline]
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child).boxed()
    }
}

/// Persistent state for a [`Scrollable`].
///
/// This type separates immutable widget configuration from the live scroll
/// engine. A parent rebuild refreshes the child, behavior, axis, bars, key, and
/// controller through [`State::adopt_config_from`] while preserving the current
/// offset and any in-flight drag or animation state.
pub struct ScrollableState<W: Widget + 'static> {
    child: Rc<W>,
    scroll_behavior: ScrollBehavior,
    axis: ScrollAxis,
    vertical_scroll_bar: Option<ScrollBar>,
    horizontal_scroll_bar: Option<ScrollBar>,
    key: Key,
    controller: Option<ScrollController>,
    scroll_state: RefCell<Option<Rc<ScrollState>>>,
    refresh_scroll_state: Cell<bool>,
}

impl<W: Widget + 'static> StatefulWidget for Scrollable<W> {
    type State = ScrollableState<W>;

    fn create_state(&self) -> Self::State {
        ScrollableState {
            child: self.child.clone(),
            scroll_behavior: self.scroll_behavior,
            axis: self.axis,
            vertical_scroll_bar: self.vertical_scroll_bar.clone(),
            horizontal_scroll_bar: self.horizontal_scroll_bar.clone(),
            key: self.key.clone(),
            controller: self.controller.clone(),
            scroll_state: RefCell::new(None),
            refresh_scroll_state: Cell::new(false),
        }
    }
}

impl<W: Widget + 'static> Widget for Scrollable<W> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "Scrollable", None)
            .0
            .boxed()
    }
}

impl<W: Widget + 'static> State<Scrollable<W>> for ScrollableState<W> {
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

    fn adopt_config_from(&mut self, new: &Self) {
        let controller_changed = match (&self.controller, &new.controller) {
            (_, None) => false,
            (Some(current), Some(new)) => !current.shares_identity_with(new),
            (None, Some(_)) => true,
        };
        let engine_config_changed =
            !same_scroll_behavior(self.scroll_behavior, new.scroll_behavior)
                || !same_scroll_axis(self.axis, new.axis)
                || self.key != new.key
                || controller_changed;

        self.child = new.child.clone();
        self.scroll_behavior = new.scroll_behavior;
        self.axis = new.axis;
        self.vertical_scroll_bar = new.vertical_scroll_bar.clone();
        self.horizontal_scroll_bar = new.horizontal_scroll_bar.clone();
        self.key = new.key.clone();

        if let Some(new_ctrl) = new.controller.clone() {
            self.controller = Some(new_ctrl);
        }

        if engine_config_changed {
            self.refresh_scroll_state.set(true);
        }
    }

    #[inline]
    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let ctrl = self.live_scroll_state(ctx);

        ScrollableFrame {
            child: self.child.clone(),
            ctrl,
            vertical_scroll_bar: self.vertical_scroll_bar.clone(),
            horizontal_scroll_bar: self.horizontal_scroll_bar.clone(),
        }
    }
}

impl<W: Widget + 'static> ScrollableState<W> {
    #[inline]
    fn live_scroll_state(&self, ctx: &BuildContext) -> Rc<ScrollState> {
        let mut live = self.scroll_state.borrow_mut();
        let scale_changed = live
            .as_ref()
            .is_some_and(|state| state.last_scale.get() != ctx.scale);
        let refresh_scroll_state = self.refresh_scroll_state.replace(false);
        if live.is_none() || scale_changed || refresh_scroll_state {
            let next = self.create_scroll_state(ctx);
            if let Some(previous) = live.as_ref() {
                next.adopt_scroll_state(previous);
            }
            if let Some(controller) = &self.controller {
                controller.attach(next.clone());
            }
            *live = Some(next);
        }

        live.as_ref()
            .expect("scroll state is initialized before the frame is built")
            .clone()
    }
    #[inline]
    fn create_scroll_state(&self, ctx: &BuildContext) -> Rc<ScrollState> {
        // Seed the initial offset: prefer a previously stored position (survives a
        // full teardown) keyed by `storage_key`; otherwise fall back to the declared
        // `scroll_behavior.scroll_offset`. Stored offsets are logical (unscaled), so
        // re-apply `ctx.scale` here just like the declared offset below.
        let mut initial_offset = scroll_storage::read_offset(&self.key)
            .map(|logical| Vec2d {
                x: logical.x * ctx.scale,
                y: logical.y * ctx.scale,
            })
            .unwrap_or(Vec2d {
                x: self.scroll_behavior.scroll_offset.x * ctx.scale,
                y: self.scroll_behavior.scroll_offset.y * ctx.scale,
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
            initial_offset = Vec2d {
                x: -logical.x * ctx.scale,
                y: -logical.y * ctx.scale,
            };
        }

        let state = Rc::new(ScrollState {
            speed_multiplier: ctx.scale,
            scroll_offset: Cell::new(initial_offset),
            storage_key: self.key.clone(),
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
            scroll_behavior: self.scroll_behavior,
            axis: self.axis,
            cursor_pos: Cell::new(None),
            velocity_history: RefCell::new(VelocityHistory::new()),
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
            // Left empty here; `live_scroll_state` re-shares any
            // app-registered
            // scroll-lifecycle callbacks when it attaches the controller.
            on_scroll_start: RefCell::new(Callback::default()),
            on_scroll_end: RefCell::new(Callback::default()),
            on_scroll: RefCell::new(Callback::default()),
            last_reported_offset: Cell::new(None),
        });

        state
    }
}

#[inline]
fn same_scroll_axis(current: ScrollAxis, new: ScrollAxis) -> bool {
    matches!(
        (current, new),
        (ScrollAxis::Vertical, ScrollAxis::Vertical)
            | (ScrollAxis::Horizontal, ScrollAxis::Horizontal)
    )
}

#[inline]
fn same_scroll_behavior(current: ScrollBehavior, new: ScrollBehavior) -> bool {
    current.max_scroll.x == new.max_scroll.x
        && current.max_scroll.y == new.max_scroll.y
        && current.min_scroll.x == new.min_scroll.x
        && current.min_scroll.y == new.min_scroll.y
        && current.velocity.x == new.velocity.x
        && current.velocity.y == new.velocity.y
        && current.scroll_offset.x == new.scroll_offset.x
        && current.scroll_offset.y == new.scroll_offset.y
        && current.bouncy == new.bouncy
        && current.bouncy_resistance == new.bouncy_resistance
        && current.bouncy_recovery == new.bouncy_recovery
        && current.friction == new.friction
}

struct ScrollableFrame<W: Widget + 'static> {
    child: Rc<W>,
    ctrl: Rc<ScrollState>,
    vertical_scroll_bar: Option<ScrollBar>,
    horizontal_scroll_bar: Option<ScrollBar>,
}

impl<W: Widget + 'static> Widget for ScrollableFrame<W> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let mut child_ctx = ctx.clone();
        match self.ctrl.axis {
            ScrollAxis::Vertical => child_ctx.box_constraint.max_height = f32::MAX,
            ScrollAxis::Horizontal => child_ctx.box_constraint.max_width = f32::MAX,
        }
        let child = self.child.to_element(&child_ctx);

        RawScrollableContainer {
            child,
            ctrl: self.ctrl.clone(),
            vertical_scroll_bar: self.vertical_scroll_bar.clone(),
            horizontal_scroll_bar: self.horizontal_scroll_bar.clone(),
            bounds: CacheBounds::with_vec2d(child_ctx.parent_pos),
        }
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use aimer_widget::{ErrorWidget, State, StatefulWidget};

    use super::{ScrollAxis, Scrollable};

    fn assert_stateful_widget<W: StatefulWidget>() {}

    #[test]
    fn scrollable_uses_the_standard_stateful_widget_lifecycle() {
        assert_stateful_widget::<Scrollable<ErrorWidget>>();
    }

    #[test]
    fn state_adopts_configuration_from_parent_rebuild() {
        let current = Scrollable::new().child(ErrorWidget::new("current"));
        let next = Scrollable::new()
            .axis(ScrollAxis::Horizontal)
            .vertical_scroll_bar(None)
            .child(ErrorWidget::new("next"));
        let mut state = current.create_state();
        let next_state = next.create_state();

        state.adopt_config_from(&next_state);

        assert!(matches!(state.axis, ScrollAxis::Horizontal));
        assert!(state.vertical_scroll_bar.is_none());
        assert!(state.refresh_scroll_state.get());
    }

    #[test]
    fn equivalent_parent_rebuild_keeps_the_live_scroll_engine() {
        let current = Scrollable::new().child(ErrorWidget::new("current"));
        let next = Scrollable::new().child(ErrorWidget::new("next"));
        let mut state = current.create_state();
        let next_state = next.create_state();

        state.adopt_config_from(&next_state);

        assert!(!state.refresh_scroll_state.get());
    }
}
