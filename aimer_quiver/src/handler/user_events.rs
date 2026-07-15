use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_widget::{Widget, dispatch_event};

use crate::aimer_app::AimerCustomAppEvent;
use crate::handler::AimerApplicationHandler;

pub(crate) fn handle_user_event<W: Widget + 'static>(
    app: &mut AimerApplicationHandler<W>,
    event: AimerCustomAppEvent,
) {
    match event {
        AimerCustomAppEvent::ForceBackspace => {
            if let Some(root) = &app.widget_root {
                let ev = ElementEvent::KeyInput {
                    key: NamedKey::Backspace,
                    action: KeyAction::Pressed,
                    modifiers: Default::default(),
                };
                let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &ev);
                #[cfg(debug_assertions)]
                if app.inspector_enabled() {
                    handled = true;
                }
                if let Some(window) = &app.window
                    && handled
                {
                    window.request_redraw();
                }
            }
        }
        AimerCustomAppEvent::InsertText(text) => {
            if let Some(root) = &app.widget_root {
                let mut handled_any = false;
                for ch in text.chars() {
                    let ev = ElementEvent::CharInput {
                        ch,
                        action: KeyAction::Pressed,
                        modifiers: Default::default(),
                    };
                    let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &ev);
                    #[cfg(debug_assertions)]
                    if app.inspector_enabled() {
                        handled = true;
                    }
                    handled_any |= handled;
                }
                if let Some(window) = &app.window
                    && handled_any
                {
                    window.request_redraw();
                }
            }
        }
        AimerCustomAppEvent::FrameReady => {
            if let Some(window) = &app.window {
                // A single on-demand present is not reliably composited on
                // macOS — the same reason the app renders a `start_up_frames`
                // burst at launch instead of one frame. A runtime state change
                // (`set_state` -> `request_animation_frame`) routes here, so a
                // lone `request_redraw()` renders the new frame but can leave it
                // sitting in the swapchain while the display keeps showing the
                // previous frame (e.g. a counter stuck on its old value even
                // though the state already advanced).
                //
                // Drive a short settle burst so `about_to_wait` keeps pumping
                // redraws until every swapchain drawable holds the latest frame
                // (`desired_maximum_frame_latency + 1` = 3 drawables). This is
                // the same, proven mechanism used at startup, scoped to a couple
                // of frames so the app still returns to idle immediately after.
                const SETTLE_FRAMES: u8 = 3;
                app.start_up_frames.set(
                    app.start_up_frames
                        .get()
                        .max(SETTLE_FRAMES),
                );
                window.request_redraw();
            }
        }
    }
}
