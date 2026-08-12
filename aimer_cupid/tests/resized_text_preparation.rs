//! Regression guard for text preparation during a live window resize.
//!
//! A wrapped layout is keyed by the width it wraps at, so a resize frame
//! misses the layout cache for everything on screen — that re-layout is what
//! the frame owes the screen. The off-screen tail a viewport asked for ahead
//! of itself is a different matter: laying it out at a width that will be
//! different again next frame is work the next frame throws away, and it
//! floods the layout cache with entries nothing will ever read. A resize
//! frame must therefore postpone the tail and prepare it on the first frame
//! whose size has settled.
//!
//! Runs against the first available adapter; skips (with a note) on machines
//! without one.

use std::sync::Arc;

use aimer_cupid::AntiAlias;
use aimer_cupid::font::{FontFamily, FontStyle};
use aimer_cupid::text_layout::TextHorizontalAlign;
use aimer_cupid::text_pipeline::{TextDrawRequest, TextOverflowMode, TextPipelineV2};
use aimer_utils::SyncFuture;

const HEIGHT: u32 = 300;
const LINE_HEIGHT: f32 = 24.0;
/// How many times taller than the viewport the document is.
const OVERDRAW_FACTOR: usize = 4;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .block()
        .ok()?;
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("resized text preparation device"),
            ..Default::default()
        })
        .block()
        .ok()
}

fn pipeline(device: &wgpu::Device) -> TextPipelineV2 {
    TextPipelineV2::new(
        device,
        wgpu::TextureFormat::Rgba8Unorm,
        None,
        AntiAlias::Analytic,
    )
}

/// One wrapped line of a document, sized against the surface width the way a
/// full-width column of text is — resizing the window changes every line's
/// wrapping width.
fn line(index: usize, surface_width: u32, scroll_offset: f32) -> TextDrawRequest {
    TextDrawRequest {
        x: 8.0,
        y: index as f32 * LINE_HEIGHT - scroll_offset,
        text: Arc::from(format!("line {index} with text that wraps to its column").as_str()),
        font_size: 16.0,
        color: [0.0, 0.0, 0.0, 1.0],
        bounds_width: surface_width as f32 - 16.0,
        bounds_height: LINE_HEIGHT,
        overflow: TextOverflowMode::Wrap,
        horizontal_align: TextHorizontalAlign::Left,
        line_height: None,
        font_family: FontFamily::SANS_SERIF,
        font_style: FontStyle::Normal,
        font_weight: None,
        language: None,
        italic: false,
        clip_rect: [0.0, 0.0, -1.0, 0.0],
        clip_border_radius: [0.0; 4],
        spans: Vec::new(),
    }
}

fn document(surface_width: u32, scroll_offset: f32) -> Vec<TextDrawRequest> {
    let line_count = (HEIGHT as f32 / LINE_HEIGHT).ceil() as usize * OVERDRAW_FACTOR;
    (0..line_count)
        .map(|index| line(index, surface_width, scroll_offset))
        .collect()
}

/// Prepares `requests` at a fixed size until nothing is postponed.
fn prepare_until_settled(
    pipeline: &mut TextPipelineV2,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    requests: &[TextDrawRequest],
) {
    for _ in 0..100 {
        pipeline.prepare(device, queue, width, HEIGHT, false, requests, &[]);
        if !pipeline.has_postponed_preparation() {
            return;
        }
    }
    panic!("text preparation never settled");
}

// The heart of the guard: the frame whose surface size changed must draw its
// visible text and postpone the tail, not spend its budget laying the tail
// out at a width the next resize frame invalidates.
#[test]
fn a_resize_frame_postpones_the_off_screen_tail() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };

    let mut pipeline = pipeline(&device);
    prepare_until_settled(&mut pipeline, &device, &queue, 400, &document(400, 0.0));

    // One live-resize frame: a new surface width, every wrapping width new.
    let resized = document(360, 0.0);
    pipeline.prepare(&device, &queue, 360, HEIGHT, false, &resized, &[]);

    let (alpha, _) = pipeline.frame_glyph_instances();
    assert!(alpha > 0, "the resize frame must still draw visible text");
    assert!(
        pipeline.has_postponed_preparation(),
        "a resize frame must postpone the off-screen tail instead of \
         laying it out at a width the next frame invalidates"
    );
}

// A wrapped layout only depends on the width when the text wraps at it. A
// document of lines that fit their column at every width of a drag must
// therefore not mint a fresh layout set per width — after the first step of
// the resize, every later step reuses the same width-independent layouts.
#[test]
fn a_width_change_that_wraps_nothing_reuses_every_layout() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };

    let mut pipeline = pipeline(&device);
    prepare_until_settled(&mut pipeline, &device, &queue, 400, &document(400, 0.0));

    // The first resize step pays the one-time canonicalization of the
    // document's layouts; none of the lines wrap at 398px.
    pipeline.prepare(&device, &queue, 398, HEIGHT, false, &document(398, 0.0), &[]);
    prepare_until_settled(&mut pipeline, &device, &queue, 398, &document(398, 0.0));
    let after_first_step = pipeline.layout_cache_entries();

    // Every further step of the drag must reuse those layouts wholesale.
    for width in [396, 394, 392, 390] {
        let resized = document(width, 0.0);
        pipeline.prepare(&device, &queue, width, HEIGHT, false, &resized, &[]);
        prepare_until_settled(&mut pipeline, &device, &queue, width, &resized);

        let (alpha, _) = pipeline.frame_glyph_instances();
        assert!(alpha > 0, "the resize frame must still draw visible text");
        assert_eq!(
            pipeline.layout_cache_entries(),
            after_first_step,
            "a width change that wraps nothing must not mint new layouts at {width}px"
        );
    }
}

// Postponing must not lose the tail: once the size settles, the whole
// document is prepared, and scrolling to its far end draws glyphs.
#[test]
fn text_arriving_after_a_resize_settles_draws_its_glyphs() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };

    let mut pipeline = pipeline(&device);
    prepare_until_settled(&mut pipeline, &device, &queue, 400, &document(400, 0.0));

    // The resize frame itself, then the settled frames after it.
    pipeline.prepare(&device, &queue, 360, HEIGHT, false, &document(360, 0.0), &[]);
    prepare_until_settled(&mut pipeline, &device, &queue, 360, &document(360, 0.0));
    let (top_alpha, _) = pipeline.frame_glyph_instances();
    assert!(top_alpha > 0, "settled text must draw glyphs");

    // Scroll to the bottom: the postponed tail is what is visible now.
    let visible_lines = (HEIGHT as f32 / LINE_HEIGHT).ceil() as usize;
    let line_count = visible_lines * OVERDRAW_FACTOR;
    let bottom_offset = (line_count - visible_lines) as f32 * LINE_HEIGHT;
    pipeline.prepare(
        &device,
        &queue,
        360,
        HEIGHT,
        false,
        &document(360, bottom_offset),
        &[],
    );
    let (bottom_alpha, _) = pipeline.frame_glyph_instances();
    assert!(
        bottom_alpha * 2 > top_alpha,
        "the tail prepared after the resize lost most of its glyphs: \
         top {top_alpha}, bottom {bottom_alpha}"
    );
}
