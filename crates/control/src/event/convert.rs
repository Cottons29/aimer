use winit::event::{ElementState, MouseButton, TouchPhase, WindowEvent};
use crate::event::event_types::EventType;

impl TryFrom<&WindowEvent> for EventType {
    type Error = ();

    fn try_from(event: &WindowEvent) -> Result<Self, Self::Error> {
        match event {
            WindowEvent::MouseInput { state, button, .. } => {
                if *button != MouseButton::Left {
                    return Err(());
                }
                match state {
                    ElementState::Pressed => Ok(EventType::PointerDown),
                    ElementState::Released => Ok(EventType::PointerUp),
                }
            }

            WindowEvent::CursorMoved { .. } => Ok(EventType::PointerMove),
            WindowEvent::CursorEntered { .. } => Ok(EventType::PointerEnter),
            WindowEvent::CursorLeft { .. } => Ok(EventType::PointerLeave),

            WindowEvent::Touch(touch) => match touch.phase {
                TouchPhase::Started => Ok(EventType::PointerDown),
                TouchPhase::Moved => Ok(EventType::PointerMove),
                TouchPhase::Ended => Ok(EventType::PointerUp),
                TouchPhase::Cancelled => Err(()),
            },

            WindowEvent::KeyboardInput { event, .. } => match event.state {
                ElementState::Pressed => Ok(EventType::KeyDown),
                ElementState::Released => Ok(EventType::KeyUp),
            },

            WindowEvent::Focused(focused) => {
                if *focused {
                    Ok(EventType::WindowFocus)
                } else {
                    Ok(EventType::WindowBlur)
                }
            }

            WindowEvent::Resized(_) => Ok(EventType::Resize),

            WindowEvent::MouseWheel { .. } => Ok(EventType::Scroll),

            WindowEvent::PinchGesture { .. } => Ok(EventType::Pinch),

            _ => Err(()),
        }
    }
}


