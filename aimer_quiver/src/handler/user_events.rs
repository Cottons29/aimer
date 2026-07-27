use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_widget::{EventResult, Widget};

use crate::aimer_app::AimerCustomAppEvent;
use crate::handler::AimerApplicationHandler;

pub(crate) fn handle_user_event<W: Widget + 'static>(
    app: &mut AimerApplicationHandler<W>,
    event: AimerCustomAppEvent,
) {
    match event {
        AimerCustomAppEvent::ForceBackspace => {
            if app.widget_root.is_some() {
                let ev = ElementEvent::KeyInput {
                    key: NamedKey::Backspace,
                    action: KeyAction::Pressed,
                    modifiers: Default::default(),
                };
                let result = app.dispatch_element_event(app.cursor_pos, &ev);
                let mut handled = result.is_consumed();
                #[cfg(debug_assertions)]
                if app.inspector_enabled() {
                    handled = true;
                }
                if let Some(window) = &app.window
                    && (handled || result.needs_redraw())
                {
                    window.request_redraw();
                }
            }
        }
        AimerCustomAppEvent::InsertText(text) => {
            if app.widget_root.is_some() {
                let mut result = EventResult::ignored();
                for ch in text.chars() {
                    let ev = ElementEvent::CharInput {
                        ch,
                        action: KeyAction::Pressed,
                        modifiers: Default::default(),
                    };
                    result = result.merge(app.dispatch_element_event(app.cursor_pos, &ev));
                }
                let mut handled = result.is_consumed();
                #[cfg(debug_assertions)]
                if app.inspector_enabled() {
                    handled = true;
                }
                if let Some(window) = &app.window
                    && (handled || result.needs_redraw())
                {
                    window.request_redraw();
                }
            }
        }
        AimerCustomAppEvent::FrameReady => {
            crate::aimer_app::frame_ready_delivered();
            if let Some(window) = &app.window {
                // println!("FrameReady");
                // const SETTLE_FRAMES: u8 = 3;
                // app.start_up_frames.set(
                //     app.start_up_frames
                //         .get()
                //         .max(SETTLE_FRAMES),
                // );
                window.request_redraw();
            }
        }
    }
}
