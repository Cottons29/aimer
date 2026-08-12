//! Pixel-level regression guard for Cupid's per-frame instance uploads.
//!
//! The renderer interleaves rect, image and text batches inside one render
//! pass, each batch drawing from its own region of a shared instance buffer,
//! and the frame's data reaches the GPU in a single deferred write per
//! pipeline that is skipped entirely when the bytes did not change. These
//! tests pin down the observable contract of that scheme:
//!
//! * batches split by z-order interleaving must not alias or vanish;
//! * a frame identical to the previous one must render the same pixels even
//!   though its uploads were skipped;
//! * a frame that *does* change must not be poisoned by the skip cache.
//!
//! Runs against the first available adapter; skips (with a note) on machines
//! without one.

use aimer_cupid::draw_cmd::DrawList;
use aimer_cupid::renderer::Renderer;
use aimer_cupid::utilities::{Color, Rect};
use aimer_utils::SyncFuture;

const SIZE: u32 = 64;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .block()
        .ok()?;
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("cupid upload regression device"),
            ..Default::default()
        })
        .block()
        .ok()
}

/// Renders `draw` into an offscreen target and returns the RGBA8 pixels.
fn render_and_read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    draw: &DrawList,
) -> Vec<u8> {
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("upload regression target"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
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

    renderer.render(device, queue, &view, SIZE, SIZE, false, draw);

    // SIZE * 4 = 256 bytes per row, which happens to satisfy wgpu's row
    // alignment requirement for texture-to-buffer copies.
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("upload regression readback"),
        size: (SIZE * SIZE * 4) as u64,
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
                bytes_per_row: Some(SIZE * 4),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| {
        result.expect("the readback buffer to map");
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("the device to finish the readback");
    let pixels = slice
        .get_mapped_range()
        .expect("the mapped readback range")
        .to_vec();
    readback.unmap();
    pixels
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * SIZE + x) * 4) as usize;
    pixels[offset..offset + 4].try_into().expect("4 channels")
}

/// Asserts the pixel is saturated in exactly the `dominant` RGB channel.
fn assert_dominant(pixels: &[u8], x: u32, y: u32, dominant: usize, what: &str) {
    let sample = pixel(pixels, x, y);
    for channel in 0..3 {
        if channel == dominant {
            assert!(
                sample[channel] > 200,
                "{what}: expected channel {channel} saturated at ({x}, {y}), got {sample:?}"
            );
        } else {
            assert!(
                sample[channel] < 50,
                "{what}: expected channel {channel} dark at ({x}, {y}), got {sample:?}"
            );
        }
    }
    assert!(sample[3] > 200, "{what}: expected opaque at ({x}, {y})");
}

/// A frame whose rect stream is split by an image draw, forcing multiple rect
/// flushes and an interleaved image batch inside the same pass: a red
/// background, a blue 1×1 image stretched over the middle, and a green square
/// on top of everything.
fn interleaved_frame(background: Color, square: Color) -> DrawList {
    let mut draw = DrawList::new();
    draw.fill_rect(
        Rect::new(0.0, 0.0, SIZE as f32, SIZE as f32),
        background,
        [0.0; 4],
        [0.0; 4],
        Color::transparent(),
    );
    let texture_id = draw.load_image(&[0, 0, 255, 255], 1, 1);
    draw.draw_image(Rect::new(16.0, 16.0, 16.0, 16.0), texture_id);
    draw.fill_rect(
        Rect::new(40.0, 40.0, 8.0, 8.0),
        square,
        [0.0; 4],
        [0.0; 4],
        Color::transparent(),
    );
    draw
}

fn assert_interleaved_pixels(pixels: &[u8], what: &str) {
    // Background rect (first batch).
    assert_dominant(pixels, 2, 2, 0, what);
    // Image drawn between the two rect batches.
    assert_dominant(pixels, 20, 20, 2, what);
    // Rect batch after the image — the region that would vanish if a later
    // upload overwrote an earlier batch, or alias if offsets were wrong.
    assert_dominant(pixels, 44, 44, 1, what);
}

#[test]
fn interleaved_batches_render_identically_across_static_frames() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, FORMAT);

    // Frame 1: every buffer is fresh, everything uploads.
    let frame = interleaved_frame(Color::red(), Color::green());
    let first = render_and_read(&device, &queue, &mut renderer, &frame);
    assert_interleaved_pixels(&first, "first frame");

    // Frame 2: byte-identical draw list — uploads may be skipped, pixels must
    // not change.
    let frame = interleaved_frame(Color::red(), Color::green());
    let second = render_and_read(&device, &queue, &mut renderer, &frame);
    assert_interleaved_pixels(&second, "identical second frame");
    assert_eq!(first, second, "a static frame must be pixel-stable");
}

#[test]
fn a_changed_frame_is_not_poisoned_by_the_skip_cache() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, FORMAT);

    let frame = interleaved_frame(Color::red(), Color::green());
    let first = render_and_read(&device, &queue, &mut renderer, &frame);
    assert_interleaved_pixels(&first, "baseline frame");

    // Same shape, swapped colors: the changed bytes must reach the GPU.
    let frame = interleaved_frame(Color::green(), Color::red());
    let pixels = render_and_read(&device, &queue, &mut renderer, &frame);
    assert_dominant(&pixels, 2, 2, 1, "swapped background");
    assert_dominant(&pixels, 20, 20, 2, "image after swap");
    assert_dominant(&pixels, 44, 44, 0, "swapped square");
}

#[test]
fn an_empty_frame_between_static_frames_keeps_the_scene_intact() {
    let Some((device, queue)) = gpu() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut renderer = Renderer::new(&device, FORMAT);

    let frame = interleaved_frame(Color::red(), Color::green());
    render_and_read(&device, &queue, &mut renderer, &frame);

    // An empty frame draws nothing and must not disturb the retained buffers.
    let empty = DrawList::new();
    let blank = render_and_read(&device, &queue, &mut renderer, &empty);
    assert_eq!(pixel(&blank, 2, 2)[3], 0, "an empty frame clears the target");

    let frame = interleaved_frame(Color::red(), Color::green());
    let third = render_and_read(&device, &queue, &mut renderer, &frame);
    assert_interleaved_pixels(&third, "frame after an empty frame");
}
