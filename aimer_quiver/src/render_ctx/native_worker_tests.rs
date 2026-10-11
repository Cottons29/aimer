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
    fn resize(&mut self, size: PhysicalSize<u32>) { self.size = size; }
    fn render(&mut self, packet: &FramePacket) -> BackendOutput {
        assert_eq!(self.size, PhysicalSize::new(packet.frame().width, packet.frame().height));
        self.started.send((self.size.width, self.size.height)).unwrap();
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
    context.worker = Some(FrameWorker::spawn(move || Ok(Presenter {
        gate, started, size: PhysicalSize::new(0, 0),
    }), move || { let _ = wake.send(()); }));
    context.canvas = Some(CupidCanvas::new());
    context.resize(PhysicalSize::new(8, 8));
    assert!(!context.can_begin_frame());
    wakes.recv_timeout(Duration::from_secs(5)).unwrap();
    context.poll_completions();
    assert!(context.can_begin_frame());
    assert_eq!(context.render_frame(|_, _, _| {}), PresentOutcome::Deferred);
    assert_eq!(rendering.recv_timeout(Duration::from_secs(5)).unwrap(), (8, 8));
    context.resize(PhysicalSize::new(16, 12));
    assert_eq!(context.render_frame(|_, width, height| assert_eq!((width, height), (16, 12))),
        PresentOutcome::Deferred);
    assert!(!context.can_begin_frame());
    assert_eq!(context.render_frame(|_, _, _| panic!("busy UI frame must not consume damage or image uploads")),
        PresentOutcome::Deferred);
    release.send(()).unwrap();
    assert_eq!(rendering.recv_timeout(Duration::from_secs(5)).unwrap(), (16, 12));
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
    assert_eq!(context.render_frame(|_, _, _| unreachable!()), PresentOutcome::Dropped);
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
    let worker = FrameWorker::spawn(move || Ok(Presenter { gate, started,
        size: PhysicalSize::new(0, 0) }), move || { let _ = wake.send(()); });
    let mut context = NativeRenderContext::with_test_worker(worker, PhysicalSize::new(8, 8));
    wakes.recv_timeout(Duration::from_secs(5)).unwrap();
    context.poll_completions();
    let packet = context.build_frame_packet(|_, width, height| (1.0, DamageSet::full(width, height))).unwrap();
    assert!(packet.render_plan().is_none());
    assert!(packet.scene().is_some(), "legacy WGPU recording needs its compositor scene");
    context.suspend();
}

#[test]
fn closing_does_not_report_a_late_frame_as_a_first_visible_frame() {
    let (release, gate) = mpsc::channel();
    let (started, rendering) = mpsc::channel();
    let (wake, wakes) = mpsc::channel();
    let worker = FrameWorker::spawn(move || Ok(Presenter { gate, started,
        size: PhysicalSize::new(0, 0) }), move || { let _ = wake.send(()); });
    let mut context = NativeRenderContext::with_test_worker(worker, PhysicalSize::new(8, 8));
    wakes.recv_timeout(Duration::from_secs(5)).unwrap();
    context.poll_completions();
    context.render_frame(|_, _, _| {});
    rendering.recv_timeout(Duration::from_secs(5)).unwrap();
    context.close();
    release.send(()).unwrap();
    context.worker.as_mut().unwrap().join();
    let completed = context.poll_completions();
    assert!(!completed.presented, "closing must not show a previously hidden window");
    assert!(!completed.retry && !completed.wake_frame);
    assert!(context.is_stopped());
}
