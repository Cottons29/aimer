use crate::handler::AimerApplicationHandler;
use aimer_events::element::{ElementEvent, KeyAction, NamedKey};
use aimer_widget::dispatch_event;

pub(crate) fn handle_user_event(app: &mut AimerApplicationHandler, event: crate::aimer_app::AimerCustomAppEvent) {
    match event {
        crate::aimer_app::AimerCustomAppEvent::ForceBackspace => {
            if let Some(root) = &app.widget_root {
                let ev = ElementEvent::KeyInput { key: NamedKey::Backspace, action: KeyAction::Pressed, modifiers: Default::default() };
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
        crate::aimer_app::AimerCustomAppEvent::InsertText(text) => {
            if let Some(root) = &app.widget_root {
                let mut handled_any = false;
                for ch in text.chars() {
                    let ev = ElementEvent::CharInput { ch, action: KeyAction::Pressed, modifiers: Default::default() };
                    let mut handled = dispatch_event(root.as_ref(), app.cursor_pos, &ev);
                    #[cfg(debug_assertions)]
                    if app.inspector.is_enabled() {
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
    }
}
