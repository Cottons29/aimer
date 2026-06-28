use crate::handler::AimerApplicationHandler;
use aimer_attribute::position::Vec2d;
use aimer_events::element::KeyAction;
use aimer_events::element::{ElementEvent, Modifiers, NamedKey};
use aimer_events::pointer::PointerSource;
use aimer_utils::{ExecTimes, info};
use aimer_widget::{broadcast_event, dispatch_event};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

pub struct WindowEventHandler;

impl WindowEventHandler {
    pub(crate) fn handle_events(app: &mut AimerApplicationHandler, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                #[cfg(target_os = "macos")]
                {
                    use winit::platform::macos::ActiveEventLoopExtMacOS;
                    event_loop.hide_application();
                }
                #[cfg(not(target_os = "macos"))]
                event_loop.exit()
            }

            WindowEvent::Touch(item) => Self::handle_touch(item, app, _id, event),

            WindowEvent::CursorMoved { position, .. } => Self::handle_cursor_move(position, app),

            WindowEvent::MouseInput { state, button, .. } => Self::handle_mouse_input(state, button, app),

            WindowEvent::ModifiersChanged(mods) => {
                let state = mods.state();
                app.current_modifiers =
                    Modifiers { ctrl: state.control_key(), shift: state.shift_key(), alt: state.alt_key(), meta: state.super_key() };
            }

            WindowEvent::KeyboardInput { event, .. } => Self::handle_keyboard_input(event, app),

            WindowEvent::Ime(ime) => Self::handle_ime(ime, app),

            WindowEvent::MouseWheel { delta, phase, .. } =>  Self::handle_mouse_wheel(delta, phase, app),

            WindowEvent::RedrawRequested => {
                #[cfg(debug_assertions)]
                ExecTimes::no_param("MainAppRenderer", || app.render(event_loop));
                #[cfg(not(debug_assertions))]
                app.render(event_loop);
            }

            WindowEvent::Resized(size) =>
            {
                #[cfg(not(target_os = "ios"))]
                Self::handle_resize(size, app, event_loop)
            }

            _ => (),
        }
    }

    fn handle_touch(item: Touch, app: &mut AimerApplicationHandler, _id: WindowId, _event: WindowEvent) {
        let scale = app.window_scale;
        let pos = Vec2d { x: (item.location.x / scale) as f32, y: (item.location.y / scale) as f32 };
        let touch_id = item.id;

        // All touch events are passed through with their finger ID.
        // Individual widgets (scrollable, gesture detector) decide which
        // fingers to track — the scrollable keeps its own primary-finger
        // filter so a second touch doesn't jump the scroll position.

        let event = match item.phase {
            TouchPhase::Started => ElementEvent::PointerDown(pos, PointerSource::Touch, touch_id),
            TouchPhase::Moved => ElementEvent::PointerMove(pos, PointerSource::Touch, touch_id),
            TouchPhase::Ended => ElementEvent::PointerUp(pos, PointerSource::Touch, touch_id),
            TouchPhase::Cancelled => ElementEvent::Cancel,
        };
        #[allow(clippy::collapsible_if)]
        {
            if let Some(root) = &app.widget_root {
                let mut handled = dispatch_event(root.as_ref(), pos, &event);
                #[cfg(debug_assertions)]
                if app.inspector.is_enabled() {
                    handled = true;
                }
                if !handled {
                    // Broadcast PointerUp/Cancel alongside PointerDown so that
                    // elements with an active drag (e.g. scrollable fling) receive
                    // the release event even when the finger lifts outside their
                    // bounds — the common case for a fast flick on touch screens.
                    if matches!(&event, ElementEvent::PointerDown(_, _, _) | ElementEvent::PointerUp(_, _, _) | ElementEvent::Cancel) {
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
        let scale = app.window_scale as f32;
        let new_pos = Vec2d { x: position.x as f32 / scale, y: position.y as f32 / scale };
        let dx = (new_pos.x - app.cursor_pos.x).abs();
        let dy = (new_pos.y - app.cursor_pos.y).abs();
        if dx < 1.0 && dy < 1.0 {
            return;
        }
        app.cursor_pos = new_pos;
        if let Some(root) = &app.widget_root {
            let event = ElementEvent::PointerMove(app.cursor_pos, PointerSource::Mouse, 0);
            let _handled = dispatch_event(root.as_ref(), app.cursor_pos, &event);
            if let Some(window) = &app.window {
                window.request_redraw();
            }
        }
    }

    fn handle_mouse_input(state: ElementState, button: MouseButton, app: &mut AimerApplicationHandler) {
        // Only handle left and right mouse buttons here.
        // Middle button and others are ignored for now.
        if !matches!(button, MouseButton::Left | MouseButton::Right) {
            return;
        }

        let c = app.cursor_pos;
        let event = if button == MouseButton::Right {
            // Right-click: only fire on press, not release.
            if state.is_pressed() {
                ElementEvent::PointerDown(c, PointerSource::Mouse, 0)
            } else {
                ElementEvent::PointerUp(c, PointerSource::Mouse, 0)
            }
        } else if state.is_pressed() {
            ElementEvent::PointerDown(c, PointerSource::Mouse, 0)
        } else {
            ElementEvent::PointerUp(c, PointerSource::Mouse, 0)
        };
        #[allow(clippy::collapsible_if)]
        if let Some(root) = &app.widget_root {
            let mut handled = dispatch_event(root.as_ref(), c, &event);
            #[cfg(debug_assertions)]
            if app.inspector.is_enabled() {
                handled = true;
            }
            if !handled {
                if matches!(&event, ElementEvent::PointerDown(_, _, _) | ElementEvent::PointerUp(_, _, _) | ElementEvent::Cancel) {
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
                    if let Some(window) = &app.window
                        && handled
                    {
                        window.request_redraw();
                    }
                }
                return;
            }
        }

        // While an IME composition is in progress the raw key strokes belong to
        // the input method (e.g. pinyin/romaji letters building up a candidate).
        // The composed result is delivered separately via `WindowEvent::Ime`, so
        // we must not also treat these keys as text or navigation input.
        if app.ime_composing {
            return;
        }

        // Resolve the textual payload of this key, if any.
        //
        // `event.text` is the source of truth for committed text on every native
        // winit backend. Crucially winit leaves it `None` for keystrokes that the
        // IME consumed — composition letters, candidate-confirm keys, and (on
        // macOS) even plain characters while IME is enabled, which instead arrive
        // via `WindowEvent::Ime(Ime::Commit(..))`. Relying solely on `event.text`
        // therefore guarantees each character is inserted exactly once, with no
        // double-typing and no stray space after confirming a CJK candidate.
        //
        // The web backend has no winit IME events; its synthetic key events carry
        // the character only in `logical_key`, so fall back to that there.
        // Multi-codepoint payloads (e.g. a committed CJK phrase) are dispatched
        // one `char` at a time instead of panicking on `parse::<char>()`.
        let text_input: Option<String> = match &event.text {
            Some(t) => Some(t.to_string()),
            #[cfg(target_arch = "wasm32")]
            None => match &event.logical_key {
                Key::Character(ch) => Some(ch.to_string()),
                _ => None,
            },
            #[cfg(not(target_arch = "wasm32"))]
            None => None,
        };

        if let Some(text) = text_input
            && !text.is_empty()
            && text.chars().all(|c| !c.is_control())
        {
            Self::dispatch_text(&text, &action, &modifiers, app);
            return;
        }

        // On the web backend, space is delivered as a named key without any
        // `event.text`, so handle it explicitly. On native platforms a real
        // space arrives through `event.text` above; the named `Space` here only
        // appears as an IME confirm key, which must NOT insert a space.
        #[cfg(target_arch = "wasm32")]
        if let Key::Named(WinitNamedKey::Space) = event.logical_key {
            Self::dispatch_text(" ", &action, &modifiers, app);
            return;
        }

        // Handle named keys
        if let Key::Named(named) = &event.logical_key {
            let key = match named {
                WinitNamedKey::Backspace => NamedKey::Backspace,
                WinitNamedKey::Delete => NamedKey::Delete,
                WinitNamedKey::ArrowUp => NamedKey::ArrowUp,
                WinitNamedKey::ArrowDown => NamedKey::ArrowDown,
                WinitNamedKey::ArrowLeft => NamedKey::ArrowLeft,
                WinitNamedKey::ArrowRight => NamedKey::ArrowRight,
                WinitNamedKey::PageUp => NamedKey::PageUp,
                WinitNamedKey::PageDown => NamedKey::PageDown,
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
                if let Some(window) = &app.window
                    && handled
                {
                    window.request_redraw();
                }
            }
        }
    }

    /// Dispatches a (possibly multi-character) text payload to the widget tree
    /// as a sequence of `CharInput` events — one per `char`. This is the single
    /// path used for plain typed characters, web text input, and committed IME
    /// text, so CJK phrases and emoji are inserted correctly.
    fn dispatch_text(text: &str, action: &KeyAction, modifiers: &Modifiers, app: &mut AimerApplicationHandler) {
        let Some(root) = &app.widget_root else { return };
        let mut handled = false;
        for ch in text.chars() {
            let ev = ElementEvent::CharInput { ch, action: action.clone(), modifiers: modifiers.clone() };
            handled |= dispatch_event(root.as_ref(), app.cursor_pos, &ev);
        }
        #[cfg(debug_assertions)]
        if app.inspector.is_enabled() {
            handled = true;
        }
        if let Some(window) = &app.window
            && handled
        {
            window.request_redraw();
        }
    }

    /// Handles input-method (IME) events so that languages requiring
    /// composition — Chinese, Japanese, Korean, etc. — can be typed.
    ///
    /// While a composition is active (`Ime::Preedit`) raw key strokes are
    /// suppressed in `handle_keyboard_input`; the finished text arrives through
    /// `Ime::Commit` and is inserted via the normal text path.
    fn handle_ime(ime: Ime, app: &mut AimerApplicationHandler) {
        info!("IME : {ime:?}");
        match ime {
            Ime::Enabled => {
                app.ime_composing = false;
            }
            Ime::Preedit(text, cursor) => {
                app.ime_composing = !text.is_empty();
                // Forward preedit to focused widget for composition rendering
                if let Some(root) = &app.widget_root {
                    let event = ElementEvent::ImePreedit {
                        text: text.clone(),
                        cursor: cursor.clone(),
                    };
                    dispatch_event(root.as_ref(), app.cursor_pos, &event);
                }
                if let Some(window) = &app.window {
                    window.request_redraw();
                }
            }
            Ime::Commit(text) => {
                app.ime_composing = false;
                let modifiers = app.current_modifiers.clone();
                Self::dispatch_text(&text, &KeyAction::Pressed, &modifiers, app);
            }
            Ime::Disabled => {
                app.ime_composing = false;
            }
        }
    }

    fn handle_mouse_wheel(delta: MouseScrollDelta, phase: TouchPhase, app: &mut AimerApplicationHandler) {
        // debug!("Mouse wheel delta: {:?}", delta);
        let scroll_delta = match delta {
            MouseScrollDelta::LineDelta(x, y) => Vec2d { x: x * 20.0, y: y * 20.0 },
            // Scale trackpad (PixelDelta) down for more resistance / less sensitivity.
            MouseScrollDelta::PixelDelta(pos) => Vec2d { x: pos.x as f32 * 0.85, y: pos.y as f32 * 0.85 },
        };

        let event = ElementEvent::Scroll { delta: scroll_delta, phase };
        if let Some(root) = &app.widget_root {
            let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &event);
            #[cfg(debug_assertions)]
            if app.inspector.is_enabled() {
                handled = true;
            }

            if let Some(window) = &app.window
                && handled
            {
                window.request_redraw();
            }
        }
    }

    fn handle_resize(size: PhysicalSize<u32>, app: &mut AimerApplicationHandler, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "ios")]
        aimer_utils::debug!("iOS handle_resize raw size: {size:?}");
        #[cfg(target_os = "ios")]
        let size = {
            use aimer_attribute::ResolvedSize;
            let is_portrait = size.width < size.height;
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
            };
            PhysicalSize::new(1200, 2000)
        };

        #[cfg(target_os = "ios")]
        aimer_utils::debug!("iOS handle_resize modified size: {size:?}");

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

        // debug!("Window resized to {:?}", size);

        app.pending_resize = Some(size);

        if let Some(root) = &app.widget_root {
            root.invalidate_layout();
        }

        // Render a frame immediately during the resize event so the
        // compositor has fresh content before it can stretch the old
        // drawable.  Without this synchronous render the compositor
        // (WindowServer on macOS) stretches the previous frame to the
        // new window size — visible as directional stretching when
        // dragging the right or bottom window edge.
        app.render(event_loop);
    }
}
