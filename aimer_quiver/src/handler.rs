pub mod event_handler;
mod user_events;

#[cfg(target_os = "android")]
use crate::aimer_app::ANDROID_APP;
#[cfg(target_os = "android")]
use crate::ffi_utils::android_screen;
#[allow(unused)]
use crate::handler;
use crate::handler::event_handler::WindowEventHandler;
use crate::handler::user_events::handle_user_event;
use crate::render_ctx::AimerRenderContext;
use aimer_attribute::BoxConstraint;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_inspector::InspectorOverlay;
use aimer_utils::{ExecTimes, debug};
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, Widget};
use std::cell::Cell;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use winit::application::ApplicationHandler;
#[allow(unused)]
use winit::dpi::{LogicalSize, PhysicalSize, Position};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
#[allow(unused)]
use winit::monitor::MonitorHandle;
#[allow(unused)]
use winit::window::{self, Fullscreen, Window, WindowAttributes, WindowId};

/// Walk the snapshot tree and find a node matching the hovered widget by name and bounds.
#[cfg(debug_assertions)]
fn find_hovered_node(node: &aimer_inspector::WidgetNode, name: &str, start: Vec2d, end: Vec2d) -> Option<u64> {
    const EPS: f32 = 1.0;
    let w = end.x - start.x;
    let h = end.y - start.y;
    if node.name == name
        && (node.x - start.x).abs() < EPS
        && (node.y - start.y).abs() < EPS
        && (node.width - w).abs() < EPS
        && (node.height - h).abs() < EPS
    {
        return Some(node.id);
    }
    for child in &node.children {
        if let Some(id) = find_hovered_node(child, name, start, end) {
            return Some(id);
        }
    }
    None
}

pub struct AimerApplicationHandler {
    pub window: Option<&'static Window>,
    pub render_ctx: AimerRenderContext,
    pub widget_root: Option<Box<dyn Element>>,
    pub pending_widget: Option<Box<dyn Widget>>,
    pub cursor_pos: Vec2d,
    pub current_modifiers: aimer_events::element::Modifiers,
    /// Whether an IME composition (pre-edit) is currently in progress.
    /// While `true`, raw key events are owned by the input method and must not
    /// be turned into text/navigation input; the composed result arrives via
    /// `WindowEvent::Ime(Ime::Commit(..))`.
    pub ime_composing: bool,
    pub window_scale: f64,
    pub native_window_size: Option<ResolvedSize>,
    pub pending_resize: Option<PhysicalSize<u32>>,
    pub start_up_frames: Cell<u8>,
    /// Timestamp of the last frame actually presented. Used by the desktop
    /// frame limiter to cap the redraw cadence to the display refresh rate.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
    pub last_frame_time: Cell<Option<std::time::Instant>>,
    /// Lazily-computed minimum interval between frames, derived from the active
    /// monitor's refresh rate (fallback 60 Hz).
    #[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
    pub frame_interval: Cell<Option<std::time::Duration>>,
    /// Set when a redraw was deferred by the frame limiter so it can be re-armed
    /// once the next refresh slot is reached.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
    pub pending_redraw: Cell<bool>,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_runtime: Runtime,
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    pub inspector: aimer_inspector::InspectorAppHandle,
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    pub inspector: aimer_inspector::InspectorHandle,
    #[cfg(debug_assertions)]
    pub inspector_change: Cell<bool>,
    #[cfg(debug_assertions)]
    pub inspector_prev_enabled: Cell<bool>,
    #[cfg(debug_assertions)]
    pub inspector_redraw_frames: Cell<u8>,
}

impl ApplicationHandler<crate::aimer_app::AimerCustomAppEvent> for AimerApplicationHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "android")]
        {
            use winit::event_loop::ControlFlow;
            event_loop.set_control_flow(ControlFlow::Poll);
            debug!("Set ControlFlow::Poll for Android");
        }

        #[cfg(target_os = "ios")]
        if let Some((width, height)) = crate::ios_screen::get_screen_resolution_pixels() {
            self.native_window_size = Some(ResolvedSize { width: width as f32, height: height as f32 })
        };

        let window_attributes = {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                WindowAttributes::default().with_title("Aimer")
            }
            #[cfg(target_os = "android")]
            {
                WindowAttributes::default()
            }
            #[cfg(target_os = "ios")]
            {
                match crate::ios_screen::get_screen_resolution_pixels() {
                    Some((w, h)) => {
                        // println!("IOS TARGET Window Size : {w}x{h}");
                        let phy_size = PhysicalSize::new(w as u32, h as u32);
                        WindowAttributes::default().with_inner_size(phy_size)
                    }
                    None => WindowAttributes::default(),
                }
            }
        };

        if self.window.is_none() {
            let window = event_loop.create_window(window_attributes).unwrap();
            let window: &'static Window = Box::leak(Box::new(window)); // Leak to static ref
            aimer_events::window::set_window(window);
            self.window = Some(window);
        }

        let window = self.window.unwrap();

        // winit's iOS window is created without a `UIWindowScene`. On the
        // iOS 26/27 SDK the scene life cycle is mandatory, so a scene-less
        // window stays invisible (black screen) and never redraws. Attach it to
        // the active window scene so it becomes visible and starts redrawing.
        #[cfg(target_os = "ios")]
        crate::ios_screen::attach_window_to_active_scene(window);

        #[allow(unused_mut)]
        let mut size = window.inner_size();

        #[cfg(target_os = "android")]
        {
            if let Some(android_app) = crate::aimer_app::ANDROID_APP.get() {
                if let Some(native_window) = android_app.native_window() {
                    let width = native_window.width() as u32;
                    let height = native_window.height() as u32;
                    size = winit::dpi::PhysicalSize::new(width, height);
                }
            }
        }

        #[cfg(target_os = "ios")]
        {
            let full = window.outer_size();
            if full.width != 0 && full.height != 0 {
                size = PhysicalSize::new(full.width, full.height);
            }
            if size.width == 0 || size.height == 0 {
                let fallback = self
                    .native_window_size
                    .map(|s| PhysicalSize::new(s.width as u32, s.height as u32))
                    .or_else(|| crate::ios_screen::get_screen_resolution_pixels().map(|(w, h)| PhysicalSize::new(w as u32, h as u32)));
                if let Some(fallback) = fallback {
                    debug!("iOS zero window size, using native screen resolution: {fallback:?}");
                    size = fallback;
                }
            }
        }

        debug!("Logical Window Size : {:?}", window.outer_size());
        debug!("Physical Window Size : {size:?}");

        self.render_ctx.initialize(window, size);

        self.window_scale = window.scale_factor();

        // On Android the surface may be (re-)created with the correct size now.
        // Schedule a resize so the GPU surface matches the actual window dimensions.
        self.pending_resize = Some(size);
        // Ensure the first few frames are always rendered even if the desktop
        // frame limiter would otherwise defer them.  On macOS the window
        // manager may fire a Resized event whose RedrawRequested races with
        // this one; without startup frames the limiter can defer the redraw
        // indefinitely, leaving a blank window until the user manually resizes.
        self.start_up_frames.set(5);
        window.request_redraw();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: crate::aimer_app::AimerCustomAppEvent) {
        // debug!("User event {:?}", event);
        handle_user_event(self, event);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        WindowEventHandler::handle_events(self, event_loop, _id, event);
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Re-arm a frame that the desktop limiter deferred. Once the display's
        // refresh interval has elapsed since the last presented frame, request
        // the redraw; otherwise keep sleeping until the deadline.
        #[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
        if self.pending_redraw.get() {
            use winit::event_loop::ControlFlow;
            if let Some(last) = self.last_frame_time.get() {
                let interval = self.frame_interval();
                if last.elapsed() >= interval {
                    self.pending_redraw.set(false);
                    _event_loop.set_control_flow(ControlFlow::Wait);
                    if let Some(window) = self.window {
                        window.request_redraw();
                    }
                } else {
                    _event_loop.set_control_flow(ControlFlow::WaitUntil(last + interval));
                }
            }
        }

        if self.start_up_frames.get() > 0 {
            let Some(window) = self.window.as_ref() else { return };
            window.request_redraw();
            self.start_up_frames.set(self.start_up_frames.get() - 1);
            // debug!("About to wait, {} frames left", self.start_up_frames.get());
        }
        #[cfg(debug_assertions)]
        {
            let current = self.inspector.is_enabled();
            let prev = self.inspector_prev_enabled.get();
            if current != prev {
                self.inspector_prev_enabled.set(current);
                self.inspector_change.set(true);
                self.inspector_redraw_frames.set(5);
            }
            let frames = self.inspector_redraw_frames.get();
            if frames > 0 {
                self.inspector_redraw_frames.set(frames - 1);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }
    }
}
#[allow(dead_code)]
impl AimerApplicationHandler {
    fn render_widget_tree(widget: &dyn Element, ctx: &BuildContext) {
        #[cfg(debug_assertions)]
        if let Ok(mut hovered) = aimer_widget::inspector_overlay::HOVERED_WIDGET.write() {
            *hovered = None;
        }

        ctx.canvas.save();
        widget.draw(ctx);
        ctx.canvas.restore();
    }

    #[cfg(debug_assertions)]
    fn broadcast_inspector_snapshot(&self) {
        if self.inspector.is_enabled() {
            let snapshot = self.widget_root.as_ref().map(|root| {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    aimer_inspector::InspectorServer::snapshot_tree(root.as_ref())
                }
                #[cfg(target_arch = "wasm32")]
                {
                    aimer_inspector::snapshot_tree(root.as_ref())
                }
            });

            let hovered_id = if let Ok(hovered) = aimer_widget::inspector_overlay::HOVERED_WIDGET.read() {
                if let Some((name, start, end)) = hovered.as_ref() {
                    snapshot
                        .as_ref()
                        .and_then(|s| find_hovered_node(s, name, *start, *end))
                } else {
                    None
                }
            } else {
                None
            };

            self.inspector.broadcast_tree(snapshot);
            self.inspector.broadcast_hovered(hovered_id);
        }
    }

    /// Desktop frame limiter. Returns `true` when the current redraw arrived
    /// sooner than the display refresh interval, in which case the caller must
    /// skip rendering; the deferred frame is re-armed in `about_to_wait`.
    ///
    /// When the frame is allowed, records its timestamp and returns to plain
    /// `ControlFlow::Wait` so the loop sleeps until the next event/redraw.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
    fn throttle_frame(&self, event_loop: &ActiveEventLoop) -> bool {
        use std::time::Instant;
        use winit::event_loop::ControlFlow;

        // Never throttle during the first few startup frames.  The GPU surface
        // may still be settling (Resized events racing with RedrawRequested),
        // and deferring here can leave the window blank until the user resizes.
        if self.start_up_frames.get() > 0 {
            self.last_frame_time.set(Some(Instant::now()));
            self.pending_redraw.set(false);
            event_loop.set_control_flow(ControlFlow::Wait);
            return false;
        }

        let interval = self.frame_interval();
        if let Some(last) = self.last_frame_time.get() {
            if last.elapsed() < interval {
                // Too soon: defer to the next refresh slot instead of painting.
                self.pending_redraw.set(true);
                event_loop.set_control_flow(ControlFlow::WaitUntil(last + interval));
                return true;
            }
        }
        self.last_frame_time.set(Some(Instant::now()));
        self.pending_redraw.set(false);
        event_loop.set_control_flow(ControlFlow::Wait);
        false
    }

    /// Minimum interval between frames, derived from the active monitor's
    /// refresh rate so the app never renders more frames than the display can
    /// show (e.g. ~8.33 ms / 120 fps on a ProMotion panel).
    ///
    /// The result is memoized **only once a real refresh rate is read**. Early
    /// in startup (or on platforms where it is briefly unavailable),
    /// `current_monitor()` / `refresh_rate_millihertz()` can return `None`; in
    /// that case we fall back to 60 Hz for this frame only and retry on the
    /// next frame instead of poisoning the cache with 60 Hz for the whole
    /// session. This is what lets a 120 Hz display actually reach 120 fps.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
    fn frame_interval(&self) -> std::time::Duration {
        use std::time::Duration;

        if let Some(interval) = self.frame_interval.get() {
            return interval;
        }
        let real_hz = self
            .window
            .and_then(|w| w.current_monitor())
            .and_then(|m| m.refresh_rate_millihertz())
            .map(|mhz| mhz as f64 / 1000.0)
            .filter(|hz| *hz > 1.0);
        let hz = real_hz.unwrap_or(60.0);
        let interval = Duration::from_secs_f64(1.0 / hz);
        // Cache only when the rate is genuine; otherwise leave the cache empty
        // so a later frame can re-read the true refresh rate.
        if real_hz.is_some() {
            debug!("Frame limiter locked to {:.1} Hz refresh rate", hz);
            self.frame_interval.set(Some(interval));
        }
        interval
    }

    #[allow(unused)]
    pub(crate) fn render(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "android")]
        {
            if let Some(android_app) = crate::aimer_app::ANDROID_APP.get() {
                let Some(native_window) = android_app.native_window() else {
                    debug!("Android native window is not ready yet");
                    return;
                };
            }
        }

        #[cfg(debug_assertions)]
        {
            let current = self.inspector.is_enabled();
            let prev = self.inspector_prev_enabled.get();
            if current != prev {
                self.inspector_prev_enabled.set(current);
                self.inspector_change.set(true);
                self.inspector_redraw_frames.set(5);
            }
            let frames = self.inspector_redraw_frames.get();
            if frames > 0 {
                self.inspector_redraw_frames.set(frames - 1);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
        }

        // Frame-rate limiter (desktop only). The momentum/scroll animation
        // re-arms `window.request_redraw()` from inside the draw cycle
        // (see `draw_scroll.rs`). Under `ControlFlow::Wait` each request wakes
        // the loop immediately, so on macOS frames render back-to-back far
        // beyond the monitor's refresh rate (framerate overflow). Pace them to
        // the display refresh interval with `ControlFlow::WaitUntil`.
        #[cfg(not(any(target_arch = "wasm32", target_os = "android", target_os = "ios")))]
        if self.throttle_frame(event_loop) {
            return;
        }

        #[allow(clippy::collapsible_if)]
        if let Some(size) = self.pending_resize.take() {
            self.render_ctx.resize(size);
        }

        let Some(window) = self.window else { return };
        let window_scale = self.window_scale;
        let cursor_pos = self.cursor_pos;

        #[cfg(not(target_arch = "wasm32"))]
        let async_handle = self.async_runtime.handle().clone();
        let widget_root = &mut self.widget_root;
        let pending_widget = &mut self.pending_widget;
        #[cfg(debug_assertions)]
        let inspector_enabled = self.inspector.is_enabled();

        let draw_widgets = |canvas: &aimer_canvas::InnerCanvas, width: u32, height: u32| {
            let canvas = aimer_canvas::Canvas::new(canvas);
            let build_ctx = BuildContext {
                parent_size: ResolvedSize { width: width as f32, height: height as f32 },
                canvas: canvas.clone(),
                scale: window_scale as f32,
                parent_pos: Default::default(),
                cursor_pos,
                box_constraint: BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: width as f32, max_height: height as f32 },
                visible_rect: None,
                window,
                #[cfg(not(target_arch = "wasm32"))]
                async_handle: async_handle.clone(),
                inherited_states: Default::default(),
            };

            #[allow(clippy::collapsible_if)]
            if widget_root.is_none() {
                if let Some(w) = pending_widget.take() {
                    *widget_root = Some(w.to_element(&build_ctx));
                }
            }

            if let Some(root) = widget_root {
                Self::render_widget_tree(root.as_ref(), &build_ctx);
                #[cfg(debug_assertions)]
                if inspector_enabled {
                    // Save and restore canvas state to ensure the inspector overlay
                    // always renders at the top layer above all widgets,
                    // unaffected by any residual transforms.
                    build_ctx.canvas.save();
                    InspectorOverlay::draw(root.as_ref(), &build_ctx.canvas, cursor_pos, build_ctx.scale);
                    build_ctx.canvas.restore();
                }
            }
        };

        let presented = self.render_ctx.render_frame(draw_widgets);
        // let presented = ExecTimes::no_param("AimerApplicationHandler::RenderingFrame", || self.render_ctx.render_frame(draw_widgets));
        #[cfg(target_os = "ios")]
        // debug!("iOS render(): presented={presented}");
        if !presented {
            // Surface texture was not available (e.g. surface outdated or
            // window not ready).  Request a redraw so we retry next frame
            // instead of staying blank.
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

        #[cfg(debug_assertions)]
        self.broadcast_inspector_snapshot();
    }
}
