use bytemuck::{Pod, Zeroable};

use super::frame_upload::FrameUpload;
use super::image_pipeline::InstanceBufferPolicy;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct RectInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    /// Per-corner border radius: [top-left, top-right, bottom-right,
    /// bottom-left]
    pub border_radius: [f32; 4],
    /// Per-side border width: [top, right, bottom, left]
    pub border_width: [f32; 4],
    pub border_color: [f32; 4],
    /// Per-side outline width: [top, right, bottom, left]
    pub outline_width: [f32; 4],
    pub outline_color: [f32; 4],
    /// Clip rect: [x, y, width, height]. If width <= 0, no clip is applied.
    pub clip_rect: [f32; 4],
    /// Border radius for the clip rect: [top-left, top-right, bottom-right,
    /// bottom-left].
    pub clip_border_radius: [f32; 4],
    /// Shadow parameters: [offset_x, offset_y, blur, spread]
    pub shadow_params: [f32; 4],
    /// Shadow color (RGBA, 0..1)
    pub shadow_color: [f32; 4],
    /// Shadow flags: [inset (0.0 or 1.0), 0, 0, 0]
    pub shadow_flags: [f32; 4],
}

impl RectInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 13] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
        8 => Float32x4,
        9 => Float32x4,
        10 => Float32x4,
        11 => Float32x4,
        12 => Float32x4,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<RectInstance>() as wgpu::BufferAddress,
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
    instance_policy: InstanceBufferPolicy,
    /// Every rect instance of the current frame, in draw order. The vector
    /// only grows during a frame; each flush records a draw over the tail it
    /// has not covered yet, and [`end_frame`] uploads the whole thing in one
    /// `write_buffer` — one staging allocation and one blit per frame instead
    /// of one per z-order split.
    ///
    /// [`end_frame`]: RectPipeline::end_frame
    instances: Vec<RectInstance>,
    /// Number of instances already covered by recorded draw calls this frame.
    /// The pipeline may be flushed multiple times per frame (e.g. when an
    /// image or custom-pipeline command splits the rect stream); each flush
    /// draws from a distinct region of the shared buffer so no batch aliases
    /// another.
    frame_instance_offset: usize,
    /// Skips the frame's single upload when the buffer already holds the
    /// frame's exact bytes — the common case for a static scene.
    upload: FrameUpload<RectInstance>,
    /// The `(width, height, is_srgb)` the viewport uniform was last written
    /// for, so an unchanged viewport costs no upload at all.
    last_viewport: Option<(u32, u32, bool)>,
}

impl RectPipeline {
    const INITIAL_CAPACITY: usize = 256;

    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        pipeline_cache: Option<&wgpu::PipelineCache>,
        antialiasing: crate::AntiAlias,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./shaders/rect.wgsl").into()),
        });

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rect viewport uniform"),
            size: 16, /* vec2<f32> + padding
                       * to 16 bytes */
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
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
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
                buffers: &[Some(RectInstance::layout())],
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
            multisample: crate::pipeline::multisample_state(antialiasing),
            multiview_mask: None,
            cache: pipeline_cache,
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
            instance_policy: InstanceBufferPolicy::new(Self::INITIAL_CAPACITY),
            instances: Vec::new(),
            frame_instance_offset: 0,
            upload: FrameUpload::new(),
            last_viewport: None,
        }
    }

    pub fn push(&mut self, instance: RectInstance) {
        self.instances.push(instance);
    }

    pub fn clear(&mut self) {
        self.instances.clear();
        // A fresh frame starts writing at the beginning of the instance buffer.
        self.frame_instance_offset = 0;
    }

    /// Opens a new frame: sizes the instance buffer for `total_rects`, writes
    /// the viewport uniform when it changed, and resets the per-frame state.
    ///
    /// Sizing the buffer up-front — the renderer knows the frame's full rect
    /// count from its resolved command list — is what lets the whole frame's
    /// instances travel in a single upload at [`end_frame`]: no flush can ever
    /// outgrow the buffer mid-pass, so every recorded draw references the same
    /// buffer the deferred write lands in.
    ///
    /// [`end_frame`]: RectPipeline::end_frame
    pub fn begin_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        total_rects: usize,
        width: u32,
        height: u32,
        is_srgb: bool,
    ) {
        let previous_capacity = self.instance_policy.capacity();
        self.instance_policy
            .record_usage(self.frame_instance_offset);
        self.instance_policy.grow_to_fit(total_rects);
        if self.instance_policy.capacity() != previous_capacity {
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rect instance buffer (resized)"),
                size: (self.instance_policy.capacity() * size_of::<RectInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.upload.invalidate();
        }

        // Update the viewport uniform only when it actually changed.
        // On Android, pass 2.0 to signal shaders to skip sRGB conversion entirely.
        #[cfg(target_os = "android")]
        let is_srgb_f32 = 2.0_f32;
        #[cfg(not(target_os = "android"))]
        let is_srgb_f32 = if is_srgb { 1.0_f32 } else { 0.0 };
        if self.last_viewport != Some((width, height, is_srgb)) {
            self.last_viewport = Some((width, height, is_srgb));
            queue.write_buffer(
                &self.viewport_buffer,
                0,
                bytemuck::cast_slice(&[width as f32, height as f32, is_srgb_f32, 0.0]),
            );
        }

        self.clear();
    }

    pub fn instance_buffer_bytes(&self) -> u64 {
        (self.instance_policy.capacity() * size_of::<RectInstance>()) as u64
    }

    /// Records a draw call for the rects pushed since the previous flush.
    ///
    /// Nothing is uploaded here. The draw references this batch's region of
    /// the shared instance buffer — batches must not alias, since every
    /// `write_buffer` is applied on the queue timeline *before* the pass
    /// executes — and the bytes for all regions land together in
    /// [`end_frame`]'s single write. A text-heavy frame (rect/text/rect/text
    /// …) therefore no longer pays a staging allocation and a blit per
    /// z-order split.
    ///
    /// [`end_frame`]: RectPipeline::end_frame
    pub fn flush(&mut self, pass: &mut wgpu::RenderPass<'_>) {
        let pending = self.instances.len() - self.frame_instance_offset;
        if pending == 0 {
            return;
        }
        debug_assert!(
            self.instances.len() <= self.instance_policy.capacity(),
            "begin_frame must be told the frame's full rect count"
        );

        let byte_offset = (self.frame_instance_offset * size_of::<RectInstance>()) as u64;
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.viewport_bind_group, &[]);
        pass.set_vertex_buffer(0, self.instance_buffer.slice(byte_offset..));
        pass.draw(0..6, 0..pending as u32);

        self.frame_instance_offset = self.instances.len();
    }

    /// Uploads the frame's rect instances in a single write, or not at all
    /// when the buffer already holds these exact bytes (a static frame).
    ///
    /// Must run after the frame's flushes and before the queue submit; a
    /// `write_buffer` issued here is applied before the submitted pass
    /// executes, so the draws recorded earlier read the fresh data.
    pub fn end_frame(&mut self, queue: &wgpu::Queue) {
        self.upload
            .upload(queue, &self.instance_buffer, &self.instances);
    }
}
