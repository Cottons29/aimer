//! UIKit input ABI. Pointer coordinates and scroll deltas are window-local points.

use aimer_attribute::position::Vec2d;
use aimer_events::element::{
    ElementEvent, KeyAction, Modifiers, NamedKey, ScrollDeltaKind, TouchPhase,
};
use aimer_events::pointer::{PointerButton, PointerInfo, PointerSource};

fn modifiers(flags: u64) -> Modifiers {
    Modifiers {
        shift: flags & (1 << 17) != 0,
        ctrl: flags & (1 << 18) != 0,
        alt: flags & (1 << 19) != 0,
        meta: flags & (1 << 20) != 0,
    }
}

fn key(usage: u32, text: &str) -> Option<NamedKey> {
    Some(match usage {
        40 | 88 => NamedKey::Enter,
        41 => NamedKey::Escape,
        42 => NamedKey::Backspace,
        43 => NamedKey::Tab,
        73 => NamedKey::Other("Insert".into()),
        74 => NamedKey::Home,
        75 => NamedKey::PageUp,
        76 => NamedKey::Delete,
        77 => NamedKey::End,
        78 => NamedKey::PageDown,
        79 => NamedKey::ArrowRight,
        80 => NamedKey::ArrowLeft,
        81 => NamedKey::ArrowDown,
        82 => NamedKey::ArrowUp,
        58..=69 => NamedKey::Other(format!("F{}", usage - 57)),
        224 | 228 => NamedKey::Other("Control".into()),
        225 | 229 => NamedKey::Other("Shift".into()),
        226 | 230 => NamedKey::Other("Alt".into()),
        227 | 231 => NamedKey::Other("Meta".into()),
        _ if !text.is_empty() && !text.chars().any(char::is_control) => {
            NamedKey::Other(text.into())
        }
        _ => return None,
    })
}

fn action(value: u32) -> Option<KeyAction> {
    match value {
        0 => Some(KeyAction::Pressed),
        1 => Some(KeyAction::Released),
        2 => Some(KeyAction::Repeat),
        _ => None,
    }
}

fn point(x: f64, y: f64) -> Option<Vec2d> {
    let point = Vec2d {
        x: x as f32,
        y: y as f32,
    };
    (point.x.is_finite() && point.y.is_finite()).then_some(point)
}

fn pointer_event(kind: u32, x: f64, y: f64, button: u32) -> Option<ElementEvent> {
    if kind == 1 {
        return Some(ElementEvent::PointerExited(PointerSource::Mouse, 0));
    }
    if kind == 4 {
        return Some(ElementEvent::Cancel);
    }
    let pos = point(x, y)?;
    let button = match button {
        0 => PointerButton::Primary,
        1 => PointerButton::Secondary,
        2 => PointerButton::Middle,
        other => PointerButton::Other(other.try_into().ok()?),
    };
    let pointer = PointerInfo::mouse(pos, button);
    match kind {
        0 => Some(ElementEvent::PointerMove(pointer)),
        2 => Some(ElementEvent::PointerDown(pointer)),
        3 => Some(ElementEvent::PointerUp(pointer)),
        _ => None,
    }
}

fn scroll_event(x: f64, y: f64, phase: u32) -> Option<ElementEvent> {
    let phase = match phase {
        0 => TouchPhase::Started,
        1 => TouchPhase::Moved,
        2 => TouchPhase::Ended,
        3 => TouchPhase::Cancelled,
        _ => return None,
    };
    Some(ElementEvent::Scroll {
        delta: point(x, y)?,
        phase,
        kind: ScrollDeltaKind::Pixel,
        is_direct_manipulation: phase == TouchPhase::Started || phase == TouchPhase::Moved,
    })
}

#[cfg(target_os = "ios")]
fn send(event: ElementEvent, flags: u64) {
    if let Some(proxy) = crate::aimer_app::EVENT_PROXY.get() {
        let _ = proxy.send_event(crate::aimer_app::AimerNativePlatformEvent::NativeInput {
            event,
            modifiers: modifiers(flags),
        });
    }
}

/// Forwards a UIKit mouse move (0), exit (1), down (2), up (3), or cancellation (4).
/// Coordinates are logical window points; button numbers are zero based.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn aimer_ios_pointer_event(kind: u32, x: f64, y: f64, button: u32, flags: u64) {
    if let Some(event) = pointer_event(kind, x, y, button) {
        send(event, flags);
    }
}

/// Forwards a UIKit scroll in logical points, with began/moved/ended/cancelled
/// encoded as 0/1/2/3. UIKit has already applied the user's scroll direction.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn aimer_ios_scroll_event(x: f64, y: f64, phase: u32, flags: u64) {
    if let Some(event) = scroll_event(x, y, phase) {
        send(event, flags);
    }
}

/// Forwards a hardware key independently of committed text and IME edits.
/// `usage` is a USB keyboard HID usage; action is down/up/repeat (0/1/2).
///
/// # Safety
/// `ptr` must reference `len` readable, initialized bytes for this call. A null
/// pointer is allowed only for zero length. Invalid UTF-8 is rejected.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn aimer_ios_key_event(
    usage: u32,
    state: u32,
    flags: u64,
    ptr: *const u8,
    len: usize,
) {
    let Some(action) = action(state) else { return };
    if len > isize::MAX as usize || (len != 0 && ptr.is_null()) {
        return;
    }
    let bytes = if len == 0 {
        &[][..]
    } else {
        // SAFETY: The caller owns the readable bytes throughout this call;
        // bounds above satisfy slice length and non-null requirements.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };
    let Some(mut key) = key(usage, text) else {
        return;
    };
    let modifiers = modifiers(flags);
    if (modifiers.ctrl || modifiers.meta)
        && let NamedKey::Other(text) = &mut key
    {
        text.make_ascii_lowercase();
    }
    send(
        ElementEvent::KeyInput {
            key,
            action,
            modifiers,
        },
        flags,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uikit_modifier_bits_are_not_winit_modifier_bits() {
        assert_eq!(
            modifiers((1 << 17) | (1 << 18) | (1 << 19) | (1 << 20)),
            Modifiers {
                shift: true,
                ctrl: true,
                alt: true,
                meta: true
            }
        );
        assert_eq!(
            modifiers(1 << 16),
            Modifiers::default(),
            "caps lock is not a shortcut modifier"
        );
    }

    #[test]
    fn hardware_keys_include_navigation_editing_and_layout_characters() {
        for (usage, expected) in [
            (40, NamedKey::Enter),
            (41, NamedKey::Escape),
            (42, NamedKey::Backspace),
            (43, NamedKey::Tab),
            (73, NamedKey::Other("Insert".into())),
            (74, NamedKey::Home),
            (75, NamedKey::PageUp),
            (76, NamedKey::Delete),
            (77, NamedKey::End),
            (78, NamedKey::PageDown),
            (79, NamedKey::ArrowRight),
            (80, NamedKey::ArrowLeft),
            (81, NamedKey::ArrowDown),
            (82, NamedKey::ArrowUp),
        ] {
            assert_eq!(key(usage, ""), Some(expected));
        }
        assert_eq!(key(4, "ф"), Some(NamedKey::Other("ф".into())));
        assert_eq!(key(44, " "), Some(NamedKey::Other(" ".into())));
        assert_eq!(key(0, ""), None);
        assert_eq!(action(0), Some(KeyAction::Pressed));
        assert_eq!(action(1), Some(KeyAction::Released));
        assert_eq!(action(2), Some(KeyAction::Repeat));
        assert_eq!(action(3), None);
    }

    #[test]
    fn malformed_pointer_and_scroll_payloads_are_ignored() {
        assert!(pointer_event(0, f64::NAN, 0.0, 0).is_none());
        assert!(pointer_event(2, 0.0, f64::MAX, 0).is_none());
        assert!(pointer_event(2, 0.0, 0.0, u32::MAX).is_none());
        assert!(pointer_event(5, 0.0, 0.0, 0).is_none());
        assert!(scroll_event(0.0, 1.0, 4).is_none());
        assert!(scroll_event(f64::INFINITY, 0.0, 1).is_none());
        assert!(
            matches!(pointer_event(2, -1.0, 2.0, 2), Some(ElementEvent::PointerDown(p))
            if p.button == PointerButton::Middle && p.pos.x == -1.0)
        );
        assert!(matches!(
            scroll_event(0.0, -12.0, 2),
            Some(ElementEvent::Scroll {
                phase: TouchPhase::Ended,
                ..
            })
        ));
    }
}
