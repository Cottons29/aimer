use std::cell::Cell;
#[allow(unused)]
use crate::render;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use widget::base::BuildContext;
use widget::{Element, Widget};
use winit::application::ApplicationHandler;
#[allow(unused)]
use winit::dpi::{LogicalSize, PhysicalSize, Position};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
#[allow(unused)]
use winit::monitor::MonitorHandle;
#[allow(unused)]
use winit::window::{self, Fullscreen, Window, WindowAttributes, WindowId};

#[cfg(any(target_os = "macos", target_os = "ios"))]
use objc2::rc::Retained;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use objc2::runtime::ProtocolObject;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use objc2_core_foundation::CGSize;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use objc2_metal::{MTLCommandBuffer, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice};
#[cfg(any(target_os = "macos", target_os = "ios"))]
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
#[cfg(any(target_os = "macos", target_os = "ios"))]
use skia_safe::gpu::{self, backend_render_targets, mtl, DirectContext, SurfaceOrigin};
#[cfg(any(target_os = "macos", target_os = "ios"))]
use skia_safe::ColorType;
#[cfg(any(target_os = "macos", target_os = "ios"))]
use winit::raw_window_handle::HasWindowHandle;

#[cfg(target_os = "android")]
use khronos_egl as egl;
use crate::window_event::handle_window_event;
#[cfg(target_os = "android")]
use skia_safe::gpu::{
    self as gpu_android, backend_render_targets as android_backend_render_targets, gl as skia_gl,
    DirectContext as AndroidDirectContext, SurfaceOrigin as AndroidSurfaceOrigin,
};
#[cfg(target_os = "android")]
use skia_safe::ColorType as AndroidColorType;
#[cfg(target_os = "android")]
use winit::raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use inspector::InspectorOverlay;
#[cfg(not(target_arch = "wasm32"))]
use inspector::{InspectorAppHandle, InspectorServer};
use utils::debug;

#[cfg(target_arch = "wasm32")]
pub(crate) type Float = f64;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type Float = f32;

/// Walk the snapshot tree and find a node matching the hovered widget by name and bounds.
#[cfg(debug_assertions)]
fn find_hovered_node(node: &inspector::WidgetNode, name: &str, start: Vec2d, end: Vec2d) -> Option<u64> {
    const EPS: f32 = 1.0;
    let w = (end.x - start.x) as f32;
    let h = (end.y - start.y) as f32;
    if node.name == name
        && (node.x - start.x as f32).abs() < EPS
        && (node.y - start.y as f32).abs() < EPS
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

pub struct AimerAppConfiguration {
    pub window: Option<&'static Window>,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub metal_layer: Option<Retained<CAMetalLayer>>,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub command_queue: Option<Retained<ProtocolObject<dyn MTLCommandQueue>>>,
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    pub skia_context: Option<DirectContext>,
    #[cfg(target_os = "android")]
    pub egl_display: Option<egl::Display>,
    #[cfg(target_os = "android")]
    pub egl_surface: Option<egl::Surface>,
    #[cfg(target_os = "android")]
    pub egl_context: Option<egl::Context>,
    #[cfg(target_os = "android")]
    pub skia_gl_context: Option<gpu_android::DirectContext>,
    #[cfg(target_arch = "wasm32")]
    pub canvas_ctx: Option<web_sys::CanvasRenderingContext2d>,
    pub widget_root: Option<Box<dyn Element>>,
    pub pending_widget: Option<Box<dyn Widget>>,
    pub cursor_pos: Vec2d,
    pub window_scale: f64,
    pub native_window_size: Option<ResolvedSize>,
    pub pending_resize: Option<PhysicalSize<u32>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_runtime: Runtime,
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    pub inspector: inspector::InspectorAppHandle,
    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    pub inspector: inspector::InspectorHandle,
    #[cfg(debug_assertions)]
    pub inspector_change: Cell<bool>,
    #[cfg(debug_assertions)]
    pub inspector_prev_enabled: Cell<bool>,
    #[cfg(debug_assertions)]
    pub inspector_redraw_frames: Cell<u8>
}

impl ApplicationHandler for AimerAppConfiguration {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "ios")]
        {
            match crate::ios_screen::get_screen_resolution_pixels() {
                Some((width, height)) => {
                    self.native_window_size = Some(ResolvedSize { width: width as f32, height: height as f32 })
                }
                None => (),
            };
        }

        let window_attributes = {
            #[cfg(not(target_os = "ios"))]
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
            self.window = Some(window);
        }

        let window = self.window.unwrap();
        let size = window.inner_size();

        println!("Window Size : {size:?}");

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if self.metal_layer.is_none() {
            let device = MTLCreateSystemDefaultDevice().expect("no Metal device found");

            let metal_layer = {
                let layer = CAMetalLayer::new();
                layer.setDevice(Some(&device));
                layer.setPixelFormat(objc2_metal::MTLPixelFormat::BGRA8Unorm);
                layer.setPresentsWithTransaction(false);
                layer.setFramebufferOnly(false);
                layer.setDrawableSize(CGSize::new(size.width as f64, size.height as f64));

                let view_ptr = match window.window_handle().unwrap().as_raw() {
                    #[cfg(target_os = "macos")]
                    raw_window_handle::RawWindowHandle::AppKit(appkit) => {
                        appkit.ns_view.as_ptr() as *mut objc2_app_kit::NSView
                    }
                    #[cfg(target_os = "ios")]
                    raw_window_handle::RawWindowHandle::UiKit(uikit) => {
                        uikit.ui_view.as_ptr() as *mut objc2_ui_kit::UIView
                    }
                    _ => panic!("Unsupported window handle type"),
                };
                let view = unsafe { view_ptr.as_ref().unwrap() };

                #[cfg(target_os = "macos")]
                {
                    view.setWantsLayer(true);
                    view.setLayer(Some(&layer.clone().into_super()));
                }

                #[cfg(target_os = "ios")]
                {
                    layer.setFrame(view.layer().frame());
                    view.layer().addSublayer(&layer);
                }

                layer
            };

            let command_queue = device
                .newCommandQueue()
                .expect("unable to get command queue");

            let backend = unsafe {
                mtl::BackendContext::new(
                    Retained::as_ptr(&device) as mtl::Handle,
                    Retained::as_ptr(&command_queue) as mtl::Handle,
                )
            };

            let skia_context = gpu::direct_contexts::make_metal(&backend, None).unwrap();

            self.metal_layer = Some(metal_layer);
            self.command_queue = Some(command_queue);
            self.skia_context = Some(skia_context);
        }

        #[cfg(target_os = "android")]
        if self.egl_display.is_none() {
            use winit::raw_window_handle::HasWindowHandle;

            let egl_lib = unsafe { egl::DynamicInstance::<egl::EGL1_4>::load_required() }.expect("failed to load EGL");

            let display = unsafe { egl_lib.get_display(egl::DEFAULT_DISPLAY) }.expect("failed to get EGL display");

            egl_lib
                .initialize(display)
                .expect("failed to initialize EGL");

            let config_attribs = [
                egl::RED_SIZE,
                8,
                egl::GREEN_SIZE,
                8,
                egl::BLUE_SIZE,
                8,
                egl::ALPHA_SIZE,
                8,
                egl::DEPTH_SIZE,
                0,
                egl::STENCIL_SIZE,
                8,
                egl::RENDERABLE_TYPE,
                egl::OPENGL_ES2_BIT,
                egl::SURFACE_TYPE,
                egl::WINDOW_BIT,
                egl::NONE,
            ];

            let config = egl_lib
                .choose_first_config(display, &config_attribs)
                .expect("failed to choose EGL config")
                .expect("no matching EGL config");

            let context_attribs = [egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE];
            let egl_context = egl_lib
                .create_context(display, config, None, &context_attribs)
                .expect("failed to create EGL context");

            let native_window = match window.window_handle().unwrap().as_raw() {
                raw_window_handle::RawWindowHandle::AndroidNdk(handle) => handle.a_native_window.as_ptr(),
                _ => panic!("Expected AndroidNdk window handle"),
            };

            let surface_attribs = [egl::NONE];
            let egl_surface =
                unsafe { egl_lib.create_window_surface(display, config, native_window, Some(&surface_attribs)) }
                    .expect("failed to create EGL window surface");

            egl_lib
                .make_current(display, Some(egl_surface), Some(egl_surface), Some(egl_context))
                .expect("failed to make EGL context current");

            let interface = skia_gl::Interface::new_native().expect("failed to create Skia GL interface");
            let skia_context =
                gpu_android::direct_contexts::make_gl(interface, None).expect("failed to create Skia GL DirectContext");

            self.egl_display = Some(display);
            self.egl_surface = Some(egl_surface);
            self.egl_context = Some(egl_context);
            self.skia_gl_context = Some(skia_context);
        }

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowExtWebSys;
            let canvas = window.canvas().unwrap();
            let web_window = web_sys::window().unwrap();
            let document = web_window.document().unwrap();
            let body = document.body().unwrap();
            utils::info!("Creating canvas...");
            body.append_child(&canvas).unwrap();
            utils::info!("Canvas created.");

            canvas.set_attribute("id", "aimer_app").unwrap();

            utils::info!("Getting canvas context...");
            let ctx = canvas
                .get_context("2d")
                .unwrap()
                .unwrap()
                .dyn_into::<web_sys::CanvasRenderingContext2d>()
                .unwrap();
            utils::info!("Canvas context obtained.");
            self.canvas_ctx = Some(ctx);
        }

        self.window_scale = window.scale_factor();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        handle_window_event(self, event_loop, _id, event);
    }

    #[cfg(debug_assertions)]
    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
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
#[allow(dead_code)]
impl AimerAppConfiguration {

    fn render_widget_tree(widget: &dyn Element, ctx: &BuildContext) {
        #[cfg(debug_assertions)]
        if let Ok(mut hovered) = widget::inspector_overlay::HOVERED_WIDGET.write() {
            *hovered = None;
        }

        ctx.canvas.save();
        widget.draw(ctx);
        ctx.canvas.restore();
    }

    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    fn broadcast_inspector_snapshot(&self) {
        if self.inspector.is_enabled() {
            let snapshot = self.widget_root.as_ref().map(|root| {
                InspectorServer::snapshot_tree(root.as_ref())
            });

            let hovered_id = if let Ok(hovered) = widget::inspector_overlay::HOVERED_WIDGET.read() {
                if let Some((name, start, end)) = hovered.as_ref() {
                    snapshot.as_ref().and_then(|s| find_hovered_node(s, name, *start, *end))
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

    #[cfg(all(debug_assertions, target_arch = "wasm32"))]
    fn broadcast_inspector_snapshot(&self) {
        if self.inspector.is_enabled() {
            let snapshot = self.widget_root.as_ref().map(|root| {
                inspector::snapshot_tree(root.as_ref())
            });

            let hovered_id = if let Ok(hovered) = widget::inspector_overlay::HOVERED_WIDGET.read() {
                if let Some((name, start, end)) = hovered.as_ref() {
                    snapshot.as_ref().and_then(|s| find_hovered_node(s, name, *start, *end))
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

        #[allow(clippy::collapsible_if)]
        if let Some(size) = self.pending_resize.take() {
            #[cfg(any(target_os = "macos", target_os = "ios"))]
            if let Some(metal_layer) = &self.metal_layer {
                metal_layer.setDrawableSize(CGSize::new(size.width as f64, size.height as f64));
            }
            #[cfg(target_arch = "wasm32")]
            if let (Some(_ctx), Some(window)) = (&self.canvas_ctx, &self.window) {
                use winit::platform::web::WindowExtWebSys;
                if let Some(canvas) = window.canvas() {
                    canvas.set_width(size.width);
                    canvas.set_height(size.height);
                }
            }
        }

        #[cfg(any(target_os = "macos", target_os = "ios"))]
        if let (Some(metal_layer), Some(command_queue), Some(skia_context), Some(window)) =
            (&self.metal_layer, &self.command_queue, &mut self.skia_context, &self.window)
        {
            let drawable = match metal_layer.nextDrawable() {
                Some(d) => d,
                None => return,
            };

            let (width, height) = {
                let size = metal_layer.drawableSize();
                (size.width as u32, size.height as u32)
            };

            let texture_info = unsafe { mtl::TextureInfo::new(Retained::as_ptr(&drawable.texture()) as mtl::Handle) };

            let backend_render_target = backend_render_targets::make_mtl((width as i32, height as i32), &texture_info);

            let mut surface = gpu::surfaces::wrap_backend_render_target(
                skia_context,
                &backend_render_target,
                SurfaceOrigin::TopLeft,
                ColorType::BGRA8888,
                None,
                None,
            );

            if let Some(ref mut surface) = surface {
                let ctx: BuildContext = BuildContext {
                    parent_size: ResolvedSize { width: width as Float, height: height as Float },
                    canvas: surface.canvas(),
                    scale: self.window_scale as Float,
                    parent_pos: Default::default(),
                    cursor_pos: self.cursor_pos,
                    box_constraint: widget::style::BoxConstraint {
                        min_width: 0.0,
                        min_height: 0.0,
                        max_width: width as Float,
                        max_height: height as Float,
                    },
                    visible_rect: None,
                    window,
                    async_handle: self.async_runtime.handle().clone(),
                    inherited_states: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
                };
                ctx.canvas.clear(skia_safe::Color::WHITE);
                #[allow(clippy::collapsible_if)]
                if self.widget_root.is_none() {
                    if let Some(w) = self.pending_widget.take() {
                        self.widget_root = Some(w.to_element(&ctx));
                    }
                }

                if let Some(root) = &self.widget_root {
                    Self::render_widget_tree(root.as_ref(), &ctx);
                    #[cfg(debug_assertions)]
                    if self.inspector.is_enabled() {
                        InspectorOverlay::draw(root.as_ref(), ctx.canvas, self.cursor_pos, ctx.scale as f32);
                    }
                }
            }

            skia_context.flush_and_submit();
            drop(surface);

            let cmd_buffer = command_queue
                .commandBuffer()
                .expect("unable to get command buffer");

            let drawable_ref: Retained<ProtocolObject<dyn objc2_metal::MTLDrawable>> = (&drawable).into();
            cmd_buffer.presentDrawable(&drawable_ref);
            cmd_buffer.commit();

            #[cfg(debug_assertions)]
            self.broadcast_inspector_snapshot();
        }

        #[cfg(target_os = "android")]
        if let (Some(skia_context), Some(egl_surface), Some(egl_display), Some(window)) =
            (&mut self.skia_gl_context, &self.egl_surface, &self.egl_display, &self.window)
        {
            let size = window.inner_size();
            let width = size.width;
            let height = size.height;

            let fb_info =
                skia_gl::FramebufferInfo { fboid: 0, format: skia_gl::Format::RGBA8.into(), ..Default::default() };

            let backend_render_target =
                android_backend_render_targets::make_gl((width as i32, height as i32), Some(0), 8, fb_info);

            let mut surface = gpu_android::surfaces::wrap_backend_render_target(
                skia_context,
                &backend_render_target,
                AndroidSurfaceOrigin::BottomLeft,
                AndroidColorType::RGBA8888,
                None,
                None,
            );

            if let Some(ref mut surface) = surface {
                let ctx: BuildContext = BuildContext {
                    parent_size: ResolvedSize { width: width as Float, height: height as Float },
                    canvas: surface.canvas(),
                    scale: self.window_scale as Float,
                    parent_pos: Default::default(),
                    cursor_pos: self.cursor_pos,
                    box_constraint: widget::style::BoxConstraint {
                        min_width: 0.0,
                        min_height: 0.0,
                        max_width: width as Float,
                        max_height: height as Float,
                    },
                    visible_rect: None,
                    window,
                    async_handle: self.async_runtime.handle().clone(),
                    inherited_states: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
                };
                ctx.canvas.clear(skia_safe::Color::WHITE);
                #[allow(clippy::collapsible_if)]
                if self.widget_root.is_none() {
                    if let Some(w) = self.pending_widget.take() {
                        self.widget_root = Some(w.to_element(&ctx));
                    }
                }

                if let Some(root) = &self.widget_root {
                    Self::render_widget_tree(root.as_ref(), &ctx);
                    #[cfg(debug_assertions)]
                    if self.inspector.is_enabled() {
                        InspectorOverlay::draw(root.as_ref(), ctx.canvas, self.cursor_pos, ctx.scale as f32);
                    }
                }
            }

            skia_context.flush_and_submit();
            drop(surface);

            let egl_lib = unsafe { egl::DynamicInstance::<egl::EGL1_4>::load_required() }.expect("failed to load EGL");
            egl_lib
                .swap_buffers(*egl_display, *egl_surface)
                .expect("failed to swap EGL buffers");

            #[cfg(debug_assertions)]
            self.broadcast_inspector_snapshot();
        }

        #[cfg(target_arch = "wasm32")]
        {
            // utils::info!("Setting up canvas context...");
            if let (Some(ctx), Some(window)) = (&self.canvas_ctx, &self.window) {
                // utils::info!("Stating render loop...");
                let width = window.inner_size().width;
                let height = window.inner_size().height;

                ctx.clear_rect(0.0, 0.0, width as f64, height as f64);

                let build_ctx = BuildContext {
                    parent_size: ResolvedSize { width: width as f64, height: height as f64 },
                    canvas: ctx,
                    scale: self.window_scale,
                    parent_pos: Default::default(),
                    cursor_pos: self.cursor_pos,
                    box_constraint: widget::style::BoxConstraint {
                        min_width: 0.0,
                        min_height: 0.0,
                        max_width: width as f64,
                        max_height: height as f64,
                    },
                    window,
                    visible_rect: None,
                    #[cfg(not(target_arch = "wasm32"))]
                    async_handle: self.async_runtime.handle().clone(),
                    inherited_states: Default::default(),
                };

                #[allow(clippy::collapsible_if)]
                if self.widget_root.is_none() {
                    if let Some(w) = self.pending_widget.take() {
                        self.widget_root = Some(w.to_element(&build_ctx));
                    }
                }

                if let Some(root) = &self.widget_root {
                    Self::render_widget_tree(root.as_ref(), &build_ctx);
                    #[cfg(debug_assertions)]
                    if self.inspector.is_enabled() {
                        InspectorOverlay::draw(root.as_ref(), build_ctx.canvas, self.cursor_pos, build_ctx.scale as f32);
                    }
                }
            } else {
                utils::info!("Canvas context is not ready yet.");
            }

            #[cfg(debug_assertions)]
            self.broadcast_inspector_snapshot();
        }
    }
}

#[cfg(test)]
mod tests {
    use attribute::position::Vec2d;
    use attribute::size::Size;
    use widget::base::BuildContext;
    use widget::{Drawable, Element};

    struct MockWidget {
        pos: Option<Vec2d>,
        size: Option<Size>,
        children: Vec<Box<dyn Element>>,
    }

    impl Drawable for MockWidget {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl Element for MockWidget {
        fn pos(&self) -> Option<Vec2d> {
            self.pos
        }
        fn size(&self) -> Option<Size> {
            self.size
        }
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }
    }
}
