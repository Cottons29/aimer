#[cfg(all(not(target_arch = "wasm32"), target_os = "windows"))]
pub mod render_ctx {
    use aimer_cupid::AntiAlias;
    use aimer_cupid::backend::GpuBackend;
    use aimer_cupid::backend::dx12::{Dx12Backend, Dx12Surface, Dx12TextureFormat};
    use aimer_cupid::canvas::CupidCanvas;
    use aimer_cupid::damage_region::DamageSet;
    use aimer_cupid::frame::{Frame, FramePacket, FrameRenderMetadata};
    use aimer_cupid::renderer::RendererImpl;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::ffi::c_void;
    use windows::Win32::Foundation::HWND;
    use winit::dpi::PhysicalSize;
    use winit::window::Window;

    use crate::frame_stats::{FramePhase, PhaseTimer};
    use crate::render_ctx::PresentOutcome;

    // The flip-model swap chain uses UNORM storage and an sRGB render-target view.
    const SURFACE_FORMAT: Dx12TextureFormat = Dx12TextureFormat::Bgra8UnormSrgb;

    /// Quiver's inline Windows presenter backed by a Direct3D 12 device.
    pub struct Dx12Api {
        backend: Option<Dx12Backend>,
        surface: Option<Dx12Surface>,
        renderer: Option<RendererImpl<Dx12Backend>>,
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
        fn new(size: PhysicalSize<u32>, max_dimension: u32) -> Self {
            Self { size: Self::clamp(size, max_dimension), max_dimension }
        }

        fn clamp(size: PhysicalSize<u32>, max_dimension: u32) -> PhysicalSize<u32> {
            PhysicalSize::new(size.width.min(max_dimension), size.height.min(max_dimension))
        }

        fn resize(&mut self, size: PhysicalSize<u32>) -> bool {
            if size.width == 0 || size.height == 0 { return false; }
            let size = Self::clamp(size, self.max_dimension);
            let changed = self.size != size;
            self.size = size;
            changed
        }

        fn width(&self) -> u32 { self.size.width }
        fn height(&self) -> u32 { self.size.height }
    }

    impl Default for Dx12Api {
        fn default() -> Self { Self::new(AntiAlias::default()) }
    }

    impl Dx12Api {
        /// Creates an uninitialized Windows rendering context.
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

        /// Returns true after device, swap chain, and renderer initialization.
        #[inline]
        pub fn is_ready(&self) -> bool {
            self.backend.is_some() && self.surface.is_some() && self.renderer.is_some()
        }

        /// Direct3D 12 frames are rendered inline on the event-loop thread.
        #[inline]
        pub fn is_offloaded(&self) -> bool { false }

        /// Creates a hardware D3D12 device, attaches a DXGI swap chain, and
        /// initializes Cupid's generic renderer on the window thread.
        pub fn initialize(&mut self, window: &'static Window, size: PhysicalSize<u32>) {
            if self.is_ready() {
                self.window = Some(window);
                self.resize(size);
                return;
            }
            let backend = Dx12Backend::new().expect("create the system Direct3D 12 device and queue");
            let raw_handle = window.window_handle().expect("get the Windows window handle").as_raw();
            let RawWindowHandle::Win32(win32_handle) = raw_handle else {
                panic!("winit returned a non-Win32 window handle on Windows");
            };
            let hwnd = HWND(win32_handle.hwnd.get() as *mut c_void);
            let max_dimension = backend.limits().max_texture_dimension_2d;
            let surface_size = SurfaceSize::new(size, max_dimension);
            let surface = Dx12Surface::new(
                &backend,
                hwnd,
                surface_size.width(),
                surface_size.height(),
                SURFACE_FORMAT,
            )
            .expect("create the DXGI window swap chain");
            let renderer = RendererImpl::with_antialiasing(&backend, SURFACE_FORMAT, self.antialiasing);
            self.backend = Some(backend);
            self.surface = Some(surface);
            self.renderer = Some(renderer);
            self.canvas = Some(CupidCanvas::new());
            self.window = Some(window);
            self.surface_size = surface_size;
        }

        /// Queues a DXGI buffer resize to the current physical client size.
        ///
        /// The swap chain applies the latest requested size when its previous
        /// GPU work has completed, so this event handler never waits for the GPU.
        pub fn resize(&mut self, size: PhysicalSize<u32>) {
            if size.width == 0 || size.height == 0 { return; }
            let changed = self.surface_size.resize(size);
            if changed { self.resource_generation = self.resource_generation.wrapping_add(1); }
            if let Some(surface) = &mut self.surface {
                surface.request_resize(self.surface_size.width(), self.surface_size.height());
            }
        }

        /// Records and presents a full-damage frame.
        pub fn render_frame(&mut self, draw_fn: impl FnOnce(&CupidCanvas, u32, u32)) -> PresentOutcome {
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

        /// Records a frame without acquiring a swap-chain buffer.
        pub fn build_frame(&mut self, draw_fn: impl FnOnce(&CupidCanvas, u32, u32)) -> Option<Frame> {
            self.build_frame_packet(|canvas, width, height| {
                draw_fn(canvas, width, height);
                (1.0, DamageSet::full(width, height))
            })
            .map(FramePacket::into_frame)
        }

        /// Records a frame packet without acquiring a swap-chain buffer.
        pub fn build_frame_packet(
            &mut self,
            draw_fn: impl FnOnce(&CupidCanvas, u32, u32) -> (f32, DamageSet),
        ) -> Option<FramePacket> {
            if !self.is_ready() { return None; }
            let canvas = self.canvas.as_ref()?;
            let (width, height) = (self.surface_size.width(), self.surface_size.height());
            let build = PhaseTimer::start();
            canvas.begin_frame();
            let (scale, damage) = draw_fn(canvas, width, height);
            let damage = if damage.target_size() == (width, height) { damage }
                         else { DamageSet::full(width, height) };
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

        /// Presents a frame packet and recycles its draw-list storage.
        pub fn present_packet(&mut self, packet: FramePacket) -> PresentOutcome {
            let presented = self.present_inner(packet.frame());
            if let Some(canvas) = &self.canvas {
                canvas.recycle_draw_list(packet.into_frame().into_draw_list());
            }
            PresentOutcome::from_presented(presented)
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

        fn present_inner(&mut self, frame: &Frame) -> bool {
            let (Some(backend), Some(surface), Some(renderer)) = (
                self.backend.as_ref(),
                self.surface.as_mut(),
                self.renderer.as_mut(),
            ) else { return false; };
            let drawable = match surface.try_acquire(backend) {
                Ok(Some(drawable)) => drawable,
                Ok(None) => return false,
                Err(error) => panic!("resize the DXGI swap chain: {error}"),
            };
            let (width, height) = drawable.size();
            if width == 0 || height == 0 { return false; }
            let encode = PhaseTimer::start();
            renderer.render(
                backend,
                drawable.view(),
                width,
                height,
                SURFACE_FORMAT.is_srgb(),
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
