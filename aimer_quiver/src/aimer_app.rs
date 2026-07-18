use std::cell::Cell;
use std::net::IpAddr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use aimer_attribute::BoxConstraint;
use aimer_attribute::size::ResolvedSize;
#[cfg(not(target_arch = "wasm32"))]
use aimer_inspector::InspectorAppHandle;
use aimer_utils::info;
use aimer_widget::base::{BuildContext, WindowHandle};
use aimer_widget::{AnyWidget, Widget};
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

use crate::handler::AimerApplicationHandler;
use crate::handler::event_handler::{HeadlessEventAction, WindowEventHandler};
use crate::render_ctx::AimerRenderContext;

#[cfg(target_os = "android")]
pub static ANDROID_APP: std::sync::OnceLock<AndroidApp> = std::sync::OnceLock::new();

static APP_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
pub enum AimerCustomAppEvent {
    ForceBackspace,
    InsertText(String),
    FrameReady,
}

pub static EVENT_PROXY: OnceLock<EventLoopProxy<AimerCustomAppEvent>> = OnceLock::new();

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn trigger_rust_backspace() {
    let Some(proxy) = EVENT_PROXY.get() else {
        aimer_utils::debug!("trigger_rust_backspace: EVENT_PROXY not initialized yet");
        return;
    };

    if let Err(e) = proxy.send_event(AimerCustomAppEvent::ForceBackspace) {
        aimer_utils::error!("trigger_rust_backspace: failed to send event: {:?}", e);
    }
}

// iOS frame scheduling: driven by a Swift `CADisplayLink` (see `main.swift`).
#[cfg(target_os = "ios")]
unsafe extern "C" {
    /// Pause the Swift `CADisplayLink` so it stops delivering vsync ticks while
    /// the app is idle.
    fn aimer_ios_pause_frames();
}

/// Called from Swift on every display-link vsync.
///
/// If a frame was requested since the last tick, forward a `FrameReady` through
/// the event loop (which routes to `request_redraw()`). If nothing is pending,
/// pause the display link so the app does not render while idle. Mirrors the
/// `EVENT_PROXY` guard used by the other `trigger_rust_*` entry points.
#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn aimer_ios_frame_tick() {
    if !aimer_events::window::take_frame_requested() {
        // No frame pending — idle the display link until the next request.
        unsafe {
            aimer_ios_pause_frames();
        }
        return;
    }

    let Some(proxy) = EVENT_PROXY.get() else {
        aimer_utils::debug!("aimer_ios_frame_tick: EVENT_PROXY not initialized yet");
        return;
    };

    if let Err(e) = proxy.send_event(AimerCustomAppEvent::FrameReady) {
        aimer_utils::error!("aimer_ios_frame_tick: failed to send event: {:?}", e);
    }
}

#[cfg(target_os = "ios")]
fn dereference_ptr<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    unsafe { std::slice::from_raw_parts(ptr, len) }
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn trigger_rust_insert_text(ptr: *const u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }

    let bytes = dereference_ptr(ptr, len);
    let text = String::from_utf8_lossy(bytes).to_string();

    let Some(proxy) = EVENT_PROXY.get() else {
        aimer_utils::debug!(
            "trigger_rust_insert_text: EVENT_PROXY not initialized yet (len={})",
            len
        );
        return;
    };

    if let Err(e) = proxy.send_event(AimerCustomAppEvent::InsertText(text)) {
        aimer_utils::error!("trigger_rust_insert_text: failed to send event: {:?}", e);
    }
}

// Android software-keyboard forwarding into Rust.
//
// These are the JNI entry points invoked by the Java `com.aimer.AimerActivity`
// helper (see the Android build template). The hidden `EditText` managed by
// that activity captures everything the soft keyboard produces — including
// IME-composed CJK text once a candidate is committed — and forwards it here.
// The text is then pushed through the same platform-agnostic
// `AimerCustomAppEvent` path used by iOS, so the focused text field inserts the
// characters exactly once.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aimer_AimerActivity_nativeInsertText<'caller>(
    mut env: jni::EnvUnowned<'caller>,
    _class: jni::objects::JClass<'caller>,
    text: jni::objects::JString<'caller>,
) {
    env.with_env(|env| -> Result<(), jni::errors::Error> {
        let text = String::from(text.mutf8_chars(env)?);
        if text.is_empty() {
            return Ok(());
        }

        let Some(proxy) = EVENT_PROXY.get() else {
            aimer_utils::debug!("nativeInsertText: EVENT_PROXY not initialized yet");
            return Ok(());
        };

        if let Err(e) = proxy.send_event(AimerCustomAppEvent::InsertText(text)) {
            aimer_utils::error!("nativeInsertText: failed to send event: {:?}", e);
        }
        Ok(())
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_aimer_AimerActivity_nativeBackspace<'caller>(
    mut env: jni::EnvUnowned<'caller>,
    _class: jni::objects::JClass<'caller>,
) {
    env.with_env(|_env| -> Result<(), jni::errors::Error> {
        let Some(proxy) = EVENT_PROXY.get() else {
            aimer_utils::debug!("nativeBackspace: EVENT_PROXY not initialized yet");
            return Ok(());
        };

        if let Err(e) = proxy.send_event(AimerCustomAppEvent::ForceBackspace) {
            aimer_utils::error!("nativeBackspace: failed to send event: {:?}", e);
        }
        Ok(())
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}

pub struct AimerApp<T> {
    _marker: std::marker::PhantomData<T>,
}

/// Mocked display properties used by a headless application.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadlessOptions {
    pub size: PhysicalSize<u32>,
    pub scale_factor: f64,
}

impl Default for HeadlessOptions {
    fn default() -> Self {
        Self { size: PhysicalSize::new(1150, 800), scale_factor: 1.0 }
    }
}

/// A running Aimer application that builds, lays out, draws, and handles events
/// without creating a native window or a `winit` event loop.
pub struct HeadlessAimerApp<W: Widget + 'static> {
    app: AimerApplicationHandler<W>,
    canvas: aimer_canvas::InnerCanvas,
    window: WindowHandle,
    size: PhysicalSize<u32>,
    exit_requested: bool,
}

impl<W: Widget + 'static> HeadlessAimerApp<W> {
    fn new(widget: W, options: HeadlessOptions) -> HeadlessAimerApp<W> {
        let scale_factor = if options
            .scale_factor
            .is_finite()
            && options.scale_factor > 0.0
        {
            options.scale_factor
        } else {
            1.0
        };

        #[cfg(not(target_arch = "wasm32"))]
        let async_runtime = Runtime::new().expect("Failed to create async runtime");

        let window = WindowHandle::headless(options.size, scale_factor);
        Self {
            app: AimerApplicationHandler {
                window: None,
                render_ctx: AimerRenderContext::default(),
                widget_root: None,
                pending_widget: Some(widget),
                cursor_pos: crate::handler::event_handler::CURSOR_OUTSIDE_POSITION,
                current_modifiers: Default::default(),
                ime_composing: false,
                window_scale: scale_factor,
                native_window_size: None,
                pending_resize: None,
                #[cfg(not(target_arch = "wasm32"))]
                async_runtime,
                #[cfg(debug_assertions)]
                inspector: None,
                #[cfg(debug_assertions)]
                inspector_change: Cell::new(false),
                #[cfg(debug_assertions)]
                inspector_prev_enabled: Cell::new(false),
                #[cfg(debug_assertions)]
                inspector_redraw_frames: Cell::new(0),
                start_up_frames: Cell::new(0),
                first_frame_notifier: Default::default(),
                active_touch_id: None,
            },
            canvas: aimer_canvas::InnerCanvas::new(),
            window,
            size: options.size,
            exit_requested: false,
        }
    }

    /// Builds and draws one frame into the non-presenting in-memory canvas.
    pub fn render_frame(&mut self) {
        if self.exit_requested {
            return;
        }

        let scale_factor = self
            .app
            .window_scale;
        let frame_size = ResolvedSize {
            width: self
                .size
                .width as f32,
            height: self
                .size
                .height as f32,
        };
        let canvas = aimer_canvas::Canvas::new(&self.canvas);
        canvas.begin_frame();
        let ctx = BuildContext {
            parent_size: frame_size,
            canvas,
            scale: scale_factor as f32,
            parent_pos: Default::default(),
            cursor_pos: self
                .app
                .cursor_pos,
            box_constraint: BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: frame_size.width,
                max_height: frame_size.height,
            },
            visible_rect: None,
            window: self
                .window
                .clone(),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: self
                .app
                .async_runtime
                .handle()
                .clone(),
            inherited_states: Default::default(),
        };

        if self
            .app
            .widget_root
            .is_none()
            && let Some(widget) = self
                .app
                .pending_widget
                .take()
        {
            self.app
                .widget_root = Some(widget.to_element(&ctx));
        }
        if let Some(root) = &self
            .app
            .widget_root
        {
            root.draw(&ctx);
        }
        self.app
            .pending_resize = None;
    }

    /// Delivers a `winit` window event to the headless application.
    pub fn send_window_event(&mut self, event: WindowEvent) {
        if let WindowEvent::Resized(size) = &event {
            self.size = *size;
            self.window
                .update_headless_metrics(
                    self.size,
                    self.app
                        .window_scale,
                );
        }
        let action = WindowEventHandler::handle_headless_event(&mut self.app, event);
        self.window
            .update_headless_metrics(
                self.size,
                self.app
                    .window_scale,
            );
        match action {
            HeadlessEventAction::None => self
                .window
                .request_redraw(),
            HeadlessEventAction::Render => self.render_frame(),
            HeadlessEventAction::Exit => self.exit_requested = true,
        }
    }

    /// Delivers an Aimer user event through the same path as the native event
    /// loop.
    pub fn send_user_event(&mut self, event: AimerCustomAppEvent) {
        crate::handler::user_events::handle_user_event(&mut self.app, event);
        self.window
            .request_redraw();
    }

    pub fn physical_size(&self) -> PhysicalSize<u32> {
        self.size
    }

    pub fn logical_size(&self) -> ResolvedSize {
        ResolvedSize {
            width: self
                .size
                .width as f32
                / self
                    .app
                    .window_scale as f32,
            height: self
                .size
                .height as f32
                / self
                    .app
                    .window_scale as f32,
        }
    }

    pub fn scale_factor(&self) -> f64 {
        self.app
            .window_scale
    }

    pub fn has_native_window(&self) -> bool {
        self.app
            .window
            .is_some()
    }

    pub fn is_exit_requested(&self) -> bool {
        self.exit_requested
    }

    /// Returns and clears whether application code requested another frame.
    pub fn take_redraw_request(&self) -> bool {
        self.window
            .take_redraw_request()
    }
}

impl<W: Widget + 'static> AimerApp<W> {
    pub fn start(widget: W) {
        start_event_loop(widget);
    }

    pub fn start_headless(widget: W) -> HeadlessAimerApp<W> {
        Self::start_headless_with(widget, HeadlessOptions::default())
    }

    pub fn start_headless_with(widget: W, options: HeadlessOptions) -> HeadlessAimerApp<W> {
        HeadlessAimerApp::new(widget, options)
    }
}

fn start_event_loop(widget: impl Widget + 'static) {
    if APP_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    info!("Initializing EventLoop...");
    #[cfg(not(target_os = "android"))]
    let event_loop = EventLoop::<AimerCustomAppEvent>::with_user_event()
        .build()
        .expect("Failed to create EventLoop");

    #[cfg(target_os = "android")]
    let event_loop = {
        use aimer_events::android_app;
        use winit::platform::android::EventLoopBuilderExtAndroid;
        let app = crate::aimer_app::ANDROID_APP
            .get()
            .expect("ANDROID_APP not set")
            .clone();

        android_app::set_android_app(app.clone());

        // Keep the JNI entry points used by `com.aimer.AimerActivity` reachable.
        // They are only ever called by the JVM at runtime (never from Rust), so
        // without an explicit reference, the linker may garbage-collect them out of
        // the final `cdylib`, which would make the soft-keyboard text bridge fail
        // with `UnsatisfiedLinkError`.
        let _keep_jni: [*const (); 2] = [
            Java_com_aimer_AimerActivity_nativeInsertText as *const (),
            Java_com_aimer_AimerActivity_nativeBackspace as *const (),
        ];
        std::hint::black_box(_keep_jni);

        EventLoop::<AimerCustomAppEvent>::with_user_event()
            .with_android_app(app)
            .build()
            .expect("Failed to create EventLoop")
    };

    EVENT_PROXY
        .set(event_loop.create_proxy())
        .ok();

    // Route animation redraws requests through the event loop instead of letting
    // animating widgets (e.g. scroll momentum) spawn a sleeping thread per frame.
    // `FrameReady` is delivered via `user_event` after the current frame, which
    // schedules the next redraw safely even on platforms (iOS) that coalesce a
    // synchronous `request_redraw()` issued from inside the draw cycle.
    aimer_events::window::set_redraw_requester(|| {
        if let Some(proxy) = EVENT_PROXY.get() {
            let _ = proxy.send_event(AimerCustomAppEvent::FrameReady);
        }
    });

    const DEFAULT_INSPECTOR_PORT: &str = env!("DEFAULT_INSPECTOR_PORT");
    const DEFAULT_INSPECTOR_ADDRESS: &str = env!("DEFAULT_INSPECTOR_ADDRESS");

    info!("DEFAULT_INSPECTOR_PORT : {}", DEFAULT_INSPECTOR_PORT);
    info!("DEFAULT_INSPECTOR_ADDRESS : {}", DEFAULT_INSPECTOR_ADDRESS);

    event_loop.set_control_flow(ControlFlow::Wait);

    aimer_utils::debug!("Creating async runtime...");
    #[cfg(not(target_arch = "wasm32"))]
    let async_runtime = Runtime::new().expect("Failed to create async runtime");

    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let inspector = InspectorAppHandle::connect(
        async_runtime.handle(),
        DEFAULT_INSPECTOR_ADDRESS
            .parse::<IpAddr>()
            .unwrap(),
        DEFAULT_INSPECTOR_PORT
            .parse::<u16>()
            .unwrap(),
    );
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    let inspector = aimer_inspector::start(
        DEFAULT_INSPECTOR_PORT
            .parse::<u16>()
            .unwrap(),
    );

    info!("Creating App instance...");
    let mut app = AimerApplicationHandler {
        window: None,
        render_ctx: AimerRenderContext::default(),
        widget_root: None,
        pending_widget: Some(widget),
        cursor_pos: crate::handler::event_handler::CURSOR_OUTSIDE_POSITION,
        current_modifiers: Default::default(),
        ime_composing: false,
        window_scale: 1.0,
        native_window_size: None,
        pending_resize: None,
        #[cfg(not(target_arch = "wasm32"))]
        async_runtime,
        #[cfg(debug_assertions)]
        inspector: Some(inspector),
        #[cfg(debug_assertions)]
        inspector_change: Cell::new(false),
        #[cfg(debug_assertions)]
        inspector_prev_enabled: Cell::new(false),
        #[cfg(debug_assertions)]
        inspector_redraw_frames: Cell::new(0),
        start_up_frames: Cell::new(255),
        first_frame_notifier: Default::default(),
        active_touch_id: None,
    };

    info!("Started main event loop");

    // On iOS, this function never returns.
    match event_loop.run_app(&mut app) {
        Ok(_) => info!("EventLoop finished successfully"),
        Err(e) => aimer_utils::error!("EventLoop::run_app failed: {:?}", e),
    }
    #[cfg(not(target_arch = "wasm32"))]
    app.async_runtime
        .shutdown_background();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use aimer_attribute::position::Vec2d;
    use aimer_attribute::size::ResolvedSize;
    use aimer_events::element::ElementEvent;
    use aimer_widget::base::BuildContext;
    use aimer_widget::{
        Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement,
    };
    use winit::dpi::{PhysicalPosition, PhysicalSize};
    use winit::event::{DeviceId, WindowEvent};

    use super::*;

    struct RecordingWidget {
        builds: Arc<AtomicUsize>,
        cancels: Arc<AtomicUsize>,
    }

    impl Widget for RecordingWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            self.builds
                .fetch_add(1, Ordering::SeqCst);
            Box::new(RecordingElement {
                cancels: self
                    .cancels
                    .clone(),
            })
        }
    }

    struct RecordingElement {
        cancels: Arc<AtomicUsize>,
    }

    impl Drawable for RecordingElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl LayoutElement for RecordingElement {}
    impl Rebuildable for RecordingElement {}
    impl VisitorElement for RecordingElement {
        fn debug_name(&self) -> &'static str {
            "RecordingElement"
        }
    }
    impl EventElement for RecordingElement {
        fn on_event(&self, event: &ElementEvent) -> bool {
            if matches!(event, ElementEvent::Cancel) {
                self.cancels
                    .fetch_add(1, Ordering::SeqCst);
            }
            false
        }
    }

    #[test]
    fn headless_start_builds_without_a_native_window() {
        let builds = Arc::new(AtomicUsize::new(0));
        let mut app = AimerApp::start_headless(RecordingWidget {
            builds: builds.clone(),
            cancels: Arc::new(AtomicUsize::new(0)),
        });

        app.render_frame();

        assert_eq!(builds.load(Ordering::SeqCst), 1);
        assert!(!app.has_native_window());
        assert_eq!(app.logical_size(), ResolvedSize { width: 1150.0, height: 800.0 });
    }

    #[test]
    fn headless_window_events_update_metrics_and_reach_widgets() {
        let cancels = Arc::new(AtomicUsize::new(0));
        let mut app = AimerApp::start_headless_with(
            RecordingWidget { builds: Arc::new(AtomicUsize::new(0)), cancels: cancels.clone() },
            HeadlessOptions { size: PhysicalSize::new(640, 480), scale_factor: 2.0 },
        );
        app.render_frame();

        app.send_window_event(WindowEvent::Focused(false));
        assert!(app.take_redraw_request());
        app.send_window_event(WindowEvent::Resized(PhysicalSize::new(800, 600)));

        assert_eq!(cancels.load(Ordering::SeqCst), 1);
        assert_eq!(app.physical_size(), PhysicalSize::new(800, 600));
        assert_eq!(app.logical_size(), ResolvedSize { width: 400.0, height: 300.0 });
    }

    #[test]
    fn cursor_boundaries_invalidate_stale_position_without_cancelling_gestures() {
        let cancels = Arc::new(AtomicUsize::new(0));
        let mut app = AimerApp::start_headless(RecordingWidget {
            builds: Arc::new(AtomicUsize::new(0)),
            cancels: cancels.clone(),
        });
        app.render_frame();
        let device_id = DeviceId::dummy();

        app.send_window_event(WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(20.0, 30.0),
        });
        assert_eq!(
            (
                app.app
                    .cursor_pos
                    .x,
                app.app
                    .cursor_pos
                    .y
            ),
            (20.0, 30.0)
        );

        app.send_window_event(WindowEvent::CursorLeft { device_id });
        assert_eq!(
            (
                app.app
                    .cursor_pos
                    .x,
                app.app
                    .cursor_pos
                    .y
            ),
            (
                crate::handler::event_handler::CURSOR_OUTSIDE_POSITION.x,
                crate::handler::event_handler::CURSOR_OUTSIDE_POSITION.y,
            ),
        );
        assert_eq!(cancels.load(Ordering::SeqCst), 0);

        app.app
            .cursor_pos = Vec2d { x: 20.0, y: 30.0 };
        app.send_window_event(WindowEvent::CursorEntered { device_id });
        assert_eq!(
            (
                app.app
                    .cursor_pos
                    .x,
                app.app
                    .cursor_pos
                    .y
            ),
            (
                crate::handler::event_handler::CURSOR_OUTSIDE_POSITION.x,
                crate::handler::event_handler::CURSOR_OUTSIDE_POSITION.y,
            ),
        );
        assert!(app.take_redraw_request());
    }

    #[test]
    fn close_requested_stops_headless_application() {
        let app = AimerApp::start_headless(RecordingWidget {
            builds: Arc::new(AtomicUsize::new(0)),
            cancels: Arc::new(AtomicUsize::new(0)),
        });
        assert!(!app.is_exit_requested());

        let mut app = app;
        app.send_window_event(WindowEvent::CloseRequested);

        assert!(app.is_exit_requested());
    }

    #[test]
    fn invalid_headless_scale_uses_safe_default() {
        let app = AimerApp::start_headless_with(
            RecordingWidget {
                builds: Arc::new(AtomicUsize::new(0)),
                cancels: Arc::new(AtomicUsize::new(0)),
            },
            HeadlessOptions { size: PhysicalSize::new(320, 240), scale_factor: 0.0 },
        );

        assert_eq!(app.scale_factor(), 1.0);
        assert_eq!(app.logical_size(), ResolvedSize { width: 320.0, height: 240.0 });
    }

    struct RedrawWidget;

    impl Widget for RedrawWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            Box::new(RedrawElement)
        }
    }

    struct RedrawElement;

    impl Drawable for RedrawElement {
        fn draw(&self, ctx: &BuildContext) {
            ctx.window
                .request_redraw();
        }
    }
    impl LayoutElement for RedrawElement {}
    impl Rebuildable for RedrawElement {}
    impl VisitorElement for RedrawElement {
        fn debug_name(&self) -> &'static str {
            "RedrawElement"
        }
    }
    impl EventElement for RedrawElement {}

    #[test]
    fn headless_redraw_requests_can_drive_a_frame_pump() {
        let mut app = AimerApp::start_headless(RedrawWidget);

        app.render_frame();

        assert!(app.take_redraw_request());
        assert!(!app.take_redraw_request());
    }
}
