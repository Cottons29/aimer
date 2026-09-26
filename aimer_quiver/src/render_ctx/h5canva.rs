#[cfg(target_arch = "wasm32")]
pub mod render_ctx {
    use std::cell::RefCell;
    use std::rc::Rc;

    use aimer_cupid::AntiAlias;
    use aimer_cupid::canvas::CupidCanvas;
    use aimer_cupid::compositor::CompositorScene;
    use aimer_cupid::damage_region::DamageSet;
    use aimer_cupid::frame::{Frame, FramePacket, FrameRenderMetadata};
    #[cfg(all(not(feature = "webgl"), not(feature = "webgpu"), feature = "wgpu"))]
    use aimer_cupid::gpu_context::GpuContext;
    #[cfg(all(not(feature = "webgl"), not(feature = "webgpu"), feature = "wgpu"))]
    use aimer_cupid::renderer::Renderer;
    #[cfg(any(feature = "webgl", feature = "webgpu"))]
    use aimer_cupid::renderer::RendererImpl;
    #[cfg(feature = "webgl")]
    use aimer_cupid::{WebGl2Backend, WebGl2TextureFormat, WebGl2TextureView};
    #[cfg(feature = "webgpu")]
    use aimer_cupid::{WebGpuBackend, WebGpuTextureView};
    use aimer_utils::info;
    use winit::dpi::PhysicalSize;
    use winit::platform::web::WindowExtWebSys;
    use winit::window::Window;

    use crate::frame_stats::{FramePhase, PhaseTimer};
    use crate::render_ctx::PresentOutcome;

    #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
    struct GpuState {
        backend: WebGl2Backend,
        surface_view: WebGl2TextureView,
        renderer: RendererImpl<WebGl2Backend>,
        size: (u32, u32),
        canvas: CupidCanvas,
    }

    #[cfg(all(feature = "webgpu", not(feature = "webgl")))]
    struct GpuState {
        backend: WebGpuBackend,
        surface_view: WebGpuTextureView,
        renderer: RendererImpl<WebGpuBackend>,
        size: (u32, u32),
        canvas: CupidCanvas,
    }

    #[cfg(all(feature = "webgpu", feature = "webgl"))]
    enum BrowserGpuState {
        WebGpu {
            backend: WebGpuBackend,
            surface_view: WebGpuTextureView,
            renderer: RendererImpl<WebGpuBackend>,
        },
        WebGl {
            backend: WebGl2Backend,
            surface_view: WebGl2TextureView,
            renderer: RendererImpl<WebGl2Backend>,
        },
    }

    #[cfg(all(feature = "webgpu", feature = "webgl"))]
    struct GpuState {
        backend: BrowserGpuState,
        size: (u32, u32),
        canvas: CupidCanvas,
    }

    #[cfg(all(not(feature = "webgl"), not(feature = "webgpu"), feature = "wgpu"))]
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
        /// Returns true when the browser GPU context is usable.
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

            #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
            {
                let canvas = window.canvas().expect("winit did not provide a browser canvas");
                let backend = WebGl2Backend::new(canvas)
                    .unwrap_or_else(|error| panic!("failed to initialize WebGL2: {error}"));
                backend.resize(size.width, size.height);
                let surface_view = backend.surface_view();
                let renderer = RendererImpl::<WebGl2Backend>::with_antialiasing(
                    &backend,
                    WebGl2TextureFormat::Rgba8Unorm,
                    self.antialiasing,
                );
                *self.state.borrow_mut() = Some(GpuState {
                    backend,
                    surface_view,
                    renderer,
                    size: (size.width, size.height),
                    canvas: CupidCanvas::new(),
                });
                info!("WebGL2 context initialized (wasm).");
                window.request_redraw();
            }

            #[cfg(all(feature = "webgpu", not(feature = "webgl")))]
            {
                let canvas = window.canvas().expect("winit did not provide a browser canvas");
                let state = self.state.clone();
                let antialiasing = self.antialiasing;
                wasm_bindgen_futures::spawn_local(async move {
                    let backend = WebGpuBackend::new(canvas)
                        .await
                        .unwrap_or_else(|error| panic!("failed to initialize WebGPU: {error}"));
                    backend.resize(size.width, size.height);
                    let surface_view = backend.surface_view();
                    let renderer = RendererImpl::<WebGpuBackend>::with_antialiasing(
                        &backend,
                        backend.format(),
                        antialiasing,
                    );
                    *state.borrow_mut() = Some(GpuState {
                        backend,
                        surface_view,
                        renderer,
                        size: (size.width, size.height),
                        canvas: CupidCanvas::new(),
                    });
                    info!("WebGPU context initialized (wasm).");
                    window.request_redraw();
                });
            }

            #[cfg(all(feature = "webgpu", feature = "webgl"))]
            {
                let canvas = window.canvas().expect("winit did not provide a browser canvas");
                let state = self.state.clone();
                let antialiasing = self.antialiasing;
                wasm_bindgen_futures::spawn_local(async move {
                    let backend = match WebGpuBackend::new(canvas.clone()).await {
                        Ok(backend) => {
                            backend.resize(size.width, size.height);
                            let surface_view = backend.surface_view();
                            let renderer = RendererImpl::<WebGpuBackend>::with_antialiasing(
                                &backend,
                                backend.format(),
                                antialiasing,
                            );
                            info!("WebGPU initialized; using it as the browser renderer.");
                            BrowserGpuState::WebGpu {
                                backend,
                                surface_view,
                                renderer,
                            }
                        }
                        Err(error) if error.can_fallback_to_webgl() => {
                            info!("WebGPU unavailable ({error}); falling back to WebGL2.");
                            let backend = WebGl2Backend::new(canvas)
                                .unwrap_or_else(|webgl_error| {
                                    panic!("WebGPU failed ({error}); WebGL2 fallback failed: {webgl_error}")
                                });
                            backend.resize(size.width, size.height);
                            let surface_view = backend.surface_view();
                            let renderer = RendererImpl::<WebGl2Backend>::with_antialiasing(
                                &backend,
                                WebGl2TextureFormat::Rgba8Unorm,
                                antialiasing,
                            );
                            BrowserGpuState::WebGl {
                                backend,
                                surface_view,
                                renderer,
                            }
                        }
                        Err(error) => panic!(
                            "WebGPU failed after acquiring the canvas context; WebGL2 fallback is unavailable: {error}"
                        ),
                    };
                    *state.borrow_mut() = Some(GpuState {
                        backend,
                        size: (size.width, size.height),
                        canvas: CupidCanvas::new(),
                    });
                    window.request_redraw();
                });
            }

            #[cfg(all(not(feature = "webgl"), not(feature = "webgpu"), feature = "wgpu"))]
            {
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
                    window.request_redraw();
                });
            }
        }

        pub fn resize(&mut self, size: PhysicalSize<u32>) {
            if let Some(state) = self.state.borrow_mut().as_mut() {
                #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
                {
                    state.backend.resize(size.width, size.height);
                    state.size = (size.width, size.height);
                }
                #[cfg(all(feature = "webgpu", not(feature = "webgl")))]
                {
                    state.backend.resize(size.width, size.height);
                    state.size = (size.width, size.height);
                }
                #[cfg(all(feature = "webgpu", feature = "webgl"))]
                {
                    match &state.backend {
                        BrowserGpuState::WebGpu { backend, .. } => {
                            backend.resize(size.width, size.height)
                        }
                        BrowserGpuState::WebGl { backend, .. } => {
                            backend.resize(size.width, size.height)
                        }
                    }
                    state.size = (size.width, size.height);
                }
                #[cfg(all(not(feature = "webgl"), not(feature = "webgpu"), feature = "wgpu"))]
                state.gpu.resize(size);
            }
        }

        /// Render a frame using the GPU pipeline, matching the native WgpuApi
        /// interface.
        pub fn render_frame(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32),
        ) -> PresentOutcome {
            match self.build_frame_packet(|canvas, width, height| {
                draw_fn(canvas, width, height);
                (1.0, DamageSet::full(width, height))
            }) {
                Some(packet) => self.present_packet(packet),
                None => PresentOutcome::Dropped,
            }
        }

        /// Records a frame with the widget damage contract and presents it.
        pub fn render_frame_packet(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32) -> (f32, DamageSet),
        ) -> PresentOutcome {
            match self.build_frame_packet(draw_fn) {
                Some(packet) => self.present_packet(packet),
                None => PresentOutcome::Dropped,
            }
        }

        /// Record a frame without touching the swap chain, mirroring the native
        /// `WgpuApi::build_frame`.
        ///
        /// The browser has no usable thread to hand the frame to — `wasm32`
        /// needs `SharedArrayBuffer` plus atomics, and browser GPU objects are
        /// bound to the realm that created them — so the frame is always
        /// presented on the same task. The split is kept so both backends share
        /// one shape, and so the widget walk stops running while a surface
        /// texture is held here too.
        ///
        /// Returns `None` until the browser GPU context is usable.
        pub fn build_frame(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32),
        ) -> Option<Frame> {
            self.build_frame_packet(|canvas, width, height| {
                draw_fn(canvas, width, height);
                (1.0, DamageSet::full(width, height))
            })
            .map(FramePacket::into_frame)
        }

        /// Records a frame and retains the same damage/scene contract as the
        /// native renderer. The web backend still presents inline, but it uses
        /// the same immutable packet shape and safe fallback policy.
        pub fn build_frame_packet(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32) -> (f32, DamageSet),
        ) -> Option<FramePacket> {
            let mut state_ref = self.state.borrow_mut();
            let state = state_ref.as_mut()?;

            #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
            let (width, height) = state.size;
            #[cfg(feature = "webgpu")]
            let (width, height) = state.size;
            #[cfg(all(not(feature = "webgl"), not(feature = "webgpu"), feature = "wgpu"))]
            let (width, height) = (state.gpu.width(), state.gpu.height());

            let build = PhaseTimer::start();
            state.canvas.begin_frame();
            let (scale, damage) = draw_fn(&state.canvas, width, height);
            let damage = if damage.target_size() == (width, height) {
                damage
            } else {
                DamageSet::full(width, height)
            };
            let draw_list = state.canvas.take_draw_list();
            let frame = Frame::new(draw_list, width, height);
            let metadata = FrameRenderMetadata::new(
                scale,
                0,
                0,
                0,
                0,
                damage,
            );
            let scene = state
                .canvas
                .take_scene(
                    &frame.draw_list,
                    width,
                    height,
                    metadata.damage().clone(),
                )
                .unwrap_or_else(|| {
                    CompositorScene::from_draw_list(
                        &frame.draw_list,
                        width,
                        height,
                        metadata.damage().clone(),
                    )
                });
            build.finish(FramePhase::Build);

            Some(FramePacket::with_scene(frame, metadata, scene))
        }

        /// Encode a recorded frame and put it on screen.
        ///
        /// Never returns [`PresentOutcome::Deferred`]: there is no raster thread
        /// on the web, so the outcome is always known by the time this returns.
        /// [`PresentOutcome::Dropped`] means the surface texture could not be
        /// acquired and the caller is expected to request another redraw. The
        /// frame's buffer goes back to the canvas either way.
        pub fn present(&mut self, frame: Frame) -> PresentOutcome {
            let metadata = FrameRenderMetadata::full(frame.width, frame.height);
            self.present_packet(FramePacket::new(frame, metadata))
        }

        /// Encodes a packet produced by [`Self::build_frame_packet`].
        pub fn present_packet(&mut self, packet: FramePacket) -> PresentOutcome {
            let mut state_ref = self.state.borrow_mut();
            let state = match state_ref.as_mut() {
                Some(state) => state,
                None => return PresentOutcome::Dropped, // GPU not ready yet
            };

            let presented = Self::encode(state, &packet);
            state
                .canvas
                .recycle_draw_list(packet.into_frame().into_draw_list());
            PresentOutcome::from_presented(presented)
        }

        fn encode(state: &mut GpuState, packet: &FramePacket) -> bool {
            let encode = PhaseTimer::start();

            #[cfg(all(feature = "webgl", not(feature = "webgpu")))]
            {
                let frame = packet.frame();
                state.renderer.render(
                    &state.backend,
                    &state.surface_view,
                    frame.width,
                    frame.height,
                    false,
                    &frame.draw_list,
                );
                encode.finish(FramePhase::Encode);

                if state.renderer.has_postponed_text_preparation() {
                    aimer_events::window::request_animation_frame();
                }

                let present = PhaseTimer::start();
                state.backend.present();
                present.finish(FramePhase::Present);
            }

            #[cfg(all(feature = "webgpu", not(feature = "webgl")))]
            {
                let frame = packet.frame();
                state.renderer.render(
                    &state.backend,
                    &state.surface_view,
                    frame.width,
                    frame.height,
                    state.backend.is_srgb(),
                    &frame.draw_list,
                );
                encode.finish(FramePhase::Encode);

                if state.renderer.has_postponed_text_preparation() {
                    aimer_events::window::request_animation_frame();
                }

                let present = PhaseTimer::start();
                state.backend.present();
                present.finish(FramePhase::Present);
            }

            #[cfg(all(feature = "webgpu", feature = "webgl"))]
            {
                let frame = packet.frame();
                match &mut state.backend {
                    BrowserGpuState::WebGpu {
                        backend,
                        surface_view,
                        renderer,
                    } => {
                        renderer.render(
                            backend,
                            surface_view,
                            frame.width,
                            frame.height,
                            backend.is_srgb(),
                            &frame.draw_list,
                        );
                        encode.finish(FramePhase::Encode);
                        if renderer.has_postponed_text_preparation() {
                            aimer_events::window::request_animation_frame();
                        }
                        let present = PhaseTimer::start();
                        backend.present();
                        present.finish(FramePhase::Present);
                    }
                    BrowserGpuState::WebGl {
                        backend,
                        surface_view,
                        renderer,
                    } => {
                        renderer.render(
                            backend,
                            surface_view,
                            frame.width,
                            frame.height,
                            false,
                            &frame.draw_list,
                        );
                        encode.finish(FramePhase::Encode);
                        if renderer.has_postponed_text_preparation() {
                            aimer_events::window::request_animation_frame();
                        }
                        let present = PhaseTimer::start();
                        backend.present();
                        present.finish(FramePhase::Present);
                    }
                }
            }

            #[cfg(all(not(feature = "webgl"), not(feature = "webgpu"), feature = "wgpu"))]
            {
            let surface = match state.gpu.begin_frame() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                _ => return false,
            };

            let view = surface.texture.create_view(&Default::default());

            state.renderer.render_packet(
                &state.gpu.device,
                &state.gpu.queue,
                &view,
                packet,
                state.gpu.is_srgb,
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
            }

            true
        }
    }
}
