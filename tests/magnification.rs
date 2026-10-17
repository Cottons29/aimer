#![cfg(feature = "wgpu")]
#[path = "backdrop_filter/fixtures.rs"]
mod fixtures;
#[path = "magnification/pixels.rs"]
mod pixels;
#[path = "magnification/styles.rs"]
mod styles;
use aimer_cupid::{WgpuBackend, frame::FramePacket, renderer::Renderer};
use aimer_utils::SyncFuture;
use fixtures::*;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[test]
fn magnification_composable_styles() {
    let (device, queue) = gpu().expect("GPU for magnification style validation");
    let backend = WgpuBackend::new(device.clone(), queue.clone());
    for aa in [
        aimer_cupid::AntiAlias::Analytic,
        aimer_cupid::AntiAlias::Msaa4x,
    ] {
        let mut renderer = Renderer::with_antialiasing(&backend, FORMAT, aa);
        styles::styles_match_sampling_and_preserve_disabled_axes(|packet| {
            render_packet_and_read(&device, &queue, &backend, &mut renderer, packet)
        });
    }
}

#[test]
fn magnification_centered_zoom_depth_and_alpha() {
    let (device, queue) = gpu().expect("GPU for magnification validation");
    let backend = WgpuBackend::new(device.clone(), queue.clone());
    for aa in [
        aimer_cupid::AntiAlias::Analytic,
        aimer_cupid::AntiAlias::Msaa4x,
    ] {
        let mut renderer = Renderer::with_antialiasing(&backend, FORMAT, aa);
        pixels::centered_zoom_depth_and_alpha(|packet| {
            render_packet_and_read(&device, &queue, &backend, &mut renderer, packet)
        });
    }
}

#[test]
fn magnification_rounded_clip_foreground_and_fresh_capture() {
    let (device, queue) = gpu().expect("GPU for magnification validation");
    let backend = WgpuBackend::new(device.clone(), queue.clone());
    let mut renderer = Renderer::new(&backend, FORMAT);
    pixels::rounded_clip_foreground_and_fresh_capture(|packet| {
        render_packet_and_read(&device, &queue, &backend, &mut renderer, packet)
    });
}

#[test]
fn magnification_corner_refraction() {
    let (device, queue) = gpu().expect("GPU for magnification validation");
    let backend = WgpuBackend::new(device.clone(), queue.clone());
    let mut renderer = Renderer::new(&backend, FORMAT);
    pixels::rounded_corner_uses_device_space_normal(|packet| {
        render_packet_and_read(&device, &queue, &backend, &mut renderer, packet)
    });
}

#[test]
fn magnification_wide_action_bar() {
    let (device, queue) = gpu().expect("GPU for wide magnification validation");
    let backend = WgpuBackend::new(device.clone(), queue.clone());
    let mut renderer = Renderer::new(&backend, FORMAT);
    pixels::wide_action_bar_magnifies_backdrop(|packet| {
        render_packet_and_read(&device, &queue, &backend, &mut renderer, packet)
    });
}

#[test]
fn magnification_nested_filters_and_budget_fallback() {
    let (device, queue) = gpu().expect("GPU for magnification validation");
    let backend = WgpuBackend::new(device.clone(), queue.clone());
    let mut renderer = Renderer::new(&backend, FORMAT);
    pixels::nested_filters_and_budget_fallback(|packet| {
        render_packet_and_read(&device, &queue, &backend, &mut renderer, packet)
    });
}

#[test]
fn magnification_thick_rounded_corners_have_no_seam() {
    let (device, queue) = gpu().expect("GPU for rounded lens validation");
    let backend = WgpuBackend::new(device.clone(), queue.clone());
    for aa in [
        aimer_cupid::AntiAlias::Analytic,
        aimer_cupid::AntiAlias::Msaa4x,
    ] {
        let mut renderer = Renderer::with_antialiasing(&backend, FORMAT, aa);
        pixels::thick_rounded_corners_have_no_interior_normal_seam(|packet| {
            render_packet_and_read(&device, &queue, &backend, &mut renderer, packet)
        });
    }
}

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .block()
        .ok()?;
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("magnification regression device"),
            ..Default::default()
        })
        .block()
        .ok()
}

fn render_packet_and_read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    backend: &WgpuBackend,
    renderer: &mut Renderer,
    packet: &FramePacket,
) -> Vec<u8> {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("magnification regression target"),
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
    renderer.render_packet(backend, &view, packet, false);

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("magnification regression readback"),
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
        result.expect("magnification readback buffer to map");
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("magnification readback to finish");
    let pixels = slice
        .get_mapped_range()
        .expect("magnification readback range")
        .to_vec();
    readback.unmap();
    pixels
}
