mod components;
mod router;
mod screen;
mod utils;

use aimer::router::Navigator;
use aimer::*;

use crate::router::AppRouter;

// this is the entry point of the app
#[main]
pub fn my_app() {
    AimerApp::start(Navigator::<AppRouter>::new(AppRouter::Home, |route| Box::new(route)));
}

#[cfg(test)]
mod test {
    use std::sync::atomic::Ordering;
    use std::thread::sleep;
    use std::time::Duration;

    use aimer::AimerApp;
    use aimer::aimer_quiver::winit::event::WindowEvent;
    use aimer::quiver::winit::dpi::PhysicalSize;
    use aimer::router::Navigator;

    use crate::components::same_looking::{CURRENT_INDEX, TEST_CLICKED, TEST_STATE_UPDATED};
    use crate::router::AppRouter;

    #[test]
    fn test_resize() {
        TEST_CLICKED.store(false, Ordering::Relaxed);
        TEST_STATE_UPDATED.store(false, Ordering::Relaxed);
        let mut app =
            AimerApp::start_headless(Navigator::<AppRouter>::new(AppRouter::Home, |route| {
                Box::new(route)
            }));
        sleep(Duration::from_millis(500));
        eprintln!("==========Rendered frame 1 call ===============");
        app.render_frame();
        eprintln!("==========Rendered frame after resize the window ===============");
        sleep(Duration::from_millis(500));
        app.send_window_event(WindowEvent::Resized(PhysicalSize::new(1000, 800)));
        // eprintln!("==========Rendered frame 3 call ===============");
        // sleep(Duration::from_millis(500));
        // app.send_window_event(WindowEvent::Resized(PhysicalSize::new(1000, 800)));
        assert!(TEST_STATE_UPDATED.load(Ordering::Relaxed));
        assert_eq!(CURRENT_INDEX.load(Ordering::Relaxed), 1);

        app.send_window_event(WindowEvent::Resized(PhysicalSize::new(390, 844)));
        app.render_frame();
    }
}
