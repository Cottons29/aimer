#[cfg(all(not(target_arch = "wasm32"), target_os = "macos"))]
pub mod render_ctx {
    use aimer_cupid::AntiAlias;
    use aimer_cupid::backend::GpuBackend;
    use aimer_cupid::backend::metal::{MetalBackend, MetalSurface};
    use aimer_cupid::canvas::CupidCanvas;
    use aimer_cupid::damage_region::DamageSet;
    use aimer_cupid::frame::{Frame, FramePacket, FrameRenderMetadata};
    use aimer_cupid::renderer::RendererImpl;
    use objc2_app_kit::NSView;
    use objc2_core_foundation::CGSize;
    use objc2_metal::MTLPixelFormat;
    use objc2_quartz_core::CAMetalLayer;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::dpi::PhysicalSize;
    use winit::window::Window;

    use crate::frame_stats::{FramePhase, PhaseTimer};
    use crate::render_ctx::PresentOutcome;

    const SURFACE_FORMAT: MTLPixelFormat = MTLPixelFormat::BGRA8Unorm_sRGB;

    /// The Quiver render context backed by a native macOS `CAMetalLayer`.
    ///
    /// This context owns the Metal backend, surface, and backend-generic Cupid
    /// renderer. Metal is selected whenever Quiver's `metal` feature is enabled;
    /// add `--no-default-features` to keep WGPU out of the build as well.
    /// The presenter currently renders each frame's complete draw list; packet
    /// damage metadata remains available for API compatibility.
    pub struct MetalApi {
        backend: Option<MetalBackend>,
        surface: Option<MetalSurface>,
        renderer: Option<RendererImpl<MetalBackend>>,
        canvas: Option<CupidCanvas>,
        window: Option<&'static Window>,
        surface_size: SurfaceSize,
        antialiasing: AntiAlias,
        surface_identity: u64,
        renderer_generation: u64,
        context_generation: u64,
        resource_generation: u64,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    struct SurfaceSize {
        size: PhysicalSize<u32>,
        max_dimension: u32,
    }

    impl SurfaceSize {
        #[inline]
        fn new(size: PhysicalSize<u32>, max_dimension: u32) -> Self {
            Self {
                size: Self::clamp(size, max_dimension),
                max_dimension,
            }
        }

        #[inline]
        fn clamp(size: PhysicalSize<u32>, max_dimension: u32) -> PhysicalSize<u32> {
            PhysicalSize::new(
                size.width.min(max_dimension),
                size.height.min(max_dimension),
            )
        }

        /// Mirror a resize request, returning whether the backing size changed.
        #[inline]
        fn resize(&mut self, size: PhysicalSize<u32>) -> bool {
            if size.width == 0 || size.height == 0 {
                return false;
            }
            let backing = Self::clamp(size, self.max_dimension);
            let changed = backing != self.size;
            self.size = backing;
            changed
        }

        #[inline]
        fn width(&self) -> u32 {
            self.size.width
        }

        #[inline]
        fn height(&self) -> u32 {
            self.size.height
        }
    }

    impl Default for MetalApi {
        #[inline]
        fn default() -> Self {
            Self::new(AntiAlias::default())
        }
    }

    impl MetalApi {
        /// Creates an uninitialized context with the requested antialiasing.
        #[inline]
        pub fn new(antialiasing: AntiAlias) -> Self {
            Self {
                backend: None,
                surface: None,
                renderer: None,
                canvas: None,
                window: None,
                surface_size: SurfaceSize::default(),
                antialiasing,
                surface_identity: 1,
                renderer_generation: 1,
                context_generation: 1,
                resource_generation: 0,
            }
        }

        /// Returns true after the Metal device, surface, and renderer are ready.
        #[inline]
        pub fn is_ready(&self) -> bool {
            self.backend.is_some() && self.surface.is_some() && self.renderer.is_some()
        }

        /// Metal frames are presented inline on the window's event-loop thread.
        #[inline]
        pub fn is_offloaded(&self) -> bool {
            false
        }

        /// Creates the Metal device, attaches a `CAMetalLayer`, and initializes
        /// Cupid's generic renderer. Surface setup runs on the main thread.
        pub fn initialize(&mut self, window: &'static Window, size: PhysicalSize<u32>) {
            if self.is_ready() {
                self.window = Some(window);
                self.resize(size);
                return;
            }

            let backend = MetalBackend::new().expect("create the system Metal device and queue");
            let raw_handle = window
                .window_handle()
                .expect("get the macOS window handle")
                .as_raw();
            let RawWindowHandle::AppKit(appkit_handle) = raw_handle else {
                panic!("winit returned a non-AppKit window handle on macOS");
            };

            // SAFETY: Winit owns this NSView for the lifetime of `window`. The
            // context stores the leaked static window for as long as the layer
            // and renderer are in use.
            let view = unsafe { &*appkit_handle.ns_view.as_ptr().cast::<NSView>() };
            let layer = CAMetalLayer::new();
            view.setWantsLayer(true);
            view.setLayer(Some(&layer));

            let surface = MetalSurface::new(&backend, layer, SURFACE_FORMAT);
            let max_dimension = backend.limits().max_texture_dimension_2d;
            let surface_size = SurfaceSize::new(size, max_dimension);
            configure_surface(&surface, window, surface_size.size);
            crate::ffi_utils::macos_surface::enable_transactional_surface_presentation(window);
            let renderer = RendererImpl::with_antialiasing(
                &backend,
                SURFACE_FORMAT,
                self.antialiasing,
            );

            self.backend = Some(backend);
            self.surface = Some(surface);
            self.renderer = Some(renderer);
            self.canvas = Some(CupidCanvas::new());
            self.window = Some(window);
            self.surface_size = surface_size;
        }

        /// Resizes the drawable layer to the current physical window dimensions.
        pub fn resize(&mut self, size: PhysicalSize<u32>) {
            if size.width == 0 || size.height == 0 {
                return;
            }

            if self.surface_size.resize(size) {
                self.resource_generation = self.resource_generation.wrapping_add(1);
            }

            if let (Some(surface), Some(window)) = (&self.surface, self.window) {
                configure_surface(surface, window, self.surface_size.size);
            }
        }

        /// Records and presents a full-damage frame.
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

        /// Records and presents a frame with damage metadata from the widget walk.
        pub fn render_frame_packet(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32) -> (f32, DamageSet),
        ) -> PresentOutcome {
            match self.build_frame_packet(draw_fn) {
                Some(packet) => self.present_packet(packet),
                None => PresentOutcome::Dropped,
            }
        }

        /// Records a frame without acquiring a Metal drawable.
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

        /// Records a frame packet without acquiring a Metal drawable.
        pub fn build_frame_packet(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32) -> (f32, DamageSet),
        ) -> Option<FramePacket> {
            if !self.is_ready() {
                return None;
            }
            let canvas = self.canvas.as_ref()?;
            let width = self.surface_size.width();
            let height = self.surface_size.height();

            let build = PhaseTimer::start();
            canvas.begin_frame();
            let (scale, damage) = draw_fn(canvas, width, height);
            let damage = if damage.target_size() == (width, height) {
                damage
            } else {
                DamageSet::full(width, height)
            };
            let draw_list = canvas.take_draw_list();
            let frame = Frame::new(draw_list, width, height);
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

        /// Presents a frame and returns its draw-list storage to the canvas.
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
                self.surface.as_ref(),
                self.renderer.as_mut(),
            ) else {
                return false;
            };
            let Some(drawable) = surface.next_drawable() else {
                return false;
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
                true,
                &frame.draw_list,
            );
            encode.finish(FramePhase::Encode);

            let present = PhaseTimer::start();
            drawable.present(backend);
            present.finish(FramePhase::Present);
            true
        }
    }

    fn configure_surface(
        surface: &MetalSurface,
        window: &Window,
        size: PhysicalSize<u32>,
    ) {
        surface.layer().setContentsScale(window.scale_factor());
        surface.layer().setDrawableSize(CGSize::new(
            f64::from(size.width),
            f64::from(size.height),
        ));
    }
}
