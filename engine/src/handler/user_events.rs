use events::element::{ElementEvent, KeyAction, NamedKey};
use widget::dispatch_event;
use crate::handler::AimerApplicationHandler;

pub(crate) fn handle_user_event(app: &mut AimerApplicationHandler, event: crate::aimer_app::AimerCustomAppEvent) {
    match event {
        crate::aimer_app::AimerCustomAppEvent::ForceBackspace => {
            if let Some(root) = &app.widget_root {
                let ev = ElementEvent::KeyInput {
                    key: NamedKey::Backspace,
                    action: KeyAction::Pressed,
                };
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
        crate::aimer_app::AimerCustomAppEvent::InsertText(text) => {
            if let Some(root) = &app.widget_root {
                let mut handled_any = false;
                for ch in text.chars() {
                    let ev = ElementEvent::CharInput { ch, action: KeyAction::Pressed };
                    let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &ev);
                    #[cfg(debug_assertions)]
                    if app.inspector.is_enabled() {
                        handled = true;
                    }
                    handled_any |= handled;
                }

                if handled_any {
                    if let Some(window) = &app.window {
                        window.request_redraw();
                    }
                }
            }
        }
    }
}