#[cfg(target_arch = "wasm32")]
pub mod render_ctx {
    use crate::aimer_app::{AimerCustomAppEvent, EVENT_PROXY};
    use aimer_cupid::canvas::CupidCanvas;
    use aimer_cupid::gpu_context::GpuContext;
    use aimer_cupid::renderer::Renderer;
    use aimer_utils::info;
    use std::cell::RefCell;
    use std::rc::Rc;
    use winit::dpi::PhysicalSize;
    use winit::event_loop::EventLoop;
    use winit::platform::web::WindowExtWebSys;
    use winit::window::Window;

    struct GpuState {
        gpu: GpuContext<'static>,
        renderer: Renderer,
        canvas: CupidCanvas,
    }

    pub struct H5CanvasApi {
        state: Rc<RefCell<Option<GpuState>>>,
    }

    impl Default for H5CanvasApi {
        fn default() -> Self {
            Self { state: Rc::new(RefCell::new(None)) }
        }
    }

    impl H5CanvasApi {
        /// Returns true when the async GPU init has completed and the context is usable.
        pub fn is_ready(&self) -> bool {
            self.state.borrow().is_some()
        }

        pub fn initialize(&mut self, window: &'static Window, size: PhysicalSize<u32>) {
            // Append the winit canvas to the DOM
            if let Some(canvas) = window.canvas() {
                let web_window = web_sys::window().unwrap();
                let document = web_window.document().unwrap();
                let body = document.body().unwrap();
                info!("Creating canvas...");
                body.append_child(&canvas).unwrap();
                canvas.set_attribute("id", "aimer_app").unwrap();

                // Without `touch-action: none`, mobile browsers treat a touch drag
                // on the canvas as a page pan/pinch and fire `pointercancel`
                // mid-gesture — winit reports that as a cancelled touch, so the
                // scrollable never receives a continuous PointerMove stream and
                // scroll feels broken/janky compared to native. Telling the browser
                // not to perform any default touch gesture on the canvas lets every
                // touchmove reach the app, matching native scroll behaviour.
                // Note: winit's `prevent_default` alone is insufficient here because
                // per the Pointer Events spec, calling preventDefault on pointerdown
                // does not stop scrolling — only `touch-action` does.
                // Use `style().set_property` (not `set_attribute("style", ..)`) so we
                // don't clobber the width/height styles winit sets on resize.
                let _ = canvas.style().set_property("touch-action", "none");
                info!("Canvas created.");
            }

            // Spawn async GPU initialization
            let state = self.state.clone();
            wasm_bindgen_futures::spawn_local(async move {
                info!("Initializing GPU context (wasm)...");
                let gpu = GpuContext::initialize_async(window, size).await;
                let canvas = CupidCanvas::new();
                let renderer = Renderer::new(&gpu.device, gpu.format);
                *state.borrow_mut() = Some(GpuState { gpu, renderer, canvas });
                info!("GPU context initialized (wasm).");
                // Request a redraw so the first frame renders
                window.request_redraw();
            });
        }

        pub fn resize(&mut self, size: PhysicalSize<u32>) {
            if let Some(state) = self.state.borrow_mut().as_mut() {
                state.gpu.resize(size);
            }
        }

        /// Render a frame using the GPU pipeline, matching the native WgpuApi interface.
        pub fn render_frame(&mut self, draw_fn: impl FnOnce(&CupidCanvas, u32, u32)) -> bool {
            let mut state_ref = self.state.borrow_mut();
            let state = match state_ref.as_mut() {
                Some(s) => s,
                None => return false, // GPU not ready yet
            };

            let frame = match state.gpu.begin_frame() {
                wgpu::CurrentSurfaceTexture::Success(texture) | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                _ => return false,
            };

            let view = frame.texture.create_view(&Default::default());

            let width = state.gpu.width();
            let height = state.gpu.height();

            state.canvas.begin_frame();
            draw_fn(&state.canvas, width, height);

            let draw_list = state.canvas.draw_list();
            state
                .renderer
                .render(&state.gpu.device, &state.gpu.queue, &view, width, height, state.gpu.is_srgb, &draw_list);

            state.gpu.end_frame(frame);
            true
        }
    }
}
