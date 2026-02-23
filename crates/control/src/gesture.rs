use std::time::{Duration, Instant};
use crate::event::{PointerEvent, PointerPosition};


pub mod button;


pub type CallbackFunction = Box<dyn Fn() + Send + Sync>;

/// Helper to box a closure into a `CallbackFunction`.
pub fn callback(f: impl Fn() + Send + Sync + 'static) -> Option<CallbackFunction> {
    Some(Box::new(f))
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


#[derive(Default)]
pub struct GestureDetector {
    pub on_tap: Option<CallbackFunction>,
    pub on_double_tap: Option<CallbackFunction>,
    pub on_long_press: Option<CallbackFunction>,

    state: GestureState,
}

#[derive(Default)]
struct GestureState {
    down_position: Option<PointerPosition>,
    down_time: Option<Instant>,
    last_tap_time: Option<Instant>,
    last_tap_position: Option<PointerPosition>,
}

impl GestureDetector {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            on_tap: None,
            on_double_tap: None,
            on_long_press: None,
            state: GestureState::default(),
        }
    }

    /// Feed a `PointerEvent` into the detector. Returns a recognized `GestureEvent` if any.
    pub fn handle_pointer_event(&mut self, event: &PointerEvent) -> Option<GestureEvent> {
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
                    if let Some(cb) = &self.on_long_press {
                        cb();
                    }
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
                        if let Some(cb) = &self.on_double_tap {
                            cb();
                        }
                        return Some(gesture);
                    }
                }

                // Single tap
                self.state.last_tap_time = Some(Instant::now());
                self.state.last_tap_position = Some(*pos);
                let gesture = GestureEvent::Tap(*pos);
                if let Some(cb) = &self.on_tap {
                    cb();
                }
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
