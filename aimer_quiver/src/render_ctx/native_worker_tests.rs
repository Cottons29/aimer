use super::*;
use crate::raster::WorkerPresenter;
use std::sync::mpsc;
use std::time::Duration;

struct Presenter {
    gate: mpsc::Receiver<()>,
    started: mpsc::Sender<(u32, u32)>,
    size: PhysicalSize<u32>,
}
impl WorkerPresenter for Presenter {
    type Output = BackendOutput;
    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.size = size;
    }
    fn render(&mut self, packet: &FramePacket) -> BackendOutput {
        assert_eq!(
            self.size,
            PhysicalSize::new(packet.frame().width, packet.frame().height)
        );
        self.started
            .send((self.size.width, self.size.height))
            .unwrap();
        self.gate.recv_timeout(Duration::from_secs(5)).unwrap();
        BackendOutput::Presented(true)
    }
}

#[test]
fn busy_native_recording_defers_without_invoking_the_ui_draw_callback() {
    let (release, gate) = mpsc::channel();
    let (started, rendering) = mpsc::channel();
    let (wake, wakes) = mpsc::channel();
    let mut context = NativeRenderContext::default();
    context.worker = Some(FrameWorker::spawn(
        move || {
            Ok(Presenter {
                gate,
                started,
                size: PhysicalSize::new(0, 0),
            })
        },
        move || {
            let _ = wake.send(());
        },
    ));
    context.canvas = Some(CupidCanvas::new());
    context.resize(PhysicalSize::new(8, 8));
    assert!(!context.can_begin_frame());
    wakes.recv_timeout(Duration::from_secs(5)).unwrap();
    context.poll_completions();
    assert!(context.can_begin_frame());
    assert_eq!(context.render_frame(|_, _, _| {}), PresentOutcome::Deferred);
    assert_eq!(
        rendering.recv_timeout(Duration::from_secs(5)).unwrap(),
        (8, 8)
    );
    context.resize(PhysicalSize::new(16, 12));
    assert_eq!(
        context.render_frame(|_, width, height| assert_eq!((width, height), (16, 12))),
        PresentOutcome::Deferred
    );
    assert!(!context.can_begin_frame());
    assert_eq!(
        context.render_frame(|_, _, _| panic!(
            "busy UI frame must not consume damage or image uploads"
        )),
        PresentOutcome::Deferred
    );
    release.send(()).unwrap();
    assert_eq!(
        rendering.recv_timeout(Duration::from_secs(5)).unwrap(),
        (16, 12)
    );
    let status = context.poll_completions();
    assert!(status.presented && status.wake_frame && !status.retry);
    assert!(context.can_begin_frame());
    release.send(()).unwrap();
    context.close();
    context.suspend();
    assert!(!context.can_begin_frame());
}

#[test]
fn uninitialized_and_zero_sized_contexts_do_not_record() {
    let mut context = NativeRenderContext::default();
    assert_eq!(
        context.render_frame(|_, _, _| unreachable!()),
        PresentOutcome::Dropped
    );
    assert!(context.build_frame(|_, _, _| unreachable!()).is_none());
    context.ready = true;
    context.resize(PhysicalSize::new(0, 8));
    assert!(!context.can_begin_frame());
    context.resize(PhysicalSize::new(8, 0));
    assert!(!context.can_begin_frame());
}

#[cfg(feature = "wgpu")]
#[test]
fn threaded_wgpu_recording_keeps_the_legacy_scene_fallback() {
    let (_release, gate) = mpsc::channel();
    let (started, _rendering) = mpsc::channel();
    let (wake, wakes) = mpsc::channel();
    let worker = FrameWorker::spawn(
        move || {
            Ok(Presenter {
                gate,
                started,
                size: PhysicalSize::new(0, 0),
            })
        },
        move || {
            let _ = wake.send(());
        },
    );
    let mut context = NativeRenderContext::with_test_worker(worker, PhysicalSize::new(8, 8));
    wakes.recv_timeout(Duration::from_secs(5)).unwrap();
    context.poll_completions();
    let packet = context
        .build_frame_packet(|_, width, height| (1.0, DamageSet::full(width, height)))
        .unwrap();
    assert!(packet.render_plan().is_none());
    assert!(
        packet.scene().is_some(),
        "legacy WGPU recording needs its compositor scene"
    );
    context.suspend();
}

#[test]
fn closing_does_not_report_a_late_frame_as_a_first_visible_frame() {
    let (release, gate) = mpsc::channel();
    let (started, rendering) = mpsc::channel();
    let (wake, wakes) = mpsc::channel();
    let worker = FrameWorker::spawn(
        move || {
            Ok(Presenter {
                gate,
                started,
                size: PhysicalSize::new(0, 0),
            })
        },
        move || {
            let _ = wake.send(());
        },
    );
    let mut context = NativeRenderContext::with_test_worker(worker, PhysicalSize::new(8, 8));
    wakes.recv_timeout(Duration::from_secs(5)).unwrap();
    context.poll_completions();
    context.render_frame(|_, _, _| {});
    rendering.recv_timeout(Duration::from_secs(5)).unwrap();
    context.close();
    release.send(()).unwrap();
    context.worker.as_mut().unwrap().join();
    let completed = context.poll_completions();
    assert!(
        !completed.presented,
        "closing must not show a previously hidden window"
    );
    assert!(!completed.retry && !completed.wake_frame);
    assert!(context.is_stopped());
}

fn ready_context(
    sync_resize: bool,
) -> (
    NativeRenderContext,
    mpsc::Sender<()>,
    mpsc::Receiver<(u32, u32)>,
) {
    let (release, gate) = mpsc::channel();
    let (started, rendering) = mpsc::channel();
    let (wake, wakes) = mpsc::channel();
    let worker = FrameWorker::spawn(
        move || {
            Ok(Presenter {
                gate,
                started,
                size: PhysicalSize::new(0, 0),
            })
        },
        move || {
            let _ = wake.send(());
        },
    );
    let mut context = NativeRenderContext::with_test_worker(worker, PhysicalSize::new(8, 8));
    wakes.recv_timeout(Duration::from_secs(5)).unwrap();
    context.poll_completions();
    context.sync_resize = sync_resize;
    (context, release, rendering)
}

fn outstanding(context: &NativeRenderContext) -> usize {
    context.worker.as_ref().unwrap().outstanding()
}

#[test]
fn a_frame_that_changes_the_size_is_presented_before_the_call_returns() {
    let (mut context, release, rendering) = ready_context(true);
    release.send(()).unwrap();
    assert_eq!(context.render_frame(|_, _, _| {}), PresentOutcome::Deferred);
    assert_eq!(
        rendering.recv_timeout(Duration::from_secs(5)).unwrap(),
        (8, 8)
    );
    assert_eq!(
        outstanding(&context),
        0,
        "a resize must not leave its frame for a later turn"
    );
    // The outcome the call consumed is still reported to whoever polls next.
    let status = context.poll_completions();
    assert!(status.presented && !status.retry);
    context.close();
    context.suspend();
}

#[test]
fn a_frame_of_unchanged_size_is_left_to_the_worker() {
    let (mut context, release, rendering) = ready_context(true);
    release.send(()).unwrap();
    context.render_frame(|_, _, _| {});
    rendering.recv_timeout(Duration::from_secs(5)).unwrap();
    // Same size again, with the presenter held: the call must not wait for it.
    let before = std::time::Instant::now();
    assert_eq!(context.render_frame(|_, _, _| {}), PresentOutcome::Deferred);
    assert!(
        before.elapsed() < Duration::from_millis(50),
        "only a resize may wait for the worker"
    );
    assert_eq!(outstanding(&context), 1);
    release.send(()).unwrap();
    context.close();
    context.suspend();
}

#[test]
fn a_stalled_worker_cannot_hold_a_resize_forever() {
    let (mut context, release, rendering) = ready_context(true);
    let before = std::time::Instant::now();
    assert_eq!(context.render_frame(|_, _, _| {}), PresentOutcome::Deferred);
    assert!(before.elapsed() < Duration::from_secs(2));
    assert_eq!(
        rendering.recv_timeout(Duration::from_secs(5)).unwrap(),
        (8, 8)
    );
    assert_eq!(
        outstanding(&context),
        1,
        "the frame stays with the worker once the wait gives up"
    );
    release.send(()).unwrap();
    context.close();
    context.suspend();
}

#[test]
fn contexts_that_do_not_opt_in_never_wait_for_a_resize() {
    let (mut context, release, rendering) = ready_context(false);
    let before = std::time::Instant::now();
    assert_eq!(context.render_frame(|_, _, _| {}), PresentOutcome::Deferred);
    assert!(before.elapsed() < Duration::from_millis(50));
    assert_eq!(
        rendering.recv_timeout(Duration::from_secs(5)).unwrap(),
        (8, 8)
    );
    release.send(()).unwrap();
    context.close();
    context.suspend();
}

/// iOS reports Control Center and Notification Center as a suspension. The
/// surface has to survive them: dropping it removes the layer and the user sees
/// (and the app switcher snapshots) a black window.
#[test]
fn a_paused_context_keeps_its_surface_and_records_nothing_until_resumed() {
    let (mut context, release, _rendering) = ready_context(false);
    assert!(context.can_begin_frame());

    context.set_paused(true);
    assert!(!context.can_begin_frame());
    assert_eq!(
        context.render_frame(|_, _, _| panic!("a paused context must not consume damage")),
        PresentOutcome::Deferred
    );
    assert!(
        context.worker.is_some() && context.is_ready(),
        "pausing must not tear the worker down"
    );

    context.set_paused(false);
    assert!(context.can_begin_frame());
    release.send(()).unwrap();
    context.close();
    context.suspend();
}
