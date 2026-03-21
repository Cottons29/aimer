use std::cell::RefCell;
use std::rc::Rc;

use glyphon::FontSystem;

use crate::draw_cmd::{DrawCommand, DrawList};
use crate::image_pipeline::{ImageInstance, ImagePipeline};
use crate::rect_pipeline::{RectInstance, RectPipeline};
use crate::text_pipeline::{TextDrawRequest, TextPipeline};
use crate::utilities::{Mat3, Rect};

fn clip_to_array(clip: Option<&Rect>) -> [f32; 4] {
    clip.map(|c| [c.x, c.y, c.width, c.height]).unwrap_or([0.0, 0.0, 0.0, 0.0])
}

pub struct Renderer {
    pub rect_pipeline: RectPipeline,
    pub text_pipeline: TextPipeline,
    pub image_pipeline: ImagePipeline,
}

impl Renderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        font_system: Rc<RefCell<FontSystem>>,
    ) -> Self {
        Self {
            rect_pipeline: RectPipeline::new(device, format),
            text_pipeline: TextPipeline::new(device, queue, format, font_system),
            image_pipeline: ImagePipeline::new(device, format),
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
        draw_list: &DrawList,
    ) {
        let mut transform_stack: Vec<Mat3> = Vec::new();
        let mut current_transform = Mat3::identity();
        let mut clip_stack: Vec<Rect> = Vec::new();
        let mut text_requests: Vec<TextDrawRequest> = Vec::new();

        // Resolved commands with transforms applied
        struct ResolvedCmd {
            kind: ResolvedKind,
        }
        enum ResolvedKind {
            Rect(RectInstance),
            Image {
                texture_id: u32,
                instance: ImageInstance,
            },
            TextIndex(()),
        }

        let mut resolved: Vec<ResolvedCmd> = Vec::new();

        for cmd in draw_list.commands() {
            match cmd {
                DrawCommand::PushTransform { matrix } => {
                    transform_stack.push(current_transform);
                    current_transform = *matrix;
                }
                DrawCommand::PopTransform => {
                    if let Some(prev) = transform_stack.pop() {
                        current_transform = prev;
                    }
                }
                DrawCommand::PushClip { rect } => {
                    let (tx, ty) = current_transform.transform_point(rect.x, rect.y);
                    clip_stack.push(Rect::new(tx, ty, rect.width, rect.height));
                }
                DrawCommand::PopClip => {
                    clip_stack.pop();
                }
                DrawCommand::FillRect {
                    rect,
                    color,
                    border_radius,
                    border_width,
                    border_color,
                } => {
                    let (tx, ty) = current_transform.transform_point(rect.x, rect.y);
                    resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Rect(RectInstance {
                            position: [tx, ty],
                            size: [rect.width, rect.height],
                            color: color.to_array(),
                            border_radius: *border_radius,
                            border_width: *border_width,
                            border_color: border_color.to_array(),
                            clip_rect: clip_to_array(clip_stack.last()),
                        }),
                    });
                }
                DrawCommand::ClearRect { rect } => {
                    let (tx, ty) = current_transform.transform_point(rect.x, rect.y);
                    resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Rect(RectInstance {
                            position: [tx, ty],
                            size: [rect.width, rect.height],
                            color: [0.0, 0.0, 0.0, 0.0],
                            border_radius: 0.0,
                            border_width: 0.0,
                            border_color: [0.0; 4],
                            clip_rect: clip_to_array(clip_stack.last()),
                        }),
                    });
                }
                DrawCommand::DrawText {
                    position,
                    text,
                    font_size,
                    color,
                } => {
                    let (tx, ty) = current_transform.transform_point(position.x, position.y);
                    let _idx = text_requests.len();
                    text_requests.push(TextDrawRequest {
                        x: tx,
                        y: ty,
                        text: text.clone(),
                        font_size: *font_size,
                        color: color.to_array(),
                        bounds_width: width as f32 - tx,
                        bounds_height: height as f32 - ty,
                    });
                    resolved.push(ResolvedCmd {
                        kind: ResolvedKind::TextIndex(()),
                    });
                }
                DrawCommand::SetTransform { matrix } => {
                    current_transform = *matrix;
                }
                DrawCommand::SetAlpha { .. } | DrawCommand::RestoreAlpha => {
                    // Alpha state is tracked at the canvas level; no GPU-side handling yet.
                }
                DrawCommand::DrawImage { rect, texture_id } => {
                    let (tx, ty) = current_transform.transform_point(rect.x, rect.y);
                    resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Image {
                            texture_id: *texture_id,
                            instance: ImageInstance {
                                position: [tx, ty],
                                size: [rect.width, rect.height],
                                uv_offset: [0.0, 0.0],
                                uv_scale: [1.0, 1.0],
                                clip_rect: clip_to_array(clip_stack.last()),
                            },
                        },
                    });
                }
            }
        }

        // Prepare text
        if !text_requests.is_empty() {
            self.text_pipeline
                .prepare(device, queue, width, height, &text_requests);
        }

        // Batch rects
        self.rect_pipeline.clear();
        for rc in &resolved {
            if let ResolvedKind::Rect(inst) = &rc.kind {
                self.rect_pipeline.push(*inst);
            }
        }

        // Create encoder and render pass
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("cupid render encoder"),
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cupid render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Flush rects (AA clipping is handled per-instance in the shader)
            self.rect_pipeline
                .flush(device, queue, &mut pass, width, height);

            // Render text (glyphon handles its own bounds clipping)
            if !text_requests.is_empty() {
                self.text_pipeline.render(&mut pass);
            }

            // Render images (AA clipping is handled per-instance in the shader)
            for rc in &resolved {
                if let ResolvedKind::Image {
                    texture_id,
                    instance,
                } = &rc.kind
                {
                    self.image_pipeline.draw_image(
                        device,
                        queue,
                        &mut pass,
                        width,
                        height,
                        *texture_id,
                        *instance,
                    );
                }
            }
        }

        queue.submit(std::iter::once(encoder.finish()));
    }
}
