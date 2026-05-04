use crate::custom_pipeline::{CustomPipeline, CustomPipelineSlot, RenderContext};
use crate::draw_cmd::{DrawCommand, DrawList};
use crate::image_pipeline::{ImageInstance, ImagePipeline};
use crate::pipeline_cache;
use crate::rect_pipeline::{RectInstance, RectPipeline};
use crate::text_pipeline::{TextDrawRequest, TextPipelineV2};
use crate::utilities::{Mat3, Rect};
use aimer_utils::debug;

struct ClipState {
    rect: Rect,
    border_radius: [f32; 4],
}

fn clip_to_array(clip: Option<&ClipState>) -> [f32; 4] {
    clip.map(|c| [c.rect.x, c.rect.y, c.rect.width, c.rect.height])
        .unwrap_or([0.0, 0.0, -1.0, 0.0])
}

fn clip_border_radius(clip: Option<&ClipState>) -> [f32; 4] {
    clip.map(|c| c.border_radius).unwrap_or([0.0; 4])
}

struct ResolvedCmd {
    kind: ResolvedKind,
}

enum ResolvedKind {
    Rect(RectInstance),
    Image { texture_id: u32, instance: ImageInstance },
    TextIndex(()),
    Custom { pipeline_index: usize },
}

pub struct Renderer {
    pub rect_pipeline: RectPipeline,
    pub text_pipeline: TextPipelineV2,
    pub image_pipeline: ImagePipeline,
    pipeline_cache: Option<wgpu::PipelineCache>,
    custom_pipelines: Vec<CustomPipelineSlot>,
    surface_format: wgpu::TextureFormat,
    // Reusable per-frame scratch buffers to avoid allocations.
    transform_stack: Vec<Mat3>,
    clip_stack: Vec<ClipState>,
    text_requests: Vec<TextDrawRequest>,
    resolved: Vec<ResolvedCmd>,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let start = chrono::Utc::now().timestamp_millis();

        let cache = pipeline_cache::create_pipeline_cache(device);

        let renderer = Self {
            rect_pipeline: RectPipeline::new(device, format, cache.as_ref()),
            text_pipeline: TextPipelineV2::new(device, format, cache.as_ref()),
            image_pipeline: ImagePipeline::new(device, format, cache.as_ref()),
            pipeline_cache: cache,
            custom_pipelines: Vec::new(),
            surface_format: format,
            transform_stack: Vec::new(),
            clip_stack: Vec::new(),
            text_requests: Vec::new(),
            resolved: Vec::new(),
        };

        let end = chrono::Utc::now().timestamp_millis();
        debug!("Renderer initialization ready {}ms", end - start);
        renderer
    }

    /// Register a user-defined custom pipeline.
    /// The pipeline will participate in the render loop whenever
    /// `DrawCommand::Custom` commands target it by name.
    pub fn register_custom_pipeline(&mut self, pipeline: impl CustomPipeline) {
        self.custom_pipelines.push(CustomPipelineSlot::new(pipeline));
    }

    /// Returns the surface texture format (useful for creating custom pipelines).
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    /// Save the pipeline cache to disk for faster startup on next launch.
    /// Called automatically on drop, or can be called manually on suspend.
    pub fn save_pipeline_cache(&self) {
        if let Some(ref cache) = self.pipeline_cache {
            pipeline_cache::save_pipeline_cache(cache);
        }
    }

    /// Process a DrawList into pipeline-specific batches and render in a single pass.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        is_srgb: bool,
        draw_list: &DrawList,
    ) {
        self.transform_stack.clear();
        let mut current_transform = Mat3::identity();
        self.clip_stack.clear();
        self.text_requests.clear();
        self.resolved.clear();

        for cmd in draw_list.commands() {
            match cmd {
                DrawCommand::PushTransform { matrix } => {
                    self.transform_stack.push(current_transform);
                    current_transform = *matrix;
                }
                DrawCommand::PopTransform => {
                    if let Some(prev) = self.transform_stack.pop() {
                        current_transform = prev;
                    }
                }
                DrawCommand::PushClip { rect, border_radius } => {
                    let (p1x, p1y) = current_transform.transform_point(rect.x, rect.y);
                    let (p2x, p2y) = current_transform.transform_point(rect.x + rect.width, rect.y + rect.height);
                    let sx = (current_transform.cols[0][0].powi(2) + current_transform.cols[0][1].powi(2)).sqrt();

                    let new_rect = Rect::new(p1x.min(p2x), p1y.min(p2y), (p2x - p1x).abs(), (p2y - p1y).abs());

                    let effective_clip = if let Some(parent) = self.clip_stack.last() {
                        let x = new_rect.x.max(parent.rect.x);
                        let y = new_rect.y.max(parent.rect.y);
                        let r = (new_rect.x + new_rect.width).min(parent.rect.x + parent.rect.width);
                        let b = (new_rect.y + new_rect.height).min(parent.rect.y + parent.rect.height);
                        Rect::new(x, y, (r - x).max(0.0), (b - y).max(0.0))
                    } else {
                        new_rect
                    };

                    let mut scaled_br = *border_radius;
                    for r in &mut scaled_br {
                        *r *= sx;
                    }

                    self.clip_stack
                        .push(ClipState { rect: effective_clip, border_radius: scaled_br });
                }
                DrawCommand::PopClip => {
                    self.clip_stack.pop();
                }
                DrawCommand::FillRect { rect, color, border_radius, border_width, border_color, outline_width, outline_color } => {
                    // Extract scale factors from the current transform matrix
                    let sx = (current_transform.cols[0][0].powi(2) + current_transform.cols[0][1].powi(2)).sqrt();
                    let sy = (current_transform.cols[1][0].powi(2) + current_transform.cols[1][1].powi(2)).sqrt();

                    // Expand the quad by the outline width so the outline ring is visible.
                    // These are in logical pixels and must be scaled to device pixels.
                    let ol = outline_width[3]; // left
                    let or = outline_width[1]; // right
                    let ot = outline_width[0]; // top
                    let ob = outline_width[2]; // bottom

                    // Transform the top-left and bottom-right corners of the expanded quad.
                    // This correctly handles translation and scaling.
                    let (p1x, p1y) = current_transform.transform_point(rect.x - ol, rect.y - ot);
                    let (p2x, p2y) = current_transform.transform_point(rect.x + rect.width + or, rect.y + rect.height + ob);

                    // let expanded_w = p2x - p1x;
                    // let expanded_h = p2y - p1y;

                    // Scale other properties by the appropriate axis
                    let mut scaled_br = *border_radius;
                    for r in &mut scaled_br {
                        *r *= sx;
                    } // Assuming uniform scale for simplicity, or use sx

                    let mut scaled_bw = *border_width;
                    scaled_bw[0] *= sy; // top
                    scaled_bw[1] *= sx; // right
                    scaled_bw[2] *= sy; // bottom
                    scaled_bw[3] *= sx; // left

                    let mut scaled_ow = *outline_width;
                    scaled_ow[0] *= sy; // top
                    scaled_ow[1] *= sx; // right
                    scaled_ow[2] *= sy; // bottom
                    scaled_ow[3] *= sx; // left

                    self.resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Rect(RectInstance {
                            position: [p1x.min(p2x), p1y.min(p2y)],
                            size: [(p2x - p1x).abs(), (p2y - p1y).abs()],
                            color: color.to_array(),
                            border_radius: scaled_br,
                            border_width: scaled_bw,
                            border_color: border_color.to_array(),
                            outline_width: scaled_ow,
                            outline_color: outline_color.to_array(),
                            clip_rect: clip_to_array(self.clip_stack.last()),
                            clip_border_radius: clip_border_radius(self.clip_stack.last()),
                            shadow_params: [0.0; 4],
                            shadow_color: [0.0; 4],
                            shadow_flags: [0.0; 4],
                        }),
                    });
                }
                DrawCommand::ClearRect { rect } => {
                    let (p1x, p1y) = current_transform.transform_point(rect.x, rect.y);
                    let (p2x, p2y) = current_transform.transform_point(rect.x + rect.width, rect.y + rect.height);
                    self.resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Rect(RectInstance {
                            position: [p1x.min(p2x), p1y.min(p2y)],
                            size: [(p2x - p1x).abs(), (p2y - p1y).abs()],
                            color: [0.0, 0.0, 0.0, 0.0],
                            border_radius: [0.0; 4],
                            border_width: [0.0; 4],
                            border_color: [0.0; 4],
                            outline_width: [0.0; 4],
                            outline_color: [0.0; 4],
                            clip_rect: clip_to_array(self.clip_stack.last()),
                            clip_border_radius: clip_border_radius(self.clip_stack.last()),
                            shadow_params: [0.0; 4],
                            shadow_color: [0.0; 4],
                            shadow_flags: [0.0; 4],
                        }),
                    });
                }
                DrawCommand::DrawText { position, text, font_size, color } => {
                    let (tx, ty) = current_transform.transform_point(position.x, position.y);
                    let _idx = self.text_requests.len();
                    self.text_requests.push(TextDrawRequest {
                        x: tx,
                        y: ty,
                        text: text.clone(),
                        font_size: *font_size,
                        color: color.to_array(),
                        bounds_width: width as f32 - tx,
                        bounds_height: height as f32 - ty,
                        clip_rect: clip_to_array(self.clip_stack.last()),
                        clip_border_radius: clip_border_radius(self.clip_stack.last()),
                    });
                    self.resolved
                        .push(ResolvedCmd { kind: ResolvedKind::TextIndex(()) });
                }
                DrawCommand::SetTransform { matrix } => {
                    current_transform = *matrix;
                }
                DrawCommand::SetAlpha { .. } | DrawCommand::RestoreAlpha => {
                    // Alpha state is tracked at the canvas level; no GPU-side handling yet.
                }
                DrawCommand::DrawImage { rect, texture_id } => {
                    let (p1x, p1y) = current_transform.transform_point(rect.x, rect.y);
                    let (p2x, p2y) = current_transform.transform_point(rect.x + rect.width, rect.y + rect.height);
                    self.resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Image {
                            texture_id: *texture_id,
                            instance: ImageInstance {
                                position: [p1x.min(p2x), p1y.min(p2y)],
                                size: [(p2x - p1x).abs(), (p2y - p1y).abs()],
                                uv_offset: [0.0, 0.0],
                                uv_scale: [1.0, 1.0],
                                clip_rect: clip_to_array(self.clip_stack.last()),
                                clip_border_radius: clip_border_radius(self.clip_stack.last()),
                            },
                        },
                    });
                }
                DrawCommand::LoadImage { bytes, texture_id, width, height } => {
                    if !self.image_pipeline.has_texture(*texture_id) {
                        self.image_pipeline.upload_image_with_id(device, queue, *texture_id, *width, *height, &bytes);
                    }
                }
                DrawCommand::LoadImageWithId { texture_id, bytes, width, height } => {
                    self.image_pipeline.upload_image_with_id(device, queue, *texture_id, *width, *height, &bytes);
                }
                DrawCommand::DrawShadowRect { rect, shadow_color, shadow_params, border_radius, inset, side_params } => {
                    let sx = (current_transform.cols[0][0].powi(2) + current_transform.cols[0][1].powi(2)).sqrt();
                    let sy = (current_transform.cols[1][0].powi(2) + current_transform.cols[1][1].powi(2)).sqrt();

                    let offset_x = shadow_params[0];
                    let offset_y = shadow_params[1];
                    let blur = shadow_params[2];
                    let spread = shadow_params[3];

                    // Expand the rect per-axis to encompass the full shadow extent
                    let expand_x = blur + spread.abs() + offset_x.abs();
                    let expand_y = blur + spread.abs() + offset_y.abs();

                    let (p1x, p1y) = current_transform.transform_point(rect.x - expand_x, rect.y - expand_y);
                    let (p2x, p2y) = current_transform.transform_point(rect.x + rect.width + expand_x, rect.y + rect.height + expand_y);

                    let mut scaled_br = *border_radius;
                    for r in &mut scaled_br {
                        *r *= sx;
                    }

                    let scaled_params = [offset_x * sx, offset_y * sy, blur * sx, spread * sx];

                    self.resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Rect(RectInstance {
                            position: [p1x.min(p2x), p1y.min(p2y)],
                            size: [(p2x - p1x).abs(), (p2y - p1y).abs()],
                            color: [0.0, 0.0, 0.0, 0.0],
                            border_radius: scaled_br,
                            border_width: [0.0; 4],
                            border_color: [0.0; 4],
                            outline_width: [0.0; 4],
                            outline_color: [0.0; 4],
                            clip_rect: clip_to_array(self.clip_stack.last()),
                            clip_border_radius: clip_border_radius(self.clip_stack.last()),
                            shadow_params: scaled_params,
                            shadow_color: shadow_color.to_array(),
                            shadow_flags: [if *inset { 1.0 } else { 0.0 }, side_params[0], side_params[1], side_params[2]],
                        }),
                    });
                }
                DrawCommand::Custom { pipeline_name, data: _ } => {
                    if let Some(idx) = self.custom_pipelines.iter().position(|s| s.pipeline.name() == pipeline_name.as_str()) {
                        self.resolved.push(ResolvedCmd {
                            kind: ResolvedKind::Custom { pipeline_index: idx },
                        });
                    }
                }
            }
        }

        // Prepare custom pipelines
        {
            let render_ctx = RenderContext {
                device,
                queue,
                width,
                height,
                is_srgb,
                format: self.surface_format,
            };
            for slot in &mut self.custom_pipelines {
                if slot.pipeline.has_work() {
                    slot.pipeline.prepare(&render_ctx);
                }
            }
        }

        // Prepare text
        if !self.text_requests.is_empty() {
            self.text_pipeline
                .prepare(device, queue, width, height, is_srgb, &self.text_requests);
        }

        // Create encoder and render pass
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("cupid render encoder") });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cupid render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Render commands in draw order to preserve correct z-ordering
            // between rects and images. Consecutive same-type commands are batched.
            self.rect_pipeline.clear();
            let mut image_batch: Vec<ImageInstance> = Vec::new();
            let mut current_texture_id: Option<u32> = None;

            for i in 0..self.resolved.len() {
                match &self.resolved[i].kind {
                    ResolvedKind::Rect(inst) => {
                        // Flush any pending image batch before switching to rects
                        if let Some(tid) = current_texture_id.take() {
                            if !image_batch.is_empty() {
                                self.image_pipeline
                                    .draw_batch(device, queue, &mut pass, width, height, is_srgb, tid, &image_batch);
                                image_batch.clear();
                            }
                        }
                        self.rect_pipeline.push(*inst);
                    }
                    ResolvedKind::Image { texture_id, instance } => {
                        // Flush any pending rects before switching to images
                        self.rect_pipeline
                            .flush(device, queue, &mut pass, width, height, is_srgb);

                        if current_texture_id.is_some() && current_texture_id != Some(*texture_id) {
                            // Flush current image batch for previous texture
                            if let Some(tid) = current_texture_id.take() {
                                if !image_batch.is_empty() {
                                    self.image_pipeline
                                        .draw_batch(device, queue, &mut pass, width, height, is_srgb, tid, &image_batch);
                                    image_batch.clear();
                                }
                            }
                        }
                        current_texture_id = Some(*texture_id);
                        image_batch.push(*instance);
                    }
                    ResolvedKind::TextIndex(()) => {
                        // Text is rendered after all other commands
                    }
                    ResolvedKind::Custom { pipeline_index } => {
                        // Flush pending built-in batches to maintain z-order
                        self.rect_pipeline
                            .flush(device, queue, &mut pass, width, height, is_srgb);
                        if let Some(tid) = current_texture_id.take() {
                            if !image_batch.is_empty() {
                                self.image_pipeline
                                    .draw_batch(device, queue, &mut pass, width, height, is_srgb, tid, &image_batch);
                                image_batch.clear();
                            }
                        }
                        // Render the custom pipeline
                        if let Some(slot) = self.custom_pipelines.get(*pipeline_index) {
                            slot.pipeline.render(&mut pass);
                        }
                    }
                }
            }

            // Flush remaining image batch
            if let Some(tid) = current_texture_id {
                if !image_batch.is_empty() {
                    self.image_pipeline
                        .draw_batch(device, queue, &mut pass, width, height, is_srgb, tid, &image_batch);
                }
            }

            // Flush remaining rects
            self.rect_pipeline
                .flush(device, queue, &mut pass, width, height, is_srgb);

            // Render text last (including inspector overlay)
            if !self.text_requests.is_empty() {
                self.text_pipeline.render(&mut pass);
            }
        }

        queue.submit(std::iter::once(encoder.finish()));
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.save_pipeline_cache();
    }
}
