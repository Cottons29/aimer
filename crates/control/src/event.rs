use winit::event::{ElementState, MouseButton, Touch, TouchPhase, WindowEvent};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub enum PointerEvent {
    Down(PointerPosition),
    Up(PointerPosition),
    Move(PointerPosition),
    Cancel,
}

#[derive(Clone, Debug)]
pub enum Event {
    Pointer(PointerEvent),
}

impl Event {
    /// Convert a `winit::event::WindowEvent` into a high-level `Event`.
    /// Returns `None` for events that are not pointer-related.
    pub fn from_window_event(event: &WindowEvent, cursor_pos: PointerPosition) -> Option<Self> {
        match event {
            WindowEvent::MouseInput { state, button, .. } => {
                if *button != MouseButton::Left {
                    return None;
                }
                match state {
                    ElementState::Pressed => Some(Event::Pointer(PointerEvent::Down(cursor_pos))),
                    ElementState::Released => Some(Event::Pointer(PointerEvent::Up(cursor_pos))),
                }
            }

            WindowEvent::CursorMoved { position, .. } => Some(Event::Pointer(PointerEvent::Move(
                PointerPosition {
                    x: position.x as f32,
                    y: position.y as f32,
                },
            ))),

            WindowEvent::Touch(Touch {
                phase, location, ..
            }) => {
                let pos = PointerPosition {
                    x: location.x as f32,
                    y: location.y as f32,
                };
                match phase {
                    TouchPhase::Started => Some(Event::Pointer(PointerEvent::Down(pos))),
                    TouchPhase::Moved => Some(Event::Pointer(PointerEvent::Move(pos))),
                    TouchPhase::Ended => Some(Event::Pointer(PointerEvent::Up(pos))),
                    TouchPhase::Cancelled => Some(Event::Pointer(PointerEvent::Cancel)),
                }
            }

            _ => None,
        }
    }
}
