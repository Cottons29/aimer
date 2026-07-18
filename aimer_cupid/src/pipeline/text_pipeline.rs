mod font_resolver;
pub mod glyph_atlas;
mod glyph_outline;
pub mod glyph_rasterizer;
pub mod text_layout;

use std::collections::HashMap;
use std::sync::Arc;

use crate::font::{FontFamily, FontStyle, FontWeight};
use aimer_utils::time_cost;
use bytemuck::{Pod, Zeroable};

use crate::pipeline::image_pipeline::InstanceBufferPolicy;
use crate::text_pipeline::glyph_atlas::{AtlasRegion, ColorGlyphAtlas, GlyphAtlas};
use crate::text_pipeline::glyph_rasterizer::{GlyphKey, GlyphRasterizer};
use crate::text_pipeline::text_layout::{
    PositionedGlyph, ShapedText, layout_shaped_text, shape_text_styled,
};

/// Per-instance data for one glyph quad.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct GlyphInstance {
    position: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    color: [f32; 4],
    /// Clip rect: [x, y, width, height]. If width <= 0, no clip is applied.
    clip_rect: [f32; 4],
    /// Border radius for the clip rect: [top-left, top-right, bottom-right,
    /// bottom-left].
    clip_border_radius: [f32; 4],
    /// Horizontal shear factor for synthetic italic (tan of the slant angle).
    /// 0 = upright. The glyph shaders slant the quad by this, pinned at its
    /// bottom edge, so the advance/layout is unchanged.
    skew: f32,
    /// Padding to keep the struct 8-byte aligned for `Pod`/vertex upload.
    _pad: [f32; 3],
}

impl GlyphInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<GlyphInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Per-instance data for one decoration line quad (underline/overline/strike).
/// The line geometry is a plain quad; the actual stroke (and its dotted/dashed/
/// wavy shape) is produced procedurally by `text_decoration.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct DecorationInstance {
    /// Top-left of the band quad, screen space.
    position: [f32; 2],
    /// Band size: [width, band_height].
    size: [f32; 2],
    color: [f32; 4],
    clip_rect: [f32; 4],
    clip_border_radius: [f32; 4],
    /// [style_id, thickness_px, period_px, band_height_px].
    params: [f32; 4],
}

impl DecorationInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<DecorationInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// A single styled decoration line to render, in final screen-space geometry.
/// The producer (widget/renderer) computes where the line sits from the text
/// metrics; the engine only rasterizes the styled stroke inside the band.
#[derive(Clone, Copy, Debug)]
pub struct TextDecorationDraw {
    /// Top-left of the band quad.
    pub x: f32,
    pub y: f32,
    /// Band width (line length).
    pub width: f32,
    /// Band height — tall enough to hold the stroke plus wave/double spacing.
    pub band_height: f32,
    /// Stroke thickness in pixels.
    pub thickness: f32,
    /// Repeat period for dotted/dashed/wavy styles (pixels).
    pub period: f32,
    /// Style id, matching `aimer_style::TextDecorationStyle::id`.
    pub style: u32,
    pub color: [f32; 4],
    pub clip_rect: [f32; 4],
    pub clip_border_radius: [f32; 4],
}

impl TextDecorationDraw {
    fn to_instance(self) -> DecorationInstance {
        DecorationInstance {
            position: [self.x, self.y],
            size: [self.width, self.band_height],
            color: self.color,
            clip_rect: self.clip_rect,
            clip_border_radius: self.clip_border_radius,
            params: [self.style as f32, self.thickness, self.period, self.band_height],
        }
    }
}

pub struct TextDrawRequest {
    pub x: f32,
    pub y: f32,
    // Reference-counted so cloning the request per frame (and from the draw
    // list) is a cheap refcount bump rather than a fresh string allocation.
    pub text: Arc<str>,
    pub font_size: f32,
    pub color: [f32; 4],
    pub bounds_width: f32,
    pub bounds_height: f32,
    pub overflow: TextOverflowMode,
    pub line_height: Option<f32>,
    pub font_family: FontFamily,
    pub font_style: FontStyle,
    pub font_weight: Option<u16>,
    pub italic: bool,
    pub clip_rect: [f32; 4],
    pub clip_border_radius: [f32; 4],
    pub spans: Vec<RichTextSpan>,
}

#[derive(Clone, Debug)]
pub struct RichTextSpan {
    pub text: Arc<str>,
    pub font_size: Option<f32>,
    pub color: Option<[f32; 4]>,
    pub font_weight: Option<u16>,
    pub italic: Option<bool>,
}

impl RichTextSpan {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self { text: text.into(), font_size: None, color: None, font_weight: None, italic: None }
    }

    pub fn with_style(mut self, font_size: Option<f32>, color: Option<[f32; 4]>) -> Self {
        self.font_size = font_size;
        self.color = color;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextOverflowMode {
    #[default]
    Clip,
    Wrap,
    Ellipsis,
}

/// Key used to memoize the output of `layout_text` across frames.
/// Uses integer bit-representations of f32 values to implement Hash + Eq.
#[derive(Debug, Hash, Eq, PartialEq, Clone)]
struct LayoutCacheKey {
    text: String,
    /// `font_size` × 100, rounded, stored as u32 to make it hashable.
    font_size_u32: u32,
    /// `bounds_width` × 100, rounded, stored as u32.
    bounds_width_u32: u32,
    font_family: FontFamily,
    font_style: FontStyle,
    font_weight: u16,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
struct ShapingCacheKey {
    text: String,
    /// `font_size` × 100, rounded, stored as u32 to make it hashable.
    font_size_u32: u32,
    font_family: FontFamily,
    font_style: FontStyle,
    font_weight: u16,
}

impl ShapingCacheKey {
    fn new(
        text: &str,
        font_size: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) -> Self {
        Self {
            text: text.to_owned(),
            font_size_u32: (font_size * 100.0).round() as u32,
            font_family,
            font_style,
            font_weight,
        }
    }
}

impl LayoutCacheKey {
    fn new(
        text: &str,
        font_size: f32,
        bounds_width: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) -> Self {
        Self {
            text: text.to_owned(),
            font_size_u32: (font_size * 100.0).round() as u32,
            bounds_width_u32: (bounds_width * 100.0).round() as u32,
            font_family,
            font_style,
            font_weight,
        }
    }
}

/// Glyph-instance ranges owned by a single text request. `[alpha_start,
/// alpha_end)` indexes `instances` and `[color_start, color_end)` indexes
/// `color_instances`. `prepare` fills both lists in request order, so each
/// request owns a contiguous slice of each and can be drawn on its own at the
/// right z-position in the draw stream.
#[derive(Clone, Copy, Default)]
struct TextRequestRange {
    alpha_start: u32,
    alpha_end: u32,
    color_start: u32,
    color_end: u32,
}

pub struct TextPipelineV2 {
    rasterizer: GlyphRasterizer,
    /// Alpha-coverage atlas (R8Unorm) for monochrome glyphs.
    atlas: GlyphAtlas,
    /// RGBA8 atlas for sbix color emoji bitmaps (Apple Color Emoji et al.).
    color_atlas: ColorGlyphAtlas,
    pipeline: wgpu::RenderPipeline,
    /// Pipeline that samples the RGBA color atlas instead of the alpha atlas.
    color_pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    /// Shared layout used by both the alpha and color bind groups (the binding
    /// shape — uniform + texture_2d<f32> + sampler — is identical).
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    color_bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    instance_buffer: wgpu::Buffer,
    instance_policy: InstanceBufferPolicy,
    instances: Vec<GlyphInstance>,
    /// Sibling buffer + scratch list for color glyph quads (drawn in a second
    /// pass after the alpha glyphs so they layer on top of the same line).
    color_instance_buffer: wgpu::Buffer,
    color_instance_policy: InstanceBufferPolicy,
    color_instances: Vec<GlyphInstance>,
    /// Decoration-line pipeline + its own instance list/buffer. Decoration
    /// quads are drawn after the glyphs (see `render`) so lines layer with
    /// their text.
    decoration_pipeline: wgpu::RenderPipeline,
    decoration_instance_buffer: wgpu::Buffer,
    decoration_instance_policy: InstanceBufferPolicy,
    decoration_instances: Vec<DecorationInstance>,
    /// Track atlas generation to only rebuild bind group when atlas texture
    /// changes.
    atlas_generation: u64,
    color_atlas_generation: u64,
    /// Cached viewport dimensions to skip redundant uniform writes.
    last_viewport: (u32, u32),
    /// Layout cache: maps a stable key derived from text content + render
    /// parameters to the pre-computed `Vec<PositionedGlyph>`.  Entries are
    /// cleared whenever the set of requests changes (different number of
    /// draw calls) so memory does not grow unboundedly across scene changes.
    layout_cache: HashMap<LayoutCacheKey, Vec<PositionedGlyph>>,
    /// Width-independent shaping cache.  Resize may invalidate final positions
    /// for wrapping/ellipsis text, but shaped glyph ids and advances only
    /// depend on text content and font size.
    shaping_cache: HashMap<ShapingCacheKey, ShapedText>,
    /// Per-request glyph ranges recorded during `prepare` so the renderer can
    /// draw a single text request at its own z-position (interleaved with
    /// rects/images) instead of drawing all text in one final pass — the
    /// latter made text ignore z-order (e.g. a `Stack`'s upper layer could not
    /// cover text belonging to a lower layer).
    request_ranges: Vec<TextRequestRange>,
}

impl TextPipelineV2 {
    const INITIAL_CAPACITY: usize = 512;
    /// Absolute upper bound on the number of cached positioned-glyph layouts.
    /// The caches are kept persistent across frames/screens (see `prepare`) and
    /// only flushed when this hard cap is exceeded, so shaping/layout work is
    /// reused instead of being thrown away on every screen transition.
    const LAYOUT_CACHE_CAPACITY: usize = 4096;
    /// Absolute upper bound on the number of cached shaped strings. Shaped
    /// results are width-independent and tiny, so this can be generous.
    const SHAPING_CACHE_CAPACITY: usize = 4096;

    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        pipeline_cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let rasterizer = GlyphRasterizer::new();
        let atlas = GlyphAtlas::new(device);
        let color_atlas = ColorGlyphAtlas::new(device);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./shaders/text.wgsl").into()),
        });
        let color_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text color shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./shaders/text_color.wgsl").into()),
        });
        let decoration_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text decoration shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./shaders/text_decoration.wgsl").into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("text atlas sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text viewport uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = Self::create_bind_group(
            device,
            &bind_group_layout,
            &viewport_buffer,
            &atlas.view,
            &sampler,
        );
        let color_bind_group = Self::create_bind_group(
            device,
            &bind_group_layout,
            &viewport_buffer,
            &color_atlas.view,
            &sampler,
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text pipeline v2"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(GlyphInstance::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: crate::pipeline::multisample_state(),
            multiview_mask: None,
            cache: pipeline_cache,
        });

        let color_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text color pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &color_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(GlyphInstance::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &color_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: crate::pipeline::multisample_state(),
            multiview_mask: None,
            cache: pipeline_cache,
        });

        let decoration_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text decoration pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &decoration_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(DecorationInstance::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &decoration_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: crate::pipeline::multisample_state(),
            multiview_mask: None,
            cache: pipeline_cache,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text instance buffer"),
            size: (Self::INITIAL_CAPACITY * size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let color_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text color instance buffer"),
            size: (Self::INITIAL_CAPACITY * size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let decoration_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text decoration instance buffer"),
            size: (Self::INITIAL_CAPACITY * size_of::<DecorationInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            rasterizer,
            atlas,
            color_atlas,
            pipeline,
            color_pipeline,
            viewport_buffer,
            bind_group_layout,
            bind_group,
            color_bind_group,
            sampler,
            instance_buffer,
            instance_policy: InstanceBufferPolicy::new(Self::INITIAL_CAPACITY),
            instances: Vec::new(),
            color_instance_buffer,
            color_instance_policy: InstanceBufferPolicy::new(Self::INITIAL_CAPACITY),
            color_instances: Vec::new(),
            decoration_pipeline,
            decoration_instance_buffer,
            decoration_instance_policy: InstanceBufferPolicy::new(Self::INITIAL_CAPACITY),
            decoration_instances: Vec::new(),
            atlas_generation: 0,
            color_atlas_generation: 0,
            last_viewport: (0, 0),
            layout_cache: HashMap::new(),
            shaping_cache: HashMap::new(),
            request_ranges: Vec::new(),
        }
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        atlas_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: viewport_buffer.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    pub fn preload_text(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        font_size: f32,
    ) {
        for (key, glyph) in self
            .rasterizer
            .preload_text(text, font_size)
        {
            self.insert_rasterized_glyph(
                device,
                queue,
                key,
                glyph.is_color,
                glyph.width,
                glyph.height,
                &glyph.bitmap,
            );
            self.rasterizer.release_bitmap(key);
        }

        self.flush_atlas(device, queue);
    }

    /// Common glyph set warmed by [`warm_glyph_set`](Self::warm_glyph_set):
    /// the space, digits, lowercase and uppercase ASCII letters, and the
    /// printable ASCII punctuation. Rasterizing this set fills the glyph atlas
    /// (the heavier of the two per-glyph costs) so even brand-new, never-seen
    /// strings only pay `rustybuzz` shaping and never glyph rasterization.
    const COMMON_GLYPH_SET: &'static str = " 0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

    /// Insert a single rasterized glyph bitmap into the matching atlas,
    /// skipping empty (zero-area) glyphs and glyphs already present.
    #[allow(clippy::too_many_arguments)]
    fn insert_rasterized_glyph(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: GlyphKey,
        is_color: bool,
        width: u32,
        height: u32,
        bitmap: &[u8],
    ) {
        if width == 0 || height == 0 {
            return;
        }
        if is_color {
            if self
                .color_atlas
                .get(&key)
                .is_none()
            {
                self.color_atlas
                    .get_or_insert(device, queue, key, width, height, bitmap);
            }
        } else if self.atlas.get(&key).is_none() {
            self.atlas
                .get_or_insert(device, queue, key, width, height, bitmap);
        }
    }

    /// Upload any pending atlas changes to the GPU and rebuild the bind groups
    /// if either atlas texture was reallocated (generation changed). Shared by
    /// the warm-up paths and `preload_text`.
    fn flush_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.atlas.upload(queue);
        self.color_atlas.upload(queue);

        let atlas_gen = self.atlas.generation();
        if atlas_gen != self.atlas_generation {
            self.atlas_generation = atlas_gen;
            self.bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.viewport_buffer,
                &self.atlas.view,
                &self.sampler,
            );
        }

        let color_gen = self.color_atlas.generation();
        if color_gen != self.color_atlas_generation {
            self.color_atlas_generation = color_gen;
            self.color_bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.viewport_buffer,
                &self.color_atlas.view,
                &self.sampler,
            );
        }
    }

    /// Level 2 warm-up — pre-rasterize the common ASCII glyph set at each of
    /// the supplied font sizes so the glyph atlas is already populated
    /// before the first frame is drawn. Because rasterization (not shaping)
    /// is the heavier per-glyph cost, this keeps even brand-new strings
    /// (numbers, usernames, live text) cheap: they only pay shaping, never
    /// glyph rasterization.
    pub fn warm_glyph_set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        font_sizes: &[f32],
    ) {
        for &font_size in font_sizes {
            for (key, glyph) in self
                .rasterizer
                .preload_text(Self::COMMON_GLYPH_SET, font_size)
            {
                self.insert_rasterized_glyph(
                    device,
                    queue,
                    key,
                    glyph.is_color,
                    glyph.width,
                    glyph.height,
                    &glyph.bitmap,
                );
                self.rasterizer.release_bitmap(key);
            }
        }

        self.flush_atlas(device, queue);
    }

    /// Level 1 warm-up — pre-shape and lay out a known static string at the
    /// given font size, populating the shaping cache, the layout cache, and the
    /// glyph atlas. After this, the string renders on the ~1 ms cache-hit path
    /// from the very first frame instead of paying the cold `rustybuzz`
    /// shaping + rasterization cost (the 27–86 ms spikes) on first paint.
    ///
    /// `layout_width` must match the wrapping width the string will be drawn
    /// with (0.0 for non-wrapping `Clip` text) for the layout cache to hit;
    /// even if it differs the width-independent shaping cache still hits,
    /// so the expensive shaping work is warmed regardless.
    pub fn warm_text(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        font_size: f32,
        layout_width: f32,
    ) {
        self.warm_layout(device, queue, text, font_size, layout_width);
        self.flush_atlas(device, queue);
    }

    /// Shared core of [`warm_text`](Self::warm_text): shape + lay out `text`,
    /// populating both caches exactly like `prepare` does, then rasterize every
    /// positioned glyph into the atlas. Does not upload/flush the atlas
    /// (callers batch a single `flush_atlas` afterwards).
    fn warm_layout(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        font_size: f32,
        layout_width: f32,
    ) {
        let cache_key = LayoutCacheKey::new(
            text,
            font_size,
            layout_width,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
        );

        // Populate (or reuse) the layout/shaping caches, mirroring the hot path
        // in `prepare`, then snapshot the glyph keys so the cache borrow ends
        // before we touch the rasterizer/atlas again.
        let glyphs: Vec<(GlyphKey, f32)> = {
            let shaping_cache = &mut self.shaping_cache;
            let rasterizer = &mut self.rasterizer;
            let positioned = self
                .layout_cache
                .entry(cache_key)
                .or_insert_with(|| {
                    let shaped_key = ShapingCacheKey::new(
                        text,
                        font_size,
                        FontFamily::SANS_SERIF,
                        FontStyle::Normal,
                        FontWeight::Normal.numeric(),
                    );
                    let shaped_text = shaping_cache
                        .entry(shaped_key)
                        .or_insert_with(|| {
                            shape_text_styled(
                                rasterizer,
                                text,
                                font_size,
                                FontFamily::SANS_SERIF,
                                FontWeight::Normal,
                                FontStyle::Normal,
                            )
                        });
                    layout_shaped_text(rasterizer, shaped_text, 0.0, 0.0, layout_width)
                });
            positioned
                .iter()
                .map(|pg| (pg.glyph_key, pg.font_size))
                .collect()
        };

        for (key, glyph_font_size) in glyphs {
            let (is_color, width, height) = {
                let rg = self
                    .rasterizer
                    .rasterize_key(key, glyph_font_size);
                (rg.is_color, rg.width, rg.height)
            };
            if width == 0 || height == 0 {
                continue;
            }
            if is_color {
                if self
                    .color_atlas
                    .get(&key)
                    .is_none()
                {
                    let rg = self
                        .rasterizer
                        .rasterize_bitmap_key(key, glyph_font_size);
                    self.color_atlas
                        .get_or_insert(device, queue, key, rg.width, rg.height, &rg.bitmap);
                    self.rasterizer.release_bitmap(key);
                }
            } else if self.atlas.get(&key).is_none() {
                let rg = self
                    .rasterizer
                    .rasterize_bitmap_key(key, glyph_font_size);
                self.atlas
                    .get_or_insert(device, queue, key, rg.width, rg.height, &rg.bitmap);
                self.rasterizer.release_bitmap(key);
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        is_srgb: bool,
        requests: &[TextDrawRequest],
        decorations: &[TextDecorationDraw],
    ) {
        self.instances.clear();
        self.color_instances.clear();
        self.decoration_instances.clear();
        self.decoration_instances.extend(
            decorations
                .iter()
                .map(|d| d.to_instance()),
        );
        self.request_ranges.clear();
        self.request_ranges
            .reserve(requests.len());

        // Atlas regions recorded in lock-step with `self.instances` /
        // `self.color_instances`. UVs depend on the atlas dimensions, which can
        // change mid-frame if inserting a glyph triggers a `grow()` (the atlas
        // doubles in size). Computing UVs inline would leave glyphs processed
        // *before* the grow with stale UVs that reference the old, smaller
        // dimensions while the bind group now points at the larger texture —
        // producing garbled/overlapping text (notably after resizing the
        // window down and back up, which reflows text and inserts many new
        // glyphs at once). We therefore record the regions here and resolve
        // their UVs once, after all insertions, using the final dimensions.
        let mut alpha_regions: Vec<AtlasRegion> = Vec::new();
        let mut color_regions: Vec<AtlasRegion> = Vec::new();

        // Cache lifetime (perf): previously both caches were wiped whenever the
        // number of draw requests changed (e.g. on every screen transition or
        // whenever a different count of text nodes was visible). That threw away
        // all shaping/layout work and forced a full, frame-stalling re-shape
        // through rustybuzz (the 27–86 ms spikes seen in the render trace).
        //
        // Shaping is width-independent and layout is origin-independent, so once
        // a string has been seen its entry stays valid across screens, scrolling
        // and animation. We therefore keep the caches *persistent* and only bound
        // them by an absolute capacity, evicting wholesale just when the hard cap
        // is exceeded (rare) instead of on every request-count change. This keeps
        // steady-state frames on the ~1 ms full-hit path needed for 120+ fps.
        if self.layout_cache.len() > Self::LAYOUT_CACHE_CAPACITY {
            self.layout_cache.clear();
        }

        if self.shaping_cache.len() > Self::SHAPING_CACHE_CAPACITY {
            self.shaping_cache.clear();
        }

        for req in requests {
            // Record the glyph ranges this request will own. Both instance lists
            // are appended to in request order, so the slice for this request is
            // `[start, len_after)` in each list.
            let alpha_start = self.instances.len() as u32;
            let color_start = self.color_instances.len() as u32;

            // Avoid cloning the span list on every frame (it ran even on a pure
            // cache hit). Borrow `req.spans` directly when present and only
            // allocate a one-element fallback when the request has no spans.
            let synthesized: [RichTextSpan; 1];
            let spans: &[RichTextSpan] = if req.spans.is_empty() {
                synthesized = [RichTextSpan::new(req.text.clone())];
                &synthesized
            } else {
                &req.spans
            };

            let mut cursor_x = req.x;
            let mut cursor_y = req.y;

            time_cost!("RichTextSpanLoops", {
                for span in spans {
                    let font_size = span
                        .font_size
                        .unwrap_or(req.font_size);
                    let color = span.color.unwrap_or(req.color);
                    let font_weight = span
                        .font_weight
                        .or(req.font_weight)
                        .unwrap_or(FontWeight::Normal.numeric());
                    // A weight of 600+ (semi-bold and up) is rendered bold.
                    let is_bold = font_weight >= 600;
                    // ponytail: synthetic (faux) italic via a horizontal shear in
                    // the glyph shaders (0.25 ≈ 14°). Ceiling: not a real italic
                    // face (no cursive glyph forms, advances unchanged). Upgrade
                    // path: load a real italic/oblique face and key the atlas by it.
                    let skew = if span.italic.unwrap_or(req.italic) { 0.25 } else { 0.0 };

                    // Re-use the positioned glyph list from the previous frame when
                    // text content, font size, and wrapping width are all unchanged.
                    // The screen-space (x, y) origin is NOT part of the key (improvement B);
                    // instead we translate the cached positions by the current cursor
                    // offset at render time.  This means scrolling or animating text
                    // never causes a cache miss.
                    //
                    // Improvement C: we store the glyphs by value in the cache and
                    // iterate directly over the cached slice, avoiding the per-frame
                    // Vec clone that was previously issued on every cache hit.
                    let layout_width = match req.overflow {
                        TextOverflowMode::Wrap | TextOverflowMode::Ellipsis => req.bounds_width,
                        TextOverflowMode::Clip => 0.0,
                    };
                    let cache_key = LayoutCacheKey::new(
                        &span.text,
                        font_size,
                        layout_width,
                        req.font_family,
                        req.font_style,
                        font_weight,
                    );
                    // Layout is always computed at origin (0, 0) so the cached
                    // positions are purely relative and can be shifted cheaply.
                    let positioned: &[PositionedGlyph] =
                        time_cost!("TextPipelineV2::prepare - LayoutText", {
                            let shaping_cache = &mut self.shaping_cache;
                            let rasterizer = &mut self.rasterizer;
                            self.layout_cache
                                .entry(cache_key)
                                .or_insert_with(|| {
                                    let shaped_key = ShapingCacheKey::new(
                                        &span.text,
                                        font_size,
                                        req.font_family,
                                        req.font_style,
                                        font_weight,
                                    );
                                    let shaped_text = shaping_cache
                                        .entry(shaped_key)
                                        .or_insert_with(|| {
                                            time_cost!(
                                                "TextPipelineV2::prepare - ShapeText",
                                                || {
                                                    shape_text_styled(
                                                        rasterizer,
                                                        &span.text,
                                                        font_size,
                                                        req.font_family,
                                                        FontWeight::Value(u32::from(font_weight)),
                                                        req.font_style,
                                                    )
                                                }
                                            )
                                        });
                                    layout_shaped_text(
                                        rasterizer,
                                        shaped_text,
                                        0.0,
                                        0.0,
                                        layout_width,
                                    )
                                })
                        });

                    for pg in positioned {
                        let key = pg.glyph_key;

                        // Step 1: rasterize (cache hit if already done) to discover
                        // whether the glyph is color or alpha. We only need three
                        // scalar fields here, so the immutable borrow ends quickly.

                        let (is_color, rg_width, rg_height) = {
                            let rg = self
                                .rasterizer
                                .rasterize_key(key, pg.font_size);
                            (rg.is_color, rg.width, rg.height)
                        };

                        // Step 2: route the bitmap into the appropriate atlas, then
                        // build a `GlyphInstance` and push it to the matching list.
                        // We keep the resolved `AtlasRegion` (rather than UVs) so the
                        // UVs can be computed after the loop against the final atlas
                        // size — inserting a glyph here may `grow()` the atlas, which
                        // would invalidate UVs computed for earlier glyphs.
                        let (region, target_color_list) = if is_color {
                            let region = if let Some(region) = self.color_atlas.get(&key) {
                                region
                            } else {
                                // Cache hit on the rasterizer side — instant.
                                let rg = self
                                    .rasterizer
                                    .rasterize_bitmap_key(key, pg.font_size);
                                let region = self.color_atlas.get_or_insert(
                                    device, queue, key, rg.width, rg.height, &rg.bitmap,
                                );
                                self.rasterizer.release_bitmap(key);
                                region
                            };
                            (region, true)
                        } else {
                            let region = if let Some(region) = self.atlas.get(&key) {
                                region
                            } else {
                                let rg = self
                                    .rasterizer
                                    .rasterize_bitmap_key(key, pg.font_size);
                                let region = self.atlas.get_or_insert(
                                    device, queue, key, rg.width, rg.height, &rg.bitmap,
                                );
                                self.rasterizer.release_bitmap(key);
                                region
                            };
                            (region, false)
                        };

                        // For color emoji we render at the rasterized size (which is
                        // already at `font_size` resolution thanks to the resampler
                        // in `rasterize_color_glyph`). For alpha glyphs we keep the
                        // historical `pg.width / pg.height` (which equals `rg_width /
                        // rg_height` when the layout came from the same rasterizer).
                        let size = if is_color {
                            [rg_width as f32, rg_height as f32]
                        } else {
                            [pg.width as f32, pg.height as f32]
                        };
                        // Improvement B: cached glyphs are positioned at origin (0,0);
                        // apply the actual screen-space cursor offset here.
                        //
                        // `uv_rect` is left as a placeholder; the final UVs are
                        // resolved after the loop once the atlas has reached its
                        // final size (see `alpha_regions` / `color_regions`).
                        let instance = GlyphInstance {
                            position: [pg.x + cursor_x, pg.y + cursor_y],
                            size,
                            uv_rect: [0.0, 0.0, 0.0, 0.0],
                            color,
                            clip_rect: req.clip_rect,
                            clip_border_radius: req.clip_border_radius,
                            skew,
                            _pad: [0.0; 3],
                        };

                        if target_color_list {
                            self.color_instances.push(instance);
                            color_regions.push(region);
                        } else {
                            self.instances.push(instance);
                            alpha_regions.push(region);
                            if is_bold {
                                // ponytail: synthetic (faux) bold via double-strike —
                                // re-draw the same alpha glyph offset horizontally to
                                // thicken the strokes. Ceiling: not a real bold face
                                // (no dedicated weight metrics/hinting, advances are
                                // unchanged). Upgrade path: load a real bold face or a
                                // variable-font weight axis and key the glyph atlas by
                                // weight.
                                let mut bold = instance;
                                bold.position[0] += (pg.font_size * 0.03).max(0.5);
                                self.instances.push(bold);
                                alpha_regions.push(region);
                            }
                        }
                    }

                    if let Some(last) = positioned.last() {
                        // Positions in the cache are relative to (0, 0); add the
                        // current cursor offset to get the true next pen position.
                        cursor_x += last.x + last.width as f32;
                        cursor_y += last.y;
                    }
                }
            });

            self.request_ranges
                .push(TextRequestRange {
                    alpha_start,
                    alpha_end: self.instances.len() as u32,
                    color_start,
                    color_end: self.color_instances.len() as u32,
                });
        }

        // Now that every glyph has been inserted, the atlases have reached
        // their final dimensions for this frame. Resolve UVs against those
        // final dimensions so glyphs inserted before a mid-frame `grow()` are
        // not left referencing stale (smaller) atlas sizes.
        let (aw, ah) = (self.atlas.width, self.atlas.height);
        for (instance, region) in self
            .instances
            .iter_mut()
            .zip(alpha_regions.iter())
        {
            instance.uv_rect = region.uvs(aw, ah);
        }
        let (cw, ch) = (self.color_atlas.width, self.color_atlas.height);
        for (instance, region) in self
            .color_instances
            .iter_mut()
            .zip(color_regions.iter())
        {
            instance.uv_rect = region.uvs(cw, ch);
        }

        // Upload both atlases if new glyphs were added.
        self.atlas.upload(queue);
        self.color_atlas.upload(queue);

        // Rebuild bind groups only when their atlas texture was recreated (grow).
        let atlas_gen = self.atlas.generation();
        if atlas_gen != self.atlas_generation {
            self.atlas_generation = atlas_gen;
            self.bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.viewport_buffer,
                &self.atlas.view,
                &self.sampler,
            );
        }
        let color_gen = self.color_atlas.generation();
        if color_gen != self.color_atlas_generation {
            self.color_atlas_generation = color_gen;
            self.color_bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.viewport_buffer,
                &self.color_atlas.view,
                &self.sampler,
            );
        }

        // Update viewport uniform only when dimensions or sRGB state change.
        // On Android, pass 2.0 to signal shaders to skip sRGB conversion entirely.
        #[cfg(target_os = "android")]
        let is_srgb_f32 = 2.0_f32;
        #[cfg(not(target_os = "android"))]
        let is_srgb_f32 = if is_srgb { 1.0_f32 } else { 0.0 };
        if self.last_viewport != (width, height) {
            self.last_viewport = (width, height);
            queue.write_buffer(
                &self.viewport_buffer,
                0,
                bytemuck::cast_slice(&[width as f32, height as f32, is_srgb_f32, 0.0]),
            );
        }

        let previous_capacity = self.instance_policy.capacity();
        self.instance_policy
            .record_usage(self.instances.len());
        if self.instance_policy.capacity() != previous_capacity {
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text instance buffer"),
                size: (self.instance_policy.capacity() * size_of::<GlyphInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let previous_color_capacity = self
            .color_instance_policy
            .capacity();
        self.color_instance_policy
            .record_usage(self.color_instances.len());
        if self
            .color_instance_policy
            .capacity()
            != previous_color_capacity
        {
            self.color_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text color instance buffer"),
                size: (self
                    .color_instance_policy
                    .capacity()
                    * size_of::<GlyphInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        let previous_decoration_capacity = self
            .decoration_instance_policy
            .capacity();
        self.decoration_instance_policy
            .record_usage(self.decoration_instances.len());
        if self
            .decoration_instance_policy
            .capacity()
            != previous_decoration_capacity
        {
            self.decoration_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text decoration instance buffer"),
                size: (self
                    .decoration_instance_policy
                    .capacity()
                    * size_of::<DecorationInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Upload instance data for all lists.
        if !self.instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&self.instances));
        }
        if !self.color_instances.is_empty() {
            queue.write_buffer(
                &self.color_instance_buffer,
                0,
                bytemuck::cast_slice(&self.color_instances),
            );
        }
        if !self
            .decoration_instances
            .is_empty()
        {
            queue.write_buffer(
                &self.decoration_instance_buffer,
                0,
                bytemuck::cast_slice(&self.decoration_instances),
            );
        }
    }

    pub fn instance_buffer_bytes(&self) -> u64 {
        (self.instance_policy.capacity() * size_of::<GlyphInstance>()) as u64
            + (self
                .color_instance_policy
                .capacity()
                * size_of::<GlyphInstance>()) as u64
            + (self
                .decoration_instance_policy
                .capacity()
                * size_of::<DecorationInstance>()) as u64
    }

    pub fn glyph_bitmap_cache_bytes(&self) -> usize {
        self.rasterizer
            .bitmap_cache_bytes()
    }

    pub fn glyph_atlas_bytes(&self) -> u64 {
        self.atlas.memory_bytes() + self.color_atlas.memory_bytes()
    }

    pub fn cached_glyph_count(&self) -> usize {
        self.rasterizer
            .cached_glyph_count()
    }

    /// Draw a single text request's glyphs at the current point in the render
    /// pass: alpha-coverage glyphs first, then color emoji so they ride on top
    /// of any monochrome glyphs sharing the same line. Drawing per request —
    /// instead of all text in one final pass — is what lets text obey z-order
    /// against rects/images (e.g. a `Stack`'s upper layer can now cover text
    /// belonging to a lower layer). `index` matches the request order passed to
    /// `prepare`.
    pub fn render_request(&self, pass: &mut wgpu::RenderPass<'_>, index: usize) {
        let Some(range) = self.request_ranges.get(index) else {
            return;
        };

        if range.alpha_end > range.alpha_start {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            pass.draw(0..6, range.alpha_start..range.alpha_end);
        }

        if range.color_end > range.color_start {
            pass.set_pipeline(&self.color_pipeline);
            pass.set_bind_group(0, &self.color_bind_group, &[]);
            pass.set_vertex_buffer(
                0,
                self.color_instance_buffer
                    .slice(..),
            );
            pass.draw(0..6, range.color_start..range.color_end);
        }
    }

    /// Draw a single decoration line (underline/overline/strike) at its
    /// position in the draw stream so it layers with its text. One
    /// decoration request maps to exactly one instance. Reuses the alpha
    /// `bind_group` (it only needs the viewport uniform).
    pub fn render_decoration(&self, pass: &mut wgpu::RenderPass<'_>, index: usize) {
        if index >= self.decoration_instances.len() {
            return;
        }
        pass.set_pipeline(&self.decoration_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(
            0,
            self.decoration_instance_buffer
                .slice(..),
        );
        let start = index as u32;
        pass.draw(0..6, start..start + 1);
    }

    /// Measure text width using the rasterizer.
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> f32 {
        self.rasterizer
            .measure_text(text, font_size)
    }
}

#[cfg(test)]
mod tests {
    use crate::font::{FontFamily, FontStyle, FontWeight};

    use super::{LayoutCacheKey, ShapingCacheKey, TextDecorationDraw};

    #[test]
    fn text_cache_keys_isolate_font_families_and_variants() {
        let sans_layout = LayoutCacheKey::new(
            "same",
            16.0,
            100.0,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
        );
        let mono_layout = LayoutCacheKey::new(
            "same",
            16.0,
            100.0,
            FontFamily::MONOSPACE,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
        );
        assert_ne!(sans_layout, mono_layout);

        let normal_shape = ShapingCacheKey::new(
            "same",
            16.0,
            FontFamily::MONOSPACE,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
        );
        let italic_shape = ShapingCacheKey::new(
            "same",
            16.0,
            FontFamily::MONOSPACE,
            FontStyle::Italic,
            FontWeight::Normal.numeric(),
        );
        assert_ne!(normal_shape, italic_shape);
    }

    // Guards the CPU->GPU packing of a decoration line: `params` must be
    // [style_id, thickness, period, band_height] and geometry must map to the
    // instance's position/size, matching what `text_decoration.wgsl` reads.
    #[test]
    fn decoration_instance_packing() {
        let draw = TextDecorationDraw {
            x: 10.0,
            y: 20.0,
            width: 120.0,
            band_height: 6.0,
            thickness: 2.0,
            period: 8.0,
            style: 4, // Wavy
            color: [1.0, 0.0, 0.0, 1.0],
            clip_rect: [0.0, 0.0, -1.0, 0.0],
            clip_border_radius: [0.0; 4],
        };
        let inst = draw.to_instance();
        assert_eq!(inst.position, [10.0, 20.0]);
        assert_eq!(inst.size, [120.0, 6.0]);
        assert_eq!(inst.color, [1.0, 0.0, 0.0, 1.0]);
        // params: style, thickness, period, band_height (band_height duplicated
        // so the fragment shader has it without relying on the interpolated size).
        assert_eq!(inst.params, [4.0, 2.0, 8.0, 6.0]);
    }
}
