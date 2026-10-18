#![cfg(all(feature = "native", not(feature = "wgpu"), target_os = "macos"))]

#[path = "backdrop_filter/fixtures.rs"]
mod fixtures;
#[path = "backdrop_filter/wide.rs"]
mod wide;
use aimer_cupid::backend::metal::MetalBackend;
use aimer_cupid::backend::*;
use aimer_cupid::frame::FramePacket;
use aimer_cupid::renderer::RendererImpl;
use aimer_decoration::BackdropFilter;
use fixtures::*;

#[test]
fn backdrop_filter_native_metal_preserves_translucent_pixels() {
    let backend =
        MetalBackend::new().expect("a Metal device is required for native pixel validation");
    let format = MetalBackend::rgba8_unorm_format();
    let mut renderer = RendererImpl::<MetalBackend>::new(&backend, format);
    let (tree, node) = backdrop_tree();
    paint_background(&tree, [255, 0, 0, 128]);
    let packet = filter_packet(
        &tree,
        node,
        BackdropFilter::new().blur_radius(0.0).saturation(0.0),
    );
    let pixels = render_packet_and_read(&backend, &mut renderer, &packet);
    let gray = pixel(&pixels, FILTER_X + 32, 32);
    assert_eq!(gray[3], 128, "native capture must preserve alpha: {gray:?}");
    assert!(
        gray[..3].iter().all(|channel| channel.abs_diff(27) <= 2),
        "native grayscale filter: {gray:?}"
    );
}

#[test]
fn backdrop_filter_native_metal_blends_thin_features_into_a_smooth_gaussian() {
    let backend =
        MetalBackend::new().expect("a Metal device is required for native pixel validation");
    let mut renderer =
        RendererImpl::<MetalBackend>::new(&backend, MetalBackend::rgba8_unorm_format());
    let (tree, node) = backdrop_tree();
    for scale in [1.0, 2.0] {
        paint_thin_cross(&tree, scale);
        for radius in [0.5, 1.0, 12.0, 48.0, 96.0] {
            let packet = filter_packet_scaled(
                &tree,
                node,
                BackdropFilter::new().blur_radius(radius),
                scale,
            );
            let pixels = render_packet_and_read(&backend, &mut renderer, &packet);
            assert_gaussian_cross(&pixels, radius, scale);
        }
    }
}

#[test]
fn backdrop_filter_wide_ipad_toolbar() {
    let backend = MetalBackend::new().expect("Metal GPU for iPad-width backdrop validation");
    for aa in [
        aimer_cupid::AntiAlias::Analytic,
        aimer_cupid::AntiAlias::Msaa4x,
    ] {
        let mut renderer = RendererImpl::<MetalBackend>::with_antialiasing(
            &backend,
            MetalBackend::rgba8_unorm_format(),
            aa,
        );
        wide::wide_toolbar_blurs_and_refreshes(|packet| {
            render_packet_and_read_sized(&backend, &mut renderer, packet, wide::WIDTH, wide::HEIGHT)
        });
    }
}

fn render_packet_and_read(
    backend: &MetalBackend,
    renderer: &mut RendererImpl<MetalBackend>,
    packet: &FramePacket,
) -> Vec<u8> {
    render_packet_and_read_sized(backend, renderer, packet, FRAME_WIDTH, FRAME_HEIGHT)
}

fn render_packet_and_read_sized(
    backend: &MetalBackend,
    renderer: &mut RendererImpl<MetalBackend>,
    packet: &FramePacket,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let format = MetalBackend::rgba8_unorm_format();
    let target = backend.create_texture(&TextureDescriptor {
        label: Some("native backdrop filter target".to_string()),
        size: (width, height, 1),
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format,
        usage: vec![TextureUsage::RenderAttachment, TextureUsage::CopySrc],
    });
    let view = backend.create_texture_view(&target, "native backdrop filter view");
    renderer.render_packet(backend, &view, packet, false);
    let bytes = u64::from(width) * u64::from(height) * 4;
    let readback = backend.create_buffer(&BufferDescriptor {
        label: Some("native backdrop filter readback".to_string()),
        size: bytes,
        usage: vec![BufferUsage::CopyDst, BufferUsage::MapRead],
    });
    let mut encoder = backend.create_command_encoder("native backdrop filter readback");
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
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
        },
        Extent3d {
            width: width,
            height: height,
            depth_or_array_layers: 1,
        },
    );
    backend.submit(encoder);
    backend.read_buffer(&readback, bytes)
}
