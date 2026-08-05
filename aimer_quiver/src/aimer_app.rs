use std::cell::Cell;
use std::net::IpAddr;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use aimer_attribute::BoxConstraint;
use aimer_attribute::size::ResolvedSize;
use aimer_cupid::AntiAlias;
#[cfg(not(target_arch = "wasm32"))]
use aimer_inspector::InspectorAppHandle;
use aimer_modal::ModalHost;
use aimer_utils::info;
use aimer_widget::Widget;
use aimer_widget::base::{BuildContext, WindowHandle};
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ControlFlow, EventLoop, EventLoopProxy};
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

use crate::handler::event_handler::{HeadlessEventAction, WindowEventHandler};
use crate::handler::{AimerApplicationHandler, StartupHook};
use crate::render_ctx::AimerRenderContext;

#[cfg(target_os = "android")]
pub static ANDROID_APP: std::sync::OnceLock<AndroidApp> = std::sync::OnceLock::new();

static APP_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
pub enum AimerNativePlatformEvent {
    ForceBackspace,
    InsertText(String),
    FrameReady,
}

pub static EVENT_PROXY: OnceLock<EventLoopProxy<AimerNativePlatformEvent>> = OnceLock::new();

/// Whether a `FrameReady` animation event is waiting in the event loop.
///
/// Cursor movement can request a direct redraw while an animation event is
/// already queued. Rendering that redraw schedules another animation frame;
/// coalescing here prevents those requests from accumulating faster than the
/// event loop can deliver them.
static FRAME_READY_PENDING: AtomicBool = AtomicBool::new(false);

fn try_begin_frame_ready_request(pending: &AtomicBool) -> bool {
    pending
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

fn complete_frame_ready_request(pending: &AtomicBool) {
    pending.store(false, Ordering::Release);
}

pub(crate) fn frame_ready_delivered() {
    complete_frame_ready_request(&FRAME_READY_PENDING);
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn trigger_rust_backspace() {
    let Some(proxy) = EVENT_PROXY.get() else {
        aimer_utils::debug!("trigger_rust_backspace: EVENT_PROXY not initialized yet");
        return;
    };

    if let Err(e) = proxy.send_event(AimerNativePlatformEvent::ForceBackspace) {
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

    if let Err(e) = proxy.send_event(AimerNativePlatformEvent::FrameReady) {
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

    if let Err(e) = proxy.send_event(AimerNativePlatformEvent::InsertText(text)) {
        aimer_utils::error!("trigger_rust_insert_text: failed to send event: {:?}", e);
    }
}

// Android software-keyboard forwarding into Rust.
//
// These are the JNI entry points invoked by the Kotlin `com.aimer.AimerActivity`
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

        if let Err(e) = proxy.send_event(AimerNativePlatformEvent::InsertText(text)) {
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

        if let Err(e) = proxy.send_event(AimerNativePlatformEvent::ForceBackspace) {
            aimer_utils::error!("nativeBackspace: failed to send event: {:?}", e);
        }
        Ok(())
    })
    .resolve::<jni::errors::ThrowRuntimeExAndDefault>();
}

/// Configures and starts an Aimer application.
///
/// Construct an application with [`Self::new`], apply configuration, and call
/// [`Self::child`] last to produce a runnable application. Existing static
/// start functions remain available and use the default configuration.
pub struct AimerApp<W = ()> {
    child: W,
    antialiasing: AntiAlias,
    startup_hooks: Vec<StartupHook>,
}

#[cfg(target_os = "macos")]
fn install_macos_menu() -> muda::Menu {
    use muda::{Menu, MenuItem, PredefinedMenuItem, Submenu};

    let menu = Menu::new();

    let app_menu = Submenu::new("Aimer", true);
    app_menu
        .append_items(&[
            &PredefinedMenuItem::about(None, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])
        .unwrap();

    let file_menu = Submenu::new("File", true);
    file_menu
        .append_items(&[
            &MenuItem::new("New", true, None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::close_window(None),
        ])
        .unwrap();

    let edit_menu = Submenu::new("Edit", true);
    edit_menu
        .append_items(&[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
        ])
        .unwrap();

    let view_menu = Submenu::new("View", true);
    view_menu
        .append_items(&[&PredefinedMenuItem::fullscreen(None)])
        .unwrap();

    let window_menu = Submenu::new("Window", true);
    window_menu
        .append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::close_window(None),
        ])
        .unwrap();

    let help_menu = Submenu::new("Help", true);
    help_menu
        .append_items(&[&MenuItem::new("Aimer Help", true, None)])
        .unwrap();

    menu.append_items(&[
        &app_menu,
        &file_menu,
        &edit_menu,
        &view_menu,
        &window_menu,
        &help_menu,
    ])
    .unwrap();

    menu.init_for_nsapp();
    menu
}

fn default_startup_hooks() -> Vec<StartupHook> {
    #[cfg(target_os = "macos")]
    {
        vec![Box::new(|| Box::new(install_macos_menu()))]
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}
/// Mocked display properties used by a headless application.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadlessOptions {
    pub size: PhysicalSize<u32>,
    pub scale_factor: f64,
}

impl Default for HeadlessOptions {
    fn default() -> Self {
        Self {
            size: PhysicalSize::new(1150, 800),
            scale_factor: 1.0,
        }
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
    fn new(widget: W, options: HeadlessOptions, antialiasing: AntiAlias) -> HeadlessAimerApp<W> {
        let scale_factor = if options.scale_factor.is_finite() && options.scale_factor > 0.0 {
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
                render_ctx: AimerRenderContext::new(antialiasing),
                widget_root: None,
                event_dispatcher: aimer_widget::EventDispatcher::new(),
                scroll_smoother: crate::handler::scroll_classifier::DualScroller::new(),
                #[cfg(target_arch = "wasm32")]
                web_scroll_phase: crate::handler::web_scroll_phase::WebScrollPhase::new(),
                pending_widget: Some(widget),
                cursor_pos: crate::handler::event_handler::CURSOR_OUTSIDE_POSITION,
                pressed_button: None,
                current_modifiers: Default::default(),
                ime_composing: false,
                window_scale: scale_factor,
                native_window_size: None,
                pending_resize: None,
                startup_hooks: Vec::new(),
                startup_resources: Vec::new(),
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
                active_touch_id: None,
                file_drag: crate::handler::file_drag::FileDrag::new(),
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

        let _ = self.app.dispatch_smoothed_scroll();

        let scale_factor = self.app.window_scale;
        let frame_size = ResolvedSize {
            width: self.size.width as f32,
            height: self.size.height as f32,
        };
        let canvas = aimer_canvas::Canvas::new(&self.canvas);
        canvas.begin_frame();
        let ctx = BuildContext {
            parent_size: frame_size,
            canvas,
            scale: scale_factor as f32,
            parent_pos: Default::default(),
            cursor_pos: self.app.cursor_pos,
            box_constraint: BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: frame_size.width,
                max_height: frame_size.height,
            },
            visible_rect: None,
            window: self.window.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: self.app.async_runtime.handle().clone(),
            inherited_states: Default::default(),
        };

        if self.app.widget_root.is_none()
            && let Some(widget) = self.app.pending_widget.take()
        {
            self.app.widget_root = Some(widget.to_element(&ctx));
        }
        if let Some(root) = &self.app.widget_root {
            root.draw(&ctx);
        }
        self.app.pending_resize = None;
    }

    /// Delivers a `winit` window event to the headless application.
    pub fn send_window_event(&mut self, event: WindowEvent) {
        if let WindowEvent::Resized(size) = &event {
            self.size = *size;
            self.window
                .update_headless_metrics(self.size, self.app.window_scale);
        }
        let action = WindowEventHandler::handle_headless_event(&mut self.app, event);
        self.window
            .update_headless_metrics(self.size, self.app.window_scale);
        match action {
            HeadlessEventAction::None => self.window.request_redraw(),
            HeadlessEventAction::Render => self.render_frame(),
            HeadlessEventAction::Exit => self.exit_requested = true,
        }
    }

    /// Delivers an Aimer user event through the same path as the native event
    /// loop.
    pub fn send_user_event(&mut self, event: AimerNativePlatformEvent) {
        crate::handler::user_events::handle_user_event(&mut self.app, event);
        self.window.request_redraw();
    }

    pub fn physical_size(&self) -> PhysicalSize<u32> {
        self.size
    }

    pub fn logical_size(&self) -> ResolvedSize {
        ResolvedSize {
            width: self.size.width as f32 / self.app.window_scale as f32,
            height: self.size.height as f32 / self.app.window_scale as f32,
        }
    }

    pub fn scale_factor(&self) -> f64 {
        self.app.window_scale
    }

    pub fn has_native_window(&self) -> bool {
        self.app.window.is_some()
    }

    pub fn is_exit_requested(&self) -> bool {
        self.exit_requested
    }

    /// Returns the cursor icon most recently selected by the widget tree.
    pub fn cursor_icon(&self) -> winit::window::CursorIcon {
        self.window
            .headless_cursor()
            .expect("a headless application always owns a headless window")
    }

    /// Returns and clears whether application code requested another frame.
    pub fn take_redraw_request(&self) -> bool {
        self.window.take_redraw_request()
    }
}

impl AimerApp {
    /// Creates an application builder using lightweight analytic antialiasing.
    #[inline]
    pub fn new() -> Self {
        Self {
            child: (),
            antialiasing: AntiAlias::default(),
            startup_hooks: default_startup_hooks(),
        }
    }

    /// Selects the antialiasing strategy used by the Cupid renderer.
    #[inline]
    pub fn with_antialiasing(mut self, antialiasing: AntiAlias) -> Self {
        self.antialiasing = antialiasing;
        self
    }

    /// Registers a callback to run when the native event loop first resumes.
    ///
    /// Setup callbacks run once, in registration order, before Aimer creates
    /// its first window. The framework's platform setup runs before callbacks
    /// registered by the application.
    ///
    /// The returned value is retained until the application exits, allowing
    /// native resources created by the callback to remain alive for the full
    /// application lifecycle.
    #[inline]
    pub fn setup<R: 'static>(mut self, setup: impl FnOnce() -> R + 'static) -> Self {
        self.startup_hooks.push(Box::new(move || Box::new(setup())));
        self
    }

    /// Installs the root widget and completes the application builder.
    #[inline]
    pub fn child<W: Widget + 'static>(self, child: W) -> AimerApp<W> {
        AimerApp {
            child,
            antialiasing: self.antialiasing,
            startup_hooks: self.startup_hooks,
        }
    }

    /// Starts a native application with `widget` as its root widget.
    pub fn start<W: Widget + 'static>(widget: W) {
        Self::new().child(widget).run();
    }

    /// Starts a native application and runs `setup` once the platform event
    /// loop is ready.
    ///
    /// The value returned by `setup` is retained until the application exits.
    /// This allows platform resources such as macOS application menus to remain
    /// alive for the complete native application lifecycle.
    ///
    /// # Platform initialization
    ///
    /// The callback runs on the event-loop thread during the application's
    /// first resume, before Aimer creates its window. APIs that require an
    /// initialized native application or main-thread access should be called
    /// from this callback rather than before [`Self::start_with_setup`].
    pub fn start_with_setup<W, R>(widget: W, setup: impl FnOnce() -> R + 'static)
    where
        W: Widget + 'static,
        R: 'static,
    {
        Self::new().child(widget).run_with_setup(setup);
    }

    pub fn start_headless<W: Widget + 'static>(widget: W) -> HeadlessAimerApp<ModalHost<W>> {
        Self::new().child(widget).run_headless()
    }

    pub fn start_headless_with<W: Widget + 'static>(
        widget: W,
        options: HeadlessOptions,
    ) -> HeadlessAimerApp<ModalHost<W>> {
        Self::new().child(widget).run_headless_with(options)
    }
}

impl Default for AimerApp {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: Widget + 'static> AimerApp<W> {
    /// Returns the configured antialiasing strategy.
    #[inline]
    pub fn antialiasing(&self) -> AntiAlias {
        self.antialiasing
    }

    /// Starts this configured application on the native event loop.
    pub fn run(self) {
        start_event_loop(
            ModalHost::new().child(self.child),
            self.startup_hooks,
            self.antialiasing,
        );
    }

    /// Starts this configured application and runs `setup` before its window is
    /// created, retaining the returned resource until shutdown.
    pub fn run_with_setup<R: 'static>(mut self, setup: impl FnOnce() -> R + 'static) {
        self.startup_hooks.push(Box::new(move || Box::new(setup())));
        start_event_loop(
            ModalHost::new().child(self.child),
            self.startup_hooks,
            self.antialiasing,
        );
    }

    /// Starts this configured application without creating a native window.
    pub fn run_headless(self) -> HeadlessAimerApp<ModalHost<W>> {
        self.run_headless_with(HeadlessOptions::default())
    }

    /// Starts this configured application headlessly with explicit display
    /// properties.
    pub fn run_headless_with(self, options: HeadlessOptions) -> HeadlessAimerApp<ModalHost<W>> {
        HeadlessAimerApp::new(
            ModalHost::new().child(self.child),
            options,
            self.antialiasing,
        )
    }
}

fn start_event_loop(
    widget: impl Widget + 'static,
    startup_hooks: Vec<StartupHook>,
    antialiasing: AntiAlias,
) {
    if APP_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }

    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    info!("Initializing EventLoop...");
    #[cfg(not(target_os = "android"))]
    let event_loop = EventLoop::<AimerNativePlatformEvent>::with_user_event()
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

        EventLoop::<AimerNativePlatformEvent>::with_user_event()
            .with_android_app(app)
            .build()
            .expect("Failed to create EventLoop")
    };

    EVENT_PROXY.set(event_loop.create_proxy()).ok();

    // Route animation redraws requests through the event loop instead of letting
    // animating widgets (e.g. scroll momentum) spawn a sleeping thread per frame.
    // `FrameReady` is delivered via `user_event` after the current frame, which
    // schedules the next redraw safely even on platforms (iOS) that coalesce a
    // synchronous `request_redraw()` issued from inside the draw cycle.
    aimer_events::window::set_redraw_requester(|| {
        if !try_begin_frame_ready_request(&FRAME_READY_PENDING) {
            return;
        }
        let sent = EVENT_PROXY
            .get()
            .is_some_and(|proxy| proxy.send_event(AimerNativePlatformEvent::FrameReady).is_ok());
        if !sent {
            complete_frame_ready_request(&FRAME_READY_PENDING);
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
        DEFAULT_INSPECTOR_ADDRESS.parse::<IpAddr>().unwrap(),
        DEFAULT_INSPECTOR_PORT.parse::<u16>().unwrap(),
    );
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    let inspector = aimer_inspector::start(DEFAULT_INSPECTOR_PORT.parse::<u16>().unwrap());

    info!("Creating App instance...");
    let mut app = AimerApplicationHandler {
        window: None,
        render_ctx: AimerRenderContext::new(antialiasing),
        widget_root: None,
        event_dispatcher: aimer_widget::EventDispatcher::new(),
        scroll_smoother: crate::handler::scroll_classifier::DualScroller::new(),
        #[cfg(target_arch = "wasm32")]
        web_scroll_phase: crate::handler::web_scroll_phase::WebScrollPhase::new(),
        pending_widget: Some(widget),
        cursor_pos: crate::handler::event_handler::CURSOR_OUTSIDE_POSITION,
        pressed_button: None,
        current_modifiers: Default::default(),
        ime_composing: false,
        window_scale: 1.0,
        native_window_size: None,
        pending_resize: None,
        startup_hooks,
        startup_resources: Vec::new(),
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
        active_touch_id: None,
        file_drag: crate::handler::file_drag::FileDrag::new(),
    };

    info!("Started main event loop");

    // On iOS, this function never returns.
    match event_loop.run_app(&mut app) {
        Ok(_) => info!("EventLoop finished successfully"),
        Err(e) => aimer_utils::error!("EventLoop::run_app failed: {:?}", e),
    }
    #[cfg(not(target_arch = "wasm32"))]
    app.async_runtime.shutdown_background();
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use aimer_attribute::position::Vec2d;
    use aimer_attribute::size::ResolvedSize;
    use aimer_events::element::{ElementEvent, ScrollDeltaKind};
    use aimer_widget::base::BuildContext;
    use aimer_widget::{
        AnyElement, Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement,
    };
    use winit::dpi::{PhysicalPosition, PhysicalSize};
    use winit::event::{DeviceId, MouseScrollDelta, TouchPhase, WindowEvent};

    use super::*;

    #[test]
    fn pending_frame_ready_requests_are_coalesced_until_delivery() {
        let pending = AtomicBool::new(false);

        assert!(try_begin_frame_ready_request(&pending));
        assert!(!try_begin_frame_ready_request(&pending));

        complete_frame_ready_request(&pending);

        assert!(try_begin_frame_ready_request(&pending));
    }

    #[test]
    fn app_builder_defaults_to_analytic_antialiasing() {
        let app = AimerApp::new().child(RedrawWidget);

        assert_eq!(app.antialiasing(), AntiAlias::Analytic);
    }

    #[test]
    fn app_builder_preserves_the_selected_antialiasing_mode() {
        let app = AimerApp::new()
            .with_antialiasing(AntiAlias::Msaa2x)
            .child(RedrawWidget);

        assert_eq!(app.antialiasing(), AntiAlias::Msaa2x);
    }

    #[test]
    fn app_builder_appends_each_setup_hook() {
        let default_hook_count = usize::from(cfg!(target_os = "macos"));
        let app = AimerApp::new()
            .setup(|| "first resource")
            .setup(|| "second resource")
            .child(RedrawWidget);

        assert_eq!(app.startup_hooks.len(), default_hook_count + 2);
    }

    struct RecordingWidget {
        builds: Arc<AtomicUsize>,
        cancels: Arc<AtomicUsize>,
    }

    impl Widget for RecordingWidget {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            self.builds.fetch_add(1, Ordering::SeqCst);
            RecordingElement {
                cancels: self.cancels.clone(),
            }
            .boxed()
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
        fn on_event(&self, event: &ElementEvent) -> aimer_widget::EventResult {
            if matches!(event, ElementEvent::Cancel) {
                self.cancels.fetch_add(1, Ordering::SeqCst);
            }
            aimer_widget::EventResult::ignored()
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
        assert_eq!(
            app.logical_size(),
            ResolvedSize {
                width: 1150.0,
                height: 800.0
            }
        );
    }

    #[test]
    fn headless_start_installs_the_framework_modal_host() {
        let mut app = AimerApp::start_headless(RecordingWidget {
            builds: Arc::new(AtomicUsize::new(0)),
            cancels: Arc::new(AtomicUsize::new(0)),
        });

        app.render_frame();

        assert_eq!(
            app.app.widget_root.as_ref().map(|root| root.debug_name()),
            Some("ModalHost")
        );
    }

    #[test]
    fn framework_modal_show_and_dismiss_use_the_headless_root_overlay() {
        use aimer_modal::{Modal, ModalController};

        let cancels = Arc::new(AtomicUsize::new(0));
        let mut app = AimerApp::start_headless(RecordingWidget {
            builds: Arc::new(AtomicUsize::new(0)),
            cancels: cancels.clone(),
        });
        let handle = Modal::new()
            .child(RecordingWidget {
                builds: Arc::new(AtomicUsize::new(0)),
                cancels: Arc::new(AtomicUsize::new(0)),
            })
            .show();

        app.render_frame();

        assert!(ModalController::is_showing());
        assert_eq!(cancels.load(Ordering::SeqCst), 1);

        assert!(handle.dismiss());
        app.render_frame();

        assert!(!ModalController::is_showing());

        Modal::new()
            .child(RecordingWidget {
                builds: Arc::new(AtomicUsize::new(0)),
                cancels: Arc::new(AtomicUsize::new(0)),
            })
            .show();
        app.render_frame();
        assert!(ModalController::is_showing());

        drop(app);
        assert!(!ModalController::is_showing());
    }

    #[test]
    fn headless_window_events_update_metrics_and_reach_widgets() {
        let cancels = Arc::new(AtomicUsize::new(0));
        let mut app = AimerApp::start_headless_with(
            RecordingWidget {
                builds: Arc::new(AtomicUsize::new(0)),
                cancels: cancels.clone(),
            },
            HeadlessOptions {
                size: PhysicalSize::new(640, 480),
                scale_factor: 2.0,
            },
        );
        app.render_frame();

        app.send_window_event(WindowEvent::Focused(false));
        assert!(app.take_redraw_request());
        app.send_window_event(WindowEvent::Resized(PhysicalSize::new(800, 600)));

        assert_eq!(cancels.load(Ordering::SeqCst), 1);
        assert_eq!(app.physical_size(), PhysicalSize::new(800, 600));
        assert_eq!(
            app.logical_size(),
            ResolvedSize {
                width: 400.0,
                height: 300.0
            }
        );
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
        assert_eq!((app.app.cursor_pos.x, app.app.cursor_pos.y), (20.0, 30.0));

        app.send_window_event(WindowEvent::CursorLeft { device_id });
        assert_eq!(
            (app.app.cursor_pos.x, app.app.cursor_pos.y),
            (
                crate::handler::event_handler::CURSOR_OUTSIDE_POSITION.x,
                crate::handler::event_handler::CURSOR_OUTSIDE_POSITION.y
            ),
        );
        assert_eq!(cancels.load(Ordering::SeqCst), 0);

        app.app.cursor_pos = Vec2d { x: 20.0, y: 30.0 };
        app.send_window_event(WindowEvent::CursorEntered { device_id });
        assert_eq!(
            (app.app.cursor_pos.x, app.app.cursor_pos.y),
            (
                crate::handler::event_handler::CURSOR_OUTSIDE_POSITION.x,
                crate::handler::event_handler::CURSOR_OUTSIDE_POSITION.y
            ),
        );
        assert!(app.take_redraw_request());
    }

    struct CapturingWidget {
        events: Arc<AtomicUsize>,
    }

    impl Widget for CapturingWidget {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            CapturingElement {
                events: self.events.clone(),
            }
            .boxed()
        }
    }

    struct CapturingElement {
        events: Arc<AtomicUsize>,
    }

    impl Drawable for CapturingElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl LayoutElement for CapturingElement {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some((Vec2d::default(), Vec2d { x: 100.0, y: 100.0 }))
        }
    }
    impl Rebuildable for CapturingElement {}
    impl VisitorElement for CapturingElement {
        fn debug_name(&self) -> &'static str {
            "CapturingElement"
        }
    }
    impl EventElement for CapturingElement {
        fn on_event(&self, event: &ElementEvent) -> aimer_widget::EventResult {
            self.events.fetch_add(1, Ordering::SeqCst);
            match event {
                ElementEvent::PointerDown(pointer) => aimer_widget::EventResult::consumed()
                    .with_pointer_capture(aimer_widget::PointerKey::new(
                        pointer.source,
                        pointer.id,
                    )),
                ElementEvent::PointerUp(pointer) => aimer_widget::EventResult::consumed()
                    .with_pointer_release(aimer_widget::PointerKey::new(
                        pointer.source,
                        pointer.id,
                    )),
                _ => aimer_widget::EventResult::consumed(),
            }
        }
    }

    #[test]
    fn headless_pointer_capture_persists_across_frames_and_releases_on_up() {
        use winit::event::{ElementState, MouseButton};

        let events = Arc::new(AtomicUsize::new(0));
        let mut app = AimerApp::start_headless(CapturingWidget {
            events: events.clone(),
        });
        app.render_frame();
        let device_id = DeviceId::dummy();
        app.send_window_event(WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(20.0, 20.0),
        });
        events.store(0, Ordering::SeqCst);
        app.send_window_event(WindowEvent::MouseInput {
            device_id,
            state: ElementState::Pressed,
            button: MouseButton::Left,
        });
        app.send_window_event(WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(200.0, 200.0),
        });
        app.render_frame();
        app.send_window_event(WindowEvent::MouseInput {
            device_id,
            state: ElementState::Released,
            button: MouseButton::Left,
        });
        assert_eq!(events.load(Ordering::SeqCst), 3);

        app.send_window_event(WindowEvent::CursorMoved {
            device_id,
            position: PhysicalPosition::new(300.0, 300.0),
        });
        assert_eq!(events.load(Ordering::SeqCst), 3);
    }

    struct ScrollRecordingWidget {
        events: Arc<Mutex<Vec<(Vec2d, ScrollDeltaKind, TouchPhase)>>>,
    }

    impl Widget for ScrollRecordingWidget {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            ScrollRecordingElement {
                events: self.events.clone(),
            }
            .boxed()
        }
    }

    struct ScrollRecordingElement {
        events: Arc<Mutex<Vec<(Vec2d, ScrollDeltaKind, TouchPhase)>>>,
    }

    impl Drawable for ScrollRecordingElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }
    impl LayoutElement for ScrollRecordingElement {
        fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
            Some((Vec2d::default(), Vec2d { x: 100.0, y: 100.0 }))
        }
    }
    impl Rebuildable for ScrollRecordingElement {}
    impl VisitorElement for ScrollRecordingElement {
        fn debug_name(&self) -> &'static str {
            "ScrollRecordingElement"
        }
    }
    impl EventElement for ScrollRecordingElement {
        fn on_event(&self, event: &ElementEvent) -> aimer_widget::EventResult {
            if let ElementEvent::Scroll {
                delta,
                kind,
                phase,
                ..
            } = event
            {
                self.events
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push((*delta, *kind, *phase));
                return aimer_widget::EventResult::consumed();
            }
            aimer_widget::EventResult::ignored()
        }
    }

    #[test]
    fn headless_wasm_wheel_delta_is_smoothed_without_changing_distance() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut app = AimerApp::start_headless(ScrollRecordingWidget {
            events: events.clone(),
        });
        app.render_frame();
        app.app.cursor_pos = Vec2d { x: 20.0, y: 20.0 };

        app.send_window_event(WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -8.00048828125)),
            phase: TouchPhase::Moved,
        });
        assert!(events.lock().unwrap().is_empty());

        let mut frames = 0;
        while app.app.scroll_smoother.is_active() || frames == 0 {
            app.render_frame();
            frames += 1;
            assert!(frames < 30);
        }

        let events = events.lock().unwrap();
        assert!(events.len() > 1);
        assert!(
            events
                .iter()
                .all(|(_, kind, _)| *kind == ScrollDeltaKind::Line)
        );
        assert!(events.iter().all(|(delta, _, _)| delta.y < 0.0));
    }

    #[test]
    fn wheel_scroll_phases_reach_the_child_widget() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut app = AimerApp::start_headless(ScrollRecordingWidget {
            events: events.clone(),
        });
        app.render_frame();
        app.app.cursor_pos = Vec2d { x: 20.0, y: 20.0 };

        app.send_window_event(WindowEvent::MouseWheel {
            device_id: DeviceId::dummy(),
            delta: MouseScrollDelta::LineDelta(0.0, -2.0),
            phase: TouchPhase::Moved,
        });

        let mut frames = 0;
        while app.app.scroll_smoother.is_active() || frames == 0 {
            app.render_frame();
            frames += 1;
            assert!(frames < 64);
        }

        let events = events.lock().unwrap();
        let phases: Vec<TouchPhase> = events.iter().map(|(_, _, phase)| *phase).collect();

        assert_eq!(phases.first(), Some(&TouchPhase::Started));
        assert_eq!(phases.last(), Some(&TouchPhase::Ended));
        assert!(
            phases
                .iter()
                .filter(|phase| **phase == TouchPhase::Started)
                .count()
                == 1
        );
        assert!(
            phases[1..phases.len() - 1]
                .iter()
                .all(|phase| *phase == TouchPhase::Moved)
        );
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
            HeadlessOptions {
                size: PhysicalSize::new(320, 240),
                scale_factor: 0.0,
            },
        );

        assert_eq!(app.scale_factor(), 1.0);
        assert_eq!(
            app.logical_size(),
            ResolvedSize {
                width: 320.0,
                height: 240.0
            }
        );
    }

    struct RedrawWidget;

    impl Widget for RedrawWidget {
        fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
            RedrawElement.boxed()
        }
    }

    struct RedrawElement;

    impl Drawable for RedrawElement {
        fn draw(&self, ctx: &BuildContext) {
            ctx.window.request_redraw();
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
