use std::{any::Any, mem::size_of};

#[cfg(feature = "wgpu")]
use std::num::NonZeroU64;

use bytemuck::{Pod, Zeroable};

#[cfg(feature = "wgpu")]
use crate::custom_pipeline::{CustomPipeline, RenderContext};

use super::{
    MaterialKind, MaterialRequest, MaterialRenderPath, MAX_INTERMEDIATE_DIMENSION,
    MAX_INTERMEDIATE_PIXELS, MATERIAL_PIPELINE_NAME, plan_material,
};

#[cfg(feature = "wgpu")]
use super::{MaterialShader, MaterialStagePlan};

const INITIAL_REQUEST_CAPACITY: usize = 16;
const MAX_REQUESTS_PER_FRAME: usize = 512;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct MaterialUniform {
    bounds: [f32; 4],
    tint: [f32; 4],
    border_color: [f32; 4],
    shadow: [f32; 4],
    effect: [f32; 4],
    light: [f32; 4],
    detail: [f32; 4],
    radii: [f32; 4],
    clip_rect: [f32; 4],
    clip_radii: [f32; 4],
    viewport: [f32; 4],
    backdrop_rect: [f32; 4],
    // x: blob amount, y: blob seed, z: magnification, w: tip pull
    liquid: [f32; 4],
    // x: chromatic aberration, y: bevel radius, z/w: reserved
    liquid2: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
struct BackdropRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl MaterialUniform {
    fn from_request(
        request: MaterialRequest,
        width: u32,
        height: u32,
        is_srgb: bool,
        backdrop_region: Option<BackdropRegion>,
    ) -> Self {
        let request = request.normalized();
        let (clip_rect, clip_radii) = request
            .clip
            .map(|clip| (clip.rect, clip.corner_radii))
            .unwrap_or(([0.0, 0.0, -1.0, 0.0], [0.0; 4]));
        Self {
            bounds: request.bounds,
            tint: request.tint,
            border_color: request.border_color,
            shadow: [
                request.shadow_color[0],
                request.shadow_color[1],
                request.shadow_color[2],
                request.shadow_color[3],
            ],
            effect: [
                request.kind as u8 as f32,
                request.opacity,
                request.effective_phase(),
                request.distortion_strength,
            ],
            light: [
                request.saturation,
                request.brightness,
                request.contrast,
                request.edge_lighting,
            ],
            detail: [
                request.specular_highlight,
                request.interaction,
                request.blur_radius,
                request.border_width,
            ],
            radii: request.corner_radii,
            clip_rect,
            clip_radii,
            // `w` tells the shader whether the bound texture contains the
            // current frame or only the neutral fallback texel.
            viewport: [
                width as f32,
                height as f32,
                if is_srgb { 1.0 } else { 0.0 },
                if backdrop_region.is_some() { 1.0 } else { 0.0 },
            ],
            backdrop_rect: backdrop_region.map_or([0.0; 4], |region| {
                [
                    region.x as f32,
                    region.y as f32,
                    region.width as f32,
                    region.height as f32,
                ]
            }),
            liquid: [
                request.blob_amount,
                request.blob_seed,
                request.magnification,
                request.tip_pull,
            ],
            liquid2: [request.chromatic_aberration, request.bevel_radius, 0.0, 0.0],
        }
    }
}

/// Cupid-owned procedural material renderer.
///
/// The stage captures only the material's expanded bounds at each ordered
/// material boundary, then samples that bounded texture for frosted Glass blur
/// or Liquid refraction. This works for both analytic and multisampled
/// rendering. An unsupported or over-budget request keeps a small neutral
/// texture bound and retains the analytic surface treatment.
#[cfg(feature = "pluggable-backend-exp")]
pub struct MaterialPipeline<B: crate::backend::GpuBackend = crate::backend::DefaultGpuBackend> {
    pipeline: B::RenderPipeline,
    bind_group_layout: B::BindGroupLayout,
    bind_group: B::BindGroup,
    uniform_buffer: B::Buffer,
    uniform_stride: u64,
    uniform_capacity: usize,
    backdrop_texture: B::Texture,
    _backdrop_view: B::TextureView,
    backdrop_sampler: B::Sampler,
    backdrop_width: u32,
    backdrop_height: u32,
    backdrop_format: B::TextureFormat,
    backdrop_available: bool,
    requests: Vec<MaterialRequest>,
    backdrop_regions: Vec<Option<BackdropRegion>>,
    upload: Vec<u8>,
}

#[cfg(not(feature = "pluggable-backend-exp"))]
#[cfg(feature = "wgpu")]
pub struct MaterialPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    uniform_stride: u64,
    uniform_capacity: usize,
    backdrop_texture: wgpu::Texture,
    _backdrop_view: wgpu::TextureView,
    backdrop_sampler: wgpu::Sampler,
    backdrop_width: u32,
    backdrop_height: u32,
    backdrop_format: wgpu::TextureFormat,
    backdrop_available: bool,
    requests: Vec<MaterialRequest>,
    backdrop_regions: Vec<Option<BackdropRegion>>,
    upload: Vec<u8>,
}

#[cfg(feature = "wgpu")]
impl MaterialPipeline {
    /// Creates the material pipeline using the renderer's target format and
    /// antialiasing sample count.
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        pipeline_cache: Option<&wgpu::PipelineCache>,
        antialiasing: crate::AntiAlias,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("material shader"),
            source: wgpu::ShaderSource::Wgsl(MaterialShader::source().into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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

        let backdrop_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("material neutral backdrop"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let backdrop_view = backdrop_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let backdrop_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material backdrop sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let uniform_stride = aligned_uniform_stride(device);
        let uniform_capacity = INITIAL_REQUEST_CAPACITY;
        let uniform_buffer = create_uniform_buffer(device, uniform_stride, uniform_capacity);
        let bind_group = create_bind_group(
            device,
            &bind_group_layout,
            &uniform_buffer,
            uniform_stride,
            &backdrop_view,
            &backdrop_sampler,
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("material pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("material pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
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

        Self {
            pipeline,
            bind_group_layout,
            bind_group,
            uniform_buffer,
            uniform_stride,
            uniform_capacity,
            backdrop_texture,
            _backdrop_view: backdrop_view,
            backdrop_sampler,
            backdrop_width: 1,
            backdrop_height: 1,
            backdrop_format: wgpu::TextureFormat::Rgba8Unorm,
            backdrop_available: false,
            requests: Vec::with_capacity(INITIAL_REQUEST_CAPACITY),
            backdrop_regions: Vec::with_capacity(INITIAL_REQUEST_CAPACITY),
            upload: Vec::new(),
        }
    }

    fn ensure_uniform_capacity(&mut self, device: &wgpu::Device, required: usize) {
        if required <= self.uniform_capacity {
            return;
        }
        let capacity = required
            .next_power_of_two()
            .min(MAX_REQUESTS_PER_FRAME);
        self.uniform_capacity = capacity;
        self.uniform_buffer = create_uniform_buffer(device, self.uniform_stride, capacity);
        self.bind_group = create_bind_group(
            device,
            &self.bind_group_layout,
            &self.uniform_buffer,
            self.uniform_stride,
            &self._backdrop_view,
            &self.backdrop_sampler,
        );
    }

    fn ensure_backdrop_texture(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        available: bool,
    ) {
        let (width, height, format) = if available {
            (width.max(1), height.max(1), format)
        } else {
            (1, 1, wgpu::TextureFormat::Rgba8Unorm)
        };
        if self.backdrop_width == width
            && self.backdrop_height == height
            && self.backdrop_format == format
        {
            return;
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(if available {
                "material captured backdrop"
            } else {
                "material neutral backdrop"
            }),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = create_bind_group(
            device,
            &self.bind_group_layout,
            &self.uniform_buffer,
            self.uniform_stride,
            &view,
            &self.backdrop_sampler,
        );
        self.backdrop_texture = texture;
        self._backdrop_view = view;
        self.backdrop_width = width;
        self.backdrop_height = height;
        self.backdrop_format = format;
    }

    fn request_from_payload(data: &(dyn Any + Send)) -> Option<MaterialRequest> {
        data.downcast_ref::<MaterialRequest>()
            .copied()
            .or_else(|| {
                data.downcast_ref::<Vec<u8>>()
                    .and_then(|bytes| MaterialRequest::decode(bytes).ok())
            })
    }
}

#[cfg(feature = "wgpu")]
impl CustomPipeline for MaterialPipeline {
    fn name(&self) -> &str {
        MATERIAL_PIPELINE_NAME
    }

    fn prepare(&mut self, ctx: &RenderContext) {
        if self.requests.is_empty() {
            return;
        }
        self.backdrop_regions.extend(self.requests.iter().copied().map(|request| {
            ctx.source_texture
                .and_then(|_| backdrop_region(request, ctx.width, ctx.height))
        }));
        self.backdrop_available = self.backdrop_regions.iter().any(Option::is_some);
        let backdrop_width = self.backdrop_regions
            .iter()
            .flatten()
            .map(|region| region.width)
            .max()
            .unwrap_or(1);
        let backdrop_height = self.backdrop_regions
            .iter()
            .flatten()
            .map(|region| region.height)
            .max()
            .unwrap_or(1);
        self.ensure_backdrop_texture(
            ctx.device,
            ctx.format,
            backdrop_width,
            backdrop_height,
            self.backdrop_available,
        );
        self.ensure_uniform_capacity(ctx.device, self.requests.len());

        let stride = self.uniform_stride as usize;
        self.upload
            .resize(stride.saturating_mul(self.requests.len()), 0);
        for (index, request) in self.requests.iter().copied().enumerate() {
            let uniform = MaterialUniform::from_request(
                request,
                ctx.width,
                ctx.height,
                ctx.is_srgb,
                self.backdrop_regions[index],
            );
            let start = index * stride;
            let end = start + size_of::<MaterialUniform>();
            self.upload[start..end].copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        ctx.queue.write_buffer(&self.uniform_buffer, 0, &self.upload);

        if !self.backdrop_available {
            // Keep the placeholder texture initialized even on backends that
            // do not guarantee zeroed newly-created texture memory.
            ctx.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.backdrop_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &[18, 28, 44, 255],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4),
                    rows_per_image: Some(1),
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    fn begin_frame(&mut self) {
        self.requests.clear();
        self.backdrop_regions.clear();
        self.upload.clear();
    }

    fn prepare_command(&mut self, data: &(dyn Any + Send)) -> Option<usize> {
        let request = Self::request_from_payload(data)?;
        let plan: MaterialStagePlan = plan_material(request, Default::default());
        if plan.path != MaterialRenderPath::Gpu || self.requests.len() >= MAX_REQUESTS_PER_FRAME {
            return None;
        }
        let index = self.requests.len();
        self.requests.push(request);
        Some(index)
    }

    fn render<'pass>(&'pass self, _pass: &mut wgpu::RenderPass<'pass>) {}

    fn render_command<'pass>(
        &'pass self,
        command_index: Option<usize>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        let Some(index) = command_index else {
            return;
        };
        if index >= self.requests.len() {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[index as u32 * self.uniform_stride as u32]);
        pass.draw(0..6, 0..1);
    }

    fn needs_backdrop(&self, command_index: Option<usize>) -> bool {
        self.backdrop_available
            && command_index.is_some_and(|index| {
                self.backdrop_regions.get(index).is_some_and(Option::is_some)
            })
    }

    fn capture_backdrop_command(
        &self,
        command_index: Option<usize>,
        encoder: &mut wgpu::CommandEncoder,
        source_texture: &wgpu::Texture,
        _width: u32,
        _height: u32,
    ) {
        let Some(region) = command_index
            .and_then(|index| self.backdrop_regions.get(index))
            .copied()
            .flatten()
        else {
            return;
        };
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: region.x,
                    y: region.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.backdrop_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: region.width,
                height: region.height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn has_work(&self) -> bool {
        !self.requests.is_empty()
    }
}

// ── Backend-agnostic generic path (pluggable-backend-exp) ───────────────────
//
// When `pluggable-backend-exp` is enabled, the pipeline can be constructed and
// driven through the [`GpuBackend`] trait using `WgpuBackend`.

#[cfg(feature = "pluggable-backend-exp")]
impl<B: crate::backend::GpuBackend> MaterialPipeline<B> {
    /// Create the pipeline through the [`GpuBackend`] trait via
    /// [`WgpuBackend`](crate::backend::wgpu::WgpuBackend).
    ///
    /// Equivalent to [`MaterialPipeline::new`] but routes all GPU resource
    /// creation through the backend trait instead of calling wgpu directly.
    pub fn new_generic(
        backend: &B,
        format: B::TextureFormat,
        antialiasing: crate::AntiAlias,
    ) -> Self {
        use crate::backend::*;

        let shader = backend.create_shader_module(
            backend.builtin_shader_source(BuiltinShader::Material),
            "material shader",
        );

        let bind_group_layout = backend.create_bind_group_layout(&[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: vec![ShaderStage::Vertex, ShaderStage::Fragment],
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: vec![ShaderStage::Fragment],
                ty: BindingType::Texture {
                    multisampled: false,
                    view_dimension: TextureViewDimension::D2,
                    sample_type: TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: vec![ShaderStage::Fragment],
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
        ]);

        let backdrop_texture = backend.create_texture(&TextureDescriptor {
            label: Some("material neutral backdrop".to_string()),
            size: (1, 1, 1),
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: B::rgba8_unorm_format(),
            usage: vec![TextureUsage::TextureBinding, TextureUsage::CopyDst],
        });
        let backdrop_view =
            backend.create_texture_view(&backdrop_texture, "material neutral backdrop view");
        let backdrop_sampler = backend.create_sampler(&SamplerDescriptor {
            label: Some("material backdrop sampler".to_string()),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: f32::MAX,
            compare: None,
            max_anisotropy: 1,
        });

        let uniform_stride = aligned_uniform_stride_generic(backend);
        let uniform_capacity = INITIAL_REQUEST_CAPACITY;
        let uniform_buffer =
            create_uniform_buffer_generic(backend, uniform_stride, uniform_capacity);
        let bind_group = create_bind_group_generic(
            backend,
            &bind_group_layout,
            &uniform_buffer,
            uniform_stride,
            &backdrop_view,
            &backdrop_sampler,
        );

        let pipeline_layout = backend.create_pipeline_layout(&[&bind_group_layout]);
        let pipeline = backend.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("material pipeline".to_string()),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(ColorTargetState {
                    format,
                    blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: ColorWriteMask::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: crate::pipeline::multisample_state_generic(antialiasing),
        });

        Self {
            pipeline,
            bind_group_layout,
            bind_group,
            uniform_buffer,
            uniform_stride,
            uniform_capacity,
            backdrop_texture,
            _backdrop_view: backdrop_view,
            backdrop_sampler,
            backdrop_width: 1,
            backdrop_height: 1,
            backdrop_format: B::rgba8_unorm_format(),
            backdrop_available: false,
            requests: Vec::with_capacity(INITIAL_REQUEST_CAPACITY),
            backdrop_regions: Vec::with_capacity(INITIAL_REQUEST_CAPACITY),
            upload: Vec::new(),
        }
    }

    /// Backend-driven equivalent of [`ensure_backdrop_texture`].
    fn ensure_backdrop_texture_generic(
        &mut self,
        backend: &B,
        format: B::TextureFormat,
        width: u32,
        height: u32,
        available: bool,
    ) {
        use crate::backend::*;

        let (width, height, format) = if available {
            (width.max(1), height.max(1), format)
        } else {
            (1, 1, B::rgba8_unorm_format())
        };
        if self.backdrop_width == width
            && self.backdrop_height == height
            && self.backdrop_format == format
        {
            return;
        }

        let texture = backend.create_texture(&TextureDescriptor {
            label: Some(if available {
                "material captured backdrop"
            } else {
                "material neutral backdrop"
            }.to_string()),
            size: (width, height, 1),
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: vec![TextureUsage::TextureBinding, TextureUsage::CopyDst],
        });
        let view = backend.create_texture_view(&texture, "material backdrop view");
        self.bind_group = create_bind_group_generic(
            backend,
            &self.bind_group_layout,
            &self.uniform_buffer,
            self.uniform_stride,
            &view,
            &self.backdrop_sampler,
        );
        self.backdrop_texture = texture;
        self._backdrop_view = view;
        self.backdrop_width = width;
        self.backdrop_height = height;
        self.backdrop_format = format;
    }

    /// Backend-driven equivalent of [`ensure_uniform_capacity`].
    fn ensure_uniform_capacity_generic(
        &mut self,
        backend: &B,
        required: usize,
    ) {
        if required <= self.uniform_capacity {
            return;
        }
        let capacity = required
            .next_power_of_two()
            .min(MAX_REQUESTS_PER_FRAME);
        self.uniform_capacity = capacity;
        self.uniform_buffer =
            create_uniform_buffer_generic(backend, self.uniform_stride, capacity);
        self.bind_group = create_bind_group_generic(
            backend,
            &self.bind_group_layout,
            &self.uniform_buffer,
            self.uniform_stride,
            &self._backdrop_view,
            &self.backdrop_sampler,
        );
    }

    /// Equivalent to [`CustomPipeline::prepare`] but routes GPU operations
    /// through the [`GpuBackend`] trait.
    pub fn prepare_generic(
        &mut self,
        ctx: &crate::custom_pipeline::RenderContextGeneric<'_, B>,
    ) {
        use crate::backend::GpuBackend;

        if self.requests.is_empty() {
            return;
        }
        self.backdrop_regions
            .extend(self.requests.iter().copied().map(|request| {
                ctx.source_texture
                    .and_then(|_| backdrop_region(request, ctx.width, ctx.height))
            }));
        self.backdrop_available = self.backdrop_regions.iter().any(Option::is_some);
        let backdrop_width = self
            .backdrop_regions
            .iter()
            .flatten()
            .map(|region| region.width)
            .max()
            .unwrap_or(1);
        let backdrop_height = self
            .backdrop_regions
            .iter()
            .flatten()
            .map(|region| region.height)
            .max()
            .unwrap_or(1);
        self.ensure_backdrop_texture_generic(
            ctx.backend,
            ctx.format,
            backdrop_width,
            backdrop_height,
            self.backdrop_available,
        );
        self.ensure_uniform_capacity_generic(ctx.backend, self.requests.len());

        let stride = self.uniform_stride as usize;
        self.upload
            .resize(stride.saturating_mul(self.requests.len()), 0);
        for (index, request) in self.requests.iter().copied().enumerate() {
            let uniform = MaterialUniform::from_request(
                request,
                ctx.width,
                ctx.height,
                ctx.is_srgb,
                self.backdrop_regions[index],
            );
            let start = index * stride;
            let end = start + size_of::<MaterialUniform>();
            self.upload[start..end]
                .copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        ctx.backend
            .write_buffer(&self.uniform_buffer, 0, &self.upload);

        if !self.backdrop_available {
            ctx.backend.write_texture(
                &crate::backend::WriteTextureDescriptor {
                    texture: &self.backdrop_texture,
                    mip_level: 0,
                    origin: crate::backend::Origin3d::ZERO,
                    aspect: crate::backend::TextureAspect::All,
                    data: &[18, 28, 44, 255],
                    buffer_layout: crate::backend::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4),
                        rows_per_image: Some(1),
                    },
                    extent: crate::backend::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                },
            );
        }
    }

    /// Backend-driven equivalent of [`CustomPipeline::capture_backdrop_command`].
    ///
    /// Routes the texture-to-texture copy through the backend trait so it stays
    /// compatible with the generic path.
    pub fn capture_backdrop_command_generic(
        &self,
        command_index: Option<usize>,
        backend: &B,
        encoder: &mut B::CommandEncoder,
        source_texture: &B::Texture,
        _width: u32,
        _height: u32,
    ) {
        use crate::backend::{GpuBackend, TexelCopyTextureInfo, Extent3d, Origin3d, TextureAspect};
        let Some(region) = command_index
            .and_then(|index| self.backdrop_regions.get(index))
            .copied()
            .flatten()
        else {
            return;
        };
        backend.copy_texture_to_texture(
            encoder,
            &TexelCopyTextureInfo {
                texture: source_texture,
                mip_level: 0,
                origin: Origin3d {
                    x: region.x,
                    y: region.y,
                    z: 0,
                },
                aspect: TextureAspect::All,
            },
            &TexelCopyTextureInfo {
                texture: &self.backdrop_texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            Extent3d {
                width: region.width,
                height: region.height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn request_from_payload_generic(data: &(dyn Any + Send)) -> Option<MaterialRequest> {
        data.downcast_ref::<MaterialRequest>()
            .copied()
            .or_else(|| {
                data.downcast_ref::<Vec<u8>>()
                    .and_then(|bytes| MaterialRequest::decode(bytes).ok())
            })
    }

    fn render_command_generic<'pass>(
        &'pass self,
        command_index: Option<usize>,
        pass: &mut B::RenderPass<'pass>,
    ) where
        B::RenderPass<'pass>: crate::backend::GpuRenderPass<B>,
    {
        use crate::backend::GpuRenderPass;
        let Some(index) = command_index.filter(|index| *index < self.requests.len()) else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(
            0,
            &self.bind_group,
            &[(index as u32).saturating_mul(self.uniform_stride as u32)],
        );
        pass.draw(0..6, 0..1);
    }
}

#[cfg(feature = "pluggable-backend-exp")]
impl<B: crate::backend::GpuBackend> crate::custom_pipeline::CustomPipelineGeneric<B>
    for MaterialPipeline<B>
{
    fn name(&self) -> &str {
        MATERIAL_PIPELINE_NAME
    }

    fn prepare(&mut self, ctx: &crate::custom_pipeline::RenderContextGeneric<'_, B>) {
        self.prepare_generic(ctx);
    }

    fn begin_frame(&mut self) {
        self.requests.clear();
        self.backdrop_regions.clear();
        self.upload.clear();
    }

    fn prepare_command(&mut self, data: &(dyn Any + Send)) -> Option<usize> {
        let request = Self::request_from_payload_generic(data)?;
        if plan_material(request, Default::default()).path != MaterialRenderPath::Gpu
            || self.requests.len() >= MAX_REQUESTS_PER_FRAME
        {
            return None;
        }
        let index = self.requests.len();
        self.requests.push(request);
        Some(index)
    }

    fn render<'pass>(&'pass self, _pass: &mut B::RenderPass<'pass>) {}

    fn render_command<'pass>(
        &'pass self,
        command_index: Option<usize>,
        pass: &mut B::RenderPass<'pass>,
    ) {
        self.render_command_generic(command_index, pass);
    }

    fn needs_backdrop(&self, command_index: Option<usize>) -> bool {
        self.backdrop_available
            && command_index.is_some_and(|index| {
                self.backdrop_regions.get(index).is_some_and(Option::is_some)
            })
    }

    fn capture_backdrop_command(
        &self,
        command_index: Option<usize>,
        backend: &B,
        encoder: &mut B::CommandEncoder,
        source_texture: &B::Texture,
        width: u32,
        height: u32,
    ) {
        self.capture_backdrop_command_generic(
            command_index,
            backend,
            encoder,
            source_texture,
            width,
            height,
        );
    }

    fn has_work(&self) -> bool {
        !self.requests.is_empty()
    }
}

#[cfg(feature = "wgpu")]
fn aligned_uniform_stride(device: &wgpu::Device) -> u64 {
    let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment.max(1));
    let size = size_of::<MaterialUniform>() as u64;
    size.div_ceil(alignment) * alignment
}

fn backdrop_fits_budget(width: u32, height: u32) -> bool {
    width > 0
        && height > 0
        && width <= MAX_INTERMEDIATE_DIMENSION
        && height <= MAX_INTERMEDIATE_DIMENSION
        && u64::from(width)
            .checked_mul(u64::from(height))
            .is_some_and(|pixels| pixels <= MAX_INTERMEDIATE_PIXELS)
}

fn backdrop_region(
    request: MaterialRequest,
    viewport_width: u32,
    viewport_height: u32,
) -> Option<BackdropRegion> {
    if viewport_width == 0 || viewport_height == 0 {
        return None;
    }
    let request = request.normalized();
    let blur_expansion = request.blur_radius.ceil().max(1.0);
    // Keep the ripple term in sync with the summed amplitudes in
    // `liquid_warp`: x = 0.006 + 0.004, y = 0.0055 + 0.0035. The bevel term
    // covers `liquid_bevel_refract_offset`, whose displacement is bounded by
    // roughly its fake depth (`bevel_radius * (1 + magnification)`),
    // strongest at the steepest part of the bevel; the margin here is
    // generous on purpose so refracted samples never clamp against the
    // capture edge, and an unreasonably large combination simply falls back
    // through the existing intermediate-budget check below.
    let (expansion_x, expansion_y) = if request.kind == MaterialKind::Liquid {
        let ripple_x = (request.bounds[2] * 0.010 * request.distortion_strength).ceil();
        let ripple_y = (request.bounds[3] * 0.009 * request.distortion_strength).ceil();
        let bevel_expansion =
            (request.bevel_radius * (1.0 + request.magnification) * 2.5).ceil();
        (
            blur_expansion.max(ripple_x).max(bevel_expansion),
            blur_expansion.max(ripple_y).max(bevel_expansion),
        )
    } else {
        (blur_expansion, blur_expansion)
    };
    let left = (request.bounds[0] - expansion_x)
        .floor()
        .clamp(0.0, viewport_width as f32) as u32;
    let top = (request.bounds[1] - expansion_y)
        .floor()
        .clamp(0.0, viewport_height as f32) as u32;
    let right = (request.bounds[0] + request.bounds[2] + expansion_x)
        .ceil()
        .clamp(0.0, viewport_width as f32) as u32;
    let bottom = (request.bounds[1] + request.bounds[3] + expansion_y)
        .ceil()
        .clamp(0.0, viewport_height as f32) as u32;
    let width = right.saturating_sub(left);
    let height = bottom.saturating_sub(top);
    backdrop_fits_budget(width, height).then_some(BackdropRegion {
        x: left,
        y: top,
        width,
        height,
    })
}

#[cfg(feature = "wgpu")]
fn create_uniform_buffer(device: &wgpu::Device, stride: u64, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("material uniform buffer"),
        size: stride * capacity as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(feature = "wgpu")]
fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buffer: &wgpu::Buffer,
    uniform_stride: u64,
    backdrop_view: &wgpu::TextureView,
    backdrop_sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                // The binding is one dynamic slot, not the whole backing
                // buffer.  This lets later requests advance the dynamic
                // offset without overrunning the bound range.
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: uniform_buffer,
                    offset: 0,
                    size: NonZeroU64::new(uniform_stride),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(backdrop_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(backdrop_sampler),
            },
        ],
    })
}

// ── Generic helper functions (pluggable-backend-exp) ────────────────────────

#[cfg(feature = "pluggable-backend-exp")]
fn aligned_uniform_stride_generic<B: crate::backend::GpuBackend>(backend: &B) -> u64 {
    use crate::backend::GpuBackend;
    let alignment =
        u64::from(backend.limits().min_uniform_buffer_offset_alignment.max(1));
    let size = size_of::<MaterialUniform>() as u64;
    size.div_ceil(alignment) * alignment
}

#[cfg(feature = "pluggable-backend-exp")]
fn create_uniform_buffer_generic<B: crate::backend::GpuBackend>(
    backend: &B,
    stride: u64,
    capacity: usize,
) -> B::Buffer {
    use crate::backend::GpuBackend;
    use crate::backend::*;
    backend.create_buffer(&BufferDescriptor {
        label: Some("material uniform buffer".to_string()),
        size: stride * capacity as u64,
        usage: vec![BufferUsage::Uniform, BufferUsage::CopyDst],
    })
}

#[cfg(feature = "pluggable-backend-exp")]
fn create_bind_group_generic<B: crate::backend::GpuBackend>(
    backend: &B,
    layout: &B::BindGroupLayout,
    uniform_buffer: &B::Buffer,
    uniform_stride: u64,
    backdrop_view: &B::TextureView,
    backdrop_sampler: &B::Sampler,
) -> B::BindGroup
where
    B: crate::backend::GpuBackend,
{
    use crate::backend::*;
    backend.create_bind_group(
        layout,
        &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::BufferRange(uniform_buffer, 0, uniform_stride),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::TextureView(backdrop_view),
            },
            BindGroupEntry {
                binding: 2,
                resource: BindingResource::Sampler(backdrop_sampler),
            },
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::{backdrop_fits_budget, backdrop_region, MaterialUniform};
    use crate::pipeline::material::{MaterialKind, MaterialRequest};

    #[test]
    fn uniform_layout_fits_one_aligned_dynamic_slot() {
        assert!(size_of::<MaterialUniform>() <= 256);
        assert_eq!(size_of::<MaterialUniform>() % 16, 0);
    }

    #[test]
    fn backdrop_capture_obeys_the_frame_budget() {
        assert!(backdrop_fits_budget(1_024, 1_024));
        assert!(backdrop_fits_budget(2_048, 2_048));
        assert!(!backdrop_fits_budget(2_048, 2_049));
        assert!(!backdrop_fits_budget(2_049, 1));
    }

    #[test]
    fn backdrop_capture_is_local_to_the_material_on_a_large_viewport() {
        let request = MaterialRequest::new(MaterialKind::Glass, [32.0, 16.0, 64.0, 32.0]);
        let region = backdrop_region(request, 4_096, 2_160).expect("local capture region");

        assert_eq!((region.x, region.y), (16, 0));
        assert_eq!((region.width, region.height), (96, 64));
    }

    #[test]
    fn liquid_capture_includes_the_bounded_refraction_envelope() {
        let mut request =
            MaterialRequest::new(MaterialKind::Liquid, [100.0, 100.0, 1_000.0, 1_000.0]);
        request.blur_radius = 0.0;
        request.distortion_strength = 1.0;
        request.magnification = 0.5;
        request.bevel_radius = 40.0;
        let region = backdrop_region(request, 2_000, 2_000).expect("liquid capture region");

        // The bevel term (`40 * 1.5 * 2.5 = 150`) dominates the ripple term
        // (`1_000 * 0.010 = 10`), so the captured region grows to cover the
        // bevel refraction's reach rather than clamping its samples.
        assert_eq!((region.x, region.y), (0, 0));
        assert_eq!((region.width, region.height), (1_250, 1_250));
    }
}
