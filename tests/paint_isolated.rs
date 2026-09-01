use std::sync::Arc;

use aimer::canvas::{Canvas, InnerCanvas};
use aimer::cupid::draw_cmd::{DrawList, RetainedLayerContent};
use aimer::cupid::renderer::Renderer;
use aimer::cupid::svg::{
    SvgColor, SvgElementKind, SvgFill, SvgFillRule, SvgGeometry, SvgNode, SvgNodeId,
    SvgPaintOrder, SvgPathCommand, SvgScene, SvgTransform, SvgViewport,
};
use aimer::{Color, FontFamily, FontStyle, ResolvedSize, Vec2d};
use aimer_utils::SyncFuture;

const FRAME_WIDTH: u32 = 96;
const FRAME_HEIGHT: u32 = 72;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const SRGB_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const CHILD_WIDTH: f32 = 60.0;
const CHILD_HEIGHT: f32 = 52.0;
const IMAGE_ID: u32 = 17;
const IMAGE_BYTES: [u8; 4] = [244, 196, 48, 255];
const READBACK_BYTES_PER_ROW: u32 = (FRAME_WIDTH * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
    * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .block()
        .ok()?;
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("paint isolation regression device"),
            ..Default::default()
        })
        .block()
        .ok()
}

fn paint_child(canvas: &Canvas<'_>) {
    canvas.fill_color_rect(
        Vec2d { x: 4.0, y: 5.0 },
        ResolvedSize {
            width: CHILD_WIDTH,
            height: CHILD_HEIGHT,
        },
        Color::Rgba(236, 82, 48, 255),
        [0.0; 4],
    );
    canvas.fill_color_rect(
        Vec2d { x: 18.0, y: 17.0 },
        ResolvedSize {
            width: 24.0,
            height: 18.0,
        },
        Color::Rgba(42, 185, 166, 255),
        [0.0; 4],
    );
}

fn background(canvas: &Canvas<'_>) {
    canvas.fill_color_rect(
        Vec2d::ZERO,
        ResolvedSize {
            width: FRAME_WIDTH as f32,
            height: FRAME_HEIGHT as f32,
        },
        Color::Rgba(16, 22, 30, 255),
        [0.0; 4],
    );
}

fn compose_direct(offset: Vec2d) -> DrawList {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);

    background(&canvas);
    canvas.save();
    canvas.translate(Vec2d {
        x: 11.0 + offset.x,
        y: 9.0 + offset.y,
    });
    canvas.set_clip(
        Vec2d { x: 7.0, y: 6.0 },
        ResolvedSize {
            width: 48.0,
            height: 37.0,
        },
    );
    paint_child(&canvas);
    canvas.clear_clip();
    canvas.restore();

    inner.take_draw_list()
}

fn record_child() -> Arc<RetainedLayerContent> {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    paint_child(&canvas);
    let recorded = inner.take_draw_list();
    Arc::new(RetainedLayerContent::from_snapshot(
        recorded
            .retained_snapshot()
            .expect("primitive child paint should be retainable"),
    ))
}

fn compose_retained(offset: Vec2d, content: Arc<RetainedLayerContent>) -> DrawList {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);

    background(&canvas);
    canvas.save();
    canvas.translate(Vec2d {
        x: 11.0 + offset.x,
        y: 9.0 + offset.y,
    });
    canvas.set_clip(
        Vec2d { x: 7.0, y: 6.0 },
        ResolvedSize {
            width: 48.0,
            height: 37.0,
        },
    );
    canvas.draw_retained_layer(71, CHILD_WIDTH, CHILD_HEIGHT, content);
    canvas.clear_clip();
    canvas.restore();

    inner.take_draw_list()
}

fn svg_scene() -> Arc<SvgScene> {
    Arc::new(SvgScene {
        viewport: SvgViewport {
            width: 24.0,
            height: 20.0,
        },
        nodes: Arc::from([SvgNode {
            node_id: SvgNodeId(0),
            svg_id: None,
            classes: Arc::from([]),
            element: SvgElementKind::Path,
            parent: None,
            children: Arc::from([]),
            transform: SvgTransform::default(),
            opacity: 1.0,
            geometry: Some(0),
            fill: Some(SvgFill {
                color: SvgColor::rgba8(226, 78, 144, 255),
                rule: SvgFillRule::NonZero,
            }),
            stroke: None,
            paint_order: SvgPaintOrder::FillAndStroke,
            visible: true,
        }]),
        geometries: Arc::from([SvgGeometry {
            commands: Arc::from([
                SvgPathCommand::MoveTo { x: 3.0, y: 2.0 },
                SvgPathCommand::LineTo { x: 21.0, y: 2.0 },
                SvgPathCommand::LineTo { x: 21.0, y: 18.0 },
                SvgPathCommand::LineTo { x: 3.0, y: 18.0 },
                SvgPathCommand::Close,
            ]),
        }]),
    })
}

fn record_svg_child() -> Arc<RetainedLayerContent> {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    canvas.draw_svg(
        svg_scene(),
        Vec2d::ZERO,
        ResolvedSize {
            width: CHILD_WIDTH,
            height: CHILD_HEIGHT,
        },
        Arc::from([]),
    );
    let recorded = inner.take_draw_list();
    Arc::new(RetainedLayerContent::from_snapshot(
        recorded
            .retained_snapshot()
            .expect("an SVG child paint should be retainable"),
    ))
}

fn compose_direct_svg(offset: Vec2d) -> DrawList {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);

    background(&canvas);
    canvas.save();
    canvas.translate(Vec2d {
        x: 11.0 + offset.x,
        y: 9.0 + offset.y,
    });
    canvas.set_clip(
        Vec2d { x: 7.0, y: 6.0 },
        ResolvedSize {
            width: 48.0,
            height: 37.0,
        },
    );
    canvas.draw_svg(
        svg_scene(),
        Vec2d::ZERO,
        ResolvedSize {
            width: CHILD_WIDTH,
            height: CHILD_HEIGHT,
        },
        Arc::from([]),
    );
    canvas.clear_clip();
    canvas.restore();

    inner.take_draw_list()
}

fn compose_retained_svg(offset: Vec2d, content: Arc<RetainedLayerContent>) -> DrawList {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);

    background(&canvas);
    canvas.save();
    canvas.translate(Vec2d {
        x: 11.0 + offset.x,
        y: 9.0 + offset.y,
    });
    canvas.set_clip(
        Vec2d { x: 7.0, y: 6.0 },
        ResolvedSize {
            width: 48.0,
            height: 37.0,
        },
    );
    canvas.draw_retained_layer(73, CHILD_WIDTH, CHILD_HEIGHT, content);
    canvas.clear_clip();
    canvas.restore();

    inner.take_draw_list()
}

fn paint_text_child(canvas: &Canvas<'_>) {
    canvas.draw_text_styled(
        "Aimer",
        Vec2d { x: 5.0, y: 30.0 },
        18.0,
        Color::Rgba(244, 236, 196, 255),
        FontFamily::MONOSPACE,
        FontStyle::Normal,
        700,
    );
}

fn record_text_child() -> Arc<RetainedLayerContent> {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    paint_text_child(&canvas);
    let recorded = inner.take_draw_list();
    Arc::new(RetainedLayerContent::from_snapshot(
        recorded
            .retained_snapshot()
            .expect("a font-backed text child should be retainable"),
    ))
}

fn compose_direct_text(offset: Vec2d) -> DrawList {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);

    background(&canvas);
    canvas.save();
    canvas.translate(Vec2d {
        x: 11.0 + offset.x,
        y: 9.0 + offset.y,
    });
    canvas.set_clip(
        Vec2d { x: 7.0, y: 6.0 },
        ResolvedSize {
            width: 48.0,
            height: 37.0,
        },
    );
    paint_text_child(&canvas);
    canvas.clear_clip();
    canvas.restore();

    inner.take_draw_list()
}

fn compose_retained_text(offset: Vec2d, content: Arc<RetainedLayerContent>) -> DrawList {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);

    background(&canvas);
    canvas.save();
    canvas.translate(Vec2d {
        x: 11.0 + offset.x,
        y: 9.0 + offset.y,
    });
    canvas.set_clip(
        Vec2d { x: 7.0, y: 6.0 },
        ResolvedSize {
            width: 48.0,
            height: 37.0,
        },
    );
    canvas.draw_retained_layer(74, CHILD_WIDTH, CHILD_HEIGHT, content);
    canvas.clear_clip();
    canvas.restore();

    inner.take_draw_list()
}

fn paint_color_child(canvas: &Canvas<'_>, color: Color) {
    canvas.fill_color_rect(
        Vec2d { x: 4.0, y: 5.0 },
        ResolvedSize {
            width: CHILD_WIDTH,
            height: CHILD_HEIGHT,
        },
        color,
        [0.0; 4],
    );
}

fn record_color_child(color: Color) -> Arc<RetainedLayerContent> {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    paint_color_child(&canvas, color);
    let recorded = inner.take_draw_list();
    Arc::new(RetainedLayerContent::from_snapshot(
        recorded
            .retained_snapshot()
            .expect("an animated color child should be retainable"),
    ))
}

fn compose_direct_color(offset: Vec2d, color: Color) -> DrawList {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);

    background(&canvas);
    canvas.save();
    canvas.translate(Vec2d {
        x: 11.0 + offset.x,
        y: 9.0 + offset.y,
    });
    canvas.set_clip(
        Vec2d { x: 7.0, y: 6.0 },
        ResolvedSize {
            width: 48.0,
            height: 37.0,
        },
    );
    paint_color_child(&canvas, color);
    canvas.clear_clip();
    canvas.restore();

    inner.take_draw_list()
}

fn compose_retained_color(offset: Vec2d, content: Arc<RetainedLayerContent>) -> DrawList {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);

    background(&canvas);
    canvas.save();
    canvas.translate(Vec2d {
        x: 11.0 + offset.x,
        y: 9.0 + offset.y,
    });
    canvas.set_clip(
        Vec2d { x: 7.0, y: 6.0 },
        ResolvedSize {
            width: 48.0,
            height: 37.0,
        },
    );
    canvas.draw_retained_layer(75, CHILD_WIDTH, CHILD_HEIGHT, content);
    canvas.clear_clip();
    canvas.restore();

    inner.take_draw_list()
}

fn record_image_child() -> Arc<RetainedLayerContent> {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    canvas.set_texture_size(IMAGE_ID, 1, 1);
    canvas.draw_image(
        IMAGE_ID,
        Vec2d { x: 4.0, y: 5.0 },
        ResolvedSize {
            width: CHILD_WIDTH,
            height: CHILD_HEIGHT,
        },
    );
    let recorded = inner.take_draw_list();
    Arc::new(RetainedLayerContent::from_snapshot(
        recorded
            .retained_snapshot()
            .expect("a loaded image draw should be retainable"),
    ))
}

fn compose_direct_image(offset: Vec2d) -> DrawList {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);

    background(&canvas);
    canvas.load_image_with_id(IMAGE_ID, &IMAGE_BYTES, 1, 1);
    canvas.save();
    canvas.translate(Vec2d {
        x: 11.0 + offset.x,
        y: 9.0 + offset.y,
    });
    canvas.set_clip(
        Vec2d { x: 7.0, y: 6.0 },
        ResolvedSize {
            width: 48.0,
            height: 37.0,
        },
    );
    canvas.draw_image(
        IMAGE_ID,
        Vec2d { x: 4.0, y: 5.0 },
        ResolvedSize {
            width: CHILD_WIDTH,
            height: CHILD_HEIGHT,
        },
    );
    canvas.clear_clip();
    canvas.restore();

    inner.take_draw_list()
}

fn compose_retained_image(
    offset: Vec2d,
    content: Arc<RetainedLayerContent>,
) -> DrawList {
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);

    background(&canvas);
    canvas.load_image_with_id(IMAGE_ID, &IMAGE_BYTES, 1, 1);
    canvas.save();
    canvas.translate(Vec2d {
        x: 11.0 + offset.x,
        y: 9.0 + offset.y,
    });
    canvas.set_clip(
        Vec2d { x: 7.0, y: 6.0 },
        ResolvedSize {
            width: 48.0,
            height: 37.0,
        },
    );
    canvas.draw_retained_layer(72, CHILD_WIDTH, CHILD_HEIGHT, content);
    canvas.clear_clip();
    canvas.restore();

    inner.take_draw_list()
}

fn render_and_read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    draw_list: &DrawList,
) -> Vec<u8> {
    render_and_read_with_format(device, queue, renderer, FORMAT, false, draw_list)
}

fn render_and_read_with_format(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    format: wgpu::TextureFormat,
    is_srgb: bool,
    draw_list: &DrawList,
) -> Vec<u8> {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("paint isolation regression target"),
        size: wgpu::Extent3d {
            width: FRAME_WIDTH,
            height: FRAME_HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    renderer.render(
        device,
        queue,
        &view,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        is_srgb,
        draw_list,
    );

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("paint isolation regression readback"),
        size: (READBACK_BYTES_PER_ROW * FRAME_HEIGHT) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(READBACK_BYTES_PER_ROW),
                rows_per_image: Some(FRAME_HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: FRAME_WIDTH,
            height: FRAME_HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| {
        result.expect("paint isolation readback to map");
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("paint isolation readback to finish");
    let mapped = slice
        .get_mapped_range()
        .expect("paint isolation readback range")
        .to_vec();
    readback.unmap();
    mapped
        .chunks_exact(READBACK_BYTES_PER_ROW as usize)
        .flat_map(|row| row[..(FRAME_WIDTH * 4) as usize].iter().copied())
        .collect()
}

fn assert_pixels_match(direct: &[u8], retained: &[u8], label: &str) {
    assert_eq!(direct.len(), retained.len());
    let mut differing_pixels = 0;
    let mut largest_channel_delta = 0;
    for (direct, retained) in direct.chunks_exact(4).zip(retained.chunks_exact(4)) {
        let channel_delta = direct
            .iter()
            .zip(retained)
            .map(|(left, right)| left.abs_diff(*right))
            .max()
            .unwrap_or(0);
        largest_channel_delta = largest_channel_delta.max(channel_delta);
        if channel_delta > 2 {
            differing_pixels += 1;
        }
    }

    assert!(
        largest_channel_delta <= 8 && differing_pixels <= 16,
        "direct and retained output diverged for {label}: {differing_pixels} pixels differed, largest channel delta={largest_channel_delta}"
    );
}

fn assert_has_foreground_pixels(pixels: &[u8], label: &str) {
    let background = [16, 22, 30, 255];
    let foreground_pixels = pixels
        .chunks_exact(4)
        .filter(|pixel| *pixel != background)
        .count();
    assert!(
        foreground_pixels > 0,
        "{label} did not rasterize any foreground pixels"
    );
}

#[test]
fn retained_paint_matches_direct_output_across_scroll_transforms_and_clip() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, FORMAT);
    let content = record_child();

    let first_offset = Vec2d { x: 3.0, y: 2.0 };
    let direct_first = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_direct(first_offset),
    );
    let retained_first = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_retained(first_offset, Arc::clone(&content)),
    );
    assert_pixels_match(&direct_first, &retained_first, "the first composition");

    let second_offset = Vec2d { x: 21.0, y: -4.0 };
    let direct_second = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_direct(second_offset),
    );
    let retained_second = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_retained(second_offset, content),
    );
    assert_pixels_match(&direct_second, &retained_second, "the replayed composition");
}

#[test]
fn retained_image_paint_matches_direct_output_after_a_texture_upload() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, FORMAT);
    let content = record_image_child();
    let offset = Vec2d { x: 19.0, y: -3.0 };

    let direct = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_direct_image(offset),
    );
    let retained = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_retained_image(offset, content),
    );
    assert_pixels_match(&direct, &retained, "a texture-backed composition");
}

#[test]
fn retained_svg_paint_matches_direct_output_across_transform_and_clip() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, FORMAT);
    let content = record_svg_child();

    let offset = Vec2d { x: 13.0, y: -2.0 };
    let direct = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_direct_svg(offset),
    );
    let retained = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_retained_svg(offset, content),
    );
    assert_pixels_match(&direct, &retained, "an SVG-backed composition");
}

#[test]
fn retained_font_backed_text_matches_direct_output_across_transform_and_clip() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, FORMAT);
    let content = record_text_child();
    let offset = Vec2d { x: -4.0, y: 7.0 };

    let direct = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_direct_text(offset),
    );
    let retained = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_retained_text(offset, content),
    );
    assert_has_foreground_pixels(&direct, "direct font-backed text");
    assert_has_foreground_pixels(&retained, "retained font-backed text");
    assert_pixels_match(&direct, &retained, "a font-backed text composition");
}

#[test]
fn retained_font_backed_text_matches_direct_output_on_an_srgb_target() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, SRGB_FORMAT);
    let content = record_text_child();
    let offset = Vec2d { x: 6.0, y: 3.0 };

    let direct = render_and_read_with_format(
        &device,
        &queue,
        &mut renderer,
        SRGB_FORMAT,
        true,
        &compose_direct_text(offset),
    );
    let retained = render_and_read_with_format(
        &device,
        &queue,
        &mut renderer,
        SRGB_FORMAT,
        true,
        &compose_retained_text(offset, content),
    );
    assert_has_foreground_pixels(&direct, "direct sRGB font-backed text");
    assert_has_foreground_pixels(&retained, "retained sRGB font-backed text");
    assert_pixels_match(&direct, &retained, "an sRGB font-backed text composition");
}

#[test]
fn retained_layer_refreshes_when_an_animation_payload_changes() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, FORMAT);
    let offset = Vec2d { x: 5.0, y: 4.0 };
    let first_color = Color::Rgba(234, 76, 84, 255);
    let second_color = Color::Rgba(52, 142, 232, 255);

    let direct_first = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_direct_color(offset, first_color),
    );
    let retained_first = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_retained_color(offset, record_color_child(first_color)),
    );
    assert_pixels_match(
        &direct_first,
        &retained_first,
        "the first animation payload",
    );

    let direct_second = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_direct_color(offset, second_color),
    );
    let retained_second = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &compose_retained_color(offset, record_color_child(second_color)),
    );
    assert_ne!(direct_first, direct_second, "the animation payload did not change pixels");
    assert_pixels_match(
        &direct_second,
        &retained_second,
        "the refreshed animation payload",
    );
}
