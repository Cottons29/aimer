use widget::{Element, Widget};
use crate::render::App;
use widget::base::{Vec2d};
use winit::event_loop::{ControlFlow, EventLoop};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

static APP_STARTED: AtomicBool = AtomicBool::new(false);

pub struct OxidizeApp;

impl OxidizeApp {
    pub fn start(widget : impl Widget + 'static) {
        if APP_STARTED.swap(true, Ordering::SeqCst) {
            eprintln!("OxidizeApp::start called multiple times. Ignoring subsequent calls.");
            return;

        }

        println!("Initializing EventLoop...");
        let event_loop = EventLoop::new().expect("Failed to create EventLoop");
        // let frame_time = Duration::from_nanos(1_000_000_000 / 120);
        // event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + frame_time));

        event_loop.set_control_flow(ControlFlow::Wait);
       

        println!("Creating async runtime...");
        let async_runtime = Runtime::new().expect("Failed to create async runtime");

        println!("Creating App instance...");
        let mut app = App {
            window: None,
            pixels: None,
            widget_root: None,
            pending_widget: Some(Box::new(widget)),
            cursor_pos: Vec2d { x: 0.0, y: 0.0 },
            window_scale: 1.0,
            native_window_size: None,
            pending_resize: None,
            async_runtime,
        };

        println!("Running App...");
        // On iOS, this function never returns.
        match event_loop.run_app(&mut app) {
            Ok(_) => println!("EventLoop finished successfully (unexpected on iOS)."),
            Err(e) => eprintln!("EventLoop::run_app failed: {:?}", e),
        }
    }
}
