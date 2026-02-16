use widget::Widget;
use crate::render::App;
use widget::base::{Vec2d};
use winit::event_loop::{ControlFlow, EventLoop};

pub struct OxidizeApp;

impl OxidizeApp {
    pub fn start(widget : impl Widget) {
        let event_loop = EventLoop::new().unwrap();
        event_loop.set_control_flow(ControlFlow::Wait);
        let mut app = App {
            window: None,
            pixels: None,
            widget_root:widget,
            cursor_pos: Vec2d { x: 0.0, y: 0.0 },
        };
        let _ = event_loop.run_app(&mut app);
    }
}
