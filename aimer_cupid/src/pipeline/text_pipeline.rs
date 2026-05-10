pub mod glyph_atlas;
pub mod glyph_rasterizer;
pub mod text_layout;
mod font_resolver;
mod performance_test;
mod glyph_outline;

use crate::text_pipeline::glyph_atlas::{ColorGlyphAtlas, GlyphAtlas};
use crate::text_pipeline::glyph_rasterizer::GlyphRasterizer;
use crate::text_pipeline::text_layout::{layout_text, PositionedGlyph};
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

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
    /// Border radius for the clip rect: [top-left, top-right, bottom-right, bottom-left].
    clip_border_radius: [f32; 4],
}

impl GlyphInstance {
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
            array_stride: size_of::<GlyphInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct TextDrawRequest {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub bounds_width: f32,
    pub bounds_height: f32,
    pub overflow: TextOverflowMode,
    pub line_height: Option<f32>,
    pub font_weight: Option<u16>,
    pub italic: bool,
    pub clip_rect: [f32; 4],
    pub clip_border_radius: [f32; 4],
    pub spans: Vec<RichTextSpan>,
}

#[derive(Clone, Debug)]
pub struct RichTextSpan {
    pub text: String,
    pub font_size: Option<f32>,
    pub color: Option<[f32; 4]>,
    pub font_weight: Option<u16>,
    pub italic: Option<bool>,
}

impl RichTextSpan {
    pub fn new(text: impl Into<String>) -> Self {
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
///
/// Improvement B: the screen-space origin is intentionally excluded from this
/// key.  Layout depends only on text content, font size, and wrapping width —
/// not on the position at which the text is drawn.  The actual (x, y) offset
/// is applied at render time (see `prepare`), so scrolling or animating text
/// no longer causes cache misses.
#[derive(Hash, Eq, PartialEq, Clone)]
struct LayoutCacheKey {
    text: String,
    /// `font_size` × 100, rounded, stored as u32 to make it hashable.
    font_size_u32: u32,
    /// `bounds_width` × 100, rounded, stored as u32.
    bounds_width_u32: u32,
}

impl LayoutCacheKey {
    fn new(text: &str, font_size: f32, bounds_width: f32) -> Self {
        Self {
            text: text.to_owned(),
            font_size_u32: (font_size * 100.0).round() as u32,
            bounds_width_u32: (bounds_width * 100.0).round() as u32,
        }
    }
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
    instance_capacity: usize,
    instances: Vec<GlyphInstance>,
    /// Sibling buffer + scratch list for color glyph quads (drawn in a second
    /// pass after the alpha glyphs so they layer on top of the same line).
    color_instance_buffer: wgpu::Buffer,
    color_instance_capacity: usize,
    color_instances: Vec<GlyphInstance>,
    /// Track atlas generation to only rebuild bind group when atlas texture changes.
    atlas_generation: u64,
    color_atlas_generation: u64,
    /// Cached viewport dimensions to skip redundant uniform writes.
    last_viewport: (u32, u32),
    /// Layout cache: maps a stable key derived from text content + render
    /// parameters to the pre-computed `Vec<PositionedGlyph>`.  Entries are
    /// cleared whenever the set of requests changes (different number of
    /// draw calls) so memory does not grow unboundedly across scene changes.
    layout_cache: HashMap<LayoutCacheKey, Vec<PositionedGlyph>>,
}

impl TextPipelineV2 {
    const INITIAL_CAPACITY: usize = 512;

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, pipeline_cache: Option<&wgpu::PipelineCache>) -> Self {
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

        let bind_group = Self::create_bind_group(device, &bind_group_layout, &viewport_buffer, &atlas.view, &sampler);
        let color_bind_group = Self::create_bind_group(device, &bind_group_layout, &viewport_buffer, &color_atlas.view, &sampler);

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
                buffers: &[GlyphInstance::layout()],
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
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: pipeline_cache,
        });

        let color_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text color pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &color_shader,
                entry_point: Some("vs_main"),
                buffers: &[GlyphInstance::layout()],
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
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
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
            instance_capacity: Self::INITIAL_CAPACITY,
            instances: Vec::new(),
            color_instance_buffer,
            color_instance_capacity: Self::INITIAL_CAPACITY,
            color_instances: Vec::new(),
            atlas_generation: 0,
            color_atlas_generation: 0,
            last_viewport: (0, 0),
            layout_cache: HashMap::new(),
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
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
            ],
        })
    }

    pub fn preload_text(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, text: &str, font_size: f32) {
        for (key, glyph) in self.rasterizer.preload_text(text, font_size) {
            if glyph.is_color {
                if self.color_atlas.get(&key).is_none() {
                    self.color_atlas
                        .get_or_insert(device, key, glyph.width, glyph.height, &glyph.bitmap);
                }
            } else if self.atlas.get(&key).is_none() {
                self.atlas
                    .get_or_insert(device, key, glyph.width, glyph.height, &glyph.bitmap);
            }
        }

        self.atlas.upload(queue);
        self.color_atlas.upload(queue);

        let atlas_gen = self.atlas.generation();
        if atlas_gen != self.atlas_generation {
            self.atlas_generation = atlas_gen;
            self.bind_group =
                Self::create_bind_group(device, &self.bind_group_layout, &self.viewport_buffer, &self.atlas.view, &self.sampler);
        }

        let color_gen = self.color_atlas.generation();
        if color_gen != self.color_atlas_generation {
            self.color_atlas_generation = color_gen;
            self.color_bind_group =
                Self::create_bind_group(device, &self.bind_group_layout, &self.viewport_buffer, &self.color_atlas.view, &self.sampler);
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        is_srgb: bool,
        requests: &[TextDrawRequest],
    ) {
        self.instances.clear();
        self.color_instances.clear();

        // Evict the layout cache when the number of draw requests changes (e.g.
        // when the UI transitions to a different screen with different text).
        // This keeps memory bounded while still giving full cache hits on
        // stable frames.
        if self.layout_cache.len() > requests.len() * 4 {
            self.layout_cache.clear();
        }

        for req in requests {
            let spans = if req.spans.is_empty() { vec![RichTextSpan::new(req.text.clone())] } else { req.spans.clone() };

            let mut cursor_x = req.x;
            let mut cursor_y = req.y;

            for span in &spans {
                let font_size = span.font_size.unwrap_or(req.font_size);
                let color = span.color.unwrap_or(req.color);

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
                let cache_key = LayoutCacheKey::new(&span.text, font_size, req.bounds_width);
                // Layout is always computed at origin (0, 0) so the cached
                // positions are purely relative and can be shifted cheaply.
                let positioned: &[PositionedGlyph] = self
                    .layout_cache
                    .entry(cache_key)
                    .or_insert_with(|| layout_text(&mut self.rasterizer, &span.text, font_size, 0.0, 0.0, req.bounds_width));

                for pg in positioned {
                    let key = pg.glyph_key;

                    // Step 1: rasterize (cache hit if already done) to discover
                    // whether the glyph is color or alpha. We only need three
                    // scalar fields here, so the immutable borrow ends quickly.

                    let (is_color, rg_width, rg_height) = {
                        let rg = self.rasterizer.rasterize_key(key, pg.font_size);
                        (rg.is_color, rg.width, rg.height)
                    };

                    // Step 2: route the bitmap into the appropriate atlas, then
                    // build a `GlyphInstance` and push it to the matching list.
                    let (uvs, target_color_list) = if is_color {
                        let region = if let Some(region) = self.color_atlas.get(&key) {
                            region
                        } else {
                            // Cache hit on the rasterizer side — instant.
                            let rg = self.rasterizer.rasterize_key(key, pg.font_size);
                            self.color_atlas
                                .get_or_insert(device, key, rg.width, rg.height, &rg.bitmap)
                        };
                        (region.uvs(self.color_atlas.width, self.color_atlas.height), true)
                    } else {
                        let region = if let Some(region) = self.atlas.get(&key) {
                            region
                        } else {
                            let rg = self.rasterizer.rasterize_key(key, pg.font_size);
                            self.atlas
                                .get_or_insert(device, key, rg.width, rg.height, &rg.bitmap)
                        };
                        (region.uvs(self.atlas.width, self.atlas.height), false)
                    };

                    // For color emoji we render at the rasterized size (which is
                    // already at `font_size` resolution thanks to the resampler
                    // in `rasterize_color_glyph`). For alpha glyphs we keep the
                    // historical `pg.width / pg.height` (which equals `rg_width /
                    // rg_height` when the layout came from the same rasterizer).
                    let size = if is_color { [rg_width as f32, rg_height as f32] } else { [pg.width as f32, pg.height as f32] };
                    // Improvement B: cached glyphs are positioned at origin (0,0);
                    // apply the actual screen-space cursor offset here.
                    let instance = GlyphInstance {
                        position: [pg.x + cursor_x, pg.y + cursor_y],
                        size,
                        uv_rect: uvs,
                        color,
                        clip_rect: req.clip_rect,
                        clip_border_radius: req.clip_border_radius,
                    };

                    if target_color_list {
                        self.color_instances.push(instance);
                    } else {
                        self.instances.push(instance);
                    }
                }

                if let Some(last) = positioned.last() {
                    // Positions in the cache are relative to (0, 0); add the
                    // current cursor offset to get the true next pen position.
                    cursor_x += last.x + last.width as f32;
                    cursor_y += last.y;
                }
            }
        }

        // Upload both atlases if new glyphs were added.
        self.atlas.upload(queue);
        self.color_atlas.upload(queue);

        // Rebuild bind groups only when their atlas texture was recreated (grow).
        let atlas_gen = self.atlas.generation();
        if atlas_gen != self.atlas_generation {
            self.atlas_generation = atlas_gen;
            self.bind_group =
                Self::create_bind_group(device, &self.bind_group_layout, &self.viewport_buffer, &self.atlas.view, &self.sampler);
        }
        let color_gen = self.color_atlas.generation();
        if color_gen != self.color_atlas_generation {
            self.color_atlas_generation = color_gen;
            self.color_bind_group =
                Self::create_bind_group(device, &self.bind_group_layout, &self.viewport_buffer, &self.color_atlas.view, &self.sampler);
        }

        // Update viewport uniform only when dimensions or sRGB state change.
        // On Android, pass 2.0 to signal shaders to skip sRGB conversion entirely.
        #[cfg(target_os = "android")]
        let is_srgb_f32 = 2.0_f32;
        #[cfg(not(target_os = "android"))]
        let is_srgb_f32 = if is_srgb { 1.0_f32 } else { 0.0 };
        if self.last_viewport != (width, height) {
            self.last_viewport = (width, height);
            queue.write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&[width as f32, height as f32, is_srgb_f32, 0.0]));
        }

        // Grow alpha instance buffer if needed.
        if self.instances.len() > self.instance_capacity {
            self.instance_capacity = self.instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text instance buffer"),
                size: (self.instance_capacity * size_of::<GlyphInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Grow color instance buffer if needed.
        if self.color_instances.len() > self.color_instance_capacity {
            self.color_instance_capacity = self.color_instances.len().next_power_of_two();
            self.color_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text color instance buffer"),
                size: (self.color_instance_capacity * size_of::<GlyphInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Upload instance data for both lists.
        if !self.instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&self.instances));
        }
        if !self.color_instances.is_empty() {
            queue.write_buffer(&self.color_instance_buffer, 0, bytemuck::cast_slice(&self.color_instances));
        }
    }

    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        // Alpha pass first, color pass second. Both within the same render
        // pass — color emoji ride on top of any monochrome glyphs sharing the
        // same line.
        if !self.instances.is_empty() {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            pass.draw(0..6, 0..self.instances.len() as u32);
        }

        if !self.color_instances.is_empty() {
            pass.set_pipeline(&self.color_pipeline);
            pass.set_bind_group(0, &self.color_bind_group, &[]);
            pass.set_vertex_buffer(0, self.color_instance_buffer.slice(..));
            pass.draw(0..6, 0..self.color_instances.len() as u32);
        }
    }

    /// Measure text width using the rasterizer.
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> f32 {
        self.rasterizer.measure_text(text, font_size)
    }
}


