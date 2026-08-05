//! The raster worker: a thread that consumes finished frames and puts them on
//! screen.
//!
//! The UI thread owns the widget tree and produces an immutable
//! [`Frame`](aimer_cupid::frame::Frame); this module owns everything downstream
//! of that — encoding the draw list and presenting the surface — on a separate
//! thread, so the UI thread is never blocked waiting for vsync.
//!
//! There is deliberately no event thread. Event handling in Aimer *is* tree
//! mutation, and on macOS, iOS and Android the OS event loop must own the main
//! thread anyway.
//!
//! Not available on `wasm32`: browsers have no thread the WebGPU objects could
//! legally move to. The web backend keeps presenting inline (see
//! `render_ctx/h5canva.rs`).

use std::sync::mpsc::{Receiver, RecvError, SyncSender, sync_channel};
use std::thread::JoinHandle;

use aimer_cupid::draw_cmd::DrawList;
use aimer_cupid::frame::Frame;
use winit::dpi::PhysicalSize;

/// The GPU-facing half of a frame's lifetime, as the raster thread sees it.
///
/// Implemented for real by the pair of `GpuContext` and `Renderer` that the
/// raster thread takes ownership of. Keeping it a trait is what lets the worker's
/// scheduling — ordering, backpressure, buffer recycling, shutdown — be tested on
/// a machine with no GPU at all.
pub trait FramePresenter: Send {
    /// Reconfigure the surface for a new size.
    ///
    /// Delivered in order with respect to frames, so a frame built for the old
    /// size is never presented against the new configuration.
    fn resize(&mut self, size: PhysicalSize<u32>);

    /// Encode `frame` and present it.
    ///
    /// Returns `false` when the surface texture could not be acquired, which the
    /// caller reports back so the UI thread can schedule another redraw.
    fn present(&mut self, frame: &Frame) -> bool;
}

/// What the UI thread sends to the raster thread.
///
/// Frames and resizes share one channel precisely so they stay ordered.
enum RasterMessage {
    Frame(Frame),
    Resize(PhysicalSize<u32>),
}

/// A handle to the raster thread.
///
/// Dropping the handle closes the channel, which ends the worker loop, and then
/// joins it — so the thread never outlives the application and any frame still in
/// flight is presented first.
///
/// # Frame pacing
///
/// The frame channel holds a single message. [`submit`] therefore blocks once one
/// frame is already queued, which caps the pipeline at one frame of latency and
/// keeps the UI thread from running arbitrarily far ahead of the display. That
/// blocking is the backpressure; without it, a UI thread that builds faster than
/// the GPU presents would grow an unbounded queue of stale frames.
///
/// [`submit`]: RasterThread::submit
pub struct RasterThread {
    messages: Option<SyncSender<RasterMessage>>,
    recycled: Receiver<DrawList>,
    worker: Option<JoinHandle<()>>,
}

impl RasterThread {
    /// Move `presenter` onto a new thread and start consuming frames.
    ///
    /// `on_present` is invoked on the raster thread after every frame with the
    /// outcome of [`FramePresenter::present`]. It is where a native backend pings
    /// the event loop (`EVENT_PROXY`) so the UI thread can retry a dropped frame
    /// or count a completed one; keep it short — it runs in the present path.
    pub fn spawn<P, F>(mut presenter: P, on_present: F) -> Self
    where
        P: FramePresenter + 'static,
        F: Fn(bool) + Send + 'static,
    {
        let (messages, inbox) = sync_channel::<RasterMessage>(1);
        let (recycle_tx, recycled) = std::sync::mpsc::channel::<DrawList>();

        let worker = std::thread::Builder::new()
            .name("aimer-raster".to_owned())
            .spawn(move || {
                loop {
                    match inbox.recv() {
                        Ok(RasterMessage::Resize(size)) => presenter.resize(size),
                        Ok(RasterMessage::Frame(frame)) => {
                            let presented = presenter.present(&frame);
                            // Hand the buffer back before reporting, so the next
                            // frame the UI thread builds can already reuse it.
                            let _ = recycle_tx.send(frame.into_draw_list());
                            on_present(presented);
                        }
                        // The handle was dropped: every queued message has been
                        // drained by now, so there is nothing left to present.
                        Err(RecvError) => break,
                    }
                }
            })
            .expect("failed to spawn the aimer raster thread");

        Self {
            messages: Some(messages),
            recycled,
            worker: Some(worker),
        }
    }

    /// Queue a frame for presentation.
    ///
    /// Blocks while a previous frame is still queued; see the pacing note on
    /// [`RasterThread`]. Returns `false` only if the worker has gone away, in
    /// which case the frame is dropped.
    pub fn submit(&self, frame: Frame) -> bool {
        self.messages
            .as_ref()
            .is_some_and(|messages| messages.send(RasterMessage::Frame(frame)).is_ok())
    }

    /// Queue a surface reconfiguration, ordered against the frame stream.
    pub fn resize(&self, size: PhysicalSize<u32>) -> bool {
        self.messages
            .as_ref()
            .is_some_and(|messages| messages.send(RasterMessage::Resize(size)).is_ok())
    }

    /// Reclaim a presented frame's command buffer, if one has come back.
    ///
    /// Returning the buffer to the canvas keeps the steady state allocation-free.
    /// A miss is not an error: it just means the raster thread has not finished
    /// the previous frame yet, and the canvas will grow a fresh buffer instead.
    pub fn take_recycled(&self) -> Option<DrawList> {
        // Both error cases mean "nothing to reclaim": empty while the worker is
        // still presenting, disconnected once it is gone.
        self.recycled.try_recv().ok()
    }
}

impl Drop for RasterThread {
    fn drop(&mut self) {
        // Closing the channel is the shutdown signal; the worker finishes the
        // frame it holds, drains the rest, and returns.
        self.messages = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    use aimer_cupid::utilities::{Color, Rect};

    use super::*;

    /// What a presenter saw, in the order it saw it.
    #[derive(Debug, PartialEq, Eq)]
    enum Seen {
        Frame(u32, u32),
        Resize(u32, u32),
    }

    struct RecordingPresenter {
        seen: Arc<Mutex<Vec<Seen>>>,
        succeed: bool,
    }

    impl FramePresenter for RecordingPresenter {
        fn resize(&mut self, size: PhysicalSize<u32>) {
            self.seen
                .lock()
                .unwrap()
                .push(Seen::Resize(size.width, size.height));
        }

        fn present(&mut self, frame: &Frame) -> bool {
            self.seen
                .lock()
                .unwrap()
                .push(Seen::Frame(frame.width, frame.height));
            self.succeed
        }
    }

    fn frame(width: u32, height: u32) -> Frame {
        let mut draw_list = DrawList::new();
        draw_list.fill_rect(
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Color::red(),
            [0.0; 4],
            [0.0; 4],
            Color::transparent(),
        );
        Frame::new(draw_list, width, height)
    }

    #[test]
    fn a_submitted_frame_is_presented_and_its_buffer_comes_back() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (done, presented) = channel();
        let raster = RasterThread::spawn(
            RecordingPresenter {
                seen: seen.clone(),
                succeed: true,
            },
            move |ok| done.send(ok).unwrap(),
        );

        assert!(raster.submit(frame(800, 600)));
        assert!(presented.recv().unwrap());

        assert_eq!(*seen.lock().unwrap(), vec![Seen::Frame(800, 600)]);
        let recycled = raster
            .take_recycled()
            .expect("the presented buffer should be returned");
        assert_eq!(recycled.commands().len(), 1);
    }

    #[test]
    fn a_resize_stays_ordered_against_the_frame_stream() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (done, presented) = channel();
        let raster = RasterThread::spawn(
            RecordingPresenter {
                seen: seen.clone(),
                succeed: true,
            },
            move |ok| done.send(ok).unwrap(),
        );

        assert!(raster.submit(frame(800, 600)));
        assert!(presented.recv().unwrap());
        assert!(raster.resize(PhysicalSize::new(1024, 768)));
        assert!(raster.submit(frame(1024, 768)));
        assert!(presented.recv().unwrap());

        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                Seen::Frame(800, 600),
                Seen::Resize(1024, 768),
                Seen::Frame(1024, 768),
            ]
        );
    }

    #[test]
    fn a_failed_present_is_reported_so_the_ui_thread_can_retry() {
        let (done, presented) = channel();
        let raster = RasterThread::spawn(
            RecordingPresenter {
                seen: Arc::new(Mutex::new(Vec::new())),
                succeed: false,
            },
            move |ok| done.send(ok).unwrap(),
        );

        assert!(raster.submit(frame(1, 1)));

        assert!(!presented.recv().unwrap());
    }

    #[test]
    fn dropping_the_handle_presents_queued_frames_then_joins() {
        static PRESENTED: AtomicUsize = AtomicUsize::new(0);
        PRESENTED.store(0, Ordering::Release);

        let raster = RasterThread::spawn(
            RecordingPresenter {
                seen: Arc::new(Mutex::new(Vec::new())),
                succeed: true,
            },
            |_| {
                PRESENTED.fetch_add(1, Ordering::AcqRel);
            },
        );

        for _ in 0..4 {
            assert!(raster.submit(frame(2, 2)));
        }
        drop(raster);

        assert_eq!(PRESENTED.load(Ordering::Acquire), 4);
    }
}
