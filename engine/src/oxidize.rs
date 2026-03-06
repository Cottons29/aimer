use crate::render::OxidizeAppConfiguration;
use attribute::position::Vec2d;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use widget::Widget;
use winit::event_loop::{ControlFlow, EventLoop};
use utils::log;

#[cfg(target_os = "android")]
pub static ANDROID_APP: std::sync::OnceLock<winit::platform::android::activity::AndroidApp> = std::sync::OnceLock::new();

static APP_STARTED: AtomicBool = AtomicBool::new(false);

pub struct OxidizeApp;

impl OxidizeApp {
    pub fn start(widget : impl Widget + 'static) {
        if APP_STARTED.swap(true, Ordering::SeqCst) {
            utils::error!("OxidizeApp::start called multiple times. Ignoring subsequent calls.");
            return;
        }

        utils::info!("Initializing EventLoop...");
        #[cfg(not(target_os = "android"))]
        let event_loop = EventLoop::new().expect("Failed to create EventLoop");
        
        #[cfg(target_os = "android")]
        let event_loop = {
            use winit::platform::android::EventLoopBuilderExtAndroid;
            let app = crate::oxidize::ANDROID_APP.get().expect("ANDROID_APP not set").clone();
            winit::event_loop::EventLoop::builder().with_android_app(app).build().expect("Failed to create EventLoop")
        };

        event_loop.set_control_flow(ControlFlow::Wait);
        // event_loop.set_control_flow(ControlFlow::Poll);


        utils::info!("Creating async runtime...");
        #[cfg(not(target_arch = "wasm32"))]
        let async_runtime = Runtime::new().expect("Failed to create async runtime");

        utils::info!("Creating App instance...");
        let mut app = OxidizeAppConfiguration {
            window: None,
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            metal_layer: None,
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            command_queue: None,
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            skia_context: None,
            #[cfg(target_os = "android")]
            egl_display: None,
            #[cfg(target_os = "android")]
            egl_surface: None,
            #[cfg(target_os = "android")]
            egl_context: None,
            #[cfg(target_os = "android")]
            skia_gl_context: None,
            #[cfg(target_arch = "wasm32")]
            canvas_ctx: None,
            widget_root: None,
            pending_widget: Some(Box::new(widget)),
            cursor_pos: Vec2d { x: 0.0, y: 0.0 },
            window_scale: 1.0,
            native_window_size: None,
            pending_resize: None,
            #[cfg(not(target_arch = "wasm32"))]
            async_runtime,
        };



        utils::info!("Running App...");
        // On iOS, this function never returns.
        match event_loop.run_app(&mut app) {
            Ok(_) => utils::info!("EventLoop finished successfully"),
            Err(e) => utils::error!("EventLoop::run_app failed: {:?}", e),
        }
        #[cfg(not(target_arch = "wasm32"))]
        app.async_runtime.shutdown_background();
    }
}
