use crate::render::OxidizeAppConfiguration;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use utils::debug;
use widget::{ElementEvent, dispatch_event};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

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

        WindowEvent::KeyboardInput { device_id, event, is_synthetic } => {
            debug!("KeyboardInput: {:?}", event);
            debug!("KeyboardInput: {:?}", is_synthetic);
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
