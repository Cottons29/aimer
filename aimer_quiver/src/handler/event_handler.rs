use aimer_attribute::position::Vec2d;
use aimer_events::element::{ElementEvent, KeyAction, Modifiers, NamedKey};
use aimer_events::pointer::PointerSource;
use aimer_utils::{ExecTimes, info};
use aimer_widget::{EventResult, PointerKey, Widget, broadcast_event};
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, Touch, TouchPhase, WindowEvent,
};
use winit::event_loop::ActiveEventLoop;
use winit::window::{CursorIcon, WindowId};

use crate::handler::AimerApplicationHandler;

pub struct WindowEventHandler;

pub(crate) const CURSOR_OUTSIDE_POSITION: Vec2d = Vec2d {
    x: f32::MIN,
    y: f32::MIN,
};

pub(crate) enum HeadlessEventAction {
    None,
    Render,
    Exit,
}

impl WindowEventHandler {
    #[inline]
    fn should_redraw(result: EventResult, legacy_handled: bool) -> bool {
        legacy_handled || result.needs_redraw()
    }

    pub(crate) fn handle_events<W: Widget + 'static>(
        app: &mut AimerApplicationHandler<W>,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        // debug!("======> Event: {event:?}");
        match event {
            WindowEvent::CloseRequested => {
                // #[cfg(target_os = "macos")]
                // {
                //     use winit::platform::macos::ActiveEventLoopExtMacOS;
                //     event_loop.hide_application();
                // }
                // #[cfg(not(target_os = "macos"))]
                event_loop.exit()
            }

            WindowEvent::Touch(item) => Self::handle_touch(item, app),

            WindowEvent::CursorMoved { position, .. } => Self::handle_cursor_move(position, app),

            WindowEvent::CursorLeft { .. } => Self::handle_cursor_left(app),

            WindowEvent::CursorEntered { .. } => Self::handle_cursor_entered(app),

            WindowEvent::MouseInput { state, button, .. } => {
                Self::handle_mouse_input(state, button, app)
            }

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

            WindowEvent::Ime(ime) => Self::handle_ime(ime, app),

            WindowEvent::MouseWheel { delta, phase, .. } => {
                Self::handle_mouse_wheel(delta, phase, app);
            }

            WindowEvent::RedrawRequested => {
                #[cfg(debug_assertions)]
                ExecTimes::no_param("MainAppRenderer", || app.render(event_loop));
                #[cfg(not(debug_assertions))]
                app.render(event_loop);
            }

            WindowEvent::Resized(size) => Self::handle_resize(size, app, event_loop),

            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                Self::update_scale_factor(&mut app.window_scale, scale_factor);
                if let Some(root) = &app.widget_root {
                    root.invalidate_layout();
                    aimer_widget::Rebuildable::mark_needs_rebuild(root.as_ref());
                }
                if let Some(window) = app.window {
                    window.request_redraw();
                }
            }

            WindowEvent::Focused(is_focus) => {
                if app.widget_root.is_none() {
                    return;
                }
                if is_focus {
                    let result = app.cancel_element_events();
                    if let Some(window) = &app.window
                        && result.needs_redraw()
                    {
                        window.request_redraw();
                    }
                }
            }

            _ => (),
        }
    }

    pub(crate) fn handle_headless_event<W: Widget + 'static>(
        app: &mut AimerApplicationHandler<W>,
        event: WindowEvent,
    ) -> HeadlessEventAction {
        match event {
            WindowEvent::CloseRequested => HeadlessEventAction::Exit,
            WindowEvent::Touch(item) => {
                Self::handle_touch(item, app);
                HeadlessEventAction::None
            }
            WindowEvent::CursorMoved { position, .. } => {
                Self::handle_cursor_move(position, app);
                HeadlessEventAction::None
            }
            WindowEvent::CursorLeft { .. } => {
                Self::handle_cursor_left(app);
                HeadlessEventAction::None
            }
            WindowEvent::CursorEntered { .. } => {
                Self::handle_cursor_entered(app);
                HeadlessEventAction::None
            }
            WindowEvent::MouseInput { state, button, .. } => {
                Self::handle_mouse_input(state, button, app);
                HeadlessEventAction::None
            }
            WindowEvent::ModifiersChanged(mods) => {
                let state = mods.state();
                app.current_modifiers = Modifiers {
                    ctrl: state.control_key(),
                    shift: state.shift_key(),
                    alt: state.alt_key(),
                    meta: state.super_key(),
                };
                HeadlessEventAction::None
            }
            WindowEvent::KeyboardInput { event, .. } => {
                Self::handle_keyboard_input(event, app);
                HeadlessEventAction::None
            }
            WindowEvent::Ime(ime) => {
                Self::handle_ime(ime, app);
                HeadlessEventAction::None
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                Self::handle_mouse_wheel(delta, phase, app);
                HeadlessEventAction::None
            }
            WindowEvent::RedrawRequested => HeadlessEventAction::Render,
            WindowEvent::Resized(size) => {
                Self::apply_resize(size, app);
                HeadlessEventAction::Render
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                Self::update_scale_factor(&mut app.window_scale, scale_factor);
                if let Some(root) = &app.widget_root {
                    root.invalidate_layout();
                    aimer_widget::Rebuildable::mark_needs_rebuild(root.as_ref());
                }
                HeadlessEventAction::Render
            }
            WindowEvent::Focused(false) => {
                if app.widget_root.is_some() {
                    let result = app.cancel_element_events();
                    if result.needs_redraw() {
                        return HeadlessEventAction::Render;
                    }
                }
                HeadlessEventAction::None
            }
            _ => HeadlessEventAction::None,
        }
    }

    fn update_scale_factor(window_scale: &mut f64, scale_factor: f64) {
        *window_scale = scale_factor;
    }

    fn handle_touch<W: Widget + 'static>(item: Touch, app: &mut AimerApplicationHandler<W>) {
        let scale = app.window_scale;
        let pos = Vec2d {
            x: (item.location.x / scale) as f32,
            y: (item.location.y / scale) as f32,
        };
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
            if app.widget_root.is_some() {
                let mut result = app.dispatch_element_event(pos, &event);
                #[cfg(debug_assertions)]
                let inspector_handled = app.inspector_enabled();
                #[cfg(not(debug_assertions))]
                let inspector_handled = false;
                if !result.is_consumed() && !inspector_handled {
                    // Broadcast PointerUp/Cancel alongside PointerDown so that
                    // elements with an active drag (e.g. scrollable fling) receive
                    // the release event even when the finger lifts outside their
                    // bounds — the common case for a fast flick on touch screens.
                    if matches!(&event, ElementEvent::PointerDown(_, _, _))
                        && let Some(root) = &app.widget_root
                    {
                        result = result.merge(broadcast_event(root.as_ref(), &event));
                    }
                }
                if let Some(window) = &app.window
                    && Self::should_redraw(result, true)
                {
                    window.request_redraw();
                }
            }
        }
    }

    fn handle_cursor_move<W: Widget + 'static>(
        position: PhysicalPosition<f64>,
        app: &mut AimerApplicationHandler<W>,
    ) {
        let scale = app.window_scale as f32;
        let new_pos = Vec2d {
            x: position.x as f32 / scale,
            y: position.y as f32 / scale,
        };
        let dx = (new_pos.x - app.cursor_pos.x).abs();
        let dy = (new_pos.y - app.cursor_pos.y).abs();
        if dx < 1.0 && dy < 1.0 {
            return;
        }
        app.cursor_pos = new_pos;
        if app.widget_root.is_some() {
            let event = ElementEvent::PointerMove(app.cursor_pos, PointerSource::Mouse, 0);
            let result = app.dispatch_element_event(app.cursor_pos, &event);
            if let Some(window) = &app.window {
                if !result.is_consumed() {
                    window.set_cursor(CursorIcon::Default);
                }
                if Self::should_redraw(result, result.is_consumed()) {
                    window.request_redraw();
                }
            }
        }
    }

    fn handle_cursor_left<W: Widget + 'static>(app: &mut AimerApplicationHandler<W>) {
        app.cursor_pos = CURSOR_OUTSIDE_POSITION;
        if app.widget_root.is_some() {
            let pointer = PointerKey::new(PointerSource::Mouse, 0);
            let was_captured = app.event_dispatcher.is_captured(pointer);
            let event = ElementEvent::PointerExited(pointer.source, pointer.id);
            let mut result = app.dispatch_element_event(app.cursor_pos, &event);
            if !was_captured && let Some(root) = &app.widget_root {
                result = result.merge(broadcast_event(root.as_ref(), &event));
            }
            if let Some(window) = &app.window
                && Self::should_redraw(result, true)
            {
                window.request_redraw();
            }
        }
    }

    fn handle_cursor_entered<W: Widget + 'static>(app: &mut AimerApplicationHandler<W>) {
        // CursorEntered carries no coordinates. Keep hover disabled until the
        // following CursorMoved supplies a valid position.
        app.cursor_pos = CURSOR_OUTSIDE_POSITION;
        if let Some(window) = &app.window {
            window.request_redraw();
        }
    }

    fn handle_mouse_input<W: Widget + 'static>(
        state: ElementState,
        button: MouseButton,
        app: &mut AimerApplicationHandler<W>,
    ) {
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
        if app.widget_root.is_some() {
            let mut result = app.dispatch_element_event(c, &event);
            #[cfg(debug_assertions)]
            let inspector_handled = app.inspector_enabled();
            #[cfg(not(debug_assertions))]
            let inspector_handled = false;
            if !result.is_consumed() && !inspector_handled {
                if matches!(&event, ElementEvent::PointerDown(_, _, _))
                    && let Some(root) = &app.widget_root
                {
                    result = result.merge(broadcast_event(root.as_ref(), &event));
                }
            }
            if let Some(window) = &app.window
                && Self::should_redraw(result, true)
            {
                window.request_redraw();
            }
        }
    }

    fn handle_keyboard_input<W: Widget + 'static>(
        event: KeyEvent,
        app: &mut AimerApplicationHandler<W>,
    ) {
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
            use winit::keyboard::{KeyCode, PhysicalKey};
            let named = match event.physical_key {
                PhysicalKey::Code(KeyCode::KeyA) => Some(NamedKey::Other("a".into())),
                PhysicalKey::Code(KeyCode::KeyC) => Some(NamedKey::Other("c".into())),
                PhysicalKey::Code(KeyCode::KeyV) => Some(NamedKey::Other("v".into())),
                PhysicalKey::Code(KeyCode::KeyX) => Some(NamedKey::Other("x".into())),
                _ => None,
            };
            if let Some(key) = named {
                let ev = ElementEvent::KeyInput {
                    key,
                    action,
                    modifiers,
                };
                if app.widget_root.is_some() {
                    let result = app.dispatch_element_event(app.cursor_pos, &ev);
                    let mut handled = result.is_consumed();
                    #[cfg(debug_assertions)]
                    if app.inspector_enabled() {
                        handled = true;
                    }
                    if let Some(window) = &app.window
                        && Self::should_redraw(result, handled)
                    {
                        window.request_redraw();
                    }
                }
                return;
            }
        }

        if app.ime_composing {
            return;
        }

        let text_input: Option<String> = match &event.text {
            Some(t) => Some(t.to_string()),
            #[cfg(target_arch = "wasm32")]
            None => match &event.logical_key {
                Key::Character(ch) => Some(ch.to_string()),
                _ => None,
            },
            #[cfg(not(target_arch = "wasm32"))]
            _ => None,
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
            let ev = ElementEvent::KeyInput {
                key,
                action,
                modifiers: modifiers.clone(),
            };
            if app.widget_root.is_some() {
                let result = app.dispatch_element_event(app.cursor_pos, &ev);
                let mut handled = result.is_consumed();
                #[cfg(debug_assertions)]
                if app.inspector_enabled() {
                    handled = true;
                }
                if let Some(window) = &app.window
                    && Self::should_redraw(result, handled)
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
    fn dispatch_text<W: Widget + 'static>(
        text: &str,
        action: &KeyAction,
        modifiers: &Modifiers,
        app: &mut AimerApplicationHandler<W>,
    ) {
        if app.widget_root.is_none() {
            return;
        }
        let mut result = EventResult::ignored();
        for ch in text.chars() {
            let ev = ElementEvent::CharInput {
                ch,
                action: action.clone(),
                modifiers: modifiers.clone(),
            };
            result = result.merge(app.dispatch_element_event(app.cursor_pos, &ev));
        }
        let mut handled = result.is_consumed();
        #[cfg(debug_assertions)]
        if app.inspector_enabled() {
            handled = true;
        }
        if let Some(window) = &app.window
            && Self::should_redraw(result, handled)
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
    fn handle_ime<W: Widget + 'static>(ime: Ime, app: &mut AimerApplicationHandler<W>) {
        info!("IME : {ime:?}");
        match ime {
            Ime::Enabled => {
                app.ime_composing = false;
            }
            Ime::Preedit(text, cursor) => {
                app.ime_composing = !text.is_empty();
                // Forward preedit to focused widget for composition rendering
                if app.widget_root.is_some() {
                    let event = ElementEvent::ImePreedit {
                        text: text.clone(),
                        cursor,
                    };
                    let result = app.dispatch_element_event(app.cursor_pos, &event);
                    if let Some(window) = &app.window
                        && Self::should_redraw(result, true)
                    {
                        window.request_redraw();
                    }
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

    pub fn handle_mouse_wheel<W: Widget + 'static>(
        delta: MouseScrollDelta,
        _phase: TouchPhase,
        app: &mut AimerApplicationHandler<W>,
    ) {
        let (scroll_delta, kind) = Self::normalize_wheel_delta(delta, app.window_scale);
        if app.widget_root.is_some() {
            let delta = PhysicalPosition::new(scroll_delta.x as f64, scroll_delta.y as f64);
            match kind {
                aimer_events::element::ScrollDeltaKind::Pixel => {
                    app.scroll_smoother.on_pixel_delta(delta);
                }
                aimer_events::element::ScrollDeltaKind::Line => {
                    app.scroll_smoother.on_wheel_delta(delta);
                }
            }

            if let Some(window) = &app.window {
                window.request_redraw();
            }
        }
    }

    pub(crate) const SCROLL_MULTIPLIER : f32 = 1.5;

    #[inline]
    fn normalize_wheel_delta(
        delta: MouseScrollDelta,
        scale_factor: f64,
    ) -> (Vec2d, aimer_events::element::ScrollDeltaKind) {
        match delta {
            MouseScrollDelta::LineDelta(x, y) => (
                Vec2d {
                    x: x * 20.0,
                    y: y * 20.0,
                },
                aimer_events::element::ScrollDeltaKind::Line,
            ),
            MouseScrollDelta::PixelDelta(pos) => {
                let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
                    scale_factor
                } else {
                    1.0
                };
                (
                    Vec2d {
                        x: (pos.x / scale) as f32 * Self::SCROLL_MULTIPLIER,
                        y: (pos.y / scale) as f32 * Self::SCROLL_MULTIPLIER,
                    },
                    aimer_events::element::ScrollDeltaKind::Pixel,
                )
            }
        }
    }

    #[cfg(any(test, target_os = "ios"))]
    fn oriented_screen_size(
        resize_size: PhysicalSize<u32>,
        screen_size: (f64, f64),
    ) -> PhysicalSize<u32> {
        let (width, height) = screen_size;
        if resize_size.width < resize_size.height {
            PhysicalSize::new(width as u32, height as u32)
        } else {
            PhysicalSize::new(height as u32, width as u32)
        }
    }

    fn handle_resize<W: Widget + 'static>(
        size: PhysicalSize<u32>,
        app: &mut AimerApplicationHandler<W>,
        event_loop: &ActiveEventLoop,
    ) {
        #[cfg(target_os = "ios")]
        aimer_utils::debug!("iOS handle_resize raw size: {size:?}");
        #[cfg(target_os = "ios")]
        let size = {
            use aimer_attribute::ResolvedSize;
            match crate::ios_screen::get_screen_resolution_pixels() {
                Some((width, height)) => {
                    app.native_window_size = Some(ResolvedSize {
                        width: width as f32,
                        height: height as f32,
                    });
                    Self::oriented_screen_size(size, (width, height))
                }
                None => {
                    if app.window.is_none() {
                        return;
                    }
                    app.window.unwrap().inner_size()
                }
            }
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

        Self::apply_resize(size, app);

        // Render a frame immediately during the resize event so the
        // compositor has fresh content before it can stretch the old
        // drawable.  Without this synchronous render the compositor
        // (WindowServer on macOS) stretches the previous frame to the
        // new window size — visible as directional stretching when
        // dragging the right or bottom window edge.
        app.render(event_loop);
    }

    fn apply_resize<W: Widget + 'static>(
        size: PhysicalSize<u32>,
        app: &mut AimerApplicationHandler<W>,
    ) {
        app.pending_resize = Some(size);

        if let Some(root) = &app.widget_root {
            root.invalidate_layout();
            aimer_widget::Rebuildable::mark_needs_rebuild(root.as_ref());
        }
    }
}

// impl WindowEventHandler {
//     const PIXELS_PER_LINE: f64 = 100.0; // total pixel distance per 1.0 line unit
//     const EXPAND_STEPS: usize = 16;     // how many sub-events to emit
//
//     /// Expands one LineDelta into a queue of synthetic PixelDelta events,
//     /// distributed over EXPAND_STEPS along an ease-out curve (fast start,
//     /// smooth taper to zero) — mimicking natural trackpad momentum.
//     fn expand_line_delta(x: f32, y: f32) -> VecDeque<PhysicalPosition<f64>> {
//         let target_x = x as f64 * Self::PIXELS_PER_LINE;
//         let target_y = y as f64 * Self::PIXELS_PER_LINE;
//
//         let mut queue = VecDeque::with_capacity(Self::EXPAND_STEPS);
//         let (mut prev_x, mut prev_y) = (0.0, 0.0);
//
//         for i in 1..=Self::EXPAND_STEPS {
//             let t = i as f64 / Self::EXPAND_STEPS as f64;
//             let eased = 1.0 - (1.0 - t).powi(3); // ease-out cubic
//
//             let pos_x = target_x * eased;
//             let pos_y = target_y * eased;
//
//             queue.push_back(PhysicalPosition::new(pos_x - prev_x, pos_y - prev_y));
//
//             prev_x = pos_x;
//             prev_y = pos_y;
//         }
//
//         queue
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ios_screen_size_follows_resize_orientation() {
        let screen_size = (1179.0, 2556.0);

        assert_eq!(
            WindowEventHandler::oriented_screen_size(PhysicalSize::new(390, 844), screen_size),
            PhysicalSize::new(1179, 2556),
        );
        assert_eq!(
            WindowEventHandler::oriented_screen_size(PhysicalSize::new(844, 390), screen_size),
            PhysicalSize::new(2556, 1179),
        );
    }

    #[test]
    fn scale_factor_change_updates_window_scale() {
        let mut window_scale = 1.0;

        WindowEventHandler::update_scale_factor(&mut window_scale, 2.0);

        assert_eq!(window_scale, 2.0);
    }

    #[test]
    fn event_result_redraws_for_legacy_handling_or_explicit_request() {
        assert!(WindowEventHandler::should_redraw(
            aimer_widget::EventResult::ignored(),
            true,
        ));
        assert!(WindowEventHandler::should_redraw(
            aimer_widget::EventResult::redraw(),
            false,
        ));
        assert!(!WindowEventHandler::should_redraw(
            aimer_widget::EventResult::ignored(),
            false,
        ));
    }

    #[test]
    fn precise_wheel_delta_preserves_logical_distance() {
        let (delta, kind) = WindowEventHandler::normalize_wheel_delta(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(24.0, -16.0)),
            2.0,
        );

        assert_eq!(delta.x, 18.0);
        assert_eq!(delta.y, -12.0);
        assert_eq!(kind, aimer_events::element::ScrollDeltaKind::Pixel);
    }

    #[test]
    fn line_wheel_delta_uses_stable_logical_step() {
        let (delta, kind) =
            WindowEventHandler::normalize_wheel_delta(MouseScrollDelta::LineDelta(1.0, -2.0), 2.0);

        assert_eq!(delta.x, 20.0);
        assert_eq!(delta.y, -40.0);
        assert_eq!(kind, aimer_events::element::ScrollDeltaKind::Line);
    }

    #[test]
    fn precise_wheel_delta_uses_safe_scale_fallback() {
        for scale in [0.0, f64::NAN, f64::INFINITY] {
            let (delta, _) = WindowEventHandler::normalize_wheel_delta(
                MouseScrollDelta::PixelDelta(PhysicalPosition::new(12.0, -8.0)),
                scale,
            );

            assert_eq!(delta.x, 12.0 * WindowEventHandler::SCROLL_MULTIPLIER);
            assert_eq!(delta.y, -8.0 *  WindowEventHandler::SCROLL_MULTIPLIER);
        }
    }
}
