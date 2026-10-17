//! UI-owned recording and lifecycle for all native rendering backends.
use aimer_cupid::AntiAlias;
use aimer_cupid::canvas::CupidCanvas;
use aimer_cupid::damage_region::DamageSet;
use aimer_cupid::frame::{Frame, FramePacket, FrameRenderMetadata};
use winit::dpi::PhysicalSize;
use winit::window::Window;

use super::{InlineRenderContext, PresentOutcome};
use crate::RenderMode;
use crate::frame_stats::{FramePhase, PhaseTimer};
use crate::raster::{FrameWorker, WorkerEvent};

#[cfg(all(not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
use super::metal_worker::{MetalHost, MetalSeed};

pub(crate) enum BackendOutput {
    Presented(bool),
    #[cfg(all(not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
    Metal(aimer_cupid::backend::metal::ScheduledMetalDrawable),
}

/// Longest a resize waits for its frame; a worker slower than this finishes it asynchronously.
const RESIZE_PRESENT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

/// Outcomes delivered by the native renderer on its UI owner thread.
#[derive(Default)]
pub struct CompletionStatus {
    /// At least one frame was successfully presented.
    pub presented: bool,
    /// A frame failed and the application needs a full repaint.
    pub retry: bool,
    /// A deferred frame can now be recorded without waiting.
    pub wake_frame: bool,
}

/// Native application renderer with UI-owned recording and bounded GPU offloading.
///
/// Platform event handling and widgets never leave the calling UI thread.
/// Manual packet producers should use [`Self::try_present_packet`] to retain
/// ownership when the two-frame pipeline is full.
pub struct NativeRenderContext {
    mode: RenderMode,
    antialiasing: AntiAlias,
    inline: Option<InlineRenderContext>,
    worker: Option<FrameWorker<BackendOutput>>,
    canvas: Option<CupidCanvas>,
    size: PhysicalSize<u32>,
    max_dimension: u32,
    generation: u64,
    ready: bool,
    waiting: bool,
    closing: bool,
    suspended: bool,
    /// The window is alive but must not be drawn to, as on iOS while the app is
    /// backgrounded. Unlike `suspended`, nothing is torn down: the surface stays
    /// attached so the system can still snapshot it.
    paused: bool,
    /// Whether a frame that changes the surface size is presented before the
    /// call that submitted it returns. See [`Self::settle_resized_frame`].
    sync_resize: bool,
    /// Size of the last frame handed to the worker.
    submitted_size: PhysicalSize<u32>,
    /// Outcomes already consumed while settling a resized frame, reported by the next poll.
    carried: CompletionStatus,
    #[cfg(all(not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
    host: Option<MetalHost>,
}

impl Default for NativeRenderContext {
    fn default() -> Self {
        Self::new(AntiAlias::default())
    }
}

impl NativeRenderContext {
    /// Creates an uninitialized native context using threaded rendering.
    pub fn new(antialiasing: AntiAlias) -> Self {
        Self {
            mode: RenderMode::Threaded,
            antialiasing,
            inline: None,
            worker: None,
            canvas: None,
            size: PhysicalSize::new(0, 0),
            max_dimension: u32::MAX,
            generation: 1,
            ready: false,
            waiting: false,
            closing: false,
            suspended: false,
            paused: false,
            sync_resize: false,
            submitted_size: PhysicalSize::new(0, 0),
            carried: CompletionStatus::default(),
            #[cfg(all(not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
            host: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_test_worker(
        worker: FrameWorker<BackendOutput>,
        size: PhysicalSize<u32>,
    ) -> Self {
        let mut context = Self::default();
        context.worker = Some(worker);
        context.canvas = Some(CupidCanvas::new());
        context.resize(size);
        context
    }

    /// Selects ownership before initialization. Panics if already initialized.
    pub fn with_render_mode(mut self, mode: RenderMode) -> Self {
        assert!(
            self.worker.is_none() && self.inline.is_none(),
            "select render mode before initialization"
        );
        self.mode = mode;
        self
    }

    /// Whether device and renderer initialization has completed.
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.ready && !self.suspended && !self.closing
    }

    /// Whether the initialized GPU owner lives on the raster worker.
    #[inline]
    pub fn is_offloaded(&self) -> bool {
        self.worker.is_some()
    }

    /// Whether recording can proceed immediately without consuming deferred damage.
    #[inline]
    pub fn can_begin_frame(&self) -> bool {
        self.is_ready()
            && self.size.width != 0
            && self.size.height != 0
            && !self.paused
            && self.worker.as_ref().is_none_or(FrameWorker::can_submit)
    }

    /// Stops (or resumes) recording frames without releasing the surface.
    ///
    /// Damage is not consumed while paused, so the first frame after resuming
    /// paints everything that changed in the meantime. Resuming does not wake the
    /// event loop; the caller asks for a redraw.
    #[inline]
    pub(crate) fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub(crate) fn defer_frame(&mut self) {
        self.waiting = true;
    }

    /// Attaches the platform surface on its required thread and starts initialization.
    pub fn initialize(&mut self, window: &'static Window, size: PhysicalSize<u32>) {
        if self.worker.is_some() || self.inline.is_some() {
            self.resize(size);
            return;
        }
        self.suspended = false;
        self.closing = false;
        self.generation = self.generation.wrapping_add(1);
        self.size = size;
        if self.mode == RenderMode::Inline {
            let mut inline = InlineRenderContext::new(self.antialiasing);
            inline.initialize(window, size);
            self.ready = inline.is_ready();
            self.inline = Some(inline);
            return;
        }
        self.canvas = Some(CupidCanvas::new());
        #[cfg(all(not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
        {
            let (host, seed, max_dimension) = MetalHost::new(window, size, self.antialiasing);
            self.host = Some(host);
            // AppKit commits the window's new geometry at the end of the resize
            // turn, so the frame for it has to be presented in that turn. UIKit
            // already gates its geometry on a matching drawable.
            self.sync_resize = cfg!(target_os = "macos");
            self.max_dimension = max_dimension;
            self.worker = Some(start_metal(seed));
        }
        #[cfg(feature = "wgpu")]
        {
            self.worker = Some(super::wgpu_ctx::render_ctx::start_worker(
                window,
                size,
                self.antialiasing,
            ));
        }
        #[cfg(all(
            not(feature = "wgpu"),
            not(any(target_os = "macos", target_os = "ios"))
        ))]
        {
            let antialiasing = self.antialiasing;
            self.worker = Some(FrameWorker::spawn(
                move || {
                    let mut presenter = InlineRenderContext::new(antialiasing);
                    presenter.initialize(window, size);
                    Ok(NativePresenter(presenter))
                },
                wake_event_loop,
            ));
        }
    }

    /// Updates UI metrics immediately; worker surface configuration travels with its next packet.
    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        let size = self.clamp(size);
        if self.size != size {
            self.size = size;
            self.generation = self.generation.wrapping_add(1);
        }
        if let Some(inline) = &mut self.inline {
            inline.resize(size);
        }
    }

    fn clamp(&self, size: PhysicalSize<u32>) -> PhysicalSize<u32> {
        #[cfg(feature = "wgpu")]
        {
            aimer_cupid::gpu_context::render_dimensions(size, self.max_dimension)
        }
        #[cfg(not(feature = "wgpu"))]
        {
            PhysicalSize::new(
                size.width.min(self.max_dimension),
                size.height.min(self.max_dimension),
            )
        }
    }

    /// Records and submits a full-damage frame.
    pub fn render_frame(&mut self, draw: impl FnOnce(&CupidCanvas, u32, u32)) -> PresentOutcome {
        self.render_frame_packet(|canvas, width, height| {
            draw(canvas, width, height);
            (1.0, DamageSet::full(width, height))
        })
    }

    /// Records only when capacity is available. A deferred draw callback is not invoked.
    pub fn render_frame_packet(
        &mut self,
        draw: impl FnOnce(&CupidCanvas, u32, u32) -> (f32, DamageSet),
    ) -> PresentOutcome {
        if let Some(inline) = &mut self.inline {
            if self.paused {
                return PresentOutcome::Dropped;
            }
            let ui = PhaseTimer::start();
            let outcome = inline.render_frame_packet(draw);
            ui.finish_ui_frame();
            return outcome;
        }
        if !self.can_begin_frame() {
            self.waiting = true;
            return if self.worker.is_some() {
                PresentOutcome::Deferred
            } else {
                PresentOutcome::Dropped
            };
        }
        let ui = PhaseTimer::start();
        let cpu = PhaseTimer::start();
        let Some(packet) = self.build_frame_packet(draw) else {
            return PresentOutcome::Dropped;
        };
        let resized = self.take_resize();
        self.worker
            .as_mut()
            .unwrap()
            .try_submit_tagged(packet, cpu, self.generation)
            .unwrap_or_else(|_| panic!("admitted frame lost capacity"));
        if resized {
            self.settle_resized_frame();
        }
        ui.finish_ui_frame();
        PresentOutcome::Deferred
    }

    /// Records a full-damage packet without acquiring a drawable.
    pub fn build_frame(&mut self, draw: impl FnOnce(&CupidCanvas, u32, u32)) -> Option<Frame> {
        self.build_frame_packet(|canvas, width, height| {
            draw(canvas, width, height);
            (1.0, DamageSet::full(width, height))
        })
        .map(FramePacket::into_frame)
    }

    /// Records without waiting. Returns None without invoking `draw` when busy.
    pub fn build_frame_packet(
        &mut self,
        draw: impl FnOnce(&CupidCanvas, u32, u32) -> (f32, DamageSet),
    ) -> Option<FramePacket> {
        if let Some(inline) = &mut self.inline {
            return inline.build_frame_packet(draw);
        }
        if !self.can_begin_frame() {
            return None;
        }
        #[cfg(all(not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
        if let Some(host) = &mut self.host {
            host.prepare(self.size);
        }
        let canvas = self.canvas.as_ref()?;
        let (width, height) = (self.size.width, self.size.height);
        let build = PhaseTimer::start();
        canvas.begin_frame();
        let (scale, damage) = draw(canvas, width, height);
        let render_plan = canvas.take_retained_render_plan();
        let damage = if damage.target_size() == (width, height) {
            damage
        } else {
            DamageSet::full(width, height)
        };
        let frame = Frame::new(canvas.take_draw_list(), width, height);
        let metadata =
            FrameRenderMetadata::new(scale, 1, 1, self.generation, self.generation, damage);
        #[cfg(feature = "wgpu")]
        let scene = render_plan.is_none().then(|| {
            aimer_cupid::compositor::CompositorScene::from_draw_list(
                &frame.draw_list,
                width,
                height,
                metadata.damage().clone(),
            )
        });
        #[cfg(not(feature = "wgpu"))]
        let scene = None;
        build.finish(FramePhase::Build);
        Some(FramePacket::with_render_plan(
            frame,
            metadata,
            scene,
            render_plan,
        ))
    }

    /// Submits without waiting, returning the original packet when the pipeline is full.
    pub fn try_present_packet(
        &mut self,
        packet: FramePacket,
    ) -> Result<PresentOutcome, FramePacket> {
        if let Some(inline) = &mut self.inline {
            return Ok(inline.present_packet(packet));
        }
        if !self.can_begin_frame() {
            return Err(packet);
        }
        #[cfg(all(not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
        if let Some(host) = &mut self.host {
            host.prepare(self.size);
        }
        let resized = self.take_resize();
        let submitted = self
            .worker
            .as_mut()
            .unwrap()
            .try_submit_tagged(packet, PhaseTimer::start(), self.generation)
            .map(|()| PresentOutcome::Deferred);
        if resized && submitted.is_ok() {
            self.settle_resized_frame();
        }
        submitted
    }

    /// Whether the frame about to be submitted changes the surface size, remembering it.
    fn take_resize(&mut self) -> bool {
        let resized = self.sync_resize && self.submitted_size != self.size;
        self.submitted_size = self.size;
        resized
    }

    /// Presents a frame that changed the surface size before returning.
    ///
    /// The window's own geometry change is committed by the platform at the end
    /// of the turn that delivered the resize. A frame presented in a *later*
    /// turn therefore leaves the newly exposed area empty for as long as the
    /// worker takes, which a live resize shows as a blank strip trailing the
    /// pointer. Waiting here, in that same turn, lets the new size and its
    /// content reach the screen together, as inline rendering always did. Only
    /// frames that change the size pay for this; ordinary frames stay
    /// pipelined, and a worker that does not answer within
    /// [`RESIZE_PRESENT_TIMEOUT`] is left to finish asynchronously.
    fn settle_resized_frame(&mut self) {
        let deadline = std::time::Instant::now() + RESIZE_PRESENT_TIMEOUT;
        let mut status = std::mem::take(&mut self.carried);
        let mut consumed = false;
        while self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.outstanding() > 0)
        {
            let Some(remaining) = deadline.checked_duration_since(std::time::Instant::now()) else {
                break;
            };
            let Some(event) = self
                .worker
                .as_mut()
                .and_then(|worker| worker.wait(remaining))
            else {
                break;
            };
            self.apply_event(event, &mut status);
            consumed = true;
        }
        self.carried = status;
        // The event loop is not told about completions this call consumed.
        if consumed {
            wake_event_loop();
        }
    }

    /// Presents a manually recorded packet after admission.
    ///
    /// Panics if the pipeline is full or uninitialized. Manual producers that
    /// do not check capacity should use [`Self::try_present_packet`] instead.
    pub fn present_packet(&mut self, packet: FramePacket) -> PresentOutcome {
        self.try_present_packet(packet)
            .unwrap_or_else(|_| panic!("frame submitted without render capacity"))
    }

    /// Presents an admitted full-damage frame.
    pub fn present(&mut self, frame: Frame) -> PresentOutcome {
        let metadata = FrameRenderMetadata::new(
            1.0,
            1,
            1,
            self.generation,
            self.generation,
            DamageSet::full(frame.width, frame.height),
        );
        self.present_packet(FramePacket::new(frame, metadata))
    }

    /// Delivers ready/completed frames without waiting, including UI presentation.
    ///
    /// Call on the same UI thread as initialization, in response to
    /// `AimerNativePlatformEvent::RenderWorkerReady`. A manually driven context
    /// must poll to observe initialization and release frame capacity.
    /// Panics if the worker reports a fatal initialization or rendering error.
    pub fn poll_completions(&mut self) -> CompletionStatus {
        let mut status = std::mem::take(&mut self.carried);
        while let Some(event) = self.worker.as_mut().and_then(FrameWorker::poll) {
            self.apply_event(event, &mut status);
        }
        if self.waiting && self.can_begin_frame() {
            self.waiting = false;
            status.wake_frame = true;
        }
        status
    }

    /// Finishes one worker event on the UI thread, including presentation.
    fn apply_event(&mut self, event: WorkerEvent<BackendOutput>, status: &mut CompletionStatus) {
        match event {
            WorkerEvent::Ready {
                max_texture_dimension,
            } => {
                self.max_dimension = max_texture_dimension;
                self.size = self.clamp(self.size);
                self.ready = true;
            }
            WorkerEvent::Completed(frame) => {
                let presented = match frame.output {
                    BackendOutput::Presented(presented) => {
                        presented && !self.suspended && !self.closing
                    }
                    #[cfg(all(not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
                    BackendOutput::Metal(drawable) => {
                        !self.suspended
                            && !self.closing
                            && frame.generation == self.generation
                            && self
                                .host
                                .as_ref()
                                .is_some_and(|host| host.present(drawable))
                    }
                };
                if let Some(canvas) = &self.canvas {
                    canvas.recycle_draw_list(frame.draw_list);
                }
                status.presented |= presented;
                status.retry |= !presented && !self.suspended && !self.closing;
            }
            WorkerEvent::Failed(error) => panic!("Aimer render worker failed: {error}"),
            WorkerEvent::Stopped => self.ready = false,
        }
    }

    pub(crate) fn close(&mut self) {
        self.closing = true;
        self.ready = false;
        if let Some(worker) = &mut self.worker {
            worker.stop();
        }
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.worker.as_ref().is_none_or(FrameWorker::is_stopped)
    }

    pub(crate) fn is_closing(&self) -> bool {
        self.closing
    }

    pub(crate) fn suspend(&mut self) {
        self.suspended = true;
        self.ready = false;
        self.submitted_size = PhysicalSize::new(0, 0);
        self.carried = CompletionStatus::default();
        if let Some(mut worker) = self.worker.take() {
            worker.stop();
            #[cfg(target_os = "windows")]
            while !worker.is_stopped() {
                super::windows_shutdown::pump_messages();
            }
            worker.join();
        }
        self.inline = None;
        #[cfg(all(not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
        {
            self.host = None;
        }
        self.generation = self.generation.wrapping_add(1);
        super::note_frame_not_presented();
    }
}

pub(super) fn wake_event_loop() {
    if let Some(proxy) = crate::aimer_app::EVENT_PROXY.get() {
        let _ = proxy.send_event(crate::aimer_app::AimerNativePlatformEvent::RenderWorkerReady);
    }
}

#[cfg(all(not(feature = "wgpu"), any(target_os = "macos", target_os = "ios")))]
fn start_metal(seed: MetalSeed) -> FrameWorker<BackendOutput> {
    FrameWorker::spawn(
        move || objc2::rc::autoreleasepool(|_| seed.create()),
        wake_event_loop,
    )
}

#[cfg(all(
    not(feature = "wgpu"),
    not(any(target_os = "macos", target_os = "ios"))
))]
struct NativePresenter(InlineRenderContext);

#[cfg(all(
    not(feature = "wgpu"),
    not(any(target_os = "macos", target_os = "ios"))
))]
impl crate::raster::WorkerPresenter for NativePresenter {
    type Output = BackendOutput;
    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.0.resize(size);
    }
    fn render(&mut self, packet: &FramePacket) -> BackendOutput {
        BackendOutput::Presented(self.0.present_worker_packet(packet))
    }
    fn max_texture_dimension(&self) -> u32 {
        self.0.worker_max_dimension()
    }
}

impl Drop for NativeRenderContext {
    fn drop(&mut self) {
        if self.worker.is_some() || self.inline.is_some() {
            self.suspend();
        }
    }
}

#[cfg(test)]
#[path = "native_worker_tests.rs"]
mod tests;
