use std::cell::UnsafeCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::{Duration, Instant};
use crate::event::{PointerEvent, PointerPosition};


pub mod button;

/// A callback that can be either synchronous or asynchronous.
pub enum Callback {
    Sync(Box<dyn FnOnce()>),
    Async(Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()>>>>),
}

impl std::fmt::Debug for Callback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Callback::Sync(_) => write!(f, "Callback::Sync(...)"),
            Callback::Async(_) => write!(f, "Callback::Async(...)"),
        }
    }
}

impl<F: FnOnce() + 'static> From<F> for Callback {
    fn from(f: F) -> Self {
        Callback::Sync(Box::new(f))
    }
}

/// Wrapper to convert an async closure into a `Callback::Async`.
pub struct AsyncCallback<F>(pub F);

impl<F, Fut> From<AsyncCallback<F>> for Callback
where
    F: FnOnce() -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn from(ac: AsyncCallback<F>) -> Self {
        Callback::Async(Box::new(move || Box::pin(ac.0())))
    }
}

/// A holder for a `Callback` that can be shared via `Rc`.
/// Accepts both sync closures and `AsyncCallback`-wrapped async closures via `.into()`.
#[derive(Debug)]
pub struct CallbackHolder(Rc<UnsafeCell<Option<Callback>>>);

impl CallbackHolder {
    pub fn get(&self) -> *mut Option<Callback> {
        self.0.get()
    }
}

impl Default for CallbackHolder {
    fn default() -> Self {
        CallbackHolder(Rc::new(UnsafeCell::new(None)))
    }
}

impl Clone for CallbackHolder {
    fn clone(&self) -> Self {
        CallbackHolder(self.0.clone())
    }
}

impl<F: FnOnce() + 'static> From<F> for CallbackHolder {
    fn from(f: F) -> Self {
        CallbackHolder(Rc::new(UnsafeCell::new(Some(Callback::Sync(Box::new(f))))))
    }
}

impl<F, Fut> From<AsyncCallback<F>> for CallbackHolder
where
    F: FnOnce() -> Fut + 'static,
    Fut: Future<Output = ()> + 'static,
{
    fn from(ac: AsyncCallback<F>) -> Self {
        CallbackHolder(Rc::new(UnsafeCell::new(Some(Callback::Async(Box::new(move || Box::pin(ac.0())))))))
    }
}


const DOUBLE_TAP_TIMEOUT: Duration = Duration::from_millis(300);
const LONG_PRESS_DURATION: Duration = Duration::from_millis(500);
const TAP_SLOP: f32 = 18.0;

#[derive(Clone, Debug)]
pub enum GestureEvent {
    Tap(PointerPosition),
    DoubleTap(PointerPosition),
    LongPress(PointerPosition),
}


#[derive(Default, Debug)]
pub struct GestureActions {
    pub on_tap: CallbackHolder,
    pub on_double_tap: CallbackHolder,
    pub on_long_press: CallbackHolder,

    pub runtime_handle: Option<tokio::runtime::Handle>,

    state: GestureState,
}

#[derive(Default)]
#[derive(Debug)]
struct GestureState {
    down_position: Option<PointerPosition>,
    down_time: Option<Instant>,
    last_tap_time: Option<Instant>,
    last_tap_position: Option<PointerPosition>,
}

impl GestureActions {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            on_tap: CallbackHolder::default(),
            on_double_tap: CallbackHolder::default(),
            on_long_press: CallbackHolder::default(),
            runtime_handle: None,
            state: GestureState::default(),
        }
    }

    fn execute_callback(cb: &CallbackHolder, runtime_handle: &Option<tokio::runtime::Handle>) {
        unsafe {
            if let Some(callback) = (*cb.get()).take() {
                match callback {
                    Callback::Sync(f) => f(),
                    Callback::Async(f) => {
                        if let Some(handle) = runtime_handle {
                            handle.block_on(f());
                        }
                    }
                }
            }
        }
    }

    /// Feed a `PointerEvent` into the detector. Returns a recognized `GestureEvent` if any.
    pub fn handle_pointer_event(&mut self, event: &PointerEvent) -> Option<GestureEvent> {
        // println!("Handling : {:?}", self);
        match event {
            PointerEvent::Down(pos) => {
                self.state.down_position = Some(*pos);
                self.state.down_time = Some(Instant::now());
                None
            }

            PointerEvent::Up(pos) => {
                let down_pos= self.state.down_position.take()?;
                let down_time = self.state.down_time.take()?;

                let elapsed = down_time.elapsed();

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
                    Self::execute_callback(&self.on_long_press, &self.runtime_handle);
                    return Some(gesture);
                }

                // Double tap check
                #[allow(clippy::collapsible_if)]
                if let (Some(last_time), Some(last_pos)) =
                    (self.state.last_tap_time, self.state.last_tap_position)
                {
                    if last_time.elapsed() < DOUBLE_TAP_TIMEOUT
                        && distance(last_pos, *pos) < TAP_SLOP
                    {
                        self.state.last_tap_time = None;
                        self.state.last_tap_position = None;
                        let gesture = GestureEvent::DoubleTap(*pos);
                        Self::execute_callback(&self.on_double_tap, &self.runtime_handle);
                        return Some(gesture);
                    }
                }

                // Single tap
                self.state.last_tap_time = Some(Instant::now());
                self.state.last_tap_position = Some(*pos);
                let gesture = GestureEvent::Tap(*pos);
                Self::execute_callback(&self.on_tap, &self.runtime_handle);
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
    use crate::event::{PointerEvent, PointerPosition};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_tap_callback_called() {
        let mut gesture = GestureActions::new();
        let tap_called = Arc::new(AtomicBool::new(false));
        let tap_called_clone = tap_called.clone();
        
        gesture.on_tap = CallbackHolder::from(move || {
            tap_called_clone.store(true, Ordering::SeqCst);
        });

        let pos = PointerPosition { x: 10.0, y: 10.0 };
        gesture.handle_pointer_event(&PointerEvent::Down(pos));
        gesture.handle_pointer_event(&PointerEvent::Up(pos));

        assert!(tap_called.load(Ordering::SeqCst), "Tap callback should have been called");
    }

    #[test]
    fn test_long_press_callback_called() {
        let mut gesture = GestureActions::new();
        let long_press_called = Arc::new(AtomicBool::new(false));
        let long_press_called_clone = long_press_called.clone();
        
        gesture.on_long_press = CallbackHolder::from(move || {
            long_press_called_clone.store(true, Ordering::SeqCst);
        });

        let pos = PointerPosition { x: 10.0, y: 10.0 };
        gesture.handle_pointer_event(&PointerEvent::Down(pos));
        
        // Wait for long press duration
        std::thread::sleep(LONG_PRESS_DURATION + Duration::from_millis(50));
        
        gesture.handle_pointer_event(&PointerEvent::Up(pos));

        assert!(long_press_called.load(Ordering::SeqCst), "Long press callback should have been called");
    }
}
