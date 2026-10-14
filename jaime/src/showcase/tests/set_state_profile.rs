//! Cost of a real `set_state` in the Jaime showcase.
//!
//! `ExampleShowcase` is the root stateful element: clicking a sidebar row calls
//! `set_state(selected = ..)`, which re-runs `build` for the sidebar *and* the
//! selected page. That makes two real-world interactions measurable:
//!
//! * **switch** – click a row that selects a different page (rebuild, plus the
//!   new page's first build);
//! * **reselect** – click the row that is already selected. `selected` does not
//!   change, so this is a pure `set_state` rebuild of the whole app with that
//!   page mounted, with no new page state created.
//!
//! Run unoptimised, which is the mode this repository optimises for:
//!
//! ```text
//! cargo nextest run -p jaime profile_showcase_set_state --run-ignored only --no-capture
//! ```

use super::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use aimer::HeadlessOptions;
use aimer::cupid::draw_cmd_v2::DrawCommand;
use aimer::quiver::frame_stats::{frame_content_stats, reset_frame_content_stats};
use aimer::quiver::winit::dpi::{PhysicalPosition, PhysicalSize};
use aimer::quiver::winit::event::{DeviceId, ElementState, MouseButton, WindowEvent};
use aimer::{Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement};

/// Logical width of the sidebar; labels to its right belong to the page.
const SIDEBAR_LOGICAL_WIDTH: f32 = 292.0;
const RESELECTS: usize = 5;

#[derive(Default)]
pub(super) struct Labels {
    /// Collection walks every text node, so it only runs while this is set.
    pub(super) capture: Cell<bool>,
    pub(super) found: RefCell<Vec<(String, f32, f32)>>,
    /// Time spent inside the showcase's `rebuild_if_dirty` since last reset:
    /// the rebuild pass alone, without layout, paint or presentation.
    rebuild: Cell<std::time::Duration>,
}

pub(super) struct Captured {
    pub(super) labels: Rc<Labels>,
}

impl aimer::PortableWidget for Captured {}
impl Widget for Captured {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        CapturedElement {
            child: ExampleShowcase::new().to_element(ctx),
            labels: self.labels,
        }
        .boxed()
    }
}

struct CapturedElement {
    child: AnyElement,
    labels: Rc<Labels>,
}

impl VisitorElement for CapturedElement {
    fn debug_name(&self) -> &'static str {
        "CapturedShowcase"
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}
impl EventElement for CapturedElement {}
impl Rebuildable for CapturedElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        let start = Instant::now();
        self.child.rebuild_if_dirty(ctx);
        self.labels.rebuild.set(self.labels.rebuild.get() + start.elapsed());
    }
}
impl LayoutElement for CapturedElement {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }
}
impl Drawable for CapturedElement {
    fn can_paint_local_v2(&self, _ctx: &BuildContext) -> bool {
        true
    }
    fn paint_local_v2(&self, ctx: &BuildContext) {
        aimer::canvas::Canvas::of(ctx).finish();
    }
    fn update(&self, ctx: &BuildContext) {
        self.child.update(ctx);
        if self.labels.capture.get() {
            let mut found = Vec::new();
            collect(self.child.as_ref(), ctx, &mut found, None);
            *self.labels.found.borrow_mut() = found;
        }
    }
}

fn collect(
    element: &dyn Element,
    ctx: &BuildContext,
    found: &mut Vec<(String, f32, f32)>,
    inherited: Option<(Vec2d, Vec2d)>,
) {
    let bounds = element.pos_start_end().or(inherited);
    if element.debug_name() == "RawTextWidget"
        && let Some((start, end)) = bounds
    {
        for paint in &aimer_widget::testing::record_retained_paint(element, ctx) {
            for command in paint.commands.iter() {
                if let DrawCommand::DrawText { text, .. } = command {
                    found.push((
                        text.to_string(),
                        (start.x + end.x) * 0.5,
                        (start.y + end.y) * 0.5,
                    ));
                }
            }
        }
    }
    element.visit_children(&mut |child| collect(child, ctx, found, bounds));
}

/// Clicks at logical `(x, y)` and returns the milliseconds of the slowest frame.
pub(super) fn click_at<W: Widget + 'static>(
    app: &mut aimer::quiver::HeadlessAimerApp<W>,
    x: f32,
    y: f32,
    scale: f64,
) -> f64 {
    let device = DeviceId::dummy();
    app.send_window_event(WindowEvent::CursorMoved {
        device_id: device,
        position: PhysicalPosition::new(x as f64 * scale, y as f64 * scale),
    });
    app.render_frame();
    let mut worst = 0.0f64;
    for state in [ElementState::Pressed, ElementState::Released] {
        let start = Instant::now();
        app.send_window_event(WindowEvent::MouseInput {
            device_id: device,
            state,
            button: MouseButton::Left,
        });
        app.render_frame();
        worst = worst.max(start.elapsed().as_secs_f64() * 1e3);
    }
    worst
}

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

#[test]
#[ignore = "profiling harness: prints timings and asserts nothing"]
fn profile_showcase_set_state() {
    // Tall enough that every sidebar row is laid out and clickable.
    let scale = 1.0;
    let labels = Rc::new(Labels::default());
    let mut app = AimerApp::start_headless_with(
        theme::provide(Captured {
            labels: labels.clone(),
        }),
        HeadlessOptions {
            size: PhysicalSize::new(1280, 4200),
            scale_factor: scale,
        },
    );
    app.pump_frames(8);

    labels.capture.set(true);
    app.render_frame();
    labels.capture.set(false);
    let sidebar: Vec<(String, f32, f32)> = labels
        .found
        .borrow()
        .iter()
        .filter(|(_, x, _)| *x < SIDEBAR_LOGICAL_WIDTH)
        .cloned()
        .collect();

    let mut nudge = 0usize;
    // Returns the milliseconds of the slowest frame of the click and the time
    // the rebuild pass took across the whole click.
    let mut click = |app: &mut aimer::quiver::HeadlessAimerApp<_>, x: f32, y: f32| -> (f64, f64) {
        // Move the pointer a few pixels between clicks so consecutive clicks on
        // one row are never read as a double tap.
        let x = x + (nudge % 3) as f32 * 7.0 - 7.0;
        nudge += 1;
        labels.rebuild.set(Default::default());
        let worst = click_at(app, x, y, scale);
        (worst, labels.rebuild.get().as_secs_f64() * 1e3)
    };

    println!(
        "\n{:<26} {:>10} {:>12} {:>11} {:>9} {:>9} {:>7}   ({} sidebar labels; counts need --features aimer/frame-stats)",
        "page",
        "switch ms",
        "reselect ms",
        "rebuild ms",
        "idle ms",
        "sf builds",
        "nodes",
        sidebar.len()
    );
    let mut reselect_all = Vec::new();
    let mut rebuild_all = Vec::new();
    let mut idle_all = Vec::new();
    let mut switch_all = Vec::new();
    let mut skipped = Vec::new();
    for example in EXAMPLES {
        let Some((_, x, y)) = sidebar.iter().find(|(text, _, _)| text == example.label()) else {
            skipped.push(example.label());
            continue;
        };
        let (switch, _) = click(&mut app, *x, *y);
        app.pump_frames(6);
        reset_frame_content_stats();
        let (reselects, rebuilds): (Vec<f64>, Vec<f64>) = (0..RESELECTS)
            .map(|_| click(&mut app, *x, *y))
            .unzip();
        let (reselect, rebuild) = (median(reselects), median(rebuilds));
        let stats = frame_content_stats();
        let per_click = |value: u64| value as f64 / RESELECTS as f64;
        app.pump_frames(2);
        let idle = median(
            (0..5)
                .map(|_| {
                    let start = Instant::now();
                    app.render_frame();
                    start.elapsed().as_secs_f64() * 1e3
                })
                .collect(),
        );
        println!(
            "{:<26} {:>10.2} {:>12.2} {:>11.2} {:>9.2} {:>9.1} {:>7.0}",
            example.label(),
            switch,
            reselect,
            rebuild,
            idle,
            per_click(stats.stateful_builds),
            per_click(stats.drawn_nodes),
        );
        rebuild_all.push(rebuild);
        idle_all.push(idle);
        switch_all.push(switch);
        reselect_all.push(reselect);
    }
    if !skipped.is_empty() {
        println!("not clickable (label not found in the sidebar): {skipped:?}");
    }
    if !reselect_all.is_empty() {
        println!(
            "\npages measured: {}   median switch {:.2} ms   reselect {:.2} ms   of which rebuild {:.2} ms   idle frame {:.2} ms",
            reselect_all.len(),
            median(switch_all),
            median(reselect_all),
            median(rebuild_all),
            median(idle_all)
        );
    }
}
