use events::element::KeyAction;
use crate::render::OxidizeAppConfiguration;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use utils::debug;
use widget::{ dispatch_event};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;
use events::element::{ElementEvent, NamedKey};

pub(crate) fn handle_window_event(
    app: &mut OxidizeAppConfiguration,
    event_loop: &ActiveEventLoop,
    _id: WindowId,
    event: WindowEvent,
) {
    match event {
        WindowEvent::CloseRequested => {
            event_loop.exit();
        }

        WindowEvent::Touch(item) => {
            let pos = Vec2d { x: item.location.x as crate::render::Float, y: item.location.y as crate::render::Float };
            // info!("Touch: {:?}", pos);
            let event = match item.phase {
                winit::event::TouchPhase::Started => Some(ElementEvent::PointerDown(pos)),
                winit::event::TouchPhase::Moved => Some(ElementEvent::PointerMove(pos)),
                winit::event::TouchPhase::Ended => Some(ElementEvent::PointerUp(pos)),
                winit::event::TouchPhase::Cancelled => Some(ElementEvent::Cancel),
            };
            #[allow(clippy::collapsible_if)]
            if let Some(event) = event {
                if let Some(root) = &app.widget_root {
                    if dispatch_event(root.as_ref(), pos, &event) {
                        if let Some(window) = &app.window {
                            window.request_redraw();
                        }
                    }
                }
            }
        }
        // WindowEvent::Focused
        WindowEvent::CursorMoved { position, .. } => {
            let new_pos = Vec2d { x: position.x as crate::render::Float, y: position.y as crate::render::Float };
            // Skip dispatch if the cursor barely moved (less than 1 logical pixel).
            let dx = (new_pos.x - app.cursor_pos.x).abs();
            let dy = (new_pos.y - app.cursor_pos.y).abs();
            if dx < 1.0 && dy < 1.0 {
                return;
            }
            app.cursor_pos = new_pos;
            if let Some(root) = &app.widget_root {
                let event = ElementEvent::PointerMove(app.cursor_pos);
                if dispatch_event(root.as_ref(), app.cursor_pos, &event) {
                    if let Some(window) = &app.window {
                        window.request_redraw();
                    }
                }
            }
        }

        WindowEvent::MouseInput { state, button, .. } => {
            if button != winit::event::MouseButton::Left {
                return;
            }

            let c = app.cursor_pos;
            let event = if state.is_pressed() { ElementEvent::PointerDown(c) } else { ElementEvent::PointerUp(c) };
            #[allow(clippy::collapsible_if)]
            if let Some(root) = &app.widget_root {
                dispatch_event(root.as_ref(), c, &event);
            }
        }

        WindowEvent::KeyboardInput { event, .. } => {
            use winit::event::ElementState;
            use winit::keyboard::{Key, NamedKey as WinitNamedKey};

            let action = if event.repeat {
                KeyAction::Repeat
            } else {
                match event.state {
                    ElementState::Pressed => KeyAction::Pressed,
                    ElementState::Released => KeyAction::Released,
                }
            };

            // Handle text input from the key event.
            // Use `logical_key` (Key::Character) as the single source of
            // printable text to avoid duplicate input.  Winit ≥0.30.6 may
            // fire two Pressed KeyboardInput events per keystroke on some
            // platforms (including WASM); using `event.text` would process
            // both, doubling every character.
            if let Key::Character(ref ch) = event.logical_key {
                if !ch.is_empty() && ch.chars().all(|c| !c.is_control()) {
                    let ev = ElementEvent::CharInput { ch: ch.parse().unwrap(), action: action.clone() };
                    if let Some(root) = &app.widget_root {
                        if dispatch_event(root.as_ref(), app.cursor_pos, &ev) {
                            if let Some(window) = &app.window {
                                window.request_redraw();
                            }
                        }
                    }
                    return;
                }
            }

            // Handle named keys
            if let Key::Named(named) = &event.logical_key {
                let key = match named {
                    WinitNamedKey::Backspace => NamedKey::Backspace,
                    WinitNamedKey::Delete => NamedKey::Delete,
                    WinitNamedKey::ArrowLeft => NamedKey::ArrowLeft,
                    WinitNamedKey::ArrowRight => NamedKey::ArrowRight,
                    WinitNamedKey::Home => NamedKey::Home,
                    WinitNamedKey::End => NamedKey::End,
                    WinitNamedKey::Enter => NamedKey::Enter,
                    WinitNamedKey::Escape => NamedKey::Escape,
                    WinitNamedKey::Tab => NamedKey::Tab,
                    other => NamedKey::Other(format!("{:?}", other)),
                };
                let ev = ElementEvent::KeyInput { key, action };
                if let Some(root) = &app.widget_root {
                    if dispatch_event(root.as_ref(), app.cursor_pos, &ev) {
                        if let Some(window) = &app.window {
                            window.request_redraw();
                        }
                    }
                }
            }
        }

        WindowEvent::MouseWheel { delta, .. } => {
            let scroll_delta = match delta {
                winit::event::MouseScrollDelta::LineDelta(x, y) => {
                    Vec2d { x: x as crate::render::Float * 30.0, y: y as crate::render::Float * 30.0 }
                }
                winit::event::MouseScrollDelta::PixelDelta(pos) => {
                    Vec2d { x: pos.x as crate::render::Float, y: pos.y as crate::render::Float }
                }
            };
            let event = ElementEvent::Scroll(scroll_delta);
            if let Some(root) = &app.widget_root {
                if dispatch_event(root.as_ref(), app.cursor_pos, &event) {
                    if let Some(window) = &app.window {
                        window.request_redraw();
                    }
                }
            }
        }

        WindowEvent::RedrawRequested => app.render(event_loop),
        WindowEvent::Resized(size) => {
            let is_portrait = size.width < size.height;
            debug!("Window resized to  Raw Value : {:?}", size);
            #[cfg(target_os = "ios")]
            let size = {
                match crate::ios_screen::get_screen_resolution_pixels() {
                    Some((width, height)) => {
                        app.native_window_size = Some(ResolvedSize { width: width as f32, height: height as f32 });
                        if is_portrait {
                            PhysicalSize::new(width as u32, height as u32)
                        } else {
                            PhysicalSize::new(height as u32, width as u32)
                        }
                    }
                    None => {
                        if app.window.is_none() {
                            return;
                        }
                        app.window.unwrap().inner_size()
                    }
                }
            };
            debug!("Window resized to {:?}", size);

            app.pending_resize = Some(size);
            if let Some(window) = &app.window {
                window.request_redraw();
            }
        }
        _ => (),
    }
}
