//! Measures completed GPU frames for Jaime-sized backdrop decorations in debug builds.
//! Run with `cargo run --features wgpu --example backdrop_filter_benchmark --offline`.
//! Add `-- --wide` to measure a full-width RustForum action bar instead.
//! Use `-- --ipad` to include blur on a 2,732-pixel-wide Retina frame.

#[cfg(not(feature = "wgpu"))]
fn main() {
    eprintln!("Enable the wgpu feature to run this benchmark.");
}

#[cfg(feature = "wgpu")]
fn main() {
    use aimer::canvas::Canvas;
    use aimer::cupid::damage_region::DamageSet;
    use aimer::cupid::draw_cmd_v2::{Rect, RenderFrame, RenderPaintSource, RenderTree};
    use aimer::cupid::frame::{FramePacket, FrameRenderMetadata};
    use aimer::cupid::{WgpuBackend, renderer::Renderer};
    use aimer::{
        BackdropFilter, Decoration, Magnification, MagnificationStyle as S, ResolvedSize, Vec2d,
    };
    use aimer_utils::SyncFuture;
    use std::time::Instant;

    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&Default::default())
        .block()
        .expect("GPU adapter");
    let (device, queue) = adapter
        .request_device(&Default::default())
        .block()
        .expect("GPU device");
    let backend = WgpuBackend::new(device.clone(), queue);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut renderer = Renderer::new(&backend, format);
    let ipad = std::env::args().any(|arg| arg == "--ipad");
    let wide = ipad || std::env::args().any(|arg| arg == "--wide");
    let (frame_width, frame_height, bounds) = if ipad {
        (1366.0, 1024.0, Rect::new(250.0, 0.0, 1116.0, 60.0))
    } else if wide {
        (1_240.0, 780.0, Rect::new(248.0, 0.0, 992.0, 60.0))
    } else {
        (640.0, 400.0, Rect::new(160.0, 92.0, 320.0, 208.0))
    };
    let tree = RenderTree::new();
    let root = tree
        .add_root(Rect::new(0.0, 0.0, frame_width, frame_height))
        .unwrap();
    tree.set_paint_source(root, RenderPaintSource::LocalV2)
        .unwrap();
    let background = Canvas::of(&tree.context(root).unwrap());
    background.fill_rect(
        Rect::new(0.0, 0.0, frame_width, frame_height),
        [218, 237, 247, 255],
    );
    for row in 0..7 {
        for column in 0..((frame_width - 24.0) / 28.0) as u32 {
            background.fill_rect(
                Rect::new(
                    24.0 + column as f32 * 28.0,
                    24.0 + row as f32 * 52.0,
                    6.0,
                    28.0,
                ),
                [28, 81, 126, 255],
            );
        }
    }
    background.finish();
    let node = tree.add_child(root, bounds).unwrap();
    tree.set_paint_source(node, RenderPaintSource::LocalV2)
        .unwrap();
    println!(
        "{}×{} decoration; debug renderer; 50 completed GPU frames after 8 warmups",
        bounds.width, bounds.height
    );
    println!("effect,scale,radius_or_thickness,cpu_submit_ms,completed_frame_ms");
    for scale in [1.0, 2.0] {
        let (width, height) = ((frame_width * scale) as u32, (frame_height * scale) as u32);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("backdrop blur benchmark target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        for (effect, radius, style) in [
            ("blur", 0.0, S::NONE),
            ("blur", 4.0, S::NONE),
            ("blur", 12.0, S::NONE),
            ("blur", 48.0, S::NONE),
            ("blur", 96.0, S::NONE),
            ("lens", 0.0, S::DEFAULT),
            ("lens", 16.0, S::DEFAULT),
            ("lens", 64.0, S::DEFAULT),
            (
                "horizontal-left",
                16.0,
                S::UNIFORM | S::HORIZONTAL | S::LEFT,
            ),
            ("combined", 16.0, S::DEFAULT | S::BULGE),
        ] {
            if wide && !ipad && effect == "blur" {
                continue;
            }
            let canvas = Canvas::of(&tree.context(node).unwrap());
            let size = ResolvedSize {
                width: bounds.width,
                height: bounds.height,
            };
            if effect != "blur" {
                Magnification::new()
                    .style(style)
                    .bulge(0.6)
                    .zoom(if wide { 1.6 } else { 2.0 })
                    .thickness(radius)
                    .paint(&canvas, Vec2d::default(), size, 1.0);
            } else {
                BackdropFilter::new()
                    .blur_radius(radius)
                    .brightness(0.99)
                    .paint(&canvas, Vec2d::default(), size, 1.0);
            }
            canvas.finish();
            let packet = FramePacket::from_v2_direct(
                RenderFrame {
                    damage: vec![Rect::new(0.0, 0.0, width as f32, height as f32)],
                    operations: tree.render_all(),
                },
                FrameRenderMetadata::new(scale, 0, 0, 0, 0, DamageSet::new(width, height)),
            )
            .unwrap();
            let mut cpu = 0.0;
            let mut total = 0.0;
            for frame in 0..58 {
                let start = Instant::now();
                renderer.render_packet(&backend, &view, &packet, false);
                let submitted = start.elapsed().as_secs_f64();
                device
                    .poll(wgpu::PollType::wait_indefinitely())
                    .expect("completed GPU frame");
                if frame >= 8 {
                    cpu += submitted;
                    total += start.elapsed().as_secs_f64();
                }
            }
            println!(
                "{effect},{scale},{radius},{:.3},{:.3}",
                cpu * 1_000.0 / 50.0,
                total * 1_000.0 / 50.0
            );
        }
    }
}
