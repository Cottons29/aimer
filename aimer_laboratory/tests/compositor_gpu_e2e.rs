//! End-to-end GPU acceptance test for Aimer's retained compositor.
//!
//! This test deliberately crosses the framework seams instead of constructing
//! a `DrawList` by hand:
//!
//! `Widget -> Element -> CupidCanvas -> CompositorScene -> FramePacket -> Renderer -> Metal`
//!
//! Run it on a macOS machine with a visible, host GPU session:
//!
//! ```text
//! AIMER_REQUIRE_GPU_E2E=1 WGPU_BACKENDS=metal WGPU_POWER_PREF=high cargo test -p aimer_laboratory --test compositor_gpu_e2e -- --nocapture
//! ```

use std::sync::OnceLock;

use aimer::{
    AnyElement, BuildContext, Color, Dimension, Drawable, Element, EventElement, LayoutElement,
    Rebuildable, RepaintBoundary, ResolvedSize, Size, Vec2d, VisitorElement, Widget,
};
use aimer::canvas::{Canvas, InnerCanvas};
use aimer::cupid::damage_region::DamageSet;
use aimer::cupid::frame::{Frame, FramePacket, FrameRenderMetadata};
use aimer::cupid::renderer::Renderer;
use aimer_widget::base::WindowHandle;
use tokio::runtime::Runtime;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct FourColorWidget;

impl aimer::PortableWidget for FourColorWidget {}

impl Widget for FourColorWidget {
    fn to_element(self, _ctx: &BuildContext) -> AnyElement {
        FourColorElement.boxed()
    }
}

struct FourColorElement;

impl VisitorElement for FourColorElement {
    fn debug_name(&self) -> &'static str {
        "GpuCompositorProbe"
    }
}

impl EventElement for FourColorElement {}
impl Rebuildable for FourColorElement {}

impl LayoutElement for FourColorElement {
    fn size(&self) -> Option<Size> {
        Some(Size {
            width: Dimension::Px(WIDTH as f32),
            height: Dimension::Px(HEIGHT as f32),
        })
    }

    fn is_layout_stable(&self) -> bool {
        true
    }
}

impl Drawable for FourColorElement {
    fn draw(&self, ctx: &BuildContext) {
        self.paint(ctx);
    }

    fn paint(&self, ctx: &BuildContext) {
        let half = WIDTH as f32 / 2.0;
        let colors = [
            (0.0, 0.0, Color::RED),
            (half, 0.0, Color::GREEN),
            (0.0, half, Color::BLUE),
            (half, half, Color::WHITE),
        ];
        for (x, y, color) in colors {
            ctx.canvas.fill_color_rect(
                Vec2d { x, y },
                ResolvedSize {
                    width: half,
                    height: half,
                },
                color,
                [0.0; 4],
            );
        }
    }

    fn is_paint_stable(&self) -> bool {
        true
    }

    fn is_paint_bounded(&self) -> bool {
        true
    }
}

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("create compositor test runtime")
    })
}

fn build_context<'a>(inner: &'a InnerCanvas) -> BuildContext<'a> {
    BuildContext::new(
        Canvas::new(inner),
        ResolvedSize {
            width: WIDTH as f32,
            height: HEIGHT as f32,
        },
        1.0,
        Vec2d::ZERO,
        Vec2d::ZERO,
        WindowHandle::headless(
            aimer::quiver::winit::dpi::PhysicalSize::new(WIDTH, HEIGHT),
            1.0,
        ),
        tokio::runtime::Handle::current(),
    )
}

fn record_packet(
    inner: &InnerCanvas,
    element: &AnyElement,
    ctx: &BuildContext<'_>,
    damage: DamageSet,
) -> FramePacket {
    aimer::aimer_widget::begin_paint_frame(WIDTH, HEIGHT);
    inner.begin_frame();
    let measured = element.layout(ctx);
    assert_eq!(measured.width, WIDTH as f32);
    assert_eq!(measured.height, HEIGHT as f32);
    ctx.canvas.save();
    element.draw(ctx);
    ctx.canvas.restore();

    let draw_list = inner.take_draw_list();
    assert_eq!(draw_list.stats().retained_layers, 1);
    let scene = inner
        .take_scene(&draw_list, WIDTH, HEIGHT, damage.clone())
        .expect("widget paint should produce a compositor scene");
    assert!(scene.is_recorded());
    assert_eq!(scene.nodes().len(), 1);
    assert_eq!(scene.surfaces().len(), 1);

    FramePacket::with_scene(
        Frame::new(draw_list, WIDTH, HEIGHT),
        FrameRenderMetadata::new(1.0, 7, 1, 1, 1, damage),
        scene,
    )
}

fn gpu() -> Option<(wgpu::Device, wgpu::Queue, wgpu::AdapterInfo)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::METAL,
        flags: wgpu::InstanceFlags::default(),
        backend_options: wgpu::BackendOptions::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        display: None,
    });
    let adapter = runtime()
        .block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: true,
        }))
        .ok()?;
    let info = adapter.get_info();
    let (device, queue) = runtime()
        .block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("aimer compositor end-to-end test"),
            ..Default::default()
        }))
        .ok()?;
    Some((device, queue, info))
}

fn read_target(device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = WIDTH * 4;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("aimer compositor end-to-end readback"),
        size: (bytes_per_row * HEIGHT) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("aimer compositor end-to-end readback encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| {
        result.expect("map compositor readback buffer");
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("wait for compositor readback");
    let pixels = slice
        .get_mapped_range()
        .expect("map compositor readback range")
        .to_vec();
    readback.unmap();
    pixels
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * WIDTH + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().expect("RGBA pixel")
}

fn assert_pixel(pixels: &[u8], x: u32, y: u32, expected: [u8; 4]) {
    let actual = pixel(pixels, x, y);
    for (channel, (actual, expected)) in actual.into_iter().zip(expected).enumerate() {
        assert!(
            actual.abs_diff(expected) <= 2,
            "pixel ({x}, {y}) channel {channel}: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn widget_scene_packet_reaches_metal_and_reuses_unchanged_content() {
    let require_gpu = std::env::var_os("AIMER_REQUIRE_GPU_E2E").is_some();
    let Some((device, queue, adapter_info)) = gpu() else {
        if require_gpu {
            panic!("Metal adapter unavailable; run this test in a host GPU session");
        }
        eprintln!("skipping: Metal adapter unavailable");
        return;
    };
    assert_eq!(adapter_info.backend, wgpu::Backend::Metal);
    println!(
        "gpu compositor e2e: adapter={} backend={:?}",
        adapter_info.name, adapter_info.backend
    );

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("aimer compositor end-to-end target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let inner = InnerCanvas::new();
    let _entered = runtime().enter();
    let ctx = build_context(&inner);
    let element = RepaintBoundary::new().child(FourColorWidget).to_element(&ctx);
    let mut renderer = Renderer::new(&device, FORMAT);

    let first_packet = record_packet(
        &inner,
        &element,
        &ctx,
        DamageSet::full(WIDTH, HEIGHT),
    );
    renderer.render_packet(&device, &queue, &view, &first_packet, false);
    let first_pixels = read_target(&device, &queue, &target);
    let first_stats = renderer.compositor_stats();
    println!(
        "first frame: nodes={} changes={} rasterized_surfaces={} composed_surfaces={}",
        first_stats.logical_nodes,
        first_stats.scene_changes,
        first_stats.rasterized_surfaces,
        first_stats.composed_surfaces,
    );
    assert_eq!(first_stats.logical_nodes, 1);
    assert_eq!(first_stats.promoted_surfaces, 1);
    assert!(first_stats.rasterized_surfaces >= 1);
    assert!(first_stats.composed_surfaces >= 1);
    assert_pixel(&first_pixels, 16, 16, [255, 0, 0, 255]);
    assert_pixel(&first_pixels, 48, 16, [0, 255, 0, 255]);
    assert_pixel(&first_pixels, 16, 48, [0, 0, 255, 255]);
    assert_pixel(&first_pixels, 48, 48, [255, 255, 255, 255]);

    let second_packet = record_packet(&inner, &element, &ctx, DamageSet::new(WIDTH, HEIGHT));
    renderer.render_packet(&device, &queue, &view, &second_packet, false);
    let second_pixels = read_target(&device, &queue, &target);
    let second_stats = renderer.compositor_stats();
    println!(
        "second frame: nodes={} changes={} reused_nodes={} rasterized_surfaces={} reused_surfaces={} full_repaint={}",
        second_stats.logical_nodes,
        second_stats.scene_changes,
        second_stats.reused_nodes,
        second_stats.rasterized_surfaces,
        second_stats.reused_surfaces,
        second_stats.full_repaint,
    );
    assert_eq!(second_stats.logical_nodes, 1);
    assert_eq!(second_stats.scene_changes, 0);
    assert_eq!(second_stats.reused_nodes, 1);
    assert_eq!(second_stats.rasterized_surfaces, 0);
    assert!(second_stats.reused_surfaces >= 1);
    assert!(!second_stats.full_repaint);
    assert_eq!(first_pixels, second_pixels);
}
