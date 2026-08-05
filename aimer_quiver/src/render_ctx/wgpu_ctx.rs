#[cfg(not(target_arch = "wasm32"))]
pub mod render_ctx {
    use aimer_cupid::AntiAlias;
    use aimer_cupid::canvas::CupidCanvas;
    use aimer_cupid::frame::Frame;
    use aimer_cupid::gpu_context::{GpuContext, render_dimensions};
    use aimer_cupid::renderer::Renderer;
    use winit::dpi::PhysicalSize;
    use winit::window::Window;

    use crate::frame_stats::{FramePhase, PhaseTimer};
    use crate::raster::FramePresenter;
    #[cfg(feature = "raster-thread")]
    use crate::raster::RasterThread;
    use crate::render_ctx::PresentOutcome;

    /// The UI thread's copy of the surface dimensions.
    ///
    /// The widget walk needs the size of the surface it is painting for, but the
    /// [`GpuContext`] that knows it may have been moved to the raster thread. So
    /// the size is mirrored here and kept in agreement with the context by
    /// running every requested size through [`render_dimensions`], exactly as
    /// [`GpuContext::resize`] does.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    struct SurfaceSize {
        size: PhysicalSize<u32>,
        max_dimension: u32,
    }

    impl SurfaceSize {
        #[inline]
        fn new(size: PhysicalSize<u32>, max_dimension: u32) -> Self {
            Self {
                size,
                max_dimension,
            }
        }

        /// Mirror a resize request, returning whether the backing size changed.
        ///
        /// A zero-sized request is ignored, matching [`GpuContext::resize`]:
        /// minimising a window must not reconfigure the surface to nothing.
        #[inline]
        fn resize(&mut self, size: PhysicalSize<u32>) -> bool {
            if size.width == 0 || size.height == 0 {
                return false;
            }
            let backing = render_dimensions(size, self.max_dimension);
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

    /// Everything downstream of the widget walk: the GPU context and the
    /// renderer.
    ///
    /// Bundled into one owner because the two only ever move together — either
    /// they stay on the UI thread, or the raster thread takes both. Implementing
    /// [`FramePresenter`] is what makes the second option possible; the type is
    /// `Send` for the same reason.
    struct GpuPresenter {
        gpu: GpuContext<'static>,
        renderer: Renderer,
    }

    impl GpuPresenter {
        /// The size the surface is currently configured for.
        #[inline]
        fn surface_size(&self) -> PhysicalSize<u32> {
            PhysicalSize::new(self.gpu.width(), self.gpu.height())
        }
    }

    impl FramePresenter for GpuPresenter {
        fn resize(&mut self, size: PhysicalSize<u32>) {
            self.gpu.resize(size);
        }

        fn present(&mut self, frame: &Frame) -> bool {
            let encode = PhaseTimer::start();

            let surface = match self.gpu.begin_frame() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                // Nothing was encoded, so nothing is worth timing: an outdated
                // surface would otherwise show up as a suspiciously cheap frame.
                _ => return false,
            };

            let view = surface.texture.create_view(&Default::default());

            self.renderer.render(
                &self.gpu.device,
                &self.gpu.queue,
                &view,
                frame.width,
                frame.height,
                self.gpu.is_srgb,
                &frame.draw_list,
            );
            encode.finish(FramePhase::Encode);

            let present = PhaseTimer::start();
            self.gpu.end_frame(surface);
            present.finish(FramePhase::Present);

            true
        }
    }

    /// Who encodes and presents a recorded frame.
    ///
    /// Which variant exists is a compile-time decision: the `raster-thread`
    /// feature replaces inline presentation wholesale, because the two differ in
    /// observable behaviour — a deferred frame reports its outcome a frame later
    /// — and carrying both paths in one build would buy nothing.
    enum Presentation {
        /// The UI thread does it itself, inside [`WgpuApi::present`].
        #[cfg(not(feature = "raster-thread"))]
        Inline(GpuPresenter),
        /// The raster thread does it; [`WgpuApi::present`] only queues the frame.
        #[cfg(feature = "raster-thread")]
        Offloaded(RasterThread),
    }

    /// Report the outcome of a frame the raster thread presented.
    ///
    /// This runs on the raster thread, which is why it may only touch shared,
    /// thread-safe state:
    ///
    /// * the first-frame notification, whose outcome is no longer available as a
    ///   return value on the UI thread;
    /// * the redraw request, which goes through the event loop proxy and the
    ///   existing `FrameReady` coalescing — so a dropped frame is retried instead
    ///   of leaving the window blank.
    ///
    /// No AppKit, UIKit or `winit` window call belongs here.
    #[cfg(feature = "raster-thread")]
    fn report_presented(presented: bool) {
        crate::first_frame::notify_first_frame_presented(presented);
        if !presented {
            aimer_events::window::request_animation_frame();
        }
    }

    pub struct WgpuApi {
        presentation: Option<Presentation>,
        canvas: Option<CupidCanvas>,
        surface_size: SurfaceSize,
        antialiasing: AntiAlias,
    }

    impl Default for WgpuApi {
        #[inline]
        fn default() -> Self {
            Self::new(AntiAlias::default())
        }
    }

    impl WgpuApi {
        #[inline]
        pub fn new(antialiasing: AntiAlias) -> Self {
            Self {
                presentation: None,
                canvas: None,
                surface_size: SurfaceSize::default(),
                antialiasing,
            }
        }

        /// Returns true when the GPU context has been initialized and is
        /// usable.
        #[inline]
        pub fn is_ready(&self) -> bool {
            self.presentation.is_some()
        }

        /// Whether frames are handed to the raster thread instead of being
        /// presented inline.
        ///
        /// False unless the crate was built with the `raster-thread` feature.
        #[inline]
        pub fn is_offloaded(&self) -> bool {
            #[cfg(feature = "raster-thread")]
            {
                matches!(self.presentation, Some(Presentation::Offloaded(_)))
            }
            #[cfg(not(feature = "raster-thread"))]
            {
                false
            }
        }

        /// Create the GPU context, the renderer and the canvas.
        ///
        /// Everything here runs on the main thread on purpose. Surface creation
        /// and, on macOS, `enable_transactional_surface_presentation` are AppKit
        /// calls, and AppKit is main-thread-only; only the encode and present
        /// steps are eligible to move to the raster thread afterwards.
        pub fn initialize(&mut self, window: &'static Window, size: PhysicalSize<u32>) {
            if self.presentation.is_some() {
                self.resize(size);
                return;
            }

            let gpu = GpuContext::initialize(window, size);
            #[cfg(target_os = "macos")]
            crate::ffi_utils::macos_surface::enable_transactional_surface_presentation(window);
            let canvas = CupidCanvas::new();
            let renderer = Renderer::with_antialiasing(&gpu.device, gpu.format, self.antialiasing);

            let presenter = GpuPresenter { gpu, renderer };
            self.surface_size = SurfaceSize::new(
                presenter.surface_size(),
                presenter.gpu.max_texture_dimension(),
            );

            #[cfg(feature = "raster-thread")]
            let presentation = Presentation::Offloaded(RasterThread::spawn(
                presenter,
                report_presented as fn(bool),
            ));
            #[cfg(not(feature = "raster-thread"))]
            let presentation = Presentation::Inline(presenter);

            self.presentation = Some(presentation);
            self.canvas = Some(canvas);
        }

        pub fn resize(&mut self, size: PhysicalSize<u32>) {
            if !self.surface_size.resize(size) {
                return;
            }

            match &mut self.presentation {
                #[cfg(not(feature = "raster-thread"))]
                Some(Presentation::Inline(presenter)) => presenter.resize(size),
                #[cfg(feature = "raster-thread")]
                Some(Presentation::Offloaded(raster)) => {
                    // Queued on the frame channel so it cannot overtake a frame
                    // that was built for the previous size.
                    raster.resize(size);
                }
                None => {}
            }
        }

        /// Record a frame by calling `draw_fn` with the canvas and the current
        /// surface dimensions, then present it.
        ///
        /// Equivalent to [`build_frame`] followed by [`present`].
        ///
        /// [`build_frame`]: WgpuApi::build_frame
        /// [`present`]: WgpuApi::present
        pub fn render_frame(&mut self, draw_fn: impl FnOnce(&CupidCanvas, u32, u32)) -> PresentOutcome {
            match self.build_frame(draw_fn) {
                Some(frame) => self.present(frame),
                None => PresentOutcome::Dropped,
            }
        }

        /// Record a frame without touching the swap chain.
        ///
        /// The widget walk runs here, entirely on the CPU. Keeping it out of
        /// [`present`] means no surface texture is held while the tree is being
        /// built and laid out, which is what shrinks the window in which a vsync
        /// deadline can be missed. The returned [`Frame`] is `Send`, so it can
        /// also be handed to a raster thread instead of presented inline.
        ///
        /// Returns `None` before the GPU context has been initialized.
        ///
        /// [`present`]: WgpuApi::present
        pub fn build_frame(&mut self, draw_fn: impl FnOnce(&CupidCanvas, u32, u32)) -> Option<Frame> {
            if !self.is_ready() {
                return None;
            }
            let canvas = self.canvas.as_ref()?;

            // Take back the buffer of an already presented frame before the
            // canvas needs one, so the steady state stays allocation-free even
            // when the raster thread owns the frame for a while.
            #[cfg(feature = "raster-thread")]
            if let Some(Presentation::Offloaded(raster)) = &self.presentation
                && let Some(draw_list) = raster.take_recycled()
            {
                canvas.recycle_draw_list(draw_list);
            }

            let width = self.surface_size.width();
            let height = self.surface_size.height();

            let build = PhaseTimer::start();
            canvas.begin_frame();
            draw_fn(canvas, width, height);
            let frame = Frame::new(canvas.take_draw_list(), width, height);
            build.finish(FramePhase::Build);

            Some(frame)
        }

        /// Put a recorded frame on screen, or hand it to the raster thread.
        ///
        /// Inline, the answer is immediate: [`PresentOutcome::Dropped`] means the
        /// surface texture could not be acquired and the caller should request
        /// another redraw. With the raster thread the answer is
        /// [`PresentOutcome::Deferred`] — the frame is queued, and its real
        /// outcome reaches `report_presented` on the raster thread instead.
        ///
        /// The frame's buffer is handed back to the canvas either way, so a
        /// dropped frame does not cost an allocation on the next one.
        pub fn present(&mut self, frame: Frame) -> PresentOutcome {
            match &mut self.presentation {
                #[cfg(not(feature = "raster-thread"))]
                Some(Presentation::Inline(presenter)) => {
                    let presented = presenter.present(&frame);
                    if let Some(canvas) = &self.canvas {
                        canvas.recycle_draw_list(frame.into_draw_list());
                    }
                    PresentOutcome::from_presented(presented)
                }
                #[cfg(feature = "raster-thread")]
                Some(Presentation::Offloaded(raster)) => {
                    // Blocks while a frame is still queued: that backpressure is
                    // what caps latency at one frame. The buffer comes back over
                    // the recycle channel, not here.
                    if raster.submit(frame) {
                        PresentOutcome::Deferred
                    } else {
                        PresentOutcome::Dropped
                    }
                }
                None => PresentOutcome::Dropped,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The raster thread can only take the presenter if it is `Send`; a
        /// non-`Send` field would otherwise fail far away, inside `spawn`.
        #[test]
        fn the_presenter_can_move_to_the_raster_thread() {
            const fn assert_send<T: Send>() {}
            assert_send::<GpuPresenter>();
        }

        #[test]
        fn the_mirrored_size_matches_what_the_context_would_configure() {
            let mut surface = SurfaceSize::new(PhysicalSize::new(800, 600), 2048);

            assert!(surface.resize(PhysicalSize::new(3072, 1728)));

            assert_eq!(
                PhysicalSize::new(surface.width(), surface.height()),
                render_dimensions(PhysicalSize::new(3072, 1728), 2048)
            );
        }

        #[test]
        fn a_zero_sized_resize_is_ignored() {
            let mut surface = SurfaceSize::new(PhysicalSize::new(800, 600), 2048);

            assert!(!surface.resize(PhysicalSize::new(0, 600)));
            assert!(!surface.resize(PhysicalSize::new(800, 0)));

            assert_eq!(surface.width(), 800);
            assert_eq!(surface.height(), 600);
        }

        #[test]
        fn resizing_to_the_same_size_reports_no_change() {
            let mut surface = SurfaceSize::new(PhysicalSize::new(800, 600), 2048);

            assert!(!surface.resize(PhysicalSize::new(800, 600)));
            assert!(surface.resize(PhysicalSize::new(801, 600)));
        }

        #[test]
        fn a_context_less_api_is_not_ready_and_drops_frames() {
            let mut api = WgpuApi::new(AntiAlias::default());

            assert!(!api.is_ready());
            assert!(!api.is_offloaded());
            assert!(api.build_frame(|_, _, _| unreachable!()).is_none());
            assert_eq!(
                api.render_frame(|_, _, _| unreachable!()),
                PresentOutcome::Dropped
            );
        }
    }
}
