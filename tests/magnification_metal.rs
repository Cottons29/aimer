#![cfg(all(feature = "native", not(feature = "wgpu"), target_os = "macos"))]
#[path = "backdrop_filter/fixtures.rs"]
mod fixtures;
#[path = "magnification/pixels.rs"]
mod pixels;
#[path = "magnification/styles.rs"]
mod styles;
use aimer_cupid::backend::metal::MetalBackend;
use aimer_cupid::backend::*;
use aimer_cupid::{frame::FramePacket, renderer::RendererImpl};
use fixtures::*;

#[test]
fn magnification_composable_styles() {
    let backend = MetalBackend::new().expect("Metal GPU for magnification style validation");
    let mut renderer =
        RendererImpl::<MetalBackend>::new(&backend, MetalBackend::rgba8_unorm_format());
    styles::styles_match_sampling_and_preserve_disabled_axes(|packet| {
        render_packet_and_read(&backend, &mut renderer, packet)
    });
}

#[test]
fn magnification_native_metal_centered_zoom_depth_and_alpha() {
    let backend = MetalBackend::new().expect("Metal GPU for magnification validation");
    let mut renderer =
        RendererImpl::<MetalBackend>::new(&backend, MetalBackend::rgba8_unorm_format());
    pixels::centered_zoom_depth_and_alpha(|packet| {
        render_packet_and_read(&backend, &mut renderer, packet)
    });
}

#[test]
fn magnification_native_metal_rounded_clip_foreground_and_fresh_capture() {
    let backend = MetalBackend::new().expect("Metal GPU for magnification validation");
    let mut renderer =
        RendererImpl::<MetalBackend>::new(&backend, MetalBackend::rgba8_unorm_format());
    pixels::rounded_clip_foreground_and_fresh_capture(|packet| {
        render_packet_and_read(&backend, &mut renderer, packet)
    });
}

#[test]
fn magnification_corner_refraction() {
    let backend = MetalBackend::new().expect("Metal GPU for magnification validation");
    let mut renderer =
        RendererImpl::<MetalBackend>::new(&backend, MetalBackend::rgba8_unorm_format());
    pixels::rounded_corner_uses_device_space_normal(|packet| {
        render_packet_and_read(&backend, &mut renderer, packet)
    });
}

#[test]
fn magnification_wide_action_bar() {
    let backend = MetalBackend::new().expect("Metal GPU for wide magnification validation");
    let mut renderer =
        RendererImpl::<MetalBackend>::new(&backend, MetalBackend::rgba8_unorm_format());
    pixels::wide_action_bar_magnifies_backdrop(|packet| {
        render_packet_and_read(&backend, &mut renderer, packet)
    });
}

#[test]
fn magnification_nested_filters_and_budget_fallback() {
    let backend = MetalBackend::new().expect("Metal GPU for magnification validation");
    let mut renderer =
        RendererImpl::<MetalBackend>::new(&backend, MetalBackend::rgba8_unorm_format());
    pixels::nested_filters_and_budget_fallback(|packet| {
        render_packet_and_read(&backend, &mut renderer, packet)
    });
}

#[test]
fn magnification_thick_rounded_corners_have_no_seam() {
    let backend = MetalBackend::new().expect("Metal GPU for rounded lens validation");
    let mut renderer =
        RendererImpl::<MetalBackend>::new(&backend, MetalBackend::rgba8_unorm_format());
    pixels::thick_rounded_corners_have_no_interior_normal_seam(|packet| {
        render_packet_and_read(&backend, &mut renderer, packet)
    });
}

fn render_packet_and_read(
    backend: &MetalBackend,
    renderer: &mut RendererImpl<MetalBackend>,
    packet: &FramePacket,
) -> Vec<u8> {
    let format = MetalBackend::rgba8_unorm_format();
    let target = backend.create_texture(&TextureDescriptor {
        label: Some("native magnification target".to_string()),
        size: (FRAME_WIDTH, FRAME_HEIGHT, 1),
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: vec![TextureUsage::RenderAttachment, TextureUsage::CopySrc],
    });
    let view = backend.create_texture_view(&target, "native magnification view");
    renderer.render_packet(backend, &view, packet, false);
    let bytes = u64::from(FRAME_WIDTH) * u64::from(FRAME_HEIGHT) * 4;
    let readback = backend.create_buffer(&BufferDescriptor {
        label: Some("native magnification readback".to_string()),
        size: bytes,
        usage: vec![BufferUsage::CopyDst, BufferUsage::MapRead],
    });
    let mut encoder = backend.create_command_encoder("native magnification readback");
    backend.copy_texture_to_buffer(
        &mut encoder,
        &TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        &TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(FRAME_WIDTH * 4),
                rows_per_image: Some(FRAME_HEIGHT),
            },
        },
        Extent3d {
            width: FRAME_WIDTH,
            height: FRAME_HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    backend.submit(encoder);
    backend.read_buffer(&readback, bytes)
}
