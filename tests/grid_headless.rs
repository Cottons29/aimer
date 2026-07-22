use std::cell::Cell;
use std::rc::Rc;

use aimer::{
    AimerApp, AnyElement, BuildContext, Drawable, Element, EventElement, Grid, GridItem, GridTrack,
    LayoutElement, Rebuildable, ResolvedSize, VisitorElement, Widget,
};

#[derive(Clone)]
struct SizeProbe {
    observed: Rc<Cell<ResolvedSize>>,
}

struct SizeProbeElement {
    observed: Rc<Cell<ResolvedSize>>,
}

impl Widget for SizeProbe {
    fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
        SizeProbeElement {
            observed: self.observed.clone(),
        }
        .boxed()
    }
}

impl Drawable for SizeProbeElement {
    fn draw(&self, ctx: &BuildContext) {
        self.observed
            .set(ctx.parent_size);
    }
}

impl EventElement for SizeProbeElement {}
impl LayoutElement for SizeProbeElement {}
impl Rebuildable for SizeProbeElement {}

impl VisitorElement for SizeProbeElement {
    fn debug_name(&self) -> &'static str {
        "SizeProbe"
    }
}

#[test]
fn grid_assigns_cell_constraints_during_a_headless_frame() {
    let first = Rc::new(Cell::new(ResolvedSize::default()));
    let second = Rc::new(Cell::new(ResolvedSize::default()));
    let grid = Grid::new()
        .columns([GridTrack::Px(100.0), GridTrack::Px(200.0)])
        .rows([GridTrack::Px(50.0)])
        .children([
            GridItem::new(SizeProbe {
                observed: first.clone(),
            }),
            GridItem::new(SizeProbe {
                observed: second.clone(),
            }),
        ]);

    let mut app = AimerApp::start_headless(grid);
    app.render_frame();

    assert_eq!(
        first.get(),
        ResolvedSize {
            width: 100.0,
            height: 50.0
        }
    );
    assert_eq!(
        second.get(),
        ResolvedSize {
            width: 200.0,
            height: 50.0
        }
    );
}

#[test]
fn invalid_grid_configuration_renders_in_a_headless_frame() {
    let observed = Rc::new(Cell::new(ResolvedSize::default()));
    let grid = Grid::new().children([GridItem::new(SizeProbe { observed })]);

    let mut app = AimerApp::start_headless(grid);
    app.render_frame();
}
