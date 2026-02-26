use crate::render::App;
use attribute::position::Vec2d;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::runtime::Runtime;
use widget::Widget;
use winit::event_loop::{ControlFlow, EventLoop};

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
        #[cfg(not(target_arch = "wasm32"))]
        let async_runtime = Runtime::new().expect("Failed to create async runtime");
        #[cfg(target_arch = "wasm32")]
        let async_runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("Failed to create async runtime");

        println!("Creating App instance...");
        let mut app = App {
            window: None,
            #[cfg(not(target_arch = "wasm32"))]
            pixels: None,
            #[cfg(target_arch = "wasm32")]
            canvas_ctx: None,
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

        app.async_runtime.shutdown_background();
    }
}
