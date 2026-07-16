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
                println!("FrameReady");
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
