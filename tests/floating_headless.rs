use std::cell::Cell;
use std::rc::Rc;

use aimer::style::{LayoutSpacing, Spacing};
use aimer::{
    Anchor, AnchorHandle, AnyElement, BuildContext, Container, Dimension, Drawable, Element,
    EventElement, Floating, FloatingAlign, FloatingSide, LayoutElement, ModalHandle, Rebuildable,
    ResolvedSize, SizedBox, VisitorElement, Widget,
};

/// Records where its parent placed it during the paint pass.
#[derive(Clone)]
struct PositionProbe {
    observed: Rc<Cell<(f32, f32)>>,
}

struct PositionProbeElement {
    observed: Rc<Cell<(f32, f32)>>,
    size: ResolvedSize,
}

impl Widget for PositionProbe {
    fn to_element(&self, _ctx: &BuildContext) -> AnyElement {
        PositionProbeElement {
            observed: self.observed.clone(),
            size: ResolvedSize {
                width: 100.0,
                height: 60.0,
            },
        }
        .boxed()
    }
}

impl Drawable for PositionProbeElement {
    fn draw(&self, ctx: &BuildContext) {
        self.observed.set(ctx.canvas.get_transform_translation());
    }
}

impl EventElement for PositionProbeElement {}
impl Rebuildable for PositionProbeElement {}

impl LayoutElement for PositionProbeElement {
    fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
        self.size
    }

    fn content_size(&self, _ctx: &BuildContext) -> ResolvedSize {
        self.size
    }
}

impl VisitorElement for PositionProbeElement {
    fn debug_name(&self) -> &'static str {
        "PositionProbe"
    }
}

/// Builds a page whose only content is a 120x40 trigger inset by 20 logical
/// pixels, so the tracked anchor rectangle is `(20, 20, 120, 40)`.
fn anchored_page(handle: AnchorHandle) -> impl Widget {
    Container::new()
        .padding(LayoutSpacing::all(Spacing::Px(20)))
        .child(
            Anchor::new().handle(handle).child(
                Container::new()
                    .width(Dimension::Px(120.0))
                    .height(Dimension::Px(40.0))
                    .child(SizedBox::new().width(120).height(40)),
            ),
        )
}

fn show_panel(
    handle: AnchorHandle,
    side: FloatingSide,
    align: FloatingAlign,
    probe: Rc<Cell<(f32, f32)>>,
) -> ModalHandle {
    Floating::new()
        .anchor(handle)
        .side(side)
        .align(align)
        .gap(4.0)
        .child(PositionProbe { observed: probe })
        .show()
}

#[test]
fn a_panel_is_painted_below_the_rectangle_reported_by_its_anchor() {
    let handle = AnchorHandle::new();
    let observed = Rc::new(Cell::new((f32::NAN, f32::NAN)));
    let panel = show_panel(
        handle.clone(),
        FloatingSide::Bottom,
        FloatingAlign::Start,
        observed.clone(),
    );

    let mut app = aimer::AimerApp::start_headless(anchored_page(handle.clone()));
    app.render_frame();

    assert_eq!(
        handle.bounds().map(|bounds| (bounds.x, bounds.y)),
        Some((20.0, 20.0)),
        "the anchor must report where its child was painted"
    );

    let (x, y) = observed.get();
    assert!(
        (x - 20.0).abs() < 1e-3 && (y - 64.0).abs() < 1e-3,
        "expected the panel at (20, 64), got ({x}, {y})"
    );

    panel.dismiss();
}

#[test]
fn a_panel_aligned_to_the_trailing_edge_ends_where_the_anchor_ends() {
    let handle = AnchorHandle::new();
    let observed = Rc::new(Cell::new((f32::NAN, f32::NAN)));
    let panel = show_panel(
        handle.clone(),
        FloatingSide::Bottom,
        FloatingAlign::End,
        observed.clone(),
    );

    let mut app = aimer::AimerApp::start_headless(anchored_page(handle));
    app.render_frame();

    let (x, y) = observed.get();
    assert!(
        (x - 40.0).abs() < 1e-3 && (y - 64.0).abs() < 1e-3,
        "expected the panel at (40, 64), got ({x}, {y})"
    );

    panel.dismiss();
}
