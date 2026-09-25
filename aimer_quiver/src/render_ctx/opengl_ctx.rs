pub mod render_ctx {
    use std::sync::Arc;

    use aimer_cupid::AntiAlias;
    use aimer_cupid::backend::opengl::{OpenGlBackend, OpenGlSurface};
    use aimer_cupid::canvas::CupidCanvas;
    use aimer_cupid::damage_region::DamageSet;
    use aimer_cupid::frame::{Frame, FramePacket, FrameRenderMetadata};
    use aimer_cupid::renderer::RendererImpl;
    use winit::dpi::PhysicalSize;
    use winit::window::Window;

    use crate::frame_stats::{FramePhase, PhaseTimer};
    use crate::render_ctx::PresentOutcome;

    /// Quiver's inline OpenGL 3.3 Core renderer for Windows and Linux.
    ///
    /// OpenGL contexts are current on the window thread; this adapter therefore
    /// does not move presentation to Quiver's optional raster thread.
    pub struct OpenGlApi {
        backend: Option<OpenGlBackend>,
        surface: Option<OpenGlSurface>,
        renderer: Option<RendererImpl<OpenGlBackend>>,
        canvas: Option<CupidCanvas>,
        surface_size: PhysicalSize<u32>,
        antialiasing: AntiAlias,
        surface_identity: u64,
        renderer_generation: u64,
        context_generation: u64,
        resource_generation: u64,
    }

    impl Default for OpenGlApi {
        fn default() -> Self {
            Self::new(AntiAlias::default())
        }
    }

    impl OpenGlApi {
        /// Creates an uninitialized desktop OpenGL render context.
        pub fn new(antialiasing: AntiAlias) -> Self {
            Self {
                backend: None,
                surface: None,
                renderer: None,
                canvas: None,
                surface_size: PhysicalSize::new(0, 0),
                antialiasing,
                surface_identity: 1,
                renderer_generation: 1,
                context_generation: 1,
                resource_generation: 0,
            }
        }

        /// Returns true after the native OpenGL context and renderer are ready.
        #[inline]
        pub fn is_ready(&self) -> bool {
            self.backend.is_some() && self.surface.is_some() && self.renderer.is_some()
        }

        /// OpenGL frames are presented inline because the context is thread-bound.
        #[inline]
        pub fn is_offloaded(&self) -> bool {
            false
        }

        /// Creates a WGL or EGL context for the native window and initializes Cupid.
        pub fn initialize(&mut self, window: &'static Window, size: PhysicalSize<u32>) {
            if self.is_ready() {
                self.resize(size);
                return;
            }

            let window_owner = Arc::new(window);
            let (backend, surface) = OpenGlBackend::new_windowed(
                window_owner,
                (size.width, size.height),
            )
            .unwrap_or_else(|error| panic!("create OpenGL 3.3 Core context: {error}"));
            let renderer = RendererImpl::with_antialiasing(
                &backend,
                surface.format(),
                self.antialiasing,
            );

            self.backend = Some(backend);
            self.surface = Some(surface);
            self.renderer = Some(renderer);
            self.canvas = Some(CupidCanvas::new());
            self.surface_size = size;
        }

        /// Updates the drawable size after a native window resize.
        pub fn resize(&mut self, size: PhysicalSize<u32>) {
            if size.width == 0 || size.height == 0 {
                return;
            }
            if self.surface_size != size {
                self.surface_size = size;
                self.resource_generation = self.resource_generation.wrapping_add(1);
            }
            if let Some(surface) = &mut self.surface {
                surface
                    .resize((size.width, size.height))
                    .expect("resize the OpenGL drawable");
            }
        }

        /// Records and presents a full-damage frame.
        pub fn render_frame(&mut self, draw_fn: impl FnOnce(&CupidCanvas, u32, u32)) -> PresentOutcome {
            self.render_frame_packet(|canvas, width, height| {
                draw_fn(canvas, width, height);
                (1.0, DamageSet::full(width, height))
            })
        }

        /// Records and presents a frame with damage metadata.
        pub fn render_frame_packet(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32) -> (f32, DamageSet),
        ) -> PresentOutcome {
            match self.build_frame_packet(draw_fn) {
                Some(packet) => self.present_packet(packet),
                None => PresentOutcome::Dropped,
            }
        }

        /// Records a full-damage frame without acquiring a drawable.
        pub fn build_frame(&mut self, draw_fn: impl FnOnce(&CupidCanvas, u32, u32)) -> Option<Frame> {
            self.build_frame_packet(|canvas, width, height| {
                draw_fn(canvas, width, height);
                (1.0, DamageSet::full(width, height))
            })
            .map(FramePacket::into_frame)
        }

        /// Records a frame packet without acquiring a drawable.
        pub fn build_frame_packet(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32) -> (f32, DamageSet),
        ) -> Option<FramePacket> {
            if !self.is_ready() || self.surface_size.width == 0 || self.surface_size.height == 0 {
                return None;
            }
            let canvas = self.canvas.as_ref()?;
            let width = self.surface_size.width;
            let height = self.surface_size.height;
            let build = PhaseTimer::start();
            canvas.begin_frame();
            let (scale, damage) = draw_fn(canvas, width, height);
            let damage = if damage.target_size() == (width, height) {
                damage
            } else {
                DamageSet::full(width, height)
            };
            let frame = Frame::new(canvas.take_draw_list(), width, height);
            let metadata = FrameRenderMetadata::new(
                scale,
                self.surface_identity,
                self.renderer_generation,
                self.context_generation,
                self.resource_generation,
                damage,
            );
            build.finish(FramePhase::Build);
            Some(FramePacket::new(frame, metadata))
        }

        /// Presents a frame and recycles its draw-list storage.
        pub fn present(&mut self, frame: Frame) -> PresentOutcome {
            let metadata = FrameRenderMetadata::new(
                1.0,
                self.surface_identity,
                self.renderer_generation,
                self.context_generation,
                self.resource_generation,
                DamageSet::full(frame.width, frame.height),
            );
            self.present_packet(FramePacket::new(frame, metadata))
        }

        /// Presents a frame packet and returns its draw-list storage to the canvas.
        pub fn present_packet(&mut self, packet: FramePacket) -> PresentOutcome {
            let presented = self.present_inner(packet.frame());
            if let Some(canvas) = &self.canvas {
                canvas.recycle_draw_list(packet.into_frame().into_draw_list());
            }
            PresentOutcome::from_presented(presented)
        }

        fn present_inner(&mut self, frame: &Frame) -> bool {
            let (Some(backend), Some(surface), Some(renderer)) = (
                self.backend.as_ref(),
                self.surface.as_mut(),
                self.renderer.as_mut(),
            ) else {
                return false;
            };
            let is_srgb = surface.is_srgb();
            let drawable = match surface.try_acquire() {
                Ok(Some(drawable)) => drawable,
                Ok(None) => return false,
                Err(error) => panic!("acquire OpenGL drawable: {error}"),
            };
            let (width, height) = drawable.size();
            if width == 0 || height == 0 {
                return false;
            }
            let encode = PhaseTimer::start();
            renderer.render(
                backend,
                drawable.view(),
                width,
                height,
                is_srgb,
                &frame.draw_list,
            );
            encode.finish(FramePhase::Encode);
            let present = PhaseTimer::start();
            let success = drawable.present().is_ok();
            present.finish(FramePhase::Present);
            success
        }
    }
}
