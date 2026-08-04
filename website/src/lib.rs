pub mod api;
mod blog_store;
mod components;
mod router;
mod screen;
mod utils;

use std::env::args;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize};

use aimer::console::{debug, info};
use aimer::router::Navigator;
use aimer::*;

use crate::router::AppRouter;
#[cfg(test)]
pub static TEST_STATE_UPDATED: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub static CURRENT_INDEX: AtomicUsize = AtomicUsize::new(0);

// this is the entry point of the app
#[main]
pub fn my_app() {
    AimerApp::start(Navigator::<AppRouter>::new(AppRouter::Home, |route| {
        route.boxed()
    }));
}

#[cfg(test)]
mod test {
    use std::sync::atomic::Ordering;
    use std::thread::sleep;
    use std::time::Duration;

    use aimer::aimer_quiver::winit::event::WindowEvent;
    use aimer::quiver::winit::dpi::PhysicalSize;
    use aimer::router::Navigator;
    use aimer::{AimerApp, Widget};

    use crate::TEST_STATE_UPDATED;
    use crate::blog_store::{BlogDetail, cache_blog_detail};
    use crate::router::{AppRouter, take_route_builds};

    #[test]
    fn direct_blog_detail_route_renders_the_cached_post() {
        let id = "introducing-aimer".to_owned();
        cache_blog_detail(&BlogDetail {
            id: id.clone(),
            upload_time: "2026-07-18T02:22:00Z".to_owned(),
            title: "Introducing Aimer".to_owned(),
            author: "Aimer Team".to_owned(),
            tags: vec!["Aimer".to_owned(), "Rust".to_owned(), "GUI".to_owned()],
            markdown: "# Introducing Aimer".to_owned(),
        });
        let mut app = AimerApp::start_headless(Navigator::<AppRouter>::new(
            AppRouter::BlogDetail { id },
            |route| route.boxed(),
        ));

        app.render_frame();
        app.send_window_event(WindowEvent::Resized(PhysicalSize::new(1024, 768)));
        app.render_frame();
    }

    /// Dragging the window rebuilds nothing above the widgets that read it.
    ///
    /// A route picks the page for a path; it never asks how wide the window is,
    /// so no width — not even one that crosses the phone breakpoint — can change
    /// what it produces. The widgets inside the page that *do* ask are rebuilt
    /// on their own, which is what keeps a drag from re-running the whole
    /// application once per pixel.
    #[test]
    fn test_resize() {
        TEST_STATE_UPDATED.store(false, Ordering::Relaxed);
        let mut app =
            AimerApp::start_headless(Navigator::<AppRouter>::new(AppRouter::Home, |route| {
                route.boxed()
            }));
        sleep(Duration::from_millis(50));
        app.render_frame();
        assert!(!take_route_builds().is_empty(), "the first frame built no route");

        for size in [
            PhysicalSize::new(1000, 800),
            PhysicalSize::new(1000, 800),
            PhysicalSize::new(390, 844),
        ] {
            sleep(Duration::from_millis(50));
            app.send_window_event(WindowEvent::Resized(size));
            app.render_frame();
            assert_eq!(
                take_route_builds(),
                Vec::new(),
                "resizing to {size:?} rebuilt a route that never reads the window"
            );
        }
    }
}
