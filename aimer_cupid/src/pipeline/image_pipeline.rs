use std::borrow::Cow;
use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::ShaderSource;

use crate::utilities::TextureId;

fn constrained_texture_size(width: u32, height: u32, max_dimension: u32) -> (u32, u32) {
    if width <= max_dimension && height <= max_dimension {
        return (width, height);
    }

    if width >= height {
        (max_dimension, ((height as u64 * max_dimension as u64) / width as u64).max(1) as u32)
    } else {
        (((width as u64 * max_dimension as u64) / height as u64).max(1) as u32, max_dimension)
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn resize_rgba8_nearest(
    source_width: u32,
    source_height: u32,
    data: &[u8],
    target_width: u32,
    target_height: u32,
) -> Vec<u8> {
    let mut resized = vec![0; target_width as usize * target_height as usize * 4];
    let source_offsets = (0..target_width as usize)
        .map(|target_x| target_x * source_width as usize / target_width as usize * 4)
        .collect::<Vec<_>>();
    for (target_y, row) in resized
        .chunks_exact_mut(target_width as usize * 4)
        .enumerate()
    {
        let source_y = target_y * source_height as usize / target_height as usize;
        let source_row_offset = source_y * source_width as usize * 4;
        for (pixel, source_x_offset) in row
            .chunks_exact_mut(4)
            .zip(&source_offsets)
        {
            let source_offset = source_row_offset + source_x_offset;
            pixel.copy_from_slice(&data[source_offset..source_offset + 4]);
        }
    }
    resized
}

fn constrain_rgba8<'a>(
    width: u32,
    height: u32,
    data: &'a [u8],
    max_dimension: u32,
) -> (u32, u32, Cow<'a, [u8]>) {
    let expected_len = (width as u64)
        .checked_mul(height as u64)
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok());
    if width == 0 || height == 0 || max_dimension == 0 || expected_len != Some(data.len()) {
        return (1, 1, Cow::Owned(vec![0; 4]));
    }

    let (target_width, target_height) = constrained_texture_size(width, height, max_dimension);
    if (target_width, target_height) == (width, height) {
        return (width, height, Cow::Borrowed(data));
    }

    #[cfg(target_arch = "wasm32")]
    let resized = resize_rgba8_nearest(width, height, data, target_width, target_height);
    #[cfg(not(target_arch = "wasm32"))]
    let resized = {
        let source = image::RgbaImage::from_raw(width, height, data.to_vec())
            .expect("validated RGBA image dimensions must match the data length");
        image::imageops::resize(
            &source,
            target_width,
            target_height,
            image::imageops::FilterType::Lanczos3,
        )
        .into_raw()
    };
    (target_width, target_height, Cow::Owned(resized))
}

const fn image_mip_level_count() -> u32 {
    1
}

fn upload_rgba8(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    data: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
}

pub(crate) struct InstanceBufferPolicy {
    initial_capacity: usize,
    capacity: usize,
    underused_frames: u16,
}

impl InstanceBufferPolicy {
    pub(crate) const SHRINK_AFTER_FRAMES: u16 = 120;

    pub(crate) const fn new(initial_capacity: usize) -> Self {
        Self { initial_capacity, capacity: initial_capacity, underused_frames: 0 }
    }

    pub(crate) const fn capacity(&self) -> usize {
        self.capacity
    }

    pub(crate) fn record_usage(&mut self, used: usize) {
        let required = self
            .initial_capacity
            .max(used.next_power_of_two());
        if required > self.capacity {
            self.capacity = required;
            self.underused_frames = 0;
        } else if required <= self.capacity / 4 {
            self.underused_frames = self
                .underused_frames
                .saturating_add(1);
            if self.underused_frames >= Self::SHRINK_AFTER_FRAMES {
                self.capacity = required;
                self.underused_frames = 0;
            }
        } else {
            self.underused_frames = 0;
        }
    }

    pub(crate) fn grow_to_fit(&mut self, required: usize) {
        if required > self.capacity {
            self.capacity = required.next_power_of_two();
            self.underused_frames = 0;
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ImageInstance {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    /// Clip rect: [x, y, width, height]. If width <= 0, no clip is applied.
    pub clip_rect: [f32; 4],
    /// Border radius for the clip rect: [top-left, top-right, bottom-right,
    /// bottom-left].
    pub clip_border_radius: [f32; 4],
    pub alpha: f32,
}

impl ImageInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x2,
        3 => Float32x2,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<ImageInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

struct TextureEntry {
    bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    texture: wgpu::Texture,
    bytes: u64,
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
    instance_policy: InstanceBufferPolicy,
    /// Running write offset (in instances) into `instance_buffer` for the
    /// current frame. Reset by `begin_frame`. Each `draw_batch` writes its
    /// instances to a distinct region so that multiple image batches within a
    /// single render pass do not alias the same buffer memory.
    frame_instance_offset: usize,
}

impl ImagePipeline {
    const INITIAL_CAPACITY: usize = 64;

    #[inline]
    const fn get_source() -> &'static str {
        #[cfg(target_os = "android")]
        {
            concat!(
                include_str!("./shaders/android_color.wgsl"),
                include_str!("./shaders/image.wgsl")
            )
        }
        #[cfg(not(target_os = "android"))]
        {
            concat!(include_str!("./shaders/color.wgsl"), include_str!("./shaders/image.wgsl"))
        }
    }

    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        pipeline_cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image shader"),
            source: ShaderSource::Wgsl(Self::get_source().into()),
        });

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image viewport uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("image viewport layout"),
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
            bind_group_layouts: &[Some(&viewport_layout), Some(&texture_bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(ImageInstance::layout())],
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 4,
            ..Default::default()
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image instance buffer"),
            size: (Self::INITIAL_CAPACITY * size_of::<ImageInstance>()) as u64,
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
            instance_policy: InstanceBufferPolicy::new(Self::INITIAL_CAPACITY),
            frame_instance_offset: 0,
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
        let id = self.next_id;
        self.next_id += 1;
        self.upload_image_with_id(device, queue, id, width, height, data);
        id
    }

    pub fn has_texture(&self, id: TextureId) -> bool {
        self.textures
            .contains_key(&id)
    }

    /// Upload RGBA8 image data only if the texture ID does not already exist.
    /// Returns `true` if a new texture was uploaded, `false` if it already
    /// existed. Uses a single HashMap lookup instead of `has_texture` +
    /// `upload_image_with_id`.
    pub fn upload_if_absent(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: TextureId,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> bool {
        use std::collections::hash_map::Entry;
        match self
            .textures
            .entry(id)
        {
            Entry::Occupied(_) => false,
            Entry::Vacant(vacant) => {
                let (width, height, data) = constrain_rgba8(
                    width,
                    height,
                    data,
                    device
                        .limits()
                        .max_texture_dimension_2d,
                );
                // Use create_texture + write_texture instead of create_texture_with_data
                // so the copy is deferred to the GPU timeline (non-blocking).
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("uploaded image"),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: image_mip_level_count(),
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                upload_rgba8(queue, &texture, width, height, data.as_ref());

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

                vacant.insert(TextureEntry {
                    bind_group,
                    texture,
                    bytes: width as u64 * height as u64 * 4,
                });
                true
            }
        }
    }

    /// Prepare the pipeline for a new frame's image batches.
    ///
    /// Resets the per-frame instance write offset, writes the viewport uniform
    /// once (instead of per batch), and ensures the shared instance buffer is
    /// large enough to hold *all* image instances of the frame at once.
    pub fn begin_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        total_instances: usize,
        width: u32,
        height: u32,
        is_srgb: bool,
    ) {
        self.frame_instance_offset = 0;
        let previous_capacity = self
            .instance_policy
            .capacity();
        self.instance_policy
            .record_usage(total_instances);
        if self
            .instance_policy
            .capacity()
            != previous_capacity
        {
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("image instance buffer (resized)"),
                size: (self
                    .instance_policy
                    .capacity()
                    * size_of::<ImageInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        // Write the viewport uniform once per frame instead of per batch.
        #[cfg(target_os = "android")]
        let is_srgb_f32 = 2.0_f32;
        #[cfg(not(target_os = "android"))]
        let is_srgb_f32 = if is_srgb { 1.0_f32 } else { 0.0 };
        queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32, is_srgb_f32, 0.0]),
        );
    }

    pub fn upload_image_with_id(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: TextureId,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        let (width, height, data) = constrain_rgba8(
            width,
            height,
            data,
            device
                .limits()
                .max_texture_dimension_2d,
        );
        // In-place update if the texture exists and dimensions match.
        if let Some(entry) = self
            .textures
            .get(&id)
        {
            let size = entry.texture.size();
            if size.width == width && size.height == height {
                upload_rgba8(queue, &entry.texture, width, height, data.as_ref());
                return;
            }
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("uploaded image"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: image_mip_level_count(),
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        upload_rgba8(queue, &texture, width, height, data.as_ref());

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

        self.textures
            .insert(
                id,
                TextureEntry { bind_group, texture, bytes: width as u64 * height as u64 * 4 },
            );
    }

    pub fn remove_texture(&mut self, id: TextureId) -> bool {
        self.textures
            .remove(&id)
            .is_some()
    }

    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    pub fn texture_bytes(&self) -> u64 {
        self.textures
            .values()
            .map(|entry| entry.bytes)
            .sum()
    }

    pub fn instance_buffer_bytes(&self) -> u64 {
        (self
            .instance_policy
            .capacity()
            * size_of::<ImageInstance>()) as u64
    }

    /// Draw a batch of instances with the same texture_id.
    pub fn draw_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        texture_id: TextureId,
        instances: &[ImageInstance],
    ) {
        if instances.is_empty() {
            return;
        }

        let entry = match self
            .textures
            .get(&texture_id)
        {
            Some(e) => e,
            None => return,
        };

        // Each batch occupies a distinct region of the shared instance buffer so
        // that multiple image batches recorded into the same render pass do not
        // overwrite one another. `queue.write_buffer` calls are all applied on
        // the queue timeline *before* the pass executes, so writing every batch
        // at offset 0 would make every draw read only the last batch's data.
        let end = self.frame_instance_offset + instances.len();
        if end
            > self
                .instance_policy
                .capacity()
        {
            // Fallback safety net: `begin_frame` should have sized the buffer for
            // the whole frame, but if it was not called, grow without dropping
            // already-written data by copying nothing (prior draws keep the old
            // buffer alive via the encoder) and restarting the offset.
            self.instance_policy
                .grow_to_fit(end);
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("image instance buffer (resized)"),
                size: (self
                    .instance_policy
                    .capacity()
                    * size_of::<ImageInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.frame_instance_offset = 0;
        }

        let byte_offset = (self.frame_instance_offset * size_of::<ImageInstance>()) as u64;

        // Viewport uniform is written once in `begin_frame` — no per-batch write
        // needed.
        queue.write_buffer(&self.instance_buffer, byte_offset, bytemuck::cast_slice(instances));

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.viewport_bind_group, &[]);
        pass.set_bind_group(1, &entry.bind_group, &[]);
        pass.set_vertex_buffer(
            0,
            self.instance_buffer
                .slice(byte_offset..),
        );
        pass.draw(0..6, 0..instances.len() as u32);

        self.frame_instance_offset = end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_images_allocate_only_the_base_mip_level() {
        assert_eq!(image_mip_level_count(), 1);
    }

    #[test]
    fn sustained_low_usage_reclaims_peak_instance_capacity() {
        let mut policy = InstanceBufferPolicy::new(64);
        policy.record_usage(4096);
        assert_eq!(policy.capacity(), 4096);

        for _ in 0..InstanceBufferPolicy::SHRINK_AFTER_FRAMES {
            policy.record_usage(32);
        }

        assert_eq!(policy.capacity(), 64);
    }

    #[test]
    fn image_instance_carries_draw_opacity() {
        let instance = ImageInstance {
            position: [0.0; 2],
            size: [1.0; 2],
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
            clip_rect: [-1.0; 4],
            clip_border_radius: [0.0; 4],
            alpha: 0.35,
        };

        assert_eq!(instance.alpha, 0.35);
    }

    #[test]
    fn texture_size_preserves_aspect_ratio_when_width_exceeds_limit() {
        assert_eq!(constrained_texture_size(2170, 1085, 2048), (2048, 1024));
    }

    #[test]
    fn texture_size_preserves_aspect_ratio_when_height_exceeds_limit() {
        assert_eq!(constrained_texture_size(1085, 2170, 2048), (1024, 2048));
    }

    #[test]
    fn texture_size_keeps_dimensions_within_limit() {
        assert_eq!(constrained_texture_size(2048, 1024, 2048), (2048, 1024));
    }

    #[test]
    fn oversized_rgba8_data_is_resized_to_the_constrained_dimensions() {
        let data = vec![255; 4 * 10 * 5];
        let (width, height, resized) = constrain_rgba8(10, 5, &data, 4);

        assert_eq!((width, height), (4, 2));
        assert_eq!(resized.len(), 4 * 4 * 2);
    }

    #[test]
    fn integer_nearest_resize_maps_destination_pixels_to_source_pixels() {
        let data = vec![1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255, 4, 0, 0, 255];

        assert_eq!(resize_rgba8_nearest(4, 1, &data, 2, 1), vec![1, 0, 0, 255, 3, 0, 0, 255],);
        assert_eq!(resize_rgba8_nearest(1, 4, &data, 1, 2), vec![1, 0, 0, 255, 3, 0, 0, 255],);
    }

    #[test]
    fn malformed_rgba8_data_uses_a_transparent_placeholder() {
        let (width, height, data) = constrain_rgba8(10, 5, &[255; 4], 4);

        assert_eq!((width, height), (1, 1));
        assert_eq!(data.as_ref(), &[0; 4]);
    }
}
