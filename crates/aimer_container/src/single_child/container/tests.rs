use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::rc::Rc;

use aimer_cupid::draw_cmd::DrawCommand;
use super::*;
use crate::SizedBox;
use aimer_widget::base::WindowHandle;
use aimer_widget::{Drawable, EventElement, LayoutElement, Rebuildable, VisitorElement};

struct DrawProbe {
    draws: Rc<Cell<usize>>,
}

impl Drawable for DrawProbe {
    fn update(&self, _ctx: &BuildContext) {
        self.draws.set(self.draws.get() + 1);
    }
}

impl EventElement for DrawProbe {}
impl LayoutElement for DrawProbe {}
impl Rebuildable for DrawProbe {}

impl VisitorElement for DrawProbe {
    fn debug_name(&self) -> &'static str {
        "DrawProbe"
    }
}

struct PaintStableProbe;

impl Drawable for PaintStableProbe {
    fn update(&self, _ctx: &BuildContext) {}

}

impl EventElement for PaintStableProbe {}
impl LayoutElement for PaintStableProbe {}
impl Rebuildable for PaintStableProbe {}

impl VisitorElement for PaintStableProbe {
    fn debug_name(&self) -> &'static str {
        "PaintStableProbe"
    }
}

#[tokio::test]
async fn retained_container_paint_includes_its_decoration() {
    let (ctx, inner) = recording_context();
    let mut element = RawContainer::new(PaintStableProbe);
    element.width = Dimension::Px(80.0);
    element.height = Dimension::Px(40.0);
    element.box_decoration = BoxDecoration::new()
        .background_color(Color::BLACK)
        .border_radius(8.0);

    let commands = record_local_v2(&element, &ctx);

    assert!(commands
        .iter()
        .any(|command| matches!(command, aimer_cupid::draw_cmd_v2::DrawCommand::FillRect { .. })));
    assert!(
        inner.draw_list().commands().is_empty(),
        "the canvas itself is not painted"
    );
}

#[test]
fn containers_are_transparent_unless_they_occlude_scroll_and_never_cache_draw_bounds() {
    let child = DrawProbe {
        draws: Rc::new(Cell::new(0)),
    }
    .boxed();
    let mut container = RawContainer::new(child);

    assert_eq!(
        container.event_tree_role(),
        aimer_widget::EventTreeRole::Transparent
    );
    assert_eq!(container.event_tree_bounds(), None);

    container.box_decoration.update_color(Color::BLACK);
    assert_eq!(
        container.event_tree_role(),
        aimer_widget::EventTreeRole::IndexedTarget
    );
}

/// Counts every call that reaches the system allocator on this thread.
///
/// The count is what makes the migration's claim checkable: a widget hands
/// its fields to its element instead of copying them, so a warm rebuild of a
/// decorated tree must not reach the allocator at all.
struct RecordingAllocator;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: Every method forwards to `System` unchanged; the counter only
// observes the call.
unsafe impl GlobalAlloc for RecordingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: RecordingAllocator = RecordingAllocator;

fn record() {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

fn context() -> BuildContext<'static> {
    let canvas = {
        let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
        aimer_canvas::FrameCanvas::new(inner)
    };
    BuildContext::new(
        canvas,
        ResolvedSize::default(),
        1.0,
        Default::default(),
        Default::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    )
}

fn recording_context() -> (BuildContext<'static>, &'static aimer_canvas::InnerCanvas) {
    let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
    let ctx = BuildContext::new(
        aimer_canvas::FrameCanvas::new(inner),
        ResolvedSize {
            width: 200.0,
            height: 160.0,
        },
        1.0,
        Default::default(),
        Default::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    );
    (ctx, inner)
}

/// Records `element`'s own retained list, as the frame loop does for a node.
fn record_local_v2(
    element: &impl Drawable,
    ctx: &BuildContext<'_>,
) -> std::sync::Arc<[aimer_cupid::draw_cmd_v2::DrawCommand]> {
    let tree = aimer_cupid::draw_cmd_v2::RenderTree::new();
    let root = tree
        .add_root(aimer_cupid::draw_cmd_v2::Rect::new(0.0, 0.0, 400.0, 300.0))
        .unwrap();
    let node_context = tree.context(root).unwrap();
    ctx.with_local_v2_paint_context(node_context, |ctx| {
        assert!(element.can_paint_local_v2(ctx));
        element.paint_local_v2(ctx);
    });
    tree.draw_list_snapshot(root).unwrap().commands
}

#[tokio::test]
async fn margin_keeps_outline_and_border_inside_the_decorated_box() {
    let (mut ctx, inner) = recording_context();
    ctx.box_constraint = aimer_attribute::BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 200.0,
        max_height: 160.0,
    };
    let border = BorderSlice::new()
        .style(BorderStyle::Solid)
        .stroke(4.0);
    let outline = BorderSlice::new()
        .style(BorderStyle::Solid)
        .stroke(6.0);

    let element = Container::new()
        .margin(LayoutSpacing::all(30))
        .padding(LayoutSpacing::all(8))
        .box_decoration(
            BoxDecoration::new()
                .border(BoxBorder::all(border))
                .outline(BoxOutline::all(outline)),
        )
        .child(crate::ZeroSizedBox)
        .to_element(&ctx);

    assert_eq!(
        element.content_size(&ctx),
        ResolvedSize {
            width: 116.0,
            height: 76.0,
        }
    );
    element.update(&ctx);

    let commands = inner.draw_list();
    assert!(matches!(commands.commands().first(), Some(DrawCommand::PushTransform { .. })));
    let margin_transform = commands.commands().get(1).and_then(|command| match command {
        DrawCommand::SetTransform { matrix } => Some(*matrix),
        _ => None,
    });
    assert_eq!(margin_transform.map(|matrix| matrix.cols[2]), Some([30.0, 30.0, 1.0]));

    // The decorated box is recorded in the node's own list, offset by the
    // margin, so the margin itself is never filled.
    let recorded = record_local_v2(&element, &ctx);
    let fill = recorded.iter().find_map(|command| match command {
        aimer_cupid::draw_cmd_v2::DrawCommand::FillRect {
            rect,
            border_width,
            outline_width,
            ..
        } => Some((*rect, *border_width, *outline_width)),
        _ => None,
    });
    let (fill, border_width, outline_width) =
        fill.expect("the decorated container should record one fill rectangle");
    assert_eq!((fill.x, fill.y), (30.0, 30.0), "the box starts after the margin");
    assert_eq!((fill.width, fill.height), (140.0, 100.0));
    assert_eq!(border_width, [4.0; 4]);
    assert_eq!(outline_width, [6.0; 4]);

    let transforms = commands
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::SetTransform { matrix } => Some(matrix.cols[2]),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(transforms, vec![[30.0, 30.0, 1.0], [42.0, 42.0, 1.0]]);
}

/// A decoration owning a shadow list, the field this migration is about.
fn shadowed() -> BoxDecoration {
    BoxDecoration {
        box_shadow: vec![BoxShadow::default(), BoxShadow::default()],
        ..BoxDecoration::default()
    }
}

/// A 100x60 container with `decoration`, plus the radii it reports for its
/// child's clip and whether it can paint locally.
fn rounded_child_clip(decoration: BoxDecoration, scale: f32) -> (bool, [f32; 4]) {
    let (mut ctx, _inner) = recording_context();
    ctx.scale = scale;
    ctx.box_constraint = aimer_attribute::BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 400.0,
        max_height: 300.0,
    };
    let element = Container::new()
        .width(Dimension::Px(100.0))
        .height(Dimension::Px(60.0))
        .box_decoration(decoration)
        .child(crate::ZeroSizedBox)
        .to_element(&ctx);
    // The render tree walker asks the inner node, not the `AnyElement`
    // wrapper, which does not forward `can_paint_local_v2`.
    let node: &dyn Element = element.as_ref();
    let mut child = None;
    node.visit_children(&mut |element| child = Some(element));
    let child = child.expect("the container exposes its child");
    (
        node.can_paint_local_v2(&ctx),
        node.retained_v2_child_clip_radius(&ctx, child),
    )
}

#[tokio::test]
async fn a_rounded_container_paints_locally_and_rounds_its_childs_clip() {
    let (local, radii) = rounded_child_clip(BoxDecoration::new().border_radius(12), 1.0);
    assert!(local, "a border radius must not force the legacy path");
    assert_eq!(radii, [12.0; 4]);
}

#[tokio::test]
async fn a_square_container_reports_a_square_child_clip() {
    let (local, radii) = rounded_child_clip(BoxDecoration::new(), 1.0);
    assert!(local);
    assert_eq!(radii, [0.0; 4]);
}

#[tokio::test]
async fn the_child_clip_radius_shrinks_by_the_border_like_the_live_path() {
    let border = BorderSlice::new().style(BorderStyle::Solid).stroke(4.0);
    let decoration = BoxDecoration::new()
        .border(BoxBorder::all(border))
        .border_radius(12);
    let (local, radii) = rounded_child_clip(decoration, 1.0);
    assert!(local);
    assert_eq!(radii, [8.0; 4]);
}

#[tokio::test]
async fn a_radius_smaller_than_the_border_clamps_the_child_clip_to_square() {
    let border = BorderSlice::new().style(BorderStyle::Solid).stroke(4.0);
    let decoration = BoxDecoration::new()
        .border(BoxBorder::all(border))
        .border_radius(3);
    let (_, radii) = rounded_child_clip(decoration, 1.0);
    assert_eq!(radii, [0.0; 4]);
}

#[tokio::test]
async fn the_child_clip_radius_is_reported_in_logical_pixels_at_any_scale() {
    let border = BorderSlice::new().style(BorderStyle::Solid).stroke(4.0);
    let decoration = BoxDecoration::new()
        .border(BoxBorder::all(border))
        .border_radius(12);
    let (_, radii) = rounded_child_clip(decoration, 2.0);
    assert_eq!(radii, [8.0; 4]);
}

/// A container with `decoration` whose parent leaves both axes unbounded in
/// the given way, plus what its v2 geometry reports for its child.
struct UnboundedProbe {
    can_paint: bool,
    clip: Option<aimer_cupid::draw_cmd_v2::Rect>,
    child_max: Option<(f32, f32)>,
    outsets: Option<[f32; 4]>,
}

fn unbounded_container(max_width: f32, max_height: f32) -> UnboundedProbe {
    let (mut ctx, _inner) = recording_context();
    ctx.box_constraint = aimer_attribute::BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width,
        max_height,
    };
    let element = Container::new()
        .box_decoration(BoxDecoration::new().background_color(Color::BLUE))
        .child(crate::ZeroSizedBox)
        .to_element(&ctx);
    let node: &dyn Element = element.as_ref();
    let mut child = None;
    node.visit_children(&mut |element| child = Some(element));
    let child = child.expect("the container exposes its child");
    UnboundedProbe {
        can_paint: node.can_paint_local_v2(&ctx),
        clip: node
            .retained_v2_child_geometry(&ctx, child)
            .and_then(|(_, clip)| clip),
        child_max: node
            .retained_v2_child_context(&ctx, child)
            .map(|ctx| (ctx.box_constraint.max_width, ctx.box_constraint.max_height)),
        outsets: node.retained_v2_paint_outsets(&ctx),
    }
}

#[tokio::test]
async fn an_auto_height_container_in_a_scroll_view_paints_locally() {
    // A scrollable offers an unbounded height as `f32::MAX`.
    let probe = unbounded_container(400.0, f32::MAX);
    assert!(probe.can_paint, "an unbounded axis must not force the legacy path");

    // The render tree gets finite, sane rectangles: `f32::MAX` doubles to
    // infinity when scaled to device pixels.
    let clip = probe.clip.expect("the child is clipped");
    assert!(clip.width.is_finite() && clip.height.is_finite());
    assert!(clip.height <= LOCAL_V2_CONTAINER_EXTENT_LIMIT, "{clip:?}");
    assert!(probe.outsets.is_some_and(|outsets| outsets.iter().all(|v| v.is_finite())));
}

#[tokio::test]
async fn the_child_still_sees_an_unbounded_axis_as_unbounded() {
    // Clamping the *render tree's* rectangles must not change layout: a
    // child sizes itself to its content only while its constraint stays
    // above the unbounded threshold.
    let probe = unbounded_container(400.0, f32::MAX);
    let (_, max_height) = probe.child_max.expect("child context");
    assert!(
        max_height > LOCAL_V2_CONTAINER_EXTENT_LIMIT,
        "child max height was clamped to {max_height}"
    );
}

#[tokio::test]
async fn an_auto_width_container_under_an_unbounded_width_paints_locally() {
    // Row-like parents offer a quarter of `f32::MAX` per child.
    let probe = unbounded_container(f32::MAX / 4.0, 120.0);
    assert!(probe.can_paint);
    let clip = probe.clip.expect("the child is clipped");
    assert!(clip.width.is_finite() && clip.width <= LOCAL_V2_CONTAINER_EXTENT_LIMIT);
    let (max_width, _) = probe.child_max.expect("child context");
    assert!(max_width > LOCAL_V2_CONTAINER_EXTENT_LIMIT);
}

#[tokio::test]
async fn a_bounded_container_is_unchanged_by_the_unbounded_handling() {
    let probe = unbounded_container(400.0, 300.0);
    assert!(probe.can_paint);
    let clip = probe.clip.expect("the child is clipped");
    assert_eq!((clip.width, clip.height), (400.0, 300.0));
    assert_eq!(probe.child_max, Some((400.0, 300.0)));
}

#[tokio::test]
async fn offscreen_container_skips_unknown_child_draw_but_keeps_traversal_views() {
    let draws = Rc::new(Cell::new(0));
    let container = RawContainer::new(DrawProbe {
        draws: draws.clone(),
    });
    let mut ctx = context();
    ctx.parent_size = ResolvedSize {
        width: 100.0,
        height: 100.0,
    };
    ctx.box_constraint = aimer_attribute::BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 100.0,
        max_height: 100.0,
    };
    ctx.visible_rect = Some((0.0, 101.0, 100.0, 20.0));

    container.update(&ctx);

    assert_eq!(
        draws.get(),
        0,
        "a clipped container must not dynamically dispatch an offscreen child"
    );

    let mut structural = 0;
    container.structural_children(&mut |_| structural += 1);
    assert_eq!(structural, 1);

    let mut events = 0;
    container.event_children(&mut |_| events += 1);
    assert_eq!(events, 1);

    let mut hit_test = 0;
    container.hit_test_children(&mut |_| hit_test += 1);
    assert_eq!(hit_test, 1);

    ctx.visible_rect = Some((0.0, 0.0, 100.0, 100.0));
    container.update(&ctx);
    assert_eq!(
        draws.get(),
        1,
        "an unknown-bounds child remains drawable when its clipping parent is visible"
    );
}

#[test]
fn the_counter_counts() {
    let before = allocations();
    let allocated = vec![0u8; 64];

    assert!(
        allocations() > before,
        "a test asserting zero allocations is worthless if nothing is counted"
    );
    drop(allocated);
}

#[tokio::test]
async fn converting_a_decorated_container_adds_no_allocation() {
    let ctx = context();

    // Warm the pooled element storage the way a running application does:
    // the first frames pay for their blocks, the steady state reuses them.
    for _ in 0..4 {
        drop(
            Container::new()
                .box_decoration(shadowed())
                .child(SizedBox::new())
                .to_element(&ctx),
        );
    }

    let before_decoration = allocations();
    let decoration = shadowed();
    let describing = allocations() - before_decoration;

    let before_build = allocations();
    drop(
        Container::new()
            .box_decoration(decoration)
            .child(SizedBox::new())
            .to_element(&ctx),
    );
    let building = allocations() - before_build;

    assert_eq!(
        describing, 1,
        "the shadow list is the one allocation a decorated container costs, \
         and it is paid when the decoration is described"
    );
    assert_eq!(
        building, 0,
        "the conversion must hand the shadow list to the element rather than \
         copy it, and must find its element block in the pool: a warm build \
         of a decorated container reaches the allocator zero times"
    );
}

/// A container's element has to fit the largest class the `aimer_rubick`
/// pool serves — 512 bytes — or every build allocates it from the global
/// allocator and every drop frees it again, because an oversized payload is
/// unpooled and can never be recycled.
///
/// The element is dominated by its [`BoxDecoration`], which is dominated by
/// its eight border and outline colors. That is why a color is stored as one
/// packed word: with a color wide enough to carry HSLA components this
/// element measured 576 bytes and spilled out of the pool.
#[test]
fn a_container_element_fits_a_pooled_block() {
    eprintln!("Container Size : {}", size_of::<Container>());

    assert!(
        size_of::<RawContainer<AnyElement>>() <= 512,
        "a container element grew past the largest pooled class: {} bytes",
        size_of::<RawContainer<AnyElement>>()
    );
}
