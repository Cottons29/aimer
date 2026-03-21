use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

use crate::utilities::TextureId;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ImageInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    /// Clip rect: [x, y, width, height]. If width <= 0, no clip is applied.
    pub clip_rect: [f32; 4],
}

impl ImageInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32x2,
        4 => Float32x4,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ImageInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

struct TextureEntry {
    bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    texture: wgpu::Texture,
}

pub struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: HashMap<TextureId, TextureEntry>,
    next_id: TextureId,
    instance_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    instance_capacity: usize,
}

impl ImagePipeline {
    const INITIAL_CAPACITY: usize = 64;

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/image.wgsl").into()),
        });

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image viewport uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let viewport_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("image viewport layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image viewport bind group"),
            layout: &viewport_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("image texture layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image pipeline layout"),
            bind_group_layouts: &[&viewport_layout, &texture_bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[ImageInstance::layout()],
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image instance buffer"),
            size: (Self::INITIAL_CAPACITY * std::mem::size_of::<ImageInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            viewport_buffer,
            viewport_bind_group,
            texture_bind_group_layout,
            sampler,
            textures: HashMap::new(),
            next_id: 1,
            instance_buffer,
            instance_capacity: Self::INITIAL_CAPACITY,
        }
    }

    /// Upload RGBA8 image data and return a TextureId.
    pub fn upload_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> TextureId {
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("uploaded image"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            data,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image bind group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        let id = self.next_id;
        self.next_id += 1;
        self.textures.insert(id, TextureEntry { bind_group, texture });
        id
    }

    /// Draw a single textured quad. Each image draw is a separate draw call
    /// (grouped by texture_id for batching in the renderer).
    pub fn draw_image(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        width: u32,
        height: u32,
        texture_id: TextureId,
        instance: ImageInstance,
    ) {
        let entry = match self.textures.get(&texture_id) {
            Some(e) => e,
            None => return,
        };

        queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32, 0.0, 0.0]),
        );

        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&[instance]),
        );

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.viewport_bind_group, &[]);
        pass.set_bind_group(1, &entry.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }
}
