//! What a window resize is allowed to rebuild.
//!
//! A drag delivers a resize event per pixel of travel, so the frame answering it
//! decides whether the window follows the cursor. Laying the tree out again is
//! unavoidable — every constraint below the root changed — but re-running the
//! `build` of a widget that never asked about the window produces the identical
//! subtree at the cost of the whole application.

use std::cell::Cell;
use std::rc::Rc;

use aimer::quiver::winit::dpi::PhysicalSize;
use aimer::quiver::winit::event::WindowEvent;
use aimer::provider::media_query::MediaQuery;
use aimer::{
    AimerApp, AnyElement, BuildContext, Column, Element, SizedBox, StatelessElement, Widget,
};

/// A widget that records every build and reports whether the window is narrow.
///
/// Written out rather than declared with `#[widget]` so the test owns the build
/// closure and can count its invocations.
#[derive(Clone)]
struct Probe {
    builds: Rc<Cell<u32>>,
    compact: Option<Rc<Cell<bool>>>,
    /// Whether the breakpoint is read as a question about the window rather
    /// than by reading the window and answering it afterwards.
    selected: bool,
}

impl Probe {
    /// A widget that reads the window itself.
    fn watching(builds: &Rc<Cell<u32>>, compact: &Rc<Cell<bool>>) -> Self {
        Self {
            builds: builds.clone(),
            compact: Some(compact.clone()),
            selected: false,
        }
    }

    /// A widget that reads only the breakpoint.
    fn selecting(builds: &Rc<Cell<u32>>, compact: &Rc<Cell<bool>>) -> Self {
        Self {
            selected: true,
            ..Self::watching(builds, compact)
        }
    }

    /// A widget that never looks at the window.
    fn indifferent(builds: &Rc<Cell<u32>>) -> Self {
        Self {
            builds: builds.clone(),
            compact: None,
            selected: false,
        }
    }
}

impl Widget for Probe {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        let source = self.clone();
        StatelessElement::from_builder(
            ctx,
            move |ctx| {
                source.builds.set(source.builds.get() + 1);
                if let Some(compact) = &source.compact {
                    compact.set(if source.selected {
                        MediaQuery::select(ctx, |media| media.size.width < 600.0)
                    } else {
                        MediaQuery::of(ctx).size.width < 600.0
                    });
                }
                SizedBox::new().width(10).height(10).to_element(ctx)
            },
            None,
            "Probe",
        )
        .boxed()
    }
}

#[test]
fn a_resize_rebuilds_only_the_widgets_that_read_the_window() {
    let watching_builds = Rc::new(Cell::new(0));
    let indifferent_builds = Rc::new(Cell::new(0));
    let compact = Rc::new(Cell::new(false));

    let page = Column::new().children([
        Probe::watching(&watching_builds, &compact).boxed(),
        Probe::indifferent(&indifferent_builds).boxed(),
    ]);

    let mut app = AimerApp::start_headless(page);
    app.render_frame();
    assert_eq!(watching_builds.get(), 1);
    assert_eq!(indifferent_builds.get(), 1);
    assert!(!compact.get(), "the window starts wider than the breakpoint");

    app.send_window_event(WindowEvent::Resized(PhysicalSize::new(390, 844)));
    app.render_frame();

    assert!(
        compact.get(),
        "the window reader kept the answer it gave for the old window"
    );
    assert!(
        watching_builds.get() > 1,
        "the window reader was never rebuilt"
    );
    assert_eq!(
        indifferent_builds.get(),
        1,
        "a widget that never read the window was rebuilt {} times by a resize",
        indifferent_builds.get()
    );
}

/// A widget that asks for the breakpoint rather than for the window sits out
/// every width where its layout cannot differ.
///
/// This is what a drag is: hundreds of resizes, none of which crosses the one
/// width the widget cares about, and one that does.
#[test]
fn a_breakpoint_reader_is_rebuilt_only_when_the_breakpoint_is_crossed() {
    let builds = Rc::new(Cell::new(0));
    let compact = Rc::new(Cell::new(false));

    let mut app = AimerApp::start_headless(Probe::selecting(&builds, &compact));
    app.render_frame();
    let settled = builds.get();

    for width in 700..800 {
        app.send_window_event(WindowEvent::Resized(PhysicalSize::new(width, 800)));
        app.render_frame();
    }

    assert_eq!(
        builds.get(),
        settled,
        "a drag that never crossed the breakpoint rebuilt the widget {} times",
        builds.get() - settled
    );

    app.send_window_event(WindowEvent::Resized(PhysicalSize::new(390, 844)));
    app.render_frame();

    assert!(compact.get(), "the breakpoint was crossed unnoticed");
}

/// The reader has to keep answering for every later resize, not just the first:
/// its registration is renewed by the rebuild the previous resize caused.
#[test]
fn a_window_reader_keeps_following_the_window_across_repeated_resizes() {
    let builds = Rc::new(Cell::new(0));
    let compact = Rc::new(Cell::new(false));

    let mut app = AimerApp::start_headless(Probe::watching(&builds, &compact));
    app.render_frame();

    for (size, expected) in [
        (PhysicalSize::new(390, 844), true),
        (PhysicalSize::new(1200, 800), false),
        (PhysicalSize::new(420, 900), true),
    ] {
        app.send_window_event(WindowEvent::Resized(size));
        app.render_frame();
        assert_eq!(
            compact.get(),
            expected,
            "the reader stopped following the window at {size:?}"
        );
    }
}
