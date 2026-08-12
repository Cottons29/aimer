//! Regression guard for request-level text culling.
//!
//! A scroll viewport hands its child more text than the screen can show, so a
//! text-heavy document reaches `TextPipelineV2::prepare` with most of its
//! requests off screen. Those requests must not cost the frame anything
//! per-glyph: no instances built, no bytes uploaded, no atlas capacity
//! reserved. The visibility rule is the one the pipeline already trusts for
//! postponing preparation — a request whose bounds meet neither the surface
//! nor its clip cannot show a pixel.
//!
//! Runs against the first available adapter; skips (with a note) on machines
//! without one.

use std::sync::Arc;

use aimer_cupid::AntiAlias;
use aimer_cupid::font::{FontFamily, FontStyle};
use aimer_cupid::text_layout::TextHorizontalAlign;
use aimer_cupid::text_pipeline::{TextDrawRequest, TextOverflowMode, TextPipelineV2};
use aimer_utils::SyncFuture;

const WIDTH: u32 = 400;
const HEIGHT: u32 = 300;
const LINE_HEIGHT: f32 = 24.0;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .block()
        .ok()?;
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("text culling regression device"),
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

/// One line of a document, `index` line-heights below the document origin,
/// shifted up by `scroll_offset` — unclipped, the way content that relies on
/// the surface edge alone arrives.
fn line(index: usize, scroll_offset: f32) -> TextDrawRequest {
    TextDrawRequest {
        x: 8.0,
        y: index as f32 * LINE_HEIGHT - scroll_offset,
        text: Arc::from(format!("line {index} with some scrolling text").as_str()),
        font_size: 16.0,
        color: [0.0, 0.0, 0.0, 1.0],
        bounds_width: WIDTH as f32 - 16.0,
        bounds_height: LINE_HEIGHT,
        overflow: TextOverflowMode::Clip,
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

/// Prepares `requests` until nothing is postponed, so every layout — visible
/// or ahead of view — is cached and the last frame is a steady-state one.
fn prepare_until_settled(
    pipeline: &mut TextPipelineV2,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    requests: &[TextDrawRequest],
) {
    for _ in 0..100 {
        pipeline.prepare(device, queue, WIDTH, HEIGHT, false, requests, &[]);
        if !pipeline.has_postponed_preparation() {
            return;
        }
    }
    panic!("text preparation never settled");
}

// The heart of the guard: a fully prepared document whose tail hangs far
// below the surface must produce exactly the instances its visible head
// produces alone. Anything more means the frame is building and uploading
// quads the screen can never show.
#[test]
fn off_screen_requests_build_no_instances() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };

    // Ceiling: a line straddling the bottom edge still shows its top row.
    let visible_lines = (HEIGHT as f32 / LINE_HEIGHT).ceil() as usize;
    let document: Vec<_> = (0..visible_lines * 4).map(|i| line(i, 0.0)).collect();

    let mut head_only = pipeline(&device);
    prepare_until_settled(
        &mut head_only,
        &device,
        &queue,
        &document[..visible_lines],
    );
    let (head_alpha, head_color) = head_only.frame_glyph_instances();
    assert!(head_alpha > 0, "the visible head must produce glyphs");

    let mut whole_document = pipeline(&device);
    prepare_until_settled(&mut whole_document, &device, &queue, &document);
    assert_eq!(
        whole_document.frame_glyph_instances(),
        (head_alpha, head_color),
        "off-screen requests must contribute no glyph instances"
    );
}

// Culling must not eat text on its way in: after scrolling a culled line into
// view, its glyphs must be drawn — the ahead-of-view preparation it received
// while off screen has to pay off on the arrival frame.
#[test]
fn a_culled_request_scrolled_into_view_draws_its_glyphs() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };

    let visible_lines = (HEIGHT as f32 / LINE_HEIGHT).ceil() as usize;
    let line_count = visible_lines * 4;

    let mut pipeline = pipeline(&device);
    let document: Vec<_> = (0..line_count).map(|i| line(i, 0.0)).collect();
    prepare_until_settled(&mut pipeline, &device, &queue, &document);
    let (top_alpha, _) = pipeline.frame_glyph_instances();

    // Scroll to the bottom: a completely different set of lines is visible.
    let bottom_offset = (line_count - visible_lines) as f32 * LINE_HEIGHT;
    let scrolled: Vec<_> = (0..line_count).map(|i| line(i, bottom_offset)).collect();
    pipeline.prepare(&device, &queue, WIDTH, HEIGHT, false, &scrolled, &[]);
    let (bottom_alpha, _) = pipeline.frame_glyph_instances();

    assert!(
        bottom_alpha > 0,
        "lines scrolled into view must draw glyphs"
    );
    // The bottom shows as many full lines as the top did (same line height,
    // same surface), so the instance count must be in the same ballpark —
    // a fraction of it would mean arrived text is missing glyphs.
    assert!(
        bottom_alpha * 2 > top_alpha,
        "arrived text lost most of its glyphs: top {top_alpha}, bottom {bottom_alpha}"
    );
}
