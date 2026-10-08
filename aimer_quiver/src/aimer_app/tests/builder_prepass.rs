//! While an `AnimatedBuilder` animates, the rebuild prepass descends only the
//! path that leads to it, not every retained boundary in the window.
//!
//! The builder has to stay reachable for as long as it animates, and it used to
//! buy that by discarding the whole dirty-path index every frame, so each frame
//! walked every row of the page just to find the one element that wanted to be
//! rebuilt. The rows here are deep on purpose: a prepass that descends them
//! costs a multiple of the row count, a pruned one costs one visit per row.

use super::*;

use std::time::Duration;

use aimer_animation::{AnimatedBuilder, AnimationController, Curve};

const ROWS: usize = 40;
const ROW_DEPTH: usize = 5;
const FRAMES: usize = 20;

fn looping() -> AnimationController {
    let controller = AnimationController::new(Duration::from_millis(1500), Curve::Linear);
    controller.set_repeat(true);
    controller.forward_from_first_tick();
    controller
}

fn deep_row() -> aimer_widget::AnyWidget {
    let mut row = SizedBox::new()
        .width(Dimension::Px(120.0))
        .height(Dimension::Px(8.0))
        .color(Color::WHITE)
        .boxed();
    for _ in 0..ROW_DEPTH {
        row = aimer_container::Container::new().child(row).boxed();
    }
    row
}

/// A page of deep static rows with one repeating builder at its top.
fn start(calls: Rc<Cell<usize>>) -> HeadlessAimerApp<impl Widget + 'static> {
    let animated = AnimatedBuilder::new(looping(), move |progress| {
        calls.set(calls.get() + 1);
        SizedBox::new()
            .width(Dimension::Px(48.0))
            .height(Dimension::Px(48.0))
            .color(Color::Rgb((progress * 255.0) as u8, 0, 0))
    });
    let mut rows = vec![animated.boxed()];
    rows.extend((0..ROWS).map(|_| deep_row()));
    let page = aimer_container::Container::new().color(Color::WHITE).child(Column::new().children(rows));
    let mut app = AimerApp::start_headless_with(page, HeadlessOptions {
        size: PhysicalSize::new(200, 800), scale_factor: 1.0,
    });
    app.pump_frames(4);
    app
}

fn next_frame<W: Widget + 'static>(app: &mut HeadlessAimerApp<W>) {
    std::thread::sleep(Duration::from_millis(12));
    assert_eq!(app.pump_frames(1), 1, "the animation keeps asking for frames");
    app.last_frame_result.take().expect("a frame was drawn");
}

/// How many boundaries each of `FRAMES` animating frames descended, and how
/// many times the builder ran across them.
fn descents_per_frame() -> (Vec<u64>, usize) {
    let calls = Rc::new(Cell::new(0usize));
    let mut app = start(calls.clone());
    // Let the first animating frames settle: the first walk after a rebuild
    // legitimately indexes the tree.
    for _ in 0..3 {
        next_frame(&mut app);
    }
    let before_calls = calls.get();
    let mut descents = Vec::new();
    for _ in 0..FRAMES {
        let before = aimer_widget::rebuild_descent_count();
        next_frame(&mut app);
        descents.push(aimer_widget::rebuild_descent_count() - before);
    }
    (descents, calls.get() - before_calls)
}

#[test]
fn an_animating_builder_descends_only_its_own_path_in_the_rebuild_prepass() {
    let _serial = VIRTUALIZED_RENDER_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (descents, runs) = descents_per_frame();

    assert!(runs >= FRAMES - 5, "the builder ran only {runs} times in {FRAMES} animating frames");
    // The path to the builder is the window root, the page container, the
    // column and the builder itself, plus the wrappers around each and the
    // builder's own fresh child. None of that depends on the number of rows,
    // which alone hold `ROWS * (ROW_DEPTH + 1)` boundaries.
    let whole_rows = (ROWS * (ROW_DEPTH + 1)) as u64;
    let worst = descents.iter().copied().max().unwrap_or(0);
    assert!(
        worst <= 12,
        "an animating frame descended {worst} boundaries, per frame: {descents:?}; \
         the rows alone hold {whole_rows}"
    );
}

#[test]
fn a_builder_in_deep_rows_keeps_animating_while_the_prepass_stays_pruned() {
    let _serial = VIRTUALIZED_RENDER_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let calls = Rc::new(Cell::new(0usize));
    let mut app = start(calls.clone());
    let before = calls.get();
    for _ in 0..FRAMES * 2 {
        next_frame(&mut app);
    }
    let runs = calls.get() - before;
    assert!(runs >= FRAMES * 2 - 5, "the builder froze: {runs} runs in {} frames", FRAMES * 2);
}

/// An animator that lives inside another builder's output is a brand-new
/// element every time the outer builder replaces its child. Nothing has walked
/// it yet, so the pass has to descend the replacement once, or the inner
/// animator would be pruned and stop as soon as the outer one finished.
#[test]
fn an_animator_inside_a_finished_builder_keeps_animating() {
    let _serial = VIRTUALIZED_RENDER_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let inner_runs = Rc::new(Cell::new(0usize));
    let outer = AnimationController::new(Duration::from_millis(40), Curve::Linear);
    outer.forward_from_first_tick();
    let inner = looping();
    let animated = {
        let inner_runs = inner_runs.clone();
        AnimatedBuilder::new(outer.clone(), move |_| {
            let inner_runs = inner_runs.clone();
            AnimatedBuilder::new(inner.clone(), move |progress| {
                inner_runs.set(inner_runs.get() + 1);
                SizedBox::new()
                    .width(Dimension::Px(48.0))
                    .height(Dimension::Px(48.0))
                    .color(Color::Rgb((progress * 255.0) as u8, 0, 0))
            })
        })
    };
    let mut rows = vec![animated.boxed()];
    rows.extend((0..ROWS).map(|_| deep_row()));
    let page = aimer_container::Container::new().color(Color::WHITE).child(Column::new().children(rows));
    let mut app = AimerApp::start_headless_with(page, HeadlessOptions {
        size: PhysicalSize::new(200, 800), scale_factor: 1.0,
    });
    app.pump_frames(4);
    // Run past the outer animation so its last replacement is long behind us.
    for _ in 0..8 {
        next_frame(&mut app);
    }
    assert!(!outer.is_animating(), "the outer animation must have finished");

    let before = inner_runs.get();
    for _ in 0..FRAMES {
        next_frame(&mut app);
    }
    let runs = inner_runs.get() - before;
    assert!(runs >= FRAMES - 5, "the inner animator ran only {runs} times in {FRAMES} frames");
}

mod moved_state {
    use super::*;

    use std::cell::RefCell;

    use aimer_widget::{StateUpdater, StatefulElement};

    /// A keyed stateful box that counts its builds and publishes its updater.
    #[derive(Clone)]
    struct Tagged {
        builds: Rc<Cell<usize>>,
        updater: Rc<RefCell<Option<StateUpdater<TaggedState>>>>,
    }

    struct TaggedState {
        shade: u8,
        builds: Rc<Cell<usize>>,
        updater: Rc<RefCell<Option<StateUpdater<TaggedState>>>>,
    }

    impl StatefulWidget for Tagged {
        type State = TaggedState;

        fn create_state(self) -> TaggedState {
            TaggedState { shade: 0, builds: self.builds, updater: self.updater }
        }
    }

    impl State<Tagged> for TaggedState {
        fn init_state(&mut self, updater: StateUpdater<Self>) {
            // Every builder frame creates a fresh state that reconciliation then
            // discards in favour of the live one, which is the first.
            self.updater.borrow_mut().get_or_insert(updater);
        }

        fn build(&self, _ctx: &BuildContext) -> impl Widget {
            self.builds.set(self.builds.get() + 1);
            SizedBox::new()
                .width(Dimension::Px(40.0))
                .height(Dimension::Px(20.0))
                .color(Color::Rgb(self.shade, 90, 90))
        }
    }

    impl Widget for Tagged {
        fn to_element(self, ctx: &BuildContext) -> AnyElement {
            StatefulElement::new_with_name(self, ctx, "Tagged", Some(aimer_widget::Key::Value("tag".into())))
                .0
                .boxed()
        }
    }

    impl aimer_widget::PortableWidget for Tagged {}

    fn spacer() -> aimer_widget::AnyWidget {
        SizedBox::new().width(Dimension::Px(40.0)).height(Dimension::Px(20.0)).boxed()
    }

    /// A keyed stateful element carries its dirty source, and with it the path
    /// it was last indexed under, into whatever the builder produces next. When
    /// the builder moves it under a different parent the carried path is wrong,
    /// and a state change after the animation ends has to find the element at
    /// its new place.
    #[test]
    fn a_stateful_element_moved_by_a_builder_still_rebuilds_when_its_state_changes() {
        let _serial = VIRTUALIZED_RENDER_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tagged = Tagged { builds: Rc::new(Cell::new(0)), updater: Rc::default() };
        let builds = tagged.builds.clone();
        let updater = tagged.updater.clone();
        let outer = AnimationController::new(Duration::from_millis(40), Curve::Linear);
        outer.forward_from_first_tick();
        let animated = AnimatedBuilder::new(outer.clone(), move |progress| {
            let holder = aimer_container::Container::new().child(tagged.clone()).boxed();
            if progress < 0.5 {
                Column::new().children(vec![holder, spacer()]).boxed()
            } else {
                Column::new().children(vec![spacer(), holder]).boxed()
            }
        });
        let mut rows = vec![animated.boxed()];
        rows.extend((0..ROWS).map(|_| deep_row()));
        let page = aimer_container::Container::new().color(Color::WHITE).child(Column::new().children(rows));
        let mut app = AimerApp::start_headless_with(page, HeadlessOptions {
            size: PhysicalSize::new(200, 800), scale_factor: 1.0,
        });
        app.pump_frames(4);
        // The animation ends partway through these, after which no frame is due.
        for _ in 0..8 {
            std::thread::sleep(Duration::from_millis(12));
            app.pump_frames(1);
        }
        assert!(!outer.is_animating(), "the builder must have finished moving the element");
        app.pump_frames(3);

        let before = builds.get();
        let updater = updater.borrow().expect("the state was initialised");
        updater.set_state(|state| state.shade = 200);
        app.pump_frames(3);
        assert!(builds.get() > before, "the moved element was not rebuilt after its state changed");
    }
}
