use crate::callback::{RawInnerCallback, VoidCallback, CallbackExecutor};
use aimer_animation::time::AnimInstant;
use aimer_events::pointer::{PointerEvent, PointerPosition};
use std::time::Duration;

pub mod button;
pub mod gesture_detector;

const DOUBLE_TAP_TIMEOUT: Duration = Duration::from_millis(300);
const LONG_PRESS_DURATION: Duration = Duration::from_millis(500);

const TAP_SLOP: f32 = 18.0;

#[derive(Clone, Debug)]
pub enum GestureEvent {
    Tap(PointerPosition),
    DoubleTap(PointerPosition),
    LongPress(PointerPosition),
    DragStart(PointerPosition),
    DragUpdate { position: PointerPosition, delta_x: f32, delta_y: f32 },
    DragEnd(PointerPosition),
}

#[derive(Default, Debug)]
pub struct GestureActions {
    pub on_tap: VoidCallback,
    pub on_double_press: VoidCallback,
    pub on_long_press: VoidCallback,
    #[cfg(not(target_arch = "wasm32"))]
    pub runtime_handle: Option<tokio::runtime::Handle>,
    state: GestureState,
}

#[derive(Default, Debug)]
struct GestureState {
    down_position: Option<PointerPosition>,
    down_time: Option<AnimInstant>,
    last_tap_time: Option<AnimInstant>,
    last_tap_position: Option<PointerPosition>,
    /// Whether a drag gesture is currently active.
    is_dragging: bool,
    /// Last known pointer position during a drag.
    last_drag_position: Option<PointerPosition>,
}

impl GestureActions {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            on_tap: VoidCallback::default(),
            on_double_press: VoidCallback::default(),
            on_long_press: VoidCallback::default(),
            #[cfg(not(target_arch = "wasm32"))]
            runtime_handle: None,
            state: GestureState::default(),
        }
    }

    fn execute_callback(cb: &VoidCallback, #[cfg(not(target_arch = "wasm32"))] runtime_handle: &Option<tokio::runtime::Handle>) {
        if let Some(callback) = (*cb.get()).as_ref() {
            match callback {
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

    /// Feed a `PointerEvent` into the detector. Returns a recognized `GestureEvent` if any.
    pub fn handle_pointer_event(&mut self, event: &PointerEvent) -> Option<GestureEvent> {
        match event {
            PointerEvent::Down(pos) => {
                let now = AnimInstant::now();
                self.state.down_position = Some(*pos);
                self.state.down_time = Some(now);
                self.state.is_dragging = false;
                self.state.last_drag_position = None;
                None
            }

            PointerEvent::Up(pos) => {
                // If we were dragging, end the drag
                if self.state.is_dragging {
                    self.state.is_dragging = false;
                    self.state.last_drag_position = None;
                    self.state.down_position = None;
                    self.state.down_time = None;
                    return Some(GestureEvent::DragEnd(*pos));
                }

                let down_pos = self.state.down_position.take()?;
                let down_time = self.state.down_time.take()?;
                let now = AnimInstant::now();
                let elapsed = now.duration_since(down_time);

                // Check if finger moved too far — not a tap
                if distance(down_pos, *pos) > TAP_SLOP {
                    self.state.last_tap_time = None;
                    self.state.last_tap_position = None;
                    return None;
                }

                // Long press
                if elapsed >= LONG_PRESS_DURATION {
                    let gesture = GestureEvent::LongPress(*pos);
                    self.state.last_tap_time = None;
                    self.state.last_tap_position = None;
                    Self::execute_callback(
                        &self.on_long_press,
                        #[cfg(not(target_arch = "wasm32"))]
                        &self.runtime_handle,
                    );
                    return Some(gesture);
                }

                // Double tap check — compare delta between taps, not absolute time
                #[allow(clippy::collapsible_if)]
                if let (Some(last_time), Some(last_pos)) = (self.state.last_tap_time, self.state.last_tap_position) {
                    let delta = now.duration_since(last_time);
                    if delta < DOUBLE_TAP_TIMEOUT && distance(last_pos, *pos) < TAP_SLOP {
                        self.state.last_tap_time = None;
                        self.state.last_tap_position = None;
                        let gesture = GestureEvent::DoubleTap(*pos);
                        Self::execute_callback(
                            &self.on_double_press,
                            #[cfg(not(target_arch = "wasm32"))]
                            &self.runtime_handle,
                        );
                        return Some(gesture);
                    }
                }

                // Single tap
                self.state.last_tap_time = Some(now);
                self.state.last_tap_position = Some(*pos);
                let gesture = GestureEvent::Tap(*pos);
                Self::execute_callback(
                    &self.on_tap,
                    #[cfg(not(target_arch = "wasm32"))]
                    &self.runtime_handle,
                );
                Some(gesture)
            }

            PointerEvent::Move(pos) => {
                if let Some(down_pos) = self.state.down_position {
                    if self.state.is_dragging {
                        // Ongoing drag — emit update with delta
                        let last = self.state.last_drag_position.unwrap_or(down_pos);
                        let delta_x = pos.x - last.x;
                        let delta_y = pos.y - last.y;
                        self.state.last_drag_position = Some(*pos);
                        return Some(GestureEvent::DragUpdate { position: *pos, delta_x, delta_y });
                    } else if distance(down_pos, *pos) > TAP_SLOP {
                        // Moved past slop threshold — start drag
                        self.state.is_dragging = true;
                        self.state.last_drag_position = Some(*pos);
                        return Some(GestureEvent::DragStart(down_pos));
                    }
                }
                None
            }

            PointerEvent::Cancel => {
                if self.state.is_dragging {
                    self.state.is_dragging = false;
                    self.state.last_drag_position = None;
                }
                self.state.down_position = None;
                self.state.down_time = None;
                None
            }
        }
    }
}

fn distance(a: PointerPosition, b: PointerPosition) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aimer_events::pointer::{PointerEvent, PointerPosition};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn test_tap_callback_called() {
        let mut gesture = GestureActions::new();
        let tap_called = Arc::new(AtomicBool::new(false));
        let tap_called_clone = tap_called.clone();

        gesture.on_tap = VoidCallback::from(move || {
            tap_called_clone.store(true, Ordering::SeqCst);
        });

        let pos = PointerPosition { x: 10.0, y: 10.0 };
        gesture.handle_pointer_event(&PointerEvent::Down(pos));
        gesture.handle_pointer_event(&PointerEvent::Up(pos));

        assert!(tap_called.load(Ordering::SeqCst), "Tap callback should have been called");
    }

    #[test]
    fn test_tap_callback_multiple_times() {
        let mut gesture = GestureActions::new();
        let tap_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let tap_count_clone = tap_count.clone();

        gesture.on_tap = VoidCallback::from(move || {
            tap_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let pos = PointerPosition { x: 10.0, y: 10.0 };
        // Use positions far apart so the second tap is not detected as a double-tap
        let pos2 = PointerPosition { x: 200.0, y: 200.0 };

        // First tap
        gesture.handle_pointer_event(&PointerEvent::Down(pos));
        gesture.handle_pointer_event(&PointerEvent::Up(pos));

        // Second tap at a different position (outside TAP_SLOP from first)
        gesture.handle_pointer_event(&PointerEvent::Down(pos2));
        gesture.handle_pointer_event(&PointerEvent::Up(pos2));

        assert_eq!(tap_count.load(Ordering::SeqCst), 2, "Tap callback should have been called twice");
    }

    #[test]
    fn test_double_tap_callback_called() {
        let mut gesture = GestureActions::new();
        let double_tap_called = Arc::new(AtomicBool::new(false));
        let double_tap_called_clone = double_tap_called.clone();

        gesture.on_double_press = VoidCallback::from(move || {
            double_tap_called_clone.store(true, Ordering::SeqCst);
        });

        let pos = PointerPosition { x: 10.0, y: 10.0 };

        // First tap
        gesture.handle_pointer_event(&PointerEvent::Down(pos));
        gesture.handle_pointer_event(&PointerEvent::Up(pos));

        // Second tap at same position (within DOUBLE_TAP_TIMEOUT)
        gesture.handle_pointer_event(&PointerEvent::Down(pos));
        gesture.handle_pointer_event(&PointerEvent::Up(pos));

        assert!(double_tap_called.load(Ordering::SeqCst), "Double tap callback should have been called");
    }

    #[test]
    fn test_long_press_callback_called() {
        let mut gesture = GestureActions::new();
        let long_press_called = Arc::new(AtomicBool::new(false));
        let long_press_called_clone = long_press_called.clone();

        gesture.on_long_press = VoidCallback::from(move || {
            long_press_called_clone.store(true, Ordering::SeqCst);
        });

        let pos = PointerPosition { x: 10.0, y: 10.0 };
        gesture.handle_pointer_event(&PointerEvent::Down(pos));

        // Wait for long press duration
        std::thread::sleep(Duration::from_millis(550));

        gesture.handle_pointer_event(&PointerEvent::Up(pos));

        assert!(long_press_called.load(Ordering::SeqCst), "Long press callback should have been called");
    }
}
