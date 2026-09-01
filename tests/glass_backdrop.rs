use aimer::canvas::{Canvas, InnerCanvas};
use aimer::cupid::renderer::Renderer;
use aimer::widget::base::WindowHandle;
use aimer::{BuildContext, Color, Glass, ResolvedSize, SizedBox, Vec2d, Widget};
use aimer_utils::SyncFuture;

const FRAME_WIDTH: u32 = 2_176;
const FRAME_HEIGHT: u32 = 64;
const GLASS_X: u32 = 1_024;
const GLASS_SIZE: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .block()
        .ok()?;
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("glass widget regression device"),
            ..Default::default()
        })
        .block()
        .ok()
}

fn glass_frame(blur_radius: f32) -> aimer::cupid::draw_cmd::DrawList {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let size = ResolvedSize {
        width: FRAME_WIDTH as f32,
        height: FRAME_HEIGHT as f32,
    };
    let context = BuildContext::new(
        canvas.clone(),
        size,
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(
            winit::dpi::PhysicalSize::new(FRAME_WIDTH, FRAME_HEIGHT),
            1.0,
        ),
        runtime.handle().clone(),
    );

    canvas.fill_color_rect(
        Vec2d {
            x: GLASS_X as f32,
            y: 0.0,
        },
        ResolvedSize {
            width: GLASS_SIZE as f32 / 2.0,
            height: GLASS_SIZE as f32,
        },
        Color::Rgba(0, 0, 0, 255),
        [0.0; 4],
    );
    canvas.fill_color_rect(
        Vec2d {
            x: GLASS_X as f32 + GLASS_SIZE as f32 / 2.0,
            y: 0.0,
        },
        ResolvedSize {
            width: GLASS_SIZE as f32 / 2.0,
            height: GLASS_SIZE as f32,
        },
        Color::Rgba(255, 255, 255, 255),
        [0.0; 4],
    );

    let element = Glass::new()
        .tint(Color::Transparent)
        .opacity(0.0)
        .blur_radius(blur_radius)
        .border_width(0.0)
        .shadow_blur(0.0)
        .corner_radius(0.0)
        .child(
            SizedBox::new()
                .width(GLASS_SIZE as f32)
                .height(GLASS_SIZE as f32),
        )
        .to_element(&context);
    element.layout(&context);
    canvas.save();
    canvas.translate(Vec2d {
        x: GLASS_X as f32,
        y: 0.0,
    });
    element.draw(&context);
    canvas.restore();

    inner.take_draw_list()
}

fn tinted_glass_frame() -> aimer::cupid::draw_cmd::DrawList {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let size = ResolvedSize {
        width: FRAME_WIDTH as f32,
        height: FRAME_HEIGHT as f32,
    };
    let context = BuildContext::new(
        canvas.clone(),
        size,
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(
            winit::dpi::PhysicalSize::new(FRAME_WIDTH, FRAME_HEIGHT),
            1.0,
        ),
        runtime.handle().clone(),
    );

    canvas.fill_color_rect(
        Vec2d {
            x: GLASS_X as f32,
            y: 0.0,
        },
        ResolvedSize {
            width: GLASS_SIZE as f32,
            height: GLASS_SIZE as f32,
        },
        Color::Rgba(32, 32, 32, 255),
        [0.0; 4],
    );

    let element = Glass::new()
        .tint(Color::Rgba(224, 24, 24, 255))
        .opacity(0.9)
        .blur_radius(16.0)
        .edge_lighting(0.0)
        .specular_highlight(0.0)
        .border_color(Color::Transparent)
        .border_width(0.0)
        .shadow_blur(0.0)
        .corner_radius(0.0)
        .child(
            SizedBox::new()
                .width(GLASS_SIZE as f32)
                .height(GLASS_SIZE as f32),
        )
        .to_element(&context);
    element.layout(&context);
    canvas.save();
    canvas.translate(Vec2d {
        x: GLASS_X as f32,
        y: 0.0,
    });
    element.draw(&context);
    canvas.restore();

    inner.take_draw_list()
}

fn render_and_read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    draw_list: &aimer::cupid::draw_cmd::DrawList,
) -> Vec<u8> {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("glass widget regression target"),
        size: wgpu::Extent3d {
            width: FRAME_WIDTH,
            height: FRAME_HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());

    // Exercise the ordinary renderer entry point. Real presentation surfaces
    // are not guaranteed to support COPY_SRC, but Glass must still work.
    renderer.render(
        device,
        queue,
        &view,
        FRAME_WIDTH,
        FRAME_HEIGHT,
        false,
        draw_list,
    );

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("glass widget regression readback"),
        size: (FRAME_WIDTH * FRAME_HEIGHT * 4) as u64,
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
                bytes_per_row: Some(FRAME_WIDTH * 4),
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
        result.expect("glass readback buffer to map");
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("glass readback to finish");
    let pixels = slice
        .get_mapped_range()
        .expect("glass readback range")
        .to_vec();
    readback.unmap();
    pixels
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * FRAME_WIDTH + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().expect("RGBA pixel")
}

#[test]
fn glass_widget_blurs_a_backdrop_without_a_copyable_surface() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, FORMAT);

    let sharp = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &glass_frame(0.0),
    );
    let frosted = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &glass_frame(64.0),
    );
    let sample_x = GLASS_X + GLASS_SIZE / 2;
    let sharp_edge = pixel(&sharp, sample_x, GLASS_SIZE / 2);
    let frosted_edge = pixel(&frosted, sample_x, GLASS_SIZE / 2);
    let difference = sharp_edge[..3]
        .iter()
        .zip(&frosted_edge[..3])
        .map(|(left, right)| left.abs_diff(*right))
        .map(u16::from)
        .sum::<u16>();

    assert!(
        difference > 24,
        "Glass should visibly change the backdrop edge without surface COPY_SRC: sharp={sharp_edge:?}, frosted={frosted_edge:?}"
    );
}

#[test]
fn configured_glass_tint_reaches_the_edge_when_lighting_is_disabled() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, FORMAT);
    let pixels = render_and_read(
        &device,
        &queue,
        &mut renderer,
        &tinted_glass_frame(),
    );
    let edge = pixel(&pixels, GLASS_X + 1, GLASS_SIZE / 2);

    assert!(
        edge[0] > edge[1].saturating_add(40) && edge[0] > edge[2].saturating_add(40),
        "the configured red tint should reach the pane edge without a fixed cyan bloom: edge={edge:?}"
    );
}
