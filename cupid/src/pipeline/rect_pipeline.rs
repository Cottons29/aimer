use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RectInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    /// Per-corner border radius: [top-left, top-right, bottom-right, bottom-left]
    pub border_radius: [f32; 4],
    /// Per-side border width: [top, right, bottom, left]
    pub border_width: [f32; 4],
    pub border_color: [f32; 4],
    /// Per-side outline width: [top, right, bottom, left]
    pub outline_width: [f32; 4],
    pub outline_color: [f32; 4],
    /// Clip rect: [x, y, width, height]. If width <= 0, no clip is applied.
    pub clip_rect: [f32; 4],
    /// Border radius for the clip rect. 0.0 means rectangular clip.
    pub clip_border_radius: f32,
}

impl RectInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
        8 => Float32x4,
        9 => Float32,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RectInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

pub struct RectPipeline {
    pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_capacity: usize,
    instances: Vec<RectInstance>,
}

impl RectPipeline {
    const INITIAL_CAPACITY: usize = 256;

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("rect.wgsl").into()),
        });

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect viewport uniform"),
            size: 16, // vec2<f32> + padding to 16 bytes
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rect bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rect viewport bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: viewport_buffer.as_entire_binding() }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[RectInstance::layout()],
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
            cache: None,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect instance buffer"),
            size: (Self::INITIAL_CAPACITY * size_of::<RectInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            viewport_buffer,
            viewport_bind_group,
            instance_buffer,
            instance_capacity: Self::INITIAL_CAPACITY,
            instances: Vec::new(),
        }
    }

    pub fn push(&mut self, instance: RectInstance) {
        self.instances.push(instance);
    }

    pub fn clear(&mut self) {
        self.instances.clear();
    }

    pub fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        width: u32,
        height: u32,
        is_srgb: bool,
    ) {
        if self.instances.is_empty() {
            return;
        }

        // Update viewport uniform
        let is_srgb_f32 = if is_srgb { 1.0 } else { 0.0 };
        queue.write_buffer(&self.viewport_buffer, 0, bytemuck::cast_slice(&[width as f32, height as f32, is_srgb_f32, 0.0]));

        // Grow instance buffer if needed
        if self.instances.len() > self.instance_capacity {
            self.instance_capacity = self.instances.len().next_power_of_two();
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rect instance buffer"),
                size: (self.instance_capacity * std::mem::size_of::<RectInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&self.instances));

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.viewport_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        pass.draw(0..6, 0..self.instances.len() as u32);

        self.instances.clear();
    }
}
