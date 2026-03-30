use events::pointer::{PointerEvent, PointerPosition};
use chrono::{Duration, Local};
use std::cell::UnsafeCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use crate::callback::{Callback, CallbackHolder};

pub mod button;
pub mod gesture_detector;


const DOUBLE_TAP_TIMEOUT: Duration = Duration::milliseconds(300);
const LONG_PRESS_DURATION: Duration = Duration::milliseconds(500);


const TAP_SLOP: f32 = 18.0;

#[derive(Clone, Debug)]
pub enum GestureEvent {
    Tap(PointerPosition),
    DoubleTap(PointerPosition),
    LongPress(PointerPosition),
}

#[derive(Default, Debug)]
pub struct GestureActions {
    pub on_tap: CallbackHolder<(),()>,
    pub on_double_tap: CallbackHolder<(),()>,
    pub on_long_press: CallbackHolder<(),()>,
    #[cfg(not(target_arch = "wasm32"))]
    pub runtime_handle: Option<tokio::runtime::Handle>,
    state: GestureState,
}

#[derive(Default, Debug)]
struct GestureState {
    down_position: Option<PointerPosition>,
    down_time: Option<Duration>,
    last_tap_time: Option<Duration>,
    last_tap_position: Option<PointerPosition>,
}

impl GestureActions {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            on_tap: CallbackHolder::default(),
            on_double_tap: CallbackHolder::default(),
            on_long_press: CallbackHolder::default(),
            #[cfg(not(target_arch = "wasm32"))]
            runtime_handle: None,
            state: GestureState::default(),
        }
    }

    fn execute_callback(cb: &CallbackHolder<(),()>, #[cfg(not(target_arch = "wasm32"))] runtime_handle: &Option<tokio::runtime::Handle>) {
        unsafe {
            if let Some(callback) = (*cb.get()).as_ref() {
                match callback {
                    Callback::Sync(f) => f(()),
                    Callback::Async(f) => {
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
    }

    /// Feed a `PointerEvent` into the detector. Returns a recognized `GestureEvent` if any.
    pub fn handle_pointer_event(&mut self, event: &PointerEvent) -> Option<GestureEvent> {
        match event {
            PointerEvent::Down(pos) => {
                let timestamp = Duration::microseconds(Local::now().timestamp_micros());
                self.state.down_position = Some(*pos);
                self.state.down_time = Some(timestamp);
                None
            }

            PointerEvent::Up(pos) => {
                let down_pos = self.state.down_position.take()?;
                let down_time = self.state.down_time.take()?;
                let now = Duration::microseconds(Local::now().timestamp_micros());
                let elapsed = now - down_time;

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
                    utils::debug!("on_tab is called ");
                    Self::execute_callback(&self.on_long_press,#[cfg(not(target_arch = "wasm32"))] &self.runtime_handle);
                    return Some(gesture);
                }

                // Double tap check
                #[allow(clippy::collapsible_if)]
                if let (Some(last_time), Some(last_pos)) = (self.state.last_tap_time, self.state.last_tap_position) {
                    if last_time < DOUBLE_TAP_TIMEOUT && distance(last_pos, *pos) < TAP_SLOP {
                        self.state.last_tap_time = None;
                        self.state.last_tap_position = None;
                        let gesture = GestureEvent::DoubleTap(*pos);
                        utils::debug!("on_double_tap is called ");
                        Self::execute_callback(&self.on_double_tap,#[cfg(not(target_arch = "wasm32"))] &self.runtime_handle);
                        return Some(gesture);
                    }
                }

                // Single tap
                let now = Duration::microseconds(Local::now().timestamp_micros());
                self.state.last_tap_time = Some(now);
                self.state.last_tap_position = Some(*pos);
                let gesture = GestureEvent::Tap(*pos);
                // utils::debug!("on_tab is called ");
                Self::execute_callback(&self.on_tap, #[cfg(not(target_arch = "wasm32"))] &self.runtime_handle);
                Some(gesture)
            }

            PointerEvent::Move(_) | PointerEvent::Cancel => {
                if matches!(event, PointerEvent::Cancel) {
                    self.state.down_position = None;
                    self.state.down_time = None;
                }
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
    use events::pointer::{PointerEvent, PointerPosition};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn test_tap_callback_called() {
        let mut gesture = GestureActions::new();
        let tap_called = Arc::new(AtomicBool::new(false));
        let tap_called_clone = tap_called.clone();

        gesture.on_tap = CallbackHolder::<(),()>::from(move |_| {
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

        gesture.on_tap = CallbackHolder::<(),()>::from(move |_| {
            tap_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let pos = PointerPosition { x: 10.0, y: 10.0 };
        
        // First tap
        gesture.handle_pointer_event(&PointerEvent::Down(pos));
        gesture.handle_pointer_event(&PointerEvent::Up(pos));
        
        // Second tap
        gesture.handle_pointer_event(&PointerEvent::Down(pos));
        gesture.handle_pointer_event(&PointerEvent::Up(pos));

        assert_eq!(tap_count.load(Ordering::SeqCst), 2, "Tap callback should have been called twice");
    }

    #[test]
    fn test_long_press_callback_called() {
        let mut gesture = GestureActions::new();
        let long_press_called = Arc::new(AtomicBool::new(false));
        let long_press_called_clone = long_press_called.clone();

        gesture.on_long_press = CallbackHolder::<(),()>::from(move |_| {
            long_press_called_clone.store(true, Ordering::SeqCst);
        });

        let pos = PointerPosition { x: 10.0, y: 10.0 };
        gesture.handle_pointer_event(&PointerEvent::Down(pos));

        // Wait for long press duration
        std::thread::sleep(std::time::Duration::from_millis(550));

        gesture.handle_pointer_event(&PointerEvent::Up(pos));

        assert!(long_press_called.load(Ordering::SeqCst), "Long press callback should have been called");
    }
}
