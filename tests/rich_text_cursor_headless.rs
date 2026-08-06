//! The hover cursor of text, driven through a headless application.
//!
//! These exercise the real pipeline: a window event enters the app, the element
//! tree resolves the hover, and the window records the requested shape.

use aimer::quiver::winit::dpi::PhysicalPosition;
use aimer::quiver::winit::event::{DeviceId, WindowEvent};
use aimer::quiver::winit::window::CursorIcon;
use aimer::{AimerApp, RichText, SelectionArea, Text, TextSpan};

fn hover(app: &mut aimer::quiver::aimer_app::HeadlessAimerApp<impl aimer::Widget + 'static>, x: f64, y: f64) {
    app.send_window_event(WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(x, y),
    });
    app.render_frame();
}

#[test]
fn hovering_a_rich_text_link_uses_the_pointer_cursor() {
    let text = RichText::new(TextSpan::new("Aimer").link("https://aimer.dev")).on_link(|_| {});
    let mut app = AimerApp::start_headless(text);
    app.render_frame();

    hover(&mut app, 1.0, 1.0);

    assert_eq!(app.cursor_icon(), CursorIcon::Pointer);
}

#[test]
fn hovering_selectable_text_inside_a_region_uses_the_text_cursor() {
    let mut app = AimerApp::start_headless(SelectionArea::new().child(Text::new("Selectable")));
    app.render_frame();

    hover(&mut app, 1.0, 1.0);

    assert_eq!(app.cursor_icon(), CursorIcon::Text);
}
