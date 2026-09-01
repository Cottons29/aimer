pub mod custom_pipeline;
pub mod draw_cmd;
pub mod font;
pub mod frame;
pub mod gpu_context;
pub mod utilities;

pub mod canvas;
mod lru_map;
mod pipeline;
pub mod pipeline_cache;
pub mod renderer;
pub mod svg;
#[cfg(target_arch = "wasm32")]
pub mod wasm_fonts;

pub use pipeline::{AntiAlias, image_pipeline, rect_pipeline, svg_pipeline, text_pipeline};

pub use crate::text_pipeline::{glyph_atlas, glyph_rasterizer, text_layout};

/// Hidden cargo-fuzz entry point for the checked font reader.
#[doc(hidden)]
pub fn fuzz_aimer_font_directory(data: &[u8]) {
    crate::pipeline::text_pipeline::aimer_font::fuzz_directory(data);
}

/// Hidden cargo-fuzz entry point for glyph and outline decoding.
#[doc(hidden)]
pub fn fuzz_aimer_font_outlines(data: &[u8]) {
    crate::pipeline::text_pipeline::aimer_font::fuzz_outlines(data);
}

#[cfg(test)]
mod deferred_frame_uploads {
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

    use std::sync::Arc;

    use crate::draw_cmd::{DrawList, RetainedLayerContent};
    use crate::renderer::Renderer;
    use crate::utilities::{Color, Rect};
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

    /// Renders draw into an offscreen target and returns the RGBA8 pixels.
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

    /// Asserts the pixel is saturated in exactly the dominant RGB channel.
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

    /// A frame whose rect stream is split by an image draw, forcing multiple
    /// rect flushes and an interleaved image batch inside the same pass: a red
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
    fn retained_layer_is_rasterized_and_then_composited_at_a_new_offset() {
        let Some((device, queue)) = gpu() else {
            eprintln!("skipping: no GPU adapter available");
            return;
        };
        let mut renderer = Renderer::new(&device, FORMAT);

        let mut recorded = DrawList::new();
        recorded.fill_rect(
            Rect::new(0.0, 0.0, 16.0, 16.0),
            Color::red(),
            [0.0; 4],
            [0.0; 4],
            Color::transparent(),
        );
        let content = Arc::new(RetainedLayerContent::from_snapshot(
            recorded
                .retained_snapshot()
                .expect("the layer content should be retainable"),
        ));

        let mut first_frame = DrawList::new();
        first_frame.draw_retained_layer(7, Rect::new(4.0, 4.0, 16.0, 16.0), content.clone());
        let first = render_and_read(&device, &queue, &mut renderer, &first_frame);
        assert_dominant(&first, 8, 8, 0, "first retained layer position");
        assert_eq!(renderer.memory_stats().retained_layer_count, 1);

        let mut second_frame = DrawList::new();
        second_frame.draw_retained_layer(7, Rect::new(12.0, 12.0, 16.0, 16.0), content);
        let second = render_and_read(&device, &queue, &mut renderer, &second_frame);
        assert_dominant(&second, 16, 16, 0, "composited retained layer position");
        assert_eq!(renderer.memory_stats().retained_layer_count, 1);
        assert_eq!(pixel(&second, 8, 8)[3], 0, "the layer should have moved");
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
}

#[cfg(test)]
mod resized_text_preparation {
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

    use crate::AntiAlias;
    use crate::font::{FontFamily, FontStyle};
    use crate::text_layout::TextHorizontalAlign;
    use crate::text_pipeline::{TextDrawRequest, TextOverflowMode, TextPipelineV2};
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
            shadow: None,
            draw_glyphs: true,
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

    /// Prepares requests at a fixed size until nothing is postponed.
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
}

#[cfg(test)]
mod scrolled_text_culling {
    //! Regression guard for request-level text culling.
    //!
    //! A scroll viewport hands its child more text than the screen can show, so a
    //! text-heavy document reaches TextPipelineV2::prepare with most of its
    //! requests off screen. Those requests must not cost the frame anything
    //! per-glyph: no instances built, no bytes uploaded, no atlas capacity
    //! reserved. The visibility rule is the one the pipeline already trusts for
    //! postponing preparation — a request whose bounds meet neither the surface
    //! nor its clip cannot show a pixel.
    //!
    //! Runs against the first available adapter; skips (with a note) on machines
    //! without one.

    use std::sync::Arc;

    use crate::AntiAlias;
    use crate::font::{FontFamily, FontStyle};
    use crate::text_layout::TextHorizontalAlign;
    use crate::text_pipeline::{TextDrawRequest, TextOverflowMode, TextPipelineV2};
    use aimer_utils::SyncFuture;

    const WIDTH: u32 = 400;
    const HEIGHT: u32 = 300;
    const LINE_HEIGHT: f32 = 24.0;
    const FONT_SIZE: f32 = 16.0;

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

    /// One line of a document, index line-heights below the document origin,
    /// shifted up by scroll_offset — unclipped, the way content that relies on
    /// the surface edge alone arrives.
    fn line(index: usize, scroll_offset: f32) -> TextDrawRequest {
        TextDrawRequest {
            x: 8.0,
            y: index as f32 * LINE_HEIGHT - scroll_offset,
            text: Arc::from(format!("line {index} with some scrolling text").as_str()),
            font_size: FONT_SIZE,
            color: [0.0, 0.0, 0.0, 1.0],
            bounds_width: WIDTH as f32 - 16.0,
            bounds_height: LINE_HEIGHT,
            overflow: TextOverflowMode::Clip,
            horizontal_align: TextHorizontalAlign::Left,
            line_height: None,
            shadow: None,
            draw_glyphs: true,
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

    /// Prepares requests until nothing is postponed, so every layout — visible
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
        //
        // The pipeline also keeps a request whose glyphs overhang its declared
        // box — an ascender or italic left-bearing reaches up to a font size past
        // the origin — so the head the screen actually shows runs a font size
        // deeper than the raw line count. FONT_SIZE is that overhang padding.
        let visible_lines = ((HEIGHT as f32 + FONT_SIZE) / LINE_HEIGHT).ceil() as usize;
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
        let scrolled: Vec<_> = (0..line_count)
            .map(|i| line(i, bottom_offset))
            .collect();
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
}
