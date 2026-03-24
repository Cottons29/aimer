use bytemuck::{Pod, Zeroable};

use super::glyph_atlas::GlyphAtlas;
use super::glyph_rasterizer::{GlyphKey, GlyphRasterizer};
use super::text_layout::layout_text;

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
    /// Border radius for the clip rect. 0.0 means rectangular clip.
    clip_border_radius: f32,
    _pad: f32,
}

impl GlyphInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphInstance>() as wgpu::BufferAddress,
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
    pub clip_rect: [f32; 4],
    pub clip_border_radius: f32,
}

pub struct TextPipelineV2 {
    rasterizer: GlyphRasterizer,
    atlas: GlyphAtlas,
    pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instances: Vec<GlyphInstance>,
    /// Track atlas generation to only rebuild bind group when atlas texture changes.
    atlas_generation: u64,
    /// Cached viewport dimensions to skip redundant uniform writes.
    last_viewport: (u32, u32),
}

impl TextPipelineV2 {
    const INITIAL_CAPACITY: usize = 512;

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let rasterizer = GlyphRasterizer::new();
        let atlas = GlyphAtlas::new(device);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text shader v2"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/text.wgsl").into()),
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
                    visibility: wgpu::ShaderStages::VERTEX,
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
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text instance buffer"),
            size: (Self::INITIAL_CAPACITY * std::mem::size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            rasterizer,
            atlas,
            pipeline,
            viewport_buffer,
            bind_group_layout,
            bind_group,
            sampler,
            instance_buffer,
            instance_capacity: Self::INITIAL_CAPACITY,
            instances: Vec::new(),
            atlas_generation: 0,
            last_viewport: (0, 0),
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

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        requests: &[TextDrawRequest],
    ) {
        self.instances.clear();

        for req in requests {
            let positioned = layout_text(
                &mut self.rasterizer,
                &req.text,
                req.font_size,
                req.x,
                req.y,
                req.bounds_width,
            );

            for pg in &positioned {
                let key = GlyphKey::new(pg.codepoint, pg.font_size);
                // Only rasterize if the atlas doesn't already have this glyph.
                let region = if let Some(region) = self.atlas.get(&key) {
                    region
                } else {
                    let rg = self.rasterizer.rasterize(pg.codepoint, pg.font_size);
                    self.atlas.get_or_insert(device, key, rg.width, rg.height, &rg.bitmap)
                };
                let uvs = region.uvs(self.atlas.width, self.atlas.height);

                self.instances.push(GlyphInstance {
                    position: [pg.x, pg.y],
                    size: [pg.width as f32, pg.height as f32],
                    uv_rect: uvs,
                    color: req.color,
                    clip_rect: req.clip_rect,
                    clip_border_radius: req.clip_border_radius,
                    _pad: 0.0,
                });
            }
        }

        // Upload atlas if new glyphs were added.
        self.atlas.upload(queue);

        // Rebuild bind group only if atlas texture was recreated (grow).
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

        // Update viewport uniform only when dimensions change.
        if self.last_viewport != (width, height) {
            self.last_viewport = (width, height);
            queue.write_buffer(
                &self.viewport_buffer,
                0,
                bytemuck::cast_slice(&[width as f32, height as f32, 0.0, 0.0]),
            );
        }

        // Grow instance buffer if needed.
        if self.instances.len() > self.instance_capacity {
            self.instance_capacity = self.instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text instance buffer"),
                size: (self.instance_capacity * std::mem::size_of::<GlyphInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Upload instance data.
        if !self.instances.is_empty() {
            queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instances),
            );
        }
    }

    pub fn render<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
    ) {
        if self.instances.is_empty() {
            return;
        }

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, 0..self.instances.len() as u32);
    }

    /// Measure text width using the rasterizer.
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> f32 {
        self.rasterizer.measure_text(text, font_size)
    }
}
