use aimer::quiver::winit::dpi::PhysicalPosition;
use aimer::quiver::winit::event::{DeviceId, WindowEvent};
use aimer::quiver::winit::window::CursorIcon;
use aimer::{AimerApp, RichText, TextSpan};

// #[test]
// fn hovering_a_rich_text_link_uses_the_pointer_cursor() {
//     let text = RichText::new(TextSpan::new("Aimer").link("https://aimer.dev")).on_link(|_| {});
//     let mut app = AimerApp::start_headless(text);
//     app.render_frame();
//
//     app.send_window_event(WindowEvent::CursorMoved {
//         device_id: DeviceId::dummy(),
//         position: PhysicalPosition::new(1.0, 1.0),
//     });
//
//     assert_eq!(app.cursor_icon(), CursorIcon::Pointer);
// }
