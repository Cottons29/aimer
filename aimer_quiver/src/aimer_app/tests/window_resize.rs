use super::*;
use aimer_widget::WindowMetrics;

#[test]
fn ios_window_size_follows_resize_orientation() {
    let mut app = AimerApp::start_headless_with(
        SizedBox::new(),
        HeadlessOptions { size: PhysicalSize::new(1179, 2556), scale_factor: 3.0 },
    );
    for size in [PhysicalSize::new(1179, 2556), PhysicalSize::new(2556, 1179)] {
        app.send_window_event(WindowEvent::Resized(size));
        assert_eq!(app.physical_size(), size);
        assert_eq!(app.window.inner_size(), size);
        assert_eq!(app.logical_size(), ResolvedSize {
            width: size.width as f32 / 3.0, height: size.height as f32 / 3.0,
        });
    }
}

#[test]
fn window_resize_tracks_current_viewport_instead_of_device_resolution() {
    let _serial = VIRTUALIZED_RENDER_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut app = AimerApp::start_headless_with(
        SizedBox::new().color(Color::RED),
        HeadlessOptions { size: PhysicalSize::new(2732, 2048), scale_factor: 2.0 },
    );
    // A window opened full screen can subsequently enter Split View or be
    // freely resized without the display's resolution changing.
    app.app.native_window_size = Some(ResolvedSize { width: 2732.0, height: 2048.0 });
    for size in [
        PhysicalSize::new(1200, 1600),
        PhysicalSize::new(1800, 1100),
        PhysicalSize::new(1100, 1100),
        PhysicalSize::new(2732, 2048),
    ] {
        app.send_window_event(WindowEvent::Resized(size));
        assert_eq!(app.app.pending_resize, None, "resize renders immediately");
        assert_eq!(app.app.native_window_size, Some(ResolvedSize {
            width: size.width as f32, height: size.height as f32,
        }));
        let metrics = WindowMetrics::of(&app.window);
        assert_eq!(metrics.physical_size, size);
        assert_eq!(metrics.logical_size(), ResolvedSize {
            width: size.width as f32 / 2.0, height: size.height as f32 / 2.0,
        });
        let (_, packet) = direct_headless_frame_packet(&mut app);
        assert_eq!((packet.frame().width, packet.frame().height), (size.width, size.height));
        assert_eq!(app.last_frame_result.as_ref().unwrap().1.target_size(), (size.width, size.height));
    }
}

#[test]
fn window_resize_recovers_after_zero_bounds_without_screen_fallback() {
    let mut app = AimerApp::start_headless_with(
        SizedBox::new(),
        HeadlessOptions { size: PhysicalSize::new(1200, 1600), scale_factor: 2.0 },
    );
    for size in [PhysicalSize::new(0, 0), PhysicalSize::new(0, 1600), PhysicalSize::new(1200, 0)] {
        app.send_window_event(WindowEvent::Resized(size));
        assert_eq!(app.app.pending_resize, None);
        assert_eq!(app.physical_size(), size);
    }
    let restored = PhysicalSize::new(1400, 1000);
    app.send_window_event(WindowEvent::Resized(restored));
    assert_eq!(app.app.native_window_size, Some(ResolvedSize { width: 1400.0, height: 1000.0 }));
    assert_eq!(app.logical_size(), ResolvedSize { width: 700.0, height: 500.0 });
}
