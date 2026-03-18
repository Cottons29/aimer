use crate::render::AimerAppConfiguration;
use attribute::position::Vec2d;
#[cfg(not(target_arch = "wasm32"))]
use inspector::InspectorAppHandle;
use std::cell::Cell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use widget::Widget;
use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy};

#[cfg(target_os = "android")]
pub static ANDROID_APP: std::sync::OnceLock<winit::platform::android::activity::AndroidApp> =
    std::sync::OnceLock::new();

static APP_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
pub enum CustomAppEvent {
    ForceBackspace,
    InsertText(String),
}

pub static EVENT_PROXY: OnceLock<EventLoopProxy<CustomAppEvent>> = OnceLock::new();

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn trigger_rust_backspace() {
    let Some(proxy) = EVENT_PROXY.get() else {
        utils::debug!("trigger_rust_backspace: EVENT_PROXY not initialized yet");
        return;
    };

    if let Err(e) = proxy.send_event(CustomAppEvent::ForceBackspace) {
        utils::error!("trigger_rust_backspace: failed to send event: {:?}", e);
    }
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn trigger_rust_insert_text(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }

    // SAFETY: caller guarantees `ptr..ptr+len` is valid for reads.
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let text = String::from_utf8_lossy(bytes).to_string();

    let Some(proxy) = EVENT_PROXY.get() else {
        utils::debug!(
            "trigger_rust_insert_text: EVENT_PROXY not initialized yet (len={})",
            len
        );
        return;
    };

    if let Err(e) = proxy.send_event(CustomAppEvent::InsertText(text)) {
        utils::error!("trigger_rust_insert_text: failed to send event: {:?}", e);
    }
}

pub struct AimerApp<T> {
    _marker: std::marker::PhantomData<T>,
}

impl<T: Widget + 'static> AimerApp<T> {
    pub fn start(widget: T) {
        start_event_loop(widget);
    }
}

fn start_event_loop(widget: impl Widget + 'static) {
    if APP_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    utils::info!("Initializing EventLoop...");
    #[cfg(not(target_os = "android"))]
    let event_loop = EventLoop::<CustomAppEvent>::with_user_event()
        .build()
        .expect("Failed to create EventLoop");

    #[cfg(target_os = "android")]
    let event_loop = {
        use winit::platform::android::EventLoopBuilderExtAndroid;
        let app = crate::aimer_app::ANDROID_APP
            .get()
            .expect("ANDROID_APP not set")
            .clone();
        winit::event_loop::EventLoop::<CustomAppEvent>::with_user_event()
            .with_android_app(app)
            .build()
            .expect("Failed to create EventLoop")
    };

    EVENT_PROXY.set(event_loop.create_proxy()).ok();

    event_loop.set_control_flow(ControlFlow::Wait);
    // event_loop.set_control_flow(ControlFlow::Poll);

    utils::debug!("Creating async runtime...");
    #[cfg(not(target_arch = "wasm32"))]
    let async_runtime = Runtime::new().expect("Failed to create async runtime");

    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let inspector = InspectorAppHandle::connect(inspector::DEFAULT_INSPECTOR_PORT, async_runtime.handle());
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    let inspector = inspector::start(inspector::DEFAULT_INSPECTOR_PORT);

    utils::info!("Creating App instance...");
    let mut app = AimerAppConfiguration {
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
        #[cfg(debug_assertions)]
        inspector,
        #[cfg(debug_assertions)]
        inspector_change: Cell::new(false),
        #[cfg(debug_assertions)]
        inspector_prev_enabled: Cell::new(false),
        #[cfg(debug_assertions)]
        inspector_redraw_frames: Cell::new(0),
    };

    // On iOS, this function never returns.
    match event_loop.run_app(&mut app) {
        Ok(_) => utils::info!("EventLoop finished successfully"),
        Err(e) => utils::error!("EventLoop::run_app failed: {:?}", e),
    }
    #[cfg(not(target_arch = "wasm32"))]
    app.async_runtime.shutdown_background();
}
