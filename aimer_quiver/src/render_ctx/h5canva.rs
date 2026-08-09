#[cfg(target_arch = "wasm32")]
pub mod render_ctx {
    use std::cell::RefCell;
    use std::rc::Rc;

    use aimer_cupid::AntiAlias;
    use aimer_cupid::canvas::CupidCanvas;
    use aimer_cupid::frame::Frame;
    use aimer_cupid::gpu_context::GpuContext;
    use aimer_cupid::renderer::Renderer;
    use aimer_utils::info;
    use winit::dpi::PhysicalSize;
    use winit::event_loop::EventLoop;
    use winit::platform::web::WindowExtWebSys;
    use winit::window::Window;

    use crate::frame_stats::{FramePhase, PhaseTimer};
    use crate::render_ctx::PresentOutcome;

    struct GpuState {
        gpu: GpuContext<'static>,
        renderer: Renderer,
        canvas: CupidCanvas,
    }

    pub struct H5CanvasApi {
        state: Rc<RefCell<Option<GpuState>>>,
        antialiasing: AntiAlias,
    }

    impl Default for H5CanvasApi {
        fn default() -> Self {
            Self {
                state: Rc::new(RefCell::new(None)),
                antialiasing: AntiAlias::default(),
            }
        }
    }

    impl H5CanvasApi {
        #[inline]
        pub fn new(antialiasing: AntiAlias) -> Self {
            Self {
                state: Rc::new(RefCell::new(None)),
                antialiasing,
            }
        }
        /// Returns true when the async GPU init has completed and the context
        /// is usable.
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
            let antialiasing = self.antialiasing;
            wasm_bindgen_futures::spawn_local(async move {
                info!("Initializing GPU context (wasm)...");
                let gpu = GpuContext::initialize_async(window, size).await;
                let canvas = CupidCanvas::new();
                let renderer = Renderer::with_antialiasing(&gpu.device, gpu.format, antialiasing);
                *state.borrow_mut() = Some(GpuState {
                    gpu,
                    renderer,
                    canvas,
                });
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

        /// Render a frame using the GPU pipeline, matching the native WgpuApi
        /// interface.
        pub fn render_frame(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32),
        ) -> PresentOutcome {
            match self.build_frame(draw_fn) {
                Some(frame) => self.present(frame),
                None => PresentOutcome::Dropped,
            }
        }

        /// Record a frame without touching the swap chain, mirroring the native
        /// `WgpuApi::build_frame`.
        ///
        /// The browser has no usable thread to hand the frame to — `wasm32`
        /// needs `SharedArrayBuffer` plus atomics, and the WebGPU objects are
        /// bound to the realm that created them — so the frame is always
        /// presented on the same task. The split is kept so both backends share
        /// one shape, and so the widget walk stops running while a surface
        /// texture is held here too.
        ///
        /// Returns `None` until the async GPU init has completed.
        pub fn build_frame(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32),
        ) -> Option<Frame> {
            let mut state_ref = self.state.borrow_mut();
            let state = state_ref.as_mut()?;

            let width = state.gpu.width();
            let height = state.gpu.height();

            let build = PhaseTimer::start();
            state.canvas.begin_frame();
            draw_fn(&state.canvas, width, height);
            let frame = Frame::new(state.canvas.take_draw_list(), width, height);
            build.finish(FramePhase::Build);

            Some(frame)
        }

        /// Encode a recorded frame and put it on screen.
        ///
        /// Never returns [`PresentOutcome::Deferred`]: there is no raster thread
        /// on the web, so the outcome is always known by the time this returns.
        /// [`PresentOutcome::Dropped`] means the surface texture could not be
        /// acquired and the caller is expected to request another redraw. The
        /// frame's buffer goes back to the canvas either way.
        pub fn present(&mut self, frame: Frame) -> PresentOutcome {
            let mut state_ref = self.state.borrow_mut();
            let state = match state_ref.as_mut() {
                Some(state) => state,
                None => return PresentOutcome::Dropped, // GPU not ready yet
            };

            let presented = Self::encode(state, &frame);
            state.canvas.recycle_draw_list(frame.into_draw_list());
            PresentOutcome::from_presented(presented)
        }

        fn encode(state: &mut GpuState, frame: &Frame) -> bool {
            let encode = PhaseTimer::start();

            let surface = match state.gpu.begin_frame() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                _ => return false,
            };

            let view = surface.texture.create_view(&Default::default());

            state.renderer.render(
                &state.gpu.device,
                &state.gpu.queue,
                &view,
                frame.width,
                frame.height,
                state.gpu.is_srgb,
                &frame.draw_list,
            );
            encode.finish(FramePhase::Encode);

            // A frame prepares text beyond the viewport edges so a line is
            // ready before it scrolls in, and stops when its budget runs out.
            // What it stopped short of is invisible, so this frame is correct
            // as it stands — but nothing else will ask for the rest, and the
            // arrival frame would pay for it. One more frame finishes it while
            // the user is still reading.
            if state.renderer.has_postponed_text_preparation() {
                aimer_events::window::request_animation_frame();
            }

            let present = PhaseTimer::start();
            state.gpu.end_frame(surface);
            present.finish(FramePhase::Present);

            true
        }
    }
}
