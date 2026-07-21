pub mod api;
mod blog_store;
mod components;
mod router;
mod screen;
mod utils;

use aimer::router::Navigator;
use aimer::*;
use std::sync::atomic::{AtomicBool, AtomicUsize};

use crate::blog_store::BlogStore;
use crate::router::AppRouter;

#[cfg(test)]
pub static TEST_STATE_UPDATED: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub static CURRENT_INDEX: AtomicUsize = AtomicUsize::new(0);

// this is the entry point of the app
#[main]
pub fn my_app() {
    AimerApp::start(
        Provider::<BlogStore>::new()
            .create(BlogStore::default)
            .child(Navigator::<AppRouter>::new(AppRouter::Home, |route| Box::new(route))),
    );
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use std::thread::sleep;
    use std::time::Duration;

    use aimer::aimer_quiver::winit::event::WindowEvent;
    use aimer::quiver::winit::dpi::PhysicalSize;
    use aimer::router::Navigator;
    use aimer::{AimerApp, Provider};

    use crate::TEST_STATE_UPDATED;
    use crate::blog_store::{BlogDetail, BlogStore, LoadState};
    use crate::router::{AppRouter, take_route_builds};

    #[test]
    fn web_loader_fades_after_the_first_rendered_frame() {
        let html = include_str!("../builds/web/index.html");

        assert!(html.contains("<div class=\"loader\""));
        assert!(html.contains("position: fixed"));
        assert!(html.contains("aimer:first-frame-rendered"));
        assert!(html.contains("loader--hidden"));
        assert!(html.contains("transitionend"));
        assert!(html.contains("loader.remove()"));
    }

    #[test]
    fn direct_blog_detail_route_keeps_the_root_provider_scope() {
        let id = "introducing-aimer".to_owned();
        let details = HashMap::from([(
            id.clone(),
            LoadState::Ready(BlogDetail {
                id: id.clone(),
                upload_time: "2026-07-18T02:22:00Z".to_owned(),
                title: "Introducing Aimer".to_owned(),
                author: "Aimer Team".to_owned(),
                tags: vec!["Aimer".to_owned(), "Rust".to_owned(), "GUI".to_owned()],
                markdown: "# Introducing Aimer".to_owned(),
            }),
        )]);
        let mut app = AimerApp::start_headless(
            Provider::<BlogStore>::new()
                .create(move || BlogStore { list: LoadState::Idle, details: details.clone() })
                .child(Navigator::<AppRouter>::new(AppRouter::BlogDetail { id }, |route| {
                    Box::new(route)
                })),
        );

        app.render_frame();
        app.send_window_event(WindowEvent::Resized(PhysicalSize::new(1024, 768)));
        app.render_frame();
    }

    #[test]
    fn test_resize() {
        TEST_STATE_UPDATED.store(false, Ordering::Relaxed);
        let mut app = AimerApp::start_headless(
            Provider::<BlogStore>::new()
                .create(BlogStore::default)
                .child(Navigator::<AppRouter>::new(AppRouter::Home, |route| Box::new(route))),
        );
        sleep(Duration::from_millis(50));
        eprintln!("==========Rendered frame 1 call ===============");
        app.render_frame();
        take_route_builds();
        eprintln!("==========Rendered frame after resize the window ===============");
        sleep(Duration::from_millis(50));
        app.send_window_event(WindowEvent::Resized(PhysicalSize::new(1000, 800)));
        assert_eq!(take_route_builds(), vec![AppRouter::Blog]);
        eprintln!("==========Rendered frame 3 call ===============");
        sleep(Duration::from_millis(50));
        app.send_window_event(WindowEvent::Resized(PhysicalSize::new(1000, 800)));
        assert_eq!(take_route_builds(), vec![AppRouter::Blog]);
        sleep(Duration::from_millis(50));

        eprintln!("==========Rendered frame 4 call ===============");
        app.send_window_event(WindowEvent::Resized(PhysicalSize::new(390, 844)));
        app.render_frame();
        assert_eq!(take_route_builds(), vec![AppRouter::Blog]);
    }
}
