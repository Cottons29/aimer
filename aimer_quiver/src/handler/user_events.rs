use aimer_events::element::{ElementEvent, KeyAction, Modifiers, NamedKey};
use aimer_widget::Widget;

use crate::aimer_app::AimerNativePlatformEvent;
use crate::handler::AimerApplicationHandler;
use crate::handler::event_handler::WindowEventHandler;


/// Delivers a platform event that did not come from the window itself.
///
/// The soft keyboard on iOS and Android, and the display link that drives
/// frames on iOS, all report through here rather than as window events. A
/// headless application drives the very same path, which is how a test can
/// inject text the way a platform keyboard does.
pub(crate) fn handle_user_event<W: Widget + 'static>(
    app: &mut AimerApplicationHandler<W>,
    event: AimerNativePlatformEvent,
) {
    match event {
        AimerNativePlatformEvent::ForceBackspace => {
            if app.active_root().is_some() {
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
        AimerNativePlatformEvent::InsertText(text) => {
            // Injected text is inserted as one edit, exactly like a committed
            // IME phrase, instead of one event per `char`.
            //
            // It *is* the commit on mobile, so it closes any composition the
            // keyboard had open; otherwise the next keystroke would still be
            // suppressed as belonging to a composition that is already gone.
            app.ime_composing = false;
            WindowEventHandler::dispatch_text(
                &text,
                &KeyAction::Pressed,
                &Modifiers::default(),
                app,
            );
        }
        AimerNativePlatformEvent::SetPreedit { text, cursor } => {
            // The mobile equivalent of `Ime::Preedit`: while the composition is
            // open the raw keystrokes behind it stay suppressed, and an empty
            // string closes it.
            app.ime_composing = !text.is_empty();
            WindowEventHandler::dispatch_ime_preedit(text, cursor, app);
        }
        AimerNativePlatformEvent::TextEditingDelta(delta) => {
            if app.active_root().is_some() {
                let result = app.dispatch_element_event(
                    app.cursor_pos,
                    &ElementEvent::TextEditingDelta(delta),
                );
                if let Some(window) = &app.window
                    && (result.is_consumed() || result.needs_redraw())
                {
                    window.request_redraw();
                }
            }
        }
        AimerNativePlatformEvent::MenuShortcut(shortcut) => {
            // The menu bar already consumed the key press, so this *is* the
            // shortcut: it travels the same positional path a `Ctrl` shortcut
            // takes, which is what makes one widget-side implementation enough.
            if app.active_root().is_some() {
                let ev = shortcut.to_event();
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
        AimerNativePlatformEvent::FrameReady => {
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
        AimerNativePlatformEvent::HotReloadCallbackReady => {
            crate::aimer_app::callback_ready_delivered();
            if let Some(window) = &app.window {
                window.request_redraw();
            }
        }
    }
}
