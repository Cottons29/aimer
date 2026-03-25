use crate::handler::AimerApplicationHandler;
use attribute::position::Vec2d;
use events::element::KeyAction;
use events::element::{ElementEvent, Modifiers, NamedKey};
use utils::{debug, info};
use widget::{broadcast_event, dispatch_event};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, Touch, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

pub struct WindowEventHandler;


impl WindowEventHandler {
    pub(crate) fn handle_events(
        app: &mut AimerApplicationHandler,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Touch(item) => Self::handle_touch(item, app, _id, event),

            WindowEvent::CursorMoved { position, .. } => Self::handle_cursor_move(position, app),

            WindowEvent::MouseInput { state, button, .. } => Self::handle_mouse_input(state, button, app),

            WindowEvent::ModifiersChanged(mods) => {
                let state = mods.state();
                app.current_modifiers = Modifiers {
                    ctrl: state.control_key(),
                    shift: state.shift_key(),
                    alt: state.alt_key(),
                    meta: state.super_key(),
                };
            }

            WindowEvent::KeyboardInput { event, .. } => Self::handle_keyboard_input(event, app),

            WindowEvent::MouseWheel { delta, .. } => Self::handle_mouse_wheel(delta, app),

            WindowEvent::RedrawRequested => {
             app.render(event_loop)
            },

            WindowEvent::Resized(size) => Self::handle_resize(size, app),

            _ => (),
        }
    }

    fn handle_touch(item: Touch, app: &mut AimerApplicationHandler, _id: WindowId, _event: WindowEvent) {
        let scale = app.window_scale as crate::handler::Float;
        let pos = Vec2d {
            x: item.location.x as crate::handler::Float / scale,
            y: item.location.y as crate::handler::Float / scale,
        };
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
                let mut handled = dispatch_event(root.as_ref(), pos, &event);
                #[cfg(debug_assertions)]
                if app.inspector.is_enabled() {
                    handled = true;
                }
                if !handled {
                    if matches!(&event, ElementEvent::PointerDown(_)) {
                        broadcast_event(root.as_ref(), &event);
                    }
                }
                if let Some(window) = &app.window {
                    window.request_redraw();
                }
            }
        }
    }

    fn handle_cursor_move(position: PhysicalPosition<f64>, app: &mut AimerApplicationHandler) {
        let scale = app.window_scale as crate::handler::Float;
        let new_pos =
            Vec2d { x: position.x as crate::handler::Float / scale, y: position.y as crate::handler::Float / scale };
        let dx = (new_pos.x - app.cursor_pos.x).abs();
        let dy = (new_pos.y - app.cursor_pos.y).abs();
        if dx < 1.0 && dy < 1.0 {
            return;
        }
        app.cursor_pos = new_pos;
        if let Some(root) = &app.widget_root {
            let event = ElementEvent::PointerMove(app.cursor_pos);
            let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &event);
            #[cfg(debug_assertions)]
            if app.inspector.is_enabled() {
                handled = true;
            }
            if handled {
                if let Some(window) = &app.window {
                    window.request_redraw();
                }
            }
        }
    }

    fn handle_mouse_input(state: ElementState, button: MouseButton, app: &mut AimerApplicationHandler) {
        if button != MouseButton::Left {
            return;
        }

        let c = app.cursor_pos;
        let event = if state.is_pressed() { ElementEvent::PointerDown(c) } else { ElementEvent::PointerUp(c) };
        #[allow(clippy::collapsible_if)]
        if let Some(root) = &app.widget_root {
            let mut handled = dispatch_event(root.as_ref(), c, &event);
            #[cfg(debug_assertions)]
            if app.inspector.is_enabled() {
                handled = true;
            }
            if !handled {
                if matches!(&event, ElementEvent::PointerDown(_)) {
                    broadcast_event(root.as_ref(), &event);
                }
            }
            if let Some(window) = &app.window {
                window.request_redraw();
            }
        }
    }

    fn handle_keyboard_input(event: KeyEvent, app: &mut AimerApplicationHandler) {
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

        let modifiers = app.current_modifiers.clone();

        if modifiers.ctrl || modifiers.meta {
            use winit::keyboard::KeyCode;
            use winit::keyboard::PhysicalKey;
            let named = match event.physical_key {
                PhysicalKey::Code(KeyCode::KeyA) => Some(NamedKey::Other("a".into())),
                PhysicalKey::Code(KeyCode::KeyC) => Some(NamedKey::Other("c".into())),
                PhysicalKey::Code(KeyCode::KeyV) => Some(NamedKey::Other("v".into())),
                PhysicalKey::Code(KeyCode::KeyX) => Some(NamedKey::Other("x".into())),
                _ => None,
            };
            if let Some(key) = named {
                let ev = ElementEvent::KeyInput { key, action, modifiers };
                if let Some(root) = &app.widget_root {
                    let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &ev);
                    #[cfg(debug_assertions)]
                    if app.inspector.is_enabled() {
                        handled = true;
                    }
                    if handled {
                        if let Some(window) = &app.window {
                            window.request_redraw();
                        }
                    }
                }
                return;
            }
        }

        if let Key::Character(ref ch) = event.logical_key {
            if !ch.is_empty() && ch.chars().all(|c| !c.is_control()) {
                let ev = ElementEvent::CharInput {
                    ch: ch.parse().unwrap(),
                    action: action.clone(),
                    modifiers: modifiers.clone(),
                };
                if let Some(root) = &app.widget_root {
                    let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &ev);
                    #[cfg(debug_assertions)]
                    if app.inspector.is_enabled() {
                        handled = true;
                    }
                    if handled {
                        if let Some(window) = &app.window {
                            window.request_redraw();
                        }
                    }
                }
                return;
            }
        }

        // Handle space as text input when it arrives as a named key
        if let Key::Named(WinitNamedKey::Space) = event.logical_key {
            let ev = ElementEvent::CharInput { ch: ' ', action: action.clone(), modifiers: modifiers.clone() };
            if let Some(root) = &app.widget_root {
                let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &ev);
                #[cfg(debug_assertions)]
                if app.inspector.is_enabled() {
                    handled = true;
                }
                if handled {
                    if let Some(window) = &app.window {
                        window.request_redraw();
                    }
                }
            }
            return;
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
            let ev = ElementEvent::KeyInput { key, action, modifiers: modifiers.clone() };
            if let Some(root) = &app.widget_root {
                let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &ev);
                #[cfg(debug_assertions)]
                if app.inspector.is_enabled() {
                    handled = true;
                }
                if handled {
                    if let Some(window) = &app.window {
                        window.request_redraw();
                    }
                }
            }
        }
    }

    fn handle_mouse_wheel(delta: MouseScrollDelta, app: &mut AimerApplicationHandler) {
        let scroll_delta = match delta {
            MouseScrollDelta::LineDelta(x, y) => {
                Vec2d { x: x as crate::handler::Float * 30.0, y: y as crate::handler::Float * 30.0 }
            }
            MouseScrollDelta::PixelDelta(pos) => {
                Vec2d { x: pos.x as crate::handler::Float, y: pos.y as crate::handler::Float }
            }
        };
        let event = ElementEvent::Scroll(scroll_delta);
        if let Some(root) = &app.widget_root {
            let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &event);
            #[cfg(debug_assertions)]
            if app.inspector.is_enabled() {
                handled = true;
            }
            if handled {
                if let Some(window) = &app.window {
                    window.request_redraw();
                }
            }
        }
    }

    fn handle_resize(size: PhysicalSize<u32>, app: &mut AimerApplicationHandler) {
        #[cfg(target_os = "ios")]
        let size = {
            let is_portrait = size.width < size.height;
            match crate::ios_screen::get_screen_resolution_pixels() {
                Some((width, height)) => {
                    app.native_window_size =
                        Some(attribute::size::ResolvedSize { width: width as f32, height: height as f32 });
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

        #[cfg(target_os = "android")]
        let size = {
            if let Some(android_app) = crate::aimer_app::ANDROID_APP.get() {
                if let Some(window) = android_app.native_window() {
                    let width = window.width() as u32;
                    let height = window.height() as u32;
                    winit::dpi::PhysicalSize::new(width, height)
                } else {
                    size
                }
            } else {
                size
            }
        };

        debug!("Window resized to {:?}", size);

        app.pending_resize = Some(size);
        if let Some(root) = &app.widget_root {
            root.invalidate_layout();
        }
        if let Some(window) = &app.window {
            window.request_redraw();
        }
    }
}
