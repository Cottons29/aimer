//! Native keyboard deltas driven through a headless application.
//!
//! A `TextEditingDelta` reported by the iOS / Android keyboard shims targets
//! an editing *session*, not a screen position: on a phone the finger lifts
//! off the screen before typing begins, and the last touch may have landed
//! anywhere. The delta must reach the field that owns the session even when
//! the pointer no longer rests on that field.

use aimer::quiver::aimer_app::AimerNativePlatformEvent;
use aimer::quiver::winit::dpi::PhysicalPosition;
use aimer::quiver::winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};
use aimer::style::{LayoutSpacing, Spacing};
use aimer::{AimerApp, Container, TextEditingController, TextField, Widget};
use aimer_events::text_editing::{NativeTextRange, TextEditingDelta};

/// Moves the pointer to `(x, y)` and taps there.
fn tap<W: Widget + 'static>(
    app: &mut aimer::quiver::aimer_app::HeadlessAimerApp<W>,
    x: f64,
    y: f64,
) {
    let device_id = DeviceId::dummy();
    app.send_window_event(WindowEvent::CursorMoved {
        device_id,
        position: PhysicalPosition::new(x, y),
    });
    app.send_window_event(WindowEvent::MouseInput {
        device_id,
        state: ElementState::Pressed,
        button: MouseButton::Left,
    });
    app.send_window_event(WindowEvent::MouseInput {
        device_id,
        state: ElementState::Released,
        button: MouseButton::Left,
    });
    app.render_frame();
}

#[test]
fn a_native_delta_reaches_the_session_field_after_the_pointer_moves_away() {
    let controller = TextEditingController::new();
    let page = Container::new()
        .padding(LayoutSpacing::all(Spacing::Px(100)))
        .child(TextField::new().controller(controller.clone()));
    let mut app = AimerApp::start_headless(page);
    app.render_frame();

    // Focus the field with a tap inside it, then park the pointer on the
    // container padding, the way a finger leaves the screen before typing.
    tap(&mut app, 150.0, 110.0);
    app.send_window_event(WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(1.0, 1.0),
    });
    app.render_frame();

    // The session id the focused field drew is not observable from out here;
    // offering the delta under every id a fresh application can have handed
    // out is fine, because only the owning session applies it.
    for session_id in 1..=8 {
        app.send_user_event(AimerNativePlatformEvent::TextEditingDelta(
            TextEditingDelta {
                session_id,
                revision: controller.revision(),
                replacement: NativeTextRange::new(0, 0),
                replacement_text: "你好".into(),
                selection: NativeTextRange::new(2, 2),
                composing: None,
            },
        ));
    }

    assert_eq!(controller.value().text(), "你好");
}
