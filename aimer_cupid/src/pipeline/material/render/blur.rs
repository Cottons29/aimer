//! Reused horizontal Gaussian intermediate for plain backdrop filters.

use super::*;
use crate::backend::*;

pub(super) struct BackdropBlur<B: GpuBackend> {
    pipeline: B::RenderPipeline,
    _texture: B::Texture,
    pub(super) view: B::TextureView,
    bind_group: B::BindGroup,
    uniform_buffer: B::Buffer,
    stride: u64,
    capacity: usize,
    width: u32,
    height: u32,
    format: B::TextureFormat,
    upload: Vec<u8>,
}

impl<B: GpuBackend> BackdropBlur<B> {
    pub(super) fn new(parent: &MaterialPipeline<B>, backend: &B) -> Self {
        let stride = parent.uniform_stride;
        let capacity = INITIAL_REQUEST_CAPACITY;
        let uniform_buffer = create_uniform_buffer_generic(backend, stride, capacity);
        let texture = create_texture(backend, parent.backdrop_format, parent.backdrop_width, parent.backdrop_height);
        let view = backend.create_texture_view(&texture, "backdrop horizontal blur view");
        let bind_group = create_bind_group(backend, &parent.bind_group_layout, &uniform_buffer,
            stride, &parent._backdrop_view, &parent.backdrop_sampler, &parent._backdrop_view);
        Self {
            pipeline: create_material_pipeline(backend, &parent.bind_group_layout, parent.backdrop_format,
                crate::AntiAlias::Analytic, true),
            _texture: texture, view, bind_group, uniform_buffer, stride, capacity,
            width: parent.backdrop_width, height: parent.backdrop_height, format: parent.backdrop_format,
            upload: Vec::new(),
        }
    }

    pub(super) fn prepare(&mut self, parent: &MaterialPipeline<B>, backend: &B, capture_changed: bool) -> bool {
        let texture_changed = self.width != parent.backdrop_width || self.height != parent.backdrop_height
            || self.format != parent.backdrop_format;
        if texture_changed {
            if self.format != parent.backdrop_format {
                self.pipeline = create_material_pipeline(backend, &parent.bind_group_layout,
                    parent.backdrop_format, crate::AntiAlias::Analytic, true);
            }
            self._texture = create_texture(backend, parent.backdrop_format, parent.backdrop_width, parent.backdrop_height);
            self.view = backend.create_texture_view(&self._texture, "backdrop horizontal blur view");
            self.width = parent.backdrop_width;
            self.height = parent.backdrop_height;
            self.format = parent.backdrop_format;
        }
        let buffer_changed = parent.requests.len() > self.capacity;
        if buffer_changed {
            self.capacity = parent.requests.len().next_power_of_two().min(MAX_REQUESTS_PER_FRAME);
            self.uniform_buffer = create_uniform_buffer_generic(backend, self.stride, self.capacity);
        }
        if capture_changed || buffer_changed {
            self.bind_group = create_bind_group(backend, &parent.bind_group_layout, &self.uniform_buffer,
                self.stride, &parent._backdrop_view, &parent.backdrop_sampler, &parent._backdrop_view);
        }
        let stride = self.stride as usize;
        self.upload.resize(stride * parent.requests.len(), 0);
        for (index, request) in parent.requests.iter().copied().enumerate() {
            let Some(region) = parent.backdrop_regions[index] else { continue };
            if request.kind != MaterialKind::BackdropFilter || request.blur_radius <= 0.0 { continue }
            // The intermediate uses capture-local device coordinates. The final
            // vertical pass keeps the original frame coordinates and clipping.
            let mut uniform = MaterialUniform::from_request(request, self.width, self.height, false, Some(region));
            uniform.bounds = [0.0, 0.0, region.width as f32, region.height as f32];
            uniform.backdrop_rect[0] = 0.0;
            uniform.backdrop_rect[1] = 0.0;
            // Negative kind is private to the horizontal pass, never a wire material.
            uniform.effect[0] = -1.0;
            let start = index * stride;
            self.upload[start..start + size_of::<MaterialUniform>()].copy_from_slice(bytemuck::bytes_of(&uniform));
        }
        backend.write_buffer(&self.uniform_buffer, 0, &self.upload);
        texture_changed
    }

    pub(super) fn encode(&self, index: usize, backend: &B, encoder: &mut B::CommandEncoder) {
        let mut pass = backend.begin_render_pass(encoder, &RenderPassDescriptor {
            label: None,
            color_attachments: &[RenderPassColorAttachment {
                view: &self.view, resolve_target: None,
                ops: Operations { load: LoadOp::Clear([0.0; 4]), store: StoreOp::Store },
            }],
            depth_stencil_attachment: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[(index as u32) * self.stride as u32]);
        pass.draw(0..6, 0..1);
    }
}

fn create_texture<B: GpuBackend>(backend: &B, format: B::TextureFormat, width: u32, height: u32) -> B::Texture {
    backend.create_texture(&TextureDescriptor {
        label: Some("backdrop horizontal blur".to_string()), size: (width, height, 1),
        mip_level_count: 1, sample_count: 1, dimension: TextureDimension::D2, format,
        usage: vec![TextureUsage::TextureBinding, TextureUsage::RenderAttachment],
    })
}

#[cfg(all(test, feature = "wgpu"))]
mod tests {
    use super::*;
    use aimer_utils::SyncFuture;
    use crate::custom_pipeline::{CustomPipelineGeneric, RenderContextGeneric};

    #[test]
    fn steady_blur_frames_reuse_gpu_resources_and_upload_storage() {
        let instance = ::wgpu::Instance::default();
        let adapter = instance.request_adapter(&Default::default()).block().expect("GPU adapter for blur resource validation");
        let (device, queue) = adapter.request_device(&Default::default()).block().expect("GPU device for blur resource validation");
        let backend = crate::WgpuBackend::new(device, queue);
        let format = ::wgpu::TextureFormat::Rgba8Unorm;
        let source = create_texture(&backend, format, 640, 400);
        let ctx = RenderContextGeneric {
            backend: &backend, width: 640, height: 400, is_srgb: false, format,
            sample_count: 1, source_texture: Some(&source),
        };
        let mut parent = MaterialPipeline::new(&backend, format, crate::AntiAlias::Analytic);
        let request = MaterialRequest::new(MaterialKind::BackdropFilter, [160.0, 92.0, 320.0, 208.0]);
        parent.prepare_command(&request);
        parent.prepare(&ctx);
        let blur = parent.blur.as_ref().unwrap();
        let original = (blur._texture.clone(), blur.uniform_buffer.clone(), blur.bind_group.clone(),
            blur.upload.as_ptr(), parent.bind_group.clone(), parent.uniform_buffer.clone());
        for _ in 0..3 {
            parent.begin_frame();
            parent.prepare_command(&request);
            parent.prepare(&ctx);
            let blur = parent.blur.as_ref().unwrap();
            assert_eq!(blur._texture, original.0);
            assert_eq!(blur.uniform_buffer, original.1);
            assert_eq!(blur.bind_group, original.2);
            assert_eq!(blur.upload.as_ptr(), original.3);
            assert_eq!(parent.bind_group, original.4);
            assert_eq!(parent.uniform_buffer, original.5);
        }
    }
}
