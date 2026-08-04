//! Regression coverage for a page whose scroll view is rebuilt by an
//! asynchronous request.
//!
//! `website/src/screen/blog_detail.rs` puts the `AsyncBuilder` at the root of
//! the page, so the `Scrollable` and every container below it is rebuilt when
//! the post arrives. The scroll range is derived from what those containers
//! report, so a measurement taken while the loading indicator was on screen
//! leaves the page unscrollable with its content already painted.

use std::thread::sleep;
use std::time::Duration;

use aimer::{
    AnyWidget, AsyncBuilder, AsyncSnapshot, BoxAlignment, Column, Container, Key, ScrollAxis,
    ScrollController, Scrollable, SizedBox, Widget,
};
use aimer_quiver::AimerApp;

/// Height of the loaded post: far taller than the headless viewport.
const CONTENT_HEIGHT: u32 = 4_000;

/// Builds the page for one snapshot, mirroring the vertical branch of the blog
/// detail screen: the `Scrollable` is part of what the request rebuilds, so the
/// loading state and the post are measured by two different container elements
/// standing in the same place.
fn detail_page(snapshot: &AsyncSnapshot<u32, String>, controller: &ScrollController) -> AnyWidget {
    let (content, key) = match snapshot {
        AsyncSnapshot::Waiting => (SizedBox::new().height(40).boxed(), Key::from("first-post")),
        AsyncSnapshot::Error(_) => (SizedBox::new().height(40).boxed(), Key::unique()),
        AsyncSnapshot::Data(height) => (
            SizedBox::new().height(*height).boxed(),
            Key::from("first-post"),
        ),
    };

    Container::new()
        .box_child(
            Scrollable::new()
                .key(key)
                .controller(controller.clone())
                .axis(ScrollAxis::Vertical)
                .child(
                    Container::new().child(
                        Column::new()
                            .horizontal_alignment(BoxAlignment::Start)
                            .children([
                                Column::new()
                                    .horizontal_alignment(BoxAlignment::Start)
                                    .children([
                                        SizedBox::new().height(28).boxed(),
                                        SizedBox::new().height(32).boxed(),
                                        content,
                                    ])
                                    .boxed(),
                                SizedBox::new().height(48).boxed(),
                            ]),
                    ),
                ),
        )
        .boxed()
}

#[test]
fn a_page_rebuilt_by_a_completed_request_can_be_scrolled() {
    let controller = ScrollController::new();
    let attached = controller.clone();
    let page = AsyncBuilder::new()
        .request_key("first-post".to_owned())
        .future(|| async { Ok::<_, String>(CONTENT_HEIGHT) })
        .child(move |snapshot| detail_page(snapshot, &attached));

    let mut app = AimerApp::start_headless(page);
    app.render_frame();
    assert_eq!(
        controller.max_extent().y,
        0.0,
        "the loading state is shorter than the viewport"
    );

    // Give the request a chance to complete, then draw the frame that swaps the
    // loading state for the post.
    sleep(Duration::from_millis(100));
    app.render_frame();
    app.render_frame();

    assert!(
        controller.max_extent().y > 0.0,
        "the loaded post is {CONTENT_HEIGHT}px tall but the page reports no scroll range"
    );
}

/// Control: the shape of `website/src/screen/blog.rs`, where the `Scrollable`
/// lives above the `AsyncBuilder` and is never rebuilt.
#[test]
fn a_stable_scroll_view_over_a_completed_request_can_be_scrolled() {
    let controller = ScrollController::new();
    let content = AsyncBuilder::new()
        .request_key("first-post".to_owned())
        .future(|| async { Ok::<_, String>(CONTENT_HEIGHT) })
        .child(|snapshot| match snapshot {
            AsyncSnapshot::Data(height) => SizedBox::new().height(*height).boxed(),
            _ => SizedBox::new().height(40).boxed(),
        })
        .boxed();

    let page = Container::new().box_child(
        Scrollable::new()
            .controller(controller.clone())
            .axis(ScrollAxis::Vertical)
            .child(
                Container::new().child(
                    Column::new()
                        .horizontal_alignment(BoxAlignment::Start)
                        .children([
                            SizedBox::new().height(28).boxed(),
                            content,
                            SizedBox::new().height(48).boxed(),
                        ]),
                ),
            ),
    );

    let mut app = AimerApp::start_headless(page);
    app.render_frame();
    sleep(Duration::from_millis(100));
    app.render_frame();
    app.render_frame();

    assert!(controller.max_extent().y > 0.0);
}
