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
    pub ime_composing: bool,
    pub window_scale: f64,
    pub native_window_size: Option<ResolvedSize>,
    pub pending_resize: Option<PhysicalSize<u32>>,
    pub start_up_frames: Cell<u8>,
    pub active_touch_id: Option<u64>,
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

        // Presentation is paced by v-sync (`PresentMode::Fifo`): `present()`
        // blocks until the display's next refresh slot, so the scroll/momentum
        // animation re-arming `request_redraw()` from inside the draw cycle is
        // naturally throttled to the panel refresh rate without a software
        // limiter racing the compositor's v-sync.
        // Only consume pending_resize if the render context is actually ready.
        // On web, GPU init is async — consuming the resize before the GPU exists
        // would silently drop it and leave the surface at the wrong size.
        #[allow(clippy::collapsible_if)]
        if self.render_ctx.is_ready() {
            if let Some(size) = self.pending_resize.take() {
                self.render_ctx.resize(size);
            }
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
        if !presented {
            // Surface texture was not available (e.g. surface outdated or
            // window not ready).  Request a redraw so we retry next frame
            // instead of staying blank.  Critical on web (async GPU init)
            // and iOS (late surface availability).
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

        #[cfg(debug_assertions)]
        self.broadcast_inspector_snapshot();
    }
}
