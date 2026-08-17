#![cfg(feature = "markdown")]

//! Custom widgets rendered by Markdown must remain interactive.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use aimer::quiver::aimer_app::HeadlessAimerApp;
use aimer::quiver::winit::dpi::PhysicalPosition;
use aimer::quiver::winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};
use aimer::{
    AimerApp, Button, MarkdownInlineRule, MarkdownInlineSyntax, MarkdownViewer, SizedBox, Widget,
};

fn move_to<W: Widget + 'static>(app: &mut HeadlessAimerApp<W>, x: f64, y: f64) {
    app.send_window_event(WindowEvent::CursorMoved {
        device_id: DeviceId::dummy(),
        position: PhysicalPosition::new(x, y),
    });
}

fn click<W: Widget + 'static>(app: &mut HeadlessAimerApp<W>, x: f64, y: f64) {
    move_to(app, x, y);
    app.send_window_event(WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state: ElementState::Pressed,
        button: MouseButton::Left,
    });
    app.render_frame();
    std::thread::sleep(Duration::from_millis(400));
    app.send_window_event(WindowEvent::MouseInput {
        device_id: DeviceId::dummy(),
        state: ElementState::Released,
        button: MouseButton::Left,
    });
}
//
// #[test]
// fn custom_inline_button_receives_events_inside_markdown_viewer() {
//
//     let presses = Rc::new(Cell::new(0));
//     let callback_presses = presses.clone();
//     let viewer = MarkdownViewer::new()
//         .markdown("{{button:Press}}")
//         .custom_inline(
//             MarkdownInlineRule::new(
//                 "button",
//                 MarkdownInlineSyntax::Paired {
//                     opening: "{{button:",
//                     closing: "}}",
//                 },
//             ),
//             move |_| {
//                 let presses = callback_presses.clone();
//                 Button::new()
//                     .on_press(move || presses.set(presses.get() + 1))
//                     .child(SizedBox::new().width(120).height(40))
//                     .boxed()
//             },
//         );
//     let mut app = AimerApp::start_headless(viewer);
//     app.render_frame();
//
//     click(&mut app, 20.0, 20.0);
//
//     assert_eq!(presses.get(), 1);
// }