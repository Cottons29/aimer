use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use aimer_attribute::{ResolvedSize, Vec2d};
use aimer_canvas::{FrameCanvas, InnerCanvas};
use aimer_color::prelude::Color;
use aimer_container::{BackdropFilter, DecorationBox, SizedBox};
use aimer_style::{BoxDecoration, BoxShadow};
use aimer_widget::base::{BuildContext, WindowHandle};
use aimer_widget::{Drawable, Widget};

struct RecordingAllocator;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

// SAFETY: All allocation operations forward unchanged to System. The counter
// only observes allocations made on the current test thread.
unsafe impl GlobalAlloc for RecordingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: RecordingAllocator = RecordingAllocator;

#[tokio::test]
async fn decoration_box_moves_shadow_storage_without_allocating_after_warmup() {
    let inner = InnerCanvas::new();
    let ctx = BuildContext::new(
        FrameCanvas::new(&inner),
        ResolvedSize { width: 200.0, height: 160.0 },
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    );
    let shadowed = || BoxDecoration::new().background_color(Color::RED)
        .add_shadow(BoxShadow::new().blur(4.0));

    for _ in 0..4 {
        drop(DecorationBox::new().decoration(shadowed()).child(SizedBox::new()).to_element(&ctx));
    }
    let before_description = ALLOCATIONS.with(Cell::get);
    let decoration = shadowed();
    let describing = ALLOCATIONS.with(Cell::get) - before_description;
    let before_build = ALLOCATIONS.with(Cell::get);
    drop(DecorationBox::new().decoration(decoration).child(SizedBox::new()).to_element(&ctx));
    let building = ALLOCATIONS.with(Cell::get) - before_build;

    assert_eq!(describing, 1, "the counter must observe the shadow vector allocation");
    assert_eq!(building, 0, "warm conversion must move the decoration into pooled element storage");
}

#[tokio::test]
async fn decoration_box_unchanged_paint_checks_allocate_nothing() {
    let inner = InnerCanvas::new();
    let ctx = BuildContext::new(
        FrameCanvas::new(&inner),
        ResolvedSize { width: 200.0, height: 160.0 },
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    );
    let element = DecorationBox::new()
        .decoration(BoxDecoration::new().background_color(Color::RED))
        .child(SizedBox::new().width(24.0).height(12.0)).to_element(&ctx);
    let tree = aimer_cupid::draw_cmd_v2::RenderTree::new();
    let root = tree.add_root(aimer_cupid::draw_cmd_v2::Rect::new(0.0, 0.0, 24.0, 12.0)).unwrap();
    ctx.with_local_v2_paint_context(tree.context(root).unwrap(), |ctx| element.paint_local_v2(ctx));
    assert!(!element.local_v2_paint_needs_recording(&ctx));

    let before = ALLOCATIONS.with(Cell::get);
    let unchanged = (0..1_024).all(|_| !element.local_v2_paint_needs_recording(&ctx));
    let checking = ALLOCATIONS.with(Cell::get) - before;
    assert!(unchanged);
    assert_eq!(checking, 0);
}

#[tokio::test]
async fn backdrop_filter_reuses_its_recording_without_allocating() {
    let inner = InnerCanvas::new();
    let ctx = BuildContext::new(
        FrameCanvas::new(&inner),
        ResolvedSize { width: 200.0, height: 160.0 },
        1.0, Vec2d::default(), Vec2d::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    );
    let element = DecorationBox::new().decoration(BackdropFilter::new().saturation(0.5))
        .child(SizedBox::new().width(24.0).height(12.0)).to_element(&ctx);
    let tree = aimer_cupid::draw_cmd_v2::RenderTree::new();
    let root = tree.add_root(aimer_cupid::draw_cmd_v2::Rect::new(0.0, 0.0, 24.0, 12.0)).unwrap();
    ctx.with_local_v2_paint_context(tree.context(root).unwrap(), |ctx| element.paint_local_v2(ctx));
    let revision = tree.draw_list_revision(root).unwrap();
    let before = ALLOCATIONS.with(Cell::get);
    let unchanged = (0..1_024).all(|_| !element.local_v2_paint_needs_recording(&ctx));
    let checking = ALLOCATIONS.with(Cell::get) - before;
    assert!(unchanged);
    assert_eq!(tree.draw_list_revision(root).unwrap(), revision);
    assert_eq!(checking, 0, "unchanged filter traversal must reuse the encoded request");
}
