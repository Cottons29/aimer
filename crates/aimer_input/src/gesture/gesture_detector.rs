use std::cell::RefCell;
use std::collections::HashMap;

use aimer_animation::AnimInstant;
use aimer_attribute::CacheBounds;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_events::pointer::{PointerEvent, PointerPosition};
use aimer_widget::base::{BuildContext, WindowHandle};
use aimer_widget::{
    AnyWidget, Drawable, Element, EventElement, LayoutElement, Rebuildable, RequiredChild,
    VisitorElement, Widget,
};

use crate::callback::{CallbackExecutor, RawInnerCallback, VoidCallback, VoidParamedFunction};
use crate::gesture::{
    DOUBLE_TAP_TIMEOUT, DragCallback, DragUpdateCallback, DragUpdateData, GestureEvent,
    LONG_PRESS_DURATION, STALE_GESTURE_TOUCH_MS, SWIPE_MAX_DURATION_MS, SWIPE_VELOCITY_THRESHOLD,
    ScaleCallback, ScaleData, ScrollCallback, ScrollData, SwipeCallback, SwipeDirection, TAP_SLOP,
};

/// A transparent widget that recognizes pointer gestures over its child.
///
/// All callbacks default to no-ops. The detector paints nothing and adopts its child's layout;
/// finish construction with [`GestureDetector::child`] or [`GestureDetector::box_child`]. Scroll
/// events are consumed only when [`GestureDetector::on_scroll`] is configured.
///
/// # Example
///
/// ```
/// use aimer_input::gesture::gesture_detector::GestureDetector;
/// use aimer_text::Text;
///
/// let detector = GestureDetector::new()
///     .on_tap(|| println!("tap"))
///     .child(Text::new("Tap me"));
/// ```
pub struct GestureDetector<W = RequiredChild> {
    pub on_tap: VoidCallback,
    pub on_double_press: VoidCallback,
    pub on_long_press: VoidCallback,
    pub on_drag_start: DragCallback,
    pub on_drag_update: DragUpdateCallback,
    pub on_drag_end: VoidCallback,
    pub on_right_tap: VoidCallback,
    pub on_swipe: SwipeCallback,
    pub on_scroll: ScrollCallback,
    pub on_scale: ScaleCallback,
    pub child: W,
}

impl Default for GestureDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureDetector {
    /// Creates a detector with no-op callbacks and a required-child placeholder.
    pub fn new() -> Self {
        Self {
            on_tap: VoidCallback::default(),
            on_double_press: VoidCallback::default(),
            on_long_press: VoidCallback::default(),
            on_drag_start: DragCallback::default(),
            on_drag_update: DragUpdateCallback::default(),
            on_drag_end: VoidCallback::default(),
            on_right_tap: VoidCallback::default(),
            on_swipe: SwipeCallback::default(),
            on_scroll: ScrollCallback::default(),
            on_scale: ScaleCallback::default(),
            child: RequiredChild,
        }
    }
}

impl<W> GestureDetector<W> {
    /// Sets the callback for a primary tap completed within the tap slop.
    pub fn on_tap(mut self, on_tap: impl Into<VoidCallback>) -> Self {
        self.on_tap = on_tap.into();
        self
    }

    /// Sets the callback fired after two qualifying taps within the double-tap timeout.
    pub fn on_double_press(mut self, on_double_press: impl Into<VoidCallback>) -> Self {
        self.on_double_press = on_double_press.into();
        self
    }

    /// Sets the callback fired once a held pointer reaches the long-press duration.
    pub fn on_long_press(mut self, on_long_press: impl Into<VoidCallback>) -> Self {
        self.on_long_press = on_long_press.into();
        self
    }

    /// Sets the callback fired when pointer movement first exceeds the tap slop.
    ///
    /// The callback receives the pointer position where the drag started.
    pub fn on_drag_start(mut self, on_drag_start: impl Into<DragCallback>) -> Self {
        self.on_drag_start = on_drag_start.into();
        self
    }

    /// Sets the callback fired for movement while a drag is active.
    ///
    /// [`DragUpdateData`] reports the current position and movement delta.
    pub fn on_drag_update(mut self, on_drag_update: impl Into<DragUpdateCallback>) -> Self {
        self.on_drag_update = on_drag_update.into();
        self
    }

    /// Sets the callback fired when an active drag ends.
    pub fn on_drag_end(mut self, on_drag_end: impl Into<VoidCallback>) -> Self {
        self.on_drag_end = on_drag_end.into();
        self
    }

    /// Sets the callback for a completed secondary-button tap.
    pub fn on_right_tap(mut self, on_right_tap: impl Into<VoidCallback>) -> Self {
        self.on_right_tap = on_right_tap.into();
        self
    }

    /// Sets the callback for a fast directional drag recognized as a swipe.
    ///
    /// The callback receives the resulting [`SwipeDirection`].
    pub fn on_swipe(mut self, on_swipe: impl Into<SwipeCallback>) -> Self {
        self.on_swipe = on_swipe.into();
        self
    }

    /// Sets the callback for mouse-wheel or trackpad scrolling over the child.
    ///
    /// Installing this callback causes the detector to consume matching scroll events; without it,
    /// those events fall through to lower layers. [`ScrollData`] contains the scroll delta.
    pub fn on_scroll(mut self, on_scroll: impl Into<ScrollCallback>) -> Self {
        self.on_scroll = on_scroll.into();
        self
    }

    /// Sets the callback for a two-pointer pinch gesture.
    ///
    /// [`ScaleData`] reports the scale relative to the initial pointer distance.
    pub fn on_scale(mut self, on_scale: impl Into<ScaleCallback>) -> Self {
        self.on_scale = on_scale.into();
        self
    }

    /// Supplies the terminal child and returns a statically typed detector.
    ///
    /// Existing callback settings are preserved. A detector without a child is only an intermediate
    /// builder and does not implement [`Widget`].
    pub fn child<C: Widget>(self, child: C) -> GestureDetector<C> {
        GestureDetector {
            on_tap: self.on_tap,
            on_double_press: self.on_double_press,
            on_long_press: self.on_long_press,
            on_drag_start: self.on_drag_start,
            on_drag_update: self.on_drag_update,
            on_drag_end: self.on_drag_end,
            on_right_tap: self.on_right_tap,
            on_swipe: self.on_swipe,
            on_scroll: self.on_scroll,
            on_scale: self.on_scale,
            child,
        }
    }

    /// Supplies the terminal child and erases the completed detector's concrete type.
    ///
    /// This is exactly equivalent to `self.child(child).boxed()`, combining
    /// [`GestureDetector::child`] with [`Widget::boxed`]. Use it when branching APIs need one
    /// [`AnyWidget`] return type despite using different concrete child types.
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl<W: Widget + 'static> Widget for GestureDetector<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        RawGestureDetector {
            child: self.child.to_element(ctx),
            cached_bounds: CacheBounds::new(),
            window: ctx.window.clone(),
            on_tap: self.on_tap.clone(),
            on_double_press: self.on_double_press.clone(),
            on_long_press: self.on_long_press.clone(),
            on_drag_start: self.on_drag_start.clone(),
            on_drag_update: self.on_drag_update.clone(),
            on_drag_end: self.on_drag_end.clone(),
            on_right_tap: self.on_right_tap.clone(),
            on_swipe: self.on_swipe.clone(),
            on_scroll: self.on_scroll.clone(),
            on_scale: self.on_scale.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            runtime_handle: Some(ctx.async_handle.clone()),
            state: RefCell::new(Default::default()),
        }
        .boxed()
    }
}

/// A pure gesture recognizer that wraps a child element.
///
/// `GestureDetector` detects tap, double-tap, long-press, drag, swipe,
/// scroll, and scale (pinch) gestures and fires the corresponding callbacks.
/// It does **not** render any visual feedback — decoration, pressed overlays,
/// and hover effects belong to higher-level widgets like [`crate::button::Button`].
///
/// This mirrors Flutter's `GestureDetector`: a transparent wrapper that
/// recognises gestures and delegates rendering entirely to its child.
#[allow(dead_code)]
pub struct RawGestureDetector<E: Element> {
    // Child
    pub child: E,
    // Hit-testing
    pub(crate) cached_bounds: CacheBounds,
    pub(crate) window: WindowHandle,
    // Press state — shared with parent (e.g. ButtonVisual) for overlay rendering
    // Callbacks
    pub on_tap: VoidCallback,
    pub on_double_press: VoidCallback,
    pub on_long_press: VoidCallback,
    pub on_drag_start: DragCallback,
    pub on_drag_update: DragUpdateCallback,
    pub on_drag_end: VoidCallback,
    pub on_right_tap: VoidCallback,
    pub on_swipe: SwipeCallback,
    pub on_scroll: ScrollCallback,
    pub on_scale: ScaleCallback,
    #[cfg(not(target_arch = "wasm32"))]
    pub runtime_handle: Option<tokio::runtime::Handle>,
    // Gesture state (interior mutability for &self access in on_event)
    pub(crate) state: RefCell<GestureState>,
}

#[derive(Clone, Default, Debug)]
pub(crate) struct GestureState {
    down_position: Option<PointerPosition>,
    down_time: Option<AnimInstant>,
    last_tap_time: Option<AnimInstant>,
    last_tap_position: Option<PointerPosition>,
    is_dragging: bool,
    last_drag_position: Option<PointerPosition>,
    touches: HashMap<u64, PointerPosition>,
    initial_pinch_distance: Option<f32>,
    current_scale: f32,
    drag_start_time: Option<AnimInstant>,
    drag_start_position: Option<PointerPosition>,
}

impl<E: Element> RawGestureDetector<E> {
    // ── Callback execution helpers ──────────────────────────────────────

    fn execute_callback(
        cb: &VoidCallback,
        #[cfg(not(target_arch = "wasm32"))] runtime_handle: &Option<tokio::runtime::Handle>,
    ) {
        if let Some(callback) = (*cb.get()).as_ref() {
            match callback {
                RawInnerCallback::Empty => {}
                RawInnerCallback::Sync(f) => f(()),
                RawInnerCallback::Async(f) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(handle) = runtime_handle {
                        handle.spawn(f(()));
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        wasm_bindgen_futures::spawn_local(f(()));
                    }
                }
            }
        }
    }

    fn execute_paramed_callback<T: 'static>(
        cb: &VoidParamedFunction<T>,
        arg: T,
        #[cfg(not(target_arch = "wasm32"))] runtime_handle: &Option<tokio::runtime::Handle>,
    ) {
        if let Some(callback) = (*cb.get()).as_ref() {
            match callback {
                RawInnerCallback::Empty => {}
                RawInnerCallback::Sync(f) => f(arg),
                RawInnerCallback::Async(f) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(handle) = runtime_handle {
                        handle.spawn(f(arg));
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        wasm_bindgen_futures::spawn_local(f(arg));
                    }
                }
            }
        }
    }

    // ── Gesture state machine ───────────────────────────────────────────

    fn process_pointer_event(&self, event: &PointerEvent) -> Option<GestureEvent> {
        // println!("pointer event: {:?}", event);
        let mut state = self.state.borrow_mut();

        match event {
            PointerEvent::Down(pos) => {
                let now = AnimInstant::now();

                // Stale-touch cleanup: if there are orphan touches from before
                // the app was backgrounded (no Cancel/Up received), clear them
                // so a fresh single touch doesn't falsely trigger a pinch.
                if !state.touches.is_empty()
                    && state
                        .down_time
                        .is_none_or(|t| {
                            now.duration_since(t)
                                .as_millis()
                                > STALE_GESTURE_TOUCH_MS as u128
                        })
                {
                    state.touches.clear();
                    state.initial_pinch_distance = None;
                    state.current_scale = 1.0;
                }

                state
                    .touches
                    .insert(pos.id, *pos);

                if state.touches.len() == 2 {
                    let positions: Vec<PointerPosition> = state
                        .touches
                        .values()
                        .copied()
                        .collect();
                    let dist = distance(positions[0], positions[1]);
                    state.initial_pinch_distance = Some(dist);
                    state.current_scale = 1.0;
                    let focal = midpoint(positions[0], positions[1]);
                    return Some(GestureEvent::ScaleStart {
                        focal_x: focal.x,
                        focal_y: focal.y,
                    });
                }

                if state.touches.len() == 1 {
                    state.down_position = Some(*pos);
                    state.down_time = Some(now);
                    state.is_dragging = false;
                    state.last_drag_position = None;
                    state.drag_start_time = None;
                    state.drag_start_position = None;
                }
                None
            }

            PointerEvent::Up(pos) => {
                state.touches.remove(&pos.id);

                if state
                    .initial_pinch_distance
                    .is_some()
                    && state.touches.len() < 2
                {
                    state.initial_pinch_distance = None;
                    state.current_scale = 1.0;
                    drop(state);
                    return Some(GestureEvent::ScaleEnd);
                }

                if state.is_dragging {
                    let start_time = state.drag_start_time.take();
                    let start_pos = state
                        .drag_start_position
                        .take();
                    state.is_dragging = false;
                    state.last_drag_position = None;
                    state.down_position = None;
                    state.down_time = None;
                    drop(state);

                    if let Some(cb) = self
                        .on_drag_end
                        .callable()
                        .as_ref()
                    {
                        Self::execute_callback(
                            cb,
                            #[cfg(not(target_arch = "wasm32"))]
                            &self.runtime_handle,
                        );
                    }

                    if let (Some(start_time), Some(start_pos)) = (start_time, start_pos)
                        && let Some(cb) = self
                            .on_swipe
                            .callable()
                            .as_ref()
                    {
                        let elapsed = AnimInstant::now().duration_since(start_time);
                        if elapsed.as_millis() as u64 <= SWIPE_MAX_DURATION_MS {
                            let dx = pos.x - start_pos.x;
                            let dy = pos.y - start_pos.y;
                            let dist = (dx * dx + dy * dy).sqrt();
                            let velocity = dist / elapsed.as_secs_f32();
                            if velocity > SWIPE_VELOCITY_THRESHOLD {
                                let direction = if dx.abs() > dy.abs() {
                                    if dx > 0.0 {
                                        SwipeDirection::Right
                                    } else {
                                        SwipeDirection::Left
                                    }
                                } else {
                                    if dy > 0.0 {
                                        SwipeDirection::Down
                                    } else {
                                        SwipeDirection::Up
                                    }
                                };
                                let vx = dx / elapsed.as_secs_f32();
                                let vy = dy / elapsed.as_secs_f32();

                                Self::execute_paramed_callback(
                                    cb,
                                    direction,
                                    #[cfg(not(target_arch = "wasm32"))]
                                    &self.runtime_handle,
                                );
                                return Some(GestureEvent::Swipe {
                                    direction,
                                    velocity_x: vx,
                                    velocity_y: vy,
                                });
                            }
                        }
                    }

                    return Some(GestureEvent::DragEnd(*pos));
                }

                let down_pos = state.down_position.take()?;
                let down_time = state.down_time.take()?;
                let now = AnimInstant::now();
                let elapsed = now.duration_since(down_time);

                if distance(down_pos, *pos) > TAP_SLOP {
                    state.last_tap_time = None;
                    state.last_tap_position = None;
                    return None;
                }

                if let Some(cb) = self
                    .on_long_press
                    .callable()
                    .as_ref()
                    && elapsed >= LONG_PRESS_DURATION
                {
                    state.last_tap_time = None;
                    state.last_tap_position = None;
                    drop(state);

                    Self::execute_callback(
                        cb,
                        #[cfg(not(target_arch = "wasm32"))]
                        &self.runtime_handle,
                    );
                    return Some(GestureEvent::LongPress(*pos));
                }

                #[allow(clippy::collapsible_if)]
                if let Some(cb) = self
                    .on_double_press
                    .callable()
                    .as_ref()
                {
                    if let (Some(last_time), Some(last_pos)) =
                        (state.last_tap_time, state.last_tap_position)
                    {
                        let delta = now.duration_since(last_time);
                        if delta < DOUBLE_TAP_TIMEOUT && distance(last_pos, *pos) < TAP_SLOP {
                            state.last_tap_time = None;
                            state.last_tap_position = None;
                            drop(state);

                            Self::execute_callback(
                                cb,
                                #[cfg(not(target_arch = "wasm32"))]
                                &self.runtime_handle,
                            );
                            return Some(GestureEvent::DoubleTap(*pos));
                        }
                    }
                }

                state.last_tap_time = Some(now);
                state.last_tap_position = Some(*pos);
                drop(state);
                if let Some(cb) = self
                    .on_tap
                    .callable()
                    .as_ref()
                {
                    Self::execute_callback(
                        cb,
                        #[cfg(not(target_arch = "wasm32"))]
                        &self.runtime_handle,
                    );
                }
                Some(GestureEvent::Tap(*pos))
            }

            PointerEvent::Move(pos) => {
                state
                    .touches
                    .insert(pos.id, *pos);

                if state.touches.len() >= 2
                    && state
                        .initial_pinch_distance
                        .is_some()
                    && let Some(cb) = self
                        .on_scale
                        .callable()
                        .as_ref()
                {
                    let positions: Vec<PointerPosition> = state
                        .touches
                        .values()
                        .copied()
                        .collect();
                    let current_dist = distance(positions[0], positions[1]);
                    let initial_dist = state
                        .initial_pinch_distance
                        .unwrap_or(current_dist);
                    if initial_dist > 0.0 {
                        let new_scale = current_dist / initial_dist;
                        let delta_scale = if state.current_scale > 0.0 {
                            new_scale / state.current_scale
                        } else {
                            1.0
                        };
                        state.current_scale = new_scale;
                        let focal = midpoint(positions[0], positions[1]);
                        let data = ScaleData {
                            focal_x: focal.x,
                            focal_y: focal.y,
                            scale: new_scale,
                            delta_scale,
                        };
                        drop(state);
                        Self::execute_paramed_callback(
                            cb,
                            data,
                            #[cfg(not(target_arch = "wasm32"))]
                            &self.runtime_handle,
                        );
                        return Some(GestureEvent::ScaleUpdate {
                            focal_x: focal.x,
                            focal_y: focal.y,
                            scale: new_scale,
                            delta_scale,
                        });
                    }
                }

                if let Some(down_pos) = state.down_position {
                    if state.is_dragging
                        && let Some(cb) = self
                            .on_drag_update
                            .callable()
                            .as_ref()
                    {
                        let last = state
                            .last_drag_position
                            .unwrap_or(down_pos);
                        let delta_x = pos.x - last.x;
                        let delta_y = pos.y - last.y;
                        state.last_drag_position = Some(*pos);
                        let data = DragUpdateData {
                            position: *pos,
                            delta_x,
                            delta_y,
                        };
                        drop(state);
                        Self::execute_paramed_callback(
                            cb,
                            data,
                            #[cfg(not(target_arch = "wasm32"))]
                            &self.runtime_handle,
                        );
                        return Some(GestureEvent::DragUpdate {
                            position: *pos,
                            delta_x,
                            delta_y,
                        });
                    } else if distance(down_pos, *pos) > TAP_SLOP
                        && let Some(cb) = self
                            .on_drag_start
                            .callable()
                            .as_ref()
                    {
                        state.is_dragging = true;
                        state.last_drag_position = Some(*pos);
                        state.drag_start_time = Some(AnimInstant::now());
                        state.drag_start_position = Some(down_pos);
                        drop(state);
                        Self::execute_paramed_callback(
                            cb,
                            down_pos,
                            #[cfg(not(target_arch = "wasm32"))]
                            &self.runtime_handle,
                        );

                        return Some(GestureEvent::DragStart(down_pos));
                    }
                }
                None
            }

            PointerEvent::Cancel => {
                if state.is_dragging {
                    state.is_dragging = false;
                    state.last_drag_position = None;
                }
                if state
                    .initial_pinch_distance
                    .is_some()
                {
                    state.initial_pinch_distance = None;
                    state.current_scale = 1.0;
                }
                state.touches.clear();
                state.down_position = None;
                state.down_time = None;
                None
            }

            PointerEvent::RightClick(pos) => {
                drop(state);
                if let Some(cb) = self
                    .on_right_tap
                    .callable()
                    .as_ref()
                {
                    Self::execute_callback(
                        cb,
                        #[cfg(not(target_arch = "wasm32"))]
                        &self.runtime_handle,
                    );
                }
                Some(GestureEvent::RightTap(*pos))
            }

            PointerEvent::Scroll { delta_x, delta_y } => {
                let data = ScrollData {
                    delta_x: *delta_x,
                    delta_y: *delta_y,
                };
                drop(state);
                if let Some(cb) = self
                    .on_scroll
                    .callable()
                    .as_ref()
                {
                    Self::execute_paramed_callback(
                        cb,
                        data,
                        #[cfg(not(target_arch = "wasm32"))]
                        &self.runtime_handle,
                    );
                }
                Some(GestureEvent::Scroll {
                    delta_x: *delta_x,
                    delta_y: *delta_y,
                })
            }
        }
    }
}

// ── Geometry helpers ────────────────────────────────────────────────────

fn distance(a: PointerPosition, b: PointerPosition) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

fn midpoint(a: PointerPosition, b: PointerPosition) -> PointerPosition {
    PointerPosition {
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
        source: a.source,
        id: a.id,
    }
}

/// Whether a gesture detector should consume (and stop propagating) a
/// `Scroll` event. A detector only claims a scroll when it actually has an
/// `on_scroll` handler; otherwise the event must fall through to whatever is
/// behind/below it (e.g. a `Scrollable` on a lower `Stack` layer). `Scroll`
/// events carry no pointer position, so the decision cannot be bounds-based.
fn detector_consumes_scroll(on_scroll: &ScrollCallback) -> bool {
    on_scroll.callable().is_some()
}

fn should_accept_pointer_event(
    cached_bounds: &CacheBounds,
    state: &GestureState,
    event: &ElementEvent,
    pos: Vec2d,
) -> bool {
    if cached_bounds.is_inside(pos.x, pos.y) {
        return true;
    }

    match event {
        ElementEvent::PointerUp(_, _, id) => state.touches.contains_key(id),
        _ => false,
    }
}

fn preserve_gesture_state(existing: &GestureState, replacement: &mut GestureState) {
    *replacement = existing.clone();
}

// ── Element trait impls ─────────────────────────────────────────────────

impl<E: Element> VisitorElement for RawGestureDetector<E> {
    fn debug_name(&self) -> &'static str {
        "GestureDetector"
    }
}

impl<E: Element> EventElement for RawGestureDetector<E> {
    fn on_event(&self, event: &ElementEvent) -> bool {
        if matches!(event, ElementEvent::Cancel) {
            self.process_pointer_event(&PointerEvent::Cancel);
            self.window.request_redraw();
            return true;
        }

        let pos = match event {
            ElementEvent::PointerDown(p, _, _)
            | ElementEvent::PointerUp(p, _, _)
            | ElementEvent::PointerMove(p, _, _) => p,
            ElementEvent::Scroll { delta, .. } => {
                // Only consume a scroll if this detector actually has a scroll
                // handler. A `Scroll` event carries no pointer position, and a
                // `MouseRegion` wrapper (which has no bounds of its own) forwards
                // every event straight to us regardless of the cursor location,
                // so returning `true` unconditionally meant a handler-less
                // detector (e.g. a header `TextButton` = MouseRegion +
                // GestureDetector) sitting on a top `Stack` layer swallowed every
                // wheel/trackpad scroll before it could reach a `Scrollable` on a
                // lower layer — scrolling appeared completely dead. Let the event
                // fall through when we have nothing to do with it.
                if !detector_consumes_scroll(&self.on_scroll) {
                    return false;
                }
                let pointer_event = PointerEvent::Scroll {
                    delta_x: delta.x,
                    delta_y: delta.y,
                };
                self.process_pointer_event(&pointer_event);
                self.window.request_redraw();
                return true;
            }
            _ => return false,
        };

        if !should_accept_pointer_event(&self.cached_bounds, &self.state.borrow(), event, *pos) {
            return false;
        }

        let pointer_event = match event {
            ElementEvent::PointerDown(pos, source, id) => PointerEvent::Down(PointerPosition {
                x: pos.x,
                y: pos.y,
                source: *source,
                id: *id,
            }),
            ElementEvent::PointerUp(pos, source, id) => PointerEvent::Up(PointerPosition {
                x: pos.x,
                y: pos.y,
                source: *source,
                id: *id,
            }),
            ElementEvent::PointerMove(pos, source, id) => PointerEvent::Move(PointerPosition {
                x: pos.x,
                y: pos.y,
                source: *source,
                id: *id,
            }),
            _ => return false,
        };

        self.process_pointer_event(&pointer_event);
        self.window.request_redraw();
        true
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }
}

impl<E: Element> LayoutElement for RawGestureDetector<E> {
    #[inline]
    fn size(&self) -> Option<Size> {
        None
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        let size = self.child.layout(ctx);
        let (abs_x, abs_y) = ctx
            .canvas
            .get_transform_translation();
        self.cached_bounds
            .save(ctx.scale, abs_x, abs_y, size.width, size.height);
        size
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.cached_bounds
            .pos_start_end()
    }
}

impl<E: Element> Drawable for RawGestureDetector<E> {
    fn draw(&self, ctx: &BuildContext<'_>) {
        let (abs_x, abs_y) = ctx
            .canvas
            .get_transform_translation();
        let child_size = self.child.computed_size(ctx);
        self.cached_bounds
            .save(ctx.scale, abs_x, abs_y, child_size.width, child_size.height);

        self.child.draw(ctx);
    }
}

impl<E: Element> Rebuildable for RawGestureDetector<E> {}

#[cfg(test)]
mod tests {
    use aimer_events::pointer::{PointerPosition, PointerSource};

    use super::*;

    struct TestWidget;

    impl Widget for TestWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            panic!("not needed for builder tests")
        }
    }

    fn touch_position(x: f32, y: f32, id: u64) -> PointerPosition {
        PointerPosition {
            x,
            y,
            source: PointerSource::Touch,
            id,
        }
    }

    fn touch_vec(x: f32, y: f32) -> Vec2d {
        Vec2d { x, y }
    }

    #[test]
    fn builder_configures_all_gestures_before_child_is_added() {
        let detector = GestureDetector::new()
            .on_tap(|| {})
            .on_double_press(|| {})
            .on_long_press(|| {})
            .on_drag_start(|_: PointerPosition| {})
            .on_drag_update(|_: DragUpdateData| {})
            .on_drag_end(|| {})
            .on_right_tap(|| {})
            .on_swipe(|_: SwipeDirection| {})
            .on_scroll(|_: ScrollData| {})
            .on_scale(|_: ScaleData| {})
            .child(TestWidget);

        assert!(detector_consumes_scroll(&detector.on_scroll));
    }

    #[test]
    fn touch_down_inside_cached_bounds_is_accepted() {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 10.0, 20.0, 100.0, 50.0);
        let state = GestureState::default();
        let pos = touch_vec(25.0, 35.0);
        let event = ElementEvent::PointerDown(pos, PointerSource::Touch, 7);

        assert!(should_accept_pointer_event(&bounds, &state, &event, pos));
    }

    #[test]
    fn touch_down_outside_cached_bounds_is_rejected() {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 10.0, 20.0, 100.0, 50.0);
        let state = GestureState::default();
        let pos = touch_vec(200.0, 35.0);
        let event = ElementEvent::PointerDown(pos, PointerSource::Touch, 7);

        assert!(!should_accept_pointer_event(&bounds, &state, &event, pos));
    }

    #[test]
    fn active_touch_up_outside_cached_bounds_is_accepted() {
        let bounds = CacheBounds::new();
        bounds.save(1.0, 10.0, 20.0, 100.0, 50.0);
        let mut state = GestureState::default();
        state
            .touches
            .insert(7, touch_position(25.0, 35.0, 7));
        let pos = touch_vec(115.0, 35.0);
        let event = ElementEvent::PointerUp(pos, PointerSource::Touch, 7);

        assert!(should_accept_pointer_event(&bounds, &state, &event, pos));
    }

    // Regression for "the Scroll is not able to scroll with mouse wheel or
    // trackpad": a gesture detector with no `on_scroll` handler (e.g. a header
    // `TextButton` = MouseRegion + GestureDetector) must NOT consume a scroll,
    // otherwise — sitting on a top `Stack` layer and dispatched first — it
    // swallows every wheel/trackpad scroll before it can reach a `Scrollable`
    // on a lower layer, and nothing scrolls.
    #[test]
    fn detector_without_scroll_handler_lets_scroll_fall_through() {
        let on_scroll = ScrollCallback::default();
        assert!(
            !detector_consumes_scroll(&on_scroll),
            "a handler-less detector must let the scroll propagate to lower layers"
        );
    }

    // A detector that actually handles scrolls still claims them.
    #[test]
    fn detector_with_scroll_handler_consumes_scroll() {
        let on_scroll: ScrollCallback = (|_: ScrollData| {}).into();
        assert!(
            detector_consumes_scroll(&on_scroll),
            "a detector with an on_scroll handler must consume the scroll"
        );
    }

    #[test]
    fn active_touch_state_is_preserved_for_replacement_detector() {
        let mut existing = GestureState::default();
        let down = touch_position(25.0, 35.0, 7);
        existing
            .touches
            .insert(7, down);
        existing.down_position = Some(down);
        existing.down_time = Some(AnimInstant::now());

        let mut replacement = GestureState::default();
        preserve_gesture_state(&existing, &mut replacement);

        assert_eq!(replacement.touches.get(&7), Some(&down));
        assert_eq!(replacement.down_position, Some(down));
        assert!(
            replacement
                .down_time
                .is_some()
        );
    }
}
