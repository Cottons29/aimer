use std::collections::HashMap;
use std::sync::Arc;

#[cfg(all(target_os = "windows", feature = "dx12"))]
use std::time::Instant;

#[cfg(all(target_os = "windows", feature = "dx12"))]
use aimer_utils::info;

use crate::backend::{
    GpuBackend, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, StoreOp,
};
use crate::custom_pipeline::{CustomPipelineGeneric, CustomPipelineSlotGeneric, RenderContextGeneric};
use crate::draw_cmd::{DrawCommand, DrawList, RetainedLayerContent};
use crate::pipeline::frame_composite::FrameCompositePipeline;
use crate::pipeline::image_pipeline::{ImageInstance, ImagePipeline};
use crate::pipeline::material::{MaterialPipeline, MATERIAL_PIPELINE_NAME};
use crate::pipeline::rect_pipeline::{RectInstance, RectPipeline};
use crate::pipeline::svg_pipeline::SvgPipeline;
use crate::pipeline::text_pipeline::{
    RichTextSpan, TextDecorationDraw, TextDrawRequest, TextPipelineV2,
    text_layout::{TextHorizontalAlign, TextWritingMode},
};
use crate::persistent_target::{
    PersistentTargetGeneric, PersistentTargetKey, TargetEnsureResult, TargetValidity,
};
use crate::utilities::{Mat3, Rect, Rgba8, TextureId};

use super::{
    AlphaState, ClipState, SvgRenderItem, apply_alpha, build_fill_rect_instances,
    clip_border_radius, clip_to_array, packed_color, resolve_material_request, resolve_svg_item,
    retained_layer_dimensions, transform_scales, transform_text_shadow,
};

const RETAINED_LAYER_CACHE_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
const RETAINED_LAYER_IDLE_FRAMES: u64 = 120;

#[cfg(all(target_os = "windows", feature = "dx12"))]
macro_rules! dx12_stage_start {
    ($enabled:expr) => {
        $enabled.then(Instant::now)
    };
}

#[cfg(not(all(target_os = "windows", feature = "dx12")))]
macro_rules! dx12_stage_start {
    ($enabled:expr) => {{
        let _ = $enabled;
        ()
    }};
}

#[cfg(all(target_os = "windows", feature = "dx12"))]
macro_rules! dx12_stage_finish {
    ($started:expr, $label:literal) => {{
        if let Some(started) = $started {
            info!(
                "DX12 renderer first-frame stage: {} {:.2} ms (CPU elapsed)",
                $label,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
    }};
}

#[cfg(not(all(target_os = "windows", feature = "dx12")))]
macro_rules! dx12_stage_finish {
    ($started:expr, $label:literal) => {{
        let _ = $started;
    }};
}

/// Backend-generic renderer for the experimental pluggable backend path.
pub struct RendererImpl<B: GpuBackend = crate::backend::DefaultGpuBackend> {
    rect_pipeline: RectPipeline<B>,
    image_pipeline: Option<ImagePipeline<B>>,
    text_pipeline: Option<TextPipelineV2<B>>,
    svg_pipeline: Option<SvgPipeline<B>>,
    frame_composite_pipeline: Option<FrameCompositePipeline<B>>,
    material_target: PersistentTargetGeneric<B>,
    material_composite_bind_group: Option<B::BindGroup>,
    custom_pipelines: Vec<CustomPipelineSlotGeneric<B>>,
    antialiasing: crate::AntiAlias,
    material_pipeline_enabled: bool,
    material_pipeline_initialized: bool,
    format: B::TextureFormat,
    resolved: Vec<ResolvedCommand<B>>,
    text_requests: Vec<TextDrawRequest>,
    decoration_requests: Vec<TextDecorationDraw>,
    svg_items: Vec<SvgRenderItem>,
    deferred_outlines: Vec<RectInstance>,
    textures_to_remove: Vec<TextureId>,
    image_batch: Vec<ImageInstance>,
    transform_stack: Vec<Mat3>,
    clip_stack: Vec<ClipState>,
    retained_layers: HashMap<u64, RetainedLayerGeneric<B>>,
    frame_index: u64,
    #[cfg(all(target_os = "windows", feature = "dx12"))]
    first_render_timing_enabled: bool,
}

enum ResolvedCommand<B: GpuBackend> {
    Rect { instance: RectInstance, clear: bool },
    Image { texture_id: TextureId, instance: ImageInstance },
    RetainedLayer { bind_group: B::BindGroup, instance: ImageInstance },
    Text(usize),
    TextDecoration { start: usize, end: usize },
    Svg(usize),
    Custom { pipeline_index: usize, command_index: Option<usize> },
}

struct RetainedLayerGeneric<B: GpuBackend> {
    content: Arc<RetainedLayerContent>,
    target: PersistentTargetGeneric<B>,
    bind_group: Option<B::BindGroup>,
    width: u32,
    height: u32,
    bytes: u64,
    last_used_frame: u64,
    is_srgb: bool,
}

impl<B: GpuBackend> RendererImpl<B> {
    /// Creates the built-in pipelines using the selected backend.
    ///
    /// The material pipeline is created on the first material draw.
    pub fn new(backend: &B, format: B::TextureFormat) -> Self {
        Self::with_antialiasing(backend, format, crate::AntiAlias::default())
    }

    /// Creates the built-in pipelines using the selected backend and antialiasing mode.
    ///
    /// The material pipeline is created on the first material draw.
    pub fn with_antialiasing(
        backend: &B,
        format: B::TextureFormat,
        antialiasing: crate::AntiAlias,
    ) -> Self {
        Self::build(backend, format, antialiasing, true)
    }

    /// Creates a renderer with only the rectangle pipeline initialized.
    ///
    /// This supports staged backend bring-up or callers that only need
    /// rectangles. Draw commands requiring another pipeline will fail with a
    /// clear initialization message.
    pub fn new_rect_only(backend: &B, format: B::TextureFormat) -> Self {
        Self::build(backend, format, crate::AntiAlias::default(), false)
    }

    fn build(
        backend: &B,
        format: B::TextureFormat,
        antialiasing: crate::AntiAlias,
        initialize_all: bool,
    ) -> Self {
        let rect_pipeline = RectPipeline::<B>::new_generic(backend, format, antialiasing);
        let image_pipeline = initialize_all
            .then(|| ImagePipeline::<B>::new_generic(backend, format, antialiasing));
        let text_pipeline = initialize_all
            .then(|| TextPipelineV2::<B>::new_generic(backend, format, antialiasing));
        let svg_pipeline =
            initialize_all.then(|| SvgPipeline::<B>::new_generic(backend, format, antialiasing));
        let frame_composite_pipeline =
            initialize_all.then(|| FrameCompositePipeline::<B>::new_generic(backend, format));
        Self {
            rect_pipeline,
            image_pipeline,
            text_pipeline,
            svg_pipeline,
            frame_composite_pipeline,
            material_target: PersistentTargetGeneric::default(),
            material_composite_bind_group: None,
            custom_pipelines: Vec::new(),
            antialiasing,
            material_pipeline_enabled: initialize_all,
            material_pipeline_initialized: false,
            format,
            resolved: Vec::new(),
            text_requests: Vec::new(),
            decoration_requests: Vec::new(),
            svg_items: Vec::new(),
            deferred_outlines: Vec::new(),
            textures_to_remove: Vec::new(),
            image_batch: Vec::new(),
            transform_stack: Vec::new(),
            clip_stack: Vec::new(),
            retained_layers: HashMap::new(),
            frame_index: 0,
            #[cfg(all(target_os = "windows", feature = "dx12"))]
            first_render_timing_enabled: false,
        }
    }

    /// Enables a one-shot breakdown of the next DX12 renderer frame.
    #[cfg(all(target_os = "windows", feature = "dx12"))]
    #[doc(hidden)]
    pub fn enable_first_render_timing(&mut self) {
        self.first_render_timing_enabled = true;
    }

    /// Returns the render-target format selected at construction.
    #[inline]
    pub fn surface_format(&self) -> B::TextureFormat {
        self.format
    }

    /// Registers a backend-generic custom pipeline.
    pub fn register_custom_pipeline(&mut self, pipeline: impl CustomPipelineGeneric<B>) {
        self.custom_pipelines
            .push(CustomPipelineSlotGeneric::new(pipeline));
    }

    /// Renders a draw list to a texture view, clearing it before drawing.
    pub fn render(
        &mut self,
        backend: &B,
        view: &B::TextureView,
        width: u32,
        height: u32,
        is_srgb: bool,
        draw_list: &DrawList,
    ) {
        self.render_with_source_texture(backend, view, None, width, height, is_srgb, draw_list);
    }

    /// Renders with an optional copyable source texture for backdrop effects.
    /// When a material command has no source texture, the renderer uses a
    /// retained offscreen target and composites its result to `view`.
    pub fn render_with_source_texture(
        &mut self,
        backend: &B,
        view: &B::TextureView,
        source_texture: Option<&B::Texture>,
        width: u32,
        height: u32,
        is_srgb: bool,
        draw_list: &DrawList,
    ) {
        if width == 0 || height == 0 {
            return;
        }
        #[cfg(all(target_os = "windows", feature = "dx12"))]
        let startup_timing = std::mem::replace(&mut self.first_render_timing_enabled, false);
        #[cfg(not(all(target_os = "windows", feature = "dx12")))]
        let startup_timing = false;
        self.frame_index = self.frame_index.wrapping_add(1);

        let retained_started = dx12_stage_start!(startup_timing);
        self.prepare_retained_layers(backend, draw_list, is_srgb);
        dx12_stage_finish!(retained_started, "retained layer preparation");

        if source_texture.is_none() && draw_list_uses_material(draw_list) {
            let key = PersistentTargetKey::new(
                width,
                height,
                1.0,
                0,
                0,
                0,
                TargetValidity::Valid,
            );
            let material_target_started = dx12_stage_start!(startup_timing);
            let result = self.material_target.ensure(backend, self.format, key);
            if matches!(result, TargetEnsureResult::Created | TargetEnsureResult::Recreated) {
                let target_view = self
                    .material_target
                    .view()
                    .expect("ensured material target has a texture view");
                self.material_composite_bind_group = Some(
                    self.frame_composite_pipeline
                        .as_ref()
                        .expect("material rendering requires the frame composite pipeline")
                        .create_bind_group_generic(backend, target_view),
                );
            }
            dx12_stage_finish!(material_target_started, "material target setup");
            if let (Some(target_view), Some(target_texture)) = (
                self.material_target.view().cloned(),
                self.material_target.texture().cloned(),
            ) {
                let contents_started = dx12_stage_start!(startup_timing);
                self.render_contents(
                    backend,
                    &target_view,
                    Some(&target_texture),
                    width,
                    height,
                    is_srgb,
                    draw_list,
                    startup_timing,
                );
                dx12_stage_finish!(contents_started, "render contents total");
                self.material_target.mark_valid();
                let composition_started = dx12_stage_start!(startup_timing);
                self.composite_material_target(backend, view);
                dx12_stage_finish!(composition_started, "material target composition");
                let reclaim_started = dx12_stage_start!(startup_timing);
                self.reclaim_retained_layers();
                dx12_stage_finish!(reclaim_started, "retained layer reclamation");
                return;
            }
        }

        let contents_started = dx12_stage_start!(startup_timing);
        self.render_contents(
            backend,
            view,
            source_texture,
            width,
            height,
            is_srgb,
            draw_list,
            startup_timing,
        );
        dx12_stage_finish!(contents_started, "render contents total");
        let reclaim_started = dx12_stage_start!(startup_timing);
        self.reclaim_retained_layers();
        dx12_stage_finish!(reclaim_started, "retained layer reclamation");
    }

    fn composite_material_target(&self, backend: &B, view: &B::TextureView) {
        let Some(bind_group) = self.material_composite_bind_group.as_ref() else {
            return;
        };
        let mut encoder = backend.create_command_encoder("cupid generic frame composite");
        {
            let attachments = [RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear([0.0; 4]),
                    store: StoreOp::Store,
                },
            }];
            let mut pass = backend.begin_render_pass(
                &mut encoder,
                &RenderPassDescriptor {
                    label: Some("cupid generic frame composite pass".to_string()),
                    color_attachments: &attachments,
                    depth_stencil_attachment: None,
                },
            );
            self.frame_composite_pipeline
                .as_ref()
                .expect("material rendering requires the frame composite pipeline")
                .render_generic(&mut pass, bind_group);
        }
        backend.submit(encoder);
    }

    fn render_contents(
        &mut self,
        backend: &B,
        view: &B::TextureView,
        source_texture: Option<&B::Texture>,
        width: u32,
        height: u32,
        is_srgb: bool,
        draw_list: &DrawList,
        startup_timing: bool,
    ) {
        let collect_started = dx12_stage_start!(startup_timing);
        self.collect_commands(backend, draw_list, width, height, startup_timing);
        dx12_stage_finish!(collect_started, "draw-list command collection");

        let custom_started = dx12_stage_start!(startup_timing);
        for slot in &mut self.custom_pipelines {
            if slot.pipeline.has_work() {
                slot.pipeline.prepare(&RenderContextGeneric {
                    backend,
                    width,
                    height,
                    is_srgb,
                    format: self.format,
                    sample_count: 1,
                    source_texture,
                });
            }
        }
        dx12_stage_finish!(custom_started, "custom pipeline preparation");

        let text_started = dx12_stage_start!(startup_timing);
        if !self.text_requests.is_empty() || !self.decoration_requests.is_empty() {
            self.text_pipeline
                .as_mut()
                .expect("text commands require the text pipeline")
                .prepare_generic(
                    backend,
                    width,
                    height,
                    is_srgb,
                    &self.text_requests,
                    &self.decoration_requests,
                );
        }
        dx12_stage_finish!(text_started, "text shaping, glyph rasterization, and upload");

        let svg_started = dx12_stage_start!(startup_timing);
        if let Some(svg_pipeline) = self.svg_pipeline.as_mut() {
            svg_pipeline.prepare_generic(backend, &self.svg_items, width, height, is_srgb);
        } else if !self.svg_items.is_empty() {
            panic!("SVG commands require the SVG pipeline");
        }
        dx12_stage_finish!(svg_started, "SVG preparation");

        let batch_started = dx12_stage_start!(startup_timing);
        let mut total_rects = 0;
        let mut total_images = 0;
        for command in &self.resolved {
            match command {
                ResolvedCommand::Rect { .. } => total_rects += 1,
                ResolvedCommand::Image { .. } | ResolvedCommand::RetainedLayer { .. } => {
                    total_images += 1
                }
                _ => {}
            }
        }
        self.rect_pipeline
            .begin_frame_generic(backend, total_rects, width, height, is_srgb);
        if let Some(image_pipeline) = self.image_pipeline.as_mut() {
            image_pipeline.begin_frame_generic(backend, total_images, width, height, is_srgb);
        } else if total_images != 0 {
            panic!("image commands require the image pipeline");
        }
        dx12_stage_finish!(batch_started, "rect and image batch setup");

        let encode_started = dx12_stage_start!(startup_timing);
        let mut encoder = backend.create_command_encoder("cupid generic render encoder");
        let mut start = 0;
        let mut first_pass = true;
        for index in 0..self.resolved.len() {
            let ResolvedCommand::Custom {
                pipeline_index,
                command_index,
            } = &self.resolved[index]
            else {
                continue;
            };
            let pipeline_index = *pipeline_index;
            let command_index = *command_index;
            let needs_backdrop = self
                .custom_pipelines
                .get(pipeline_index)
                .is_some_and(|slot| slot.pipeline.needs_backdrop(command_index));
            if !needs_backdrop {
                continue;
            }
            self.render_resolved_range(
                backend,
                &mut encoder,
                view,
                start..index,
                if first_pass {
                    LoadOp::Clear([0.0; 4])
                } else {
                    LoadOp::Load
                },
            );
            first_pass = false;
            if let (Some(source_texture), Some(slot)) = (
                source_texture,
                self.custom_pipelines.get(pipeline_index),
            ) {
                slot.pipeline.capture_backdrop_command(
                    command_index,
                    backend,
                    &mut encoder,
                    source_texture,
                    width,
                    height,
                );
            }
            self.render_resolved_range(
                backend,
                &mut encoder,
                view,
                index..index + 1,
                LoadOp::Load,
            );
            start = index + 1;
        }
        if first_pass || start < self.resolved.len() {
            self.render_resolved_range(
                backend,
                &mut encoder,
                view,
                start..self.resolved.len(),
                if first_pass {
                    LoadOp::Clear([0.0; 4])
                } else {
                    LoadOp::Load
                },
            );
        }

        self.rect_pipeline.end_frame_generic(backend);
        if let Some(image_pipeline) = self.image_pipeline.as_mut() {
            image_pipeline.end_frame_generic(backend);
        }
        backend.submit(encoder);
        dx12_stage_finish!(encode_started, "render command encoding and submission");

        let cleanup_started = dx12_stage_start!(startup_timing);
        for texture_id in self.textures_to_remove.drain(..) {
            if let Some(image_pipeline) = self.image_pipeline.as_mut() {
                image_pipeline.remove_texture(texture_id);
            }
        }
        if let Some(image_pipeline) = self.image_pipeline.as_mut() {
            for texture_id in image_pipeline.eviction_candidates() {
                if !draw_list.has_live_texture_reference(texture_id) {
                    let serial = draw_list.texture_registry_serial(texture_id);
                    image_pipeline.remove_texture(texture_id);
                    if let Some(serial) = serial {
                        draw_list.mark_texture_evicted(texture_id, serial);
                    }
                }
            }
        }
        dx12_stage_finish!(cleanup_started, "post-submit texture cleanup");
    }

    fn render_resolved_range(
        &mut self,
        backend: &B,
        encoder: &mut B::CommandEncoder,
        view: &B::TextureView,
        range: std::ops::Range<usize>,
        load: LoadOp<[f64; 4]>,
    ) {
        let attachments = [RenderPassColorAttachment {
            view,
            resolve_target: None,
            ops: Operations {
                load,
                store: StoreOp::Store,
            },
        }];
        let mut pass = backend.begin_render_pass(
            encoder,
            &RenderPassDescriptor {
                label: Some("cupid generic render pass".to_string()),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
            },
        );
        self.draw_resolved_range(backend, &mut pass, range);
    }

    fn draw_resolved_range<'pass>(
        &'pass mut self,
        backend: &B,
        pass: &mut B::RenderPass<'pass>,
        range: std::ops::Range<usize>,
    ) {
        let Self {
            rect_pipeline,
            image_pipeline,
            text_pipeline,
            svg_pipeline,
            custom_pipelines,
            resolved,
            image_batch,
            ..
        } = self;
        let mut current_texture_id = None;
        image_batch.clear();
        for index in range {
            match &resolved[index] {
                ResolvedCommand::Rect { instance, clear } => {
                    flush_image_batch(
                        backend,
                        pass,
                        image_pipeline.as_mut(),
                        image_batch,
                        &mut current_texture_id,
                    );
                    if *clear {
                        rect_pipeline.flush_generic(pass);
                        rect_pipeline.push(*instance);
                        rect_pipeline.flush_clear_generic(pass);
                    } else {
                        rect_pipeline.push(*instance);
                    }
                }
                ResolvedCommand::Image { texture_id, instance } => {
                    rect_pipeline.flush_generic(pass);
                    if current_texture_id.is_some_and(|current| current != *texture_id) {
                        flush_image_batch(
                            backend,
                            pass,
                            image_pipeline.as_mut(),
                            image_batch,
                            &mut current_texture_id,
                        );
                    }
                    current_texture_id = Some(*texture_id);
                    image_batch.push(*instance);
                }
                ResolvedCommand::RetainedLayer { bind_group, instance } => {
                    rect_pipeline.flush_generic(pass);
                    flush_image_batch(
                        backend,
                        pass,
                        image_pipeline.as_mut(),
                        image_batch,
                        &mut current_texture_id,
                    );
                    image_pipeline
                        .as_mut()
                        .expect("retained layers require the image pipeline")
                        .draw_external_batch_generic(
                        backend,
                        pass,
                        bind_group,
                        std::slice::from_ref(instance),
                    );
                }
                ResolvedCommand::Text(text_index) => {
                    rect_pipeline.flush_generic(pass);
                    flush_image_batch(
                        backend,
                        pass,
                        image_pipeline.as_mut(),
                        image_batch,
                        &mut current_texture_id,
                    );
                    text_pipeline
                        .as_ref()
                        .expect("text commands require the text pipeline")
                        .render_request_generic(pass, *text_index);
                }
                ResolvedCommand::TextDecoration { start, end } => {
                    rect_pipeline.flush_generic(pass);
                    flush_image_batch(
                        backend,
                        pass,
                        image_pipeline.as_mut(),
                        image_batch,
                        &mut current_texture_id,
                    );
                    text_pipeline
                        .as_ref()
                        .expect("text decorations require the text pipeline")
                        .render_decoration_range_generic(pass, *start, *end);
                }
                ResolvedCommand::Svg(svg_index) => {
                    rect_pipeline.flush_generic(pass);
                    flush_image_batch(
                        backend,
                        pass,
                        image_pipeline.as_mut(),
                        image_batch,
                        &mut current_texture_id,
                    );
                    svg_pipeline
                        .as_ref()
                        .expect("SVG commands require the SVG pipeline")
                        .draw_item_generic(pass, *svg_index);
                }
                ResolvedCommand::Custom {
                    pipeline_index,
                    command_index,
                } => {
                    rect_pipeline.flush_generic(pass);
                    flush_image_batch(
                        backend,
                        pass,
                        image_pipeline.as_mut(),
                        image_batch,
                        &mut current_texture_id,
                    );
                    if let Some(slot) = custom_pipelines.get(*pipeline_index) {
                        slot.pipeline.render_command(*command_index, pass);
                    }
                }
            }
        }
        flush_image_batch(
            backend,
            pass,
            image_pipeline.as_mut(),
            image_batch,
            &mut current_texture_id,
        );
        rect_pipeline.flush_generic(pass);
    }

    fn collect_commands(
        &mut self,
        backend: &B,
        draw_list: &DrawList,
        width: u32,
        height: u32,
        startup_timing: bool,
    ) {
        self.resolved.clear();
        self.text_requests.clear();
        self.decoration_requests.clear();
        self.svg_items.clear();
        self.deferred_outlines.clear();
        self.textures_to_remove.clear();
        self.transform_stack.clear();
        self.clip_stack.clear();
        for slot in &mut self.custom_pipelines {
            slot.pipeline.begin_frame();
        }

        let mut current_transform = Mat3::identity();
        let mut current_scales = (1.0, 1.0);
        let mut current_italic = false;
        let mut current_language = None;
        let mut alpha_state = AlphaState::new();
        for command in draw_list.commands() {
            match command {
                DrawCommand::PushTransform { matrix } => {
                    self.transform_stack.push(current_transform);
                    alpha_state.save();
                    current_transform = matrix.pixel_aligned();
                    current_scales = transform_scales(&current_transform);
                }
                DrawCommand::PopTransform => {
                    if let Some(previous) = self.transform_stack.pop() {
                        current_transform = previous;
                        current_scales = transform_scales(&current_transform);
                    }
                    alpha_state.restore();
                }
                DrawCommand::PushClip { rect, border_radius } => {
                    let (x1, y1) = current_transform.transform_point(rect.x, rect.y);
                    let (x2, y2) = current_transform
                        .transform_point(rect.x + rect.width, rect.y + rect.height);
                    let (sx, _) = current_scales;
                    let next = Rect::new(
                        x1.min(x2),
                        y1.min(y2),
                        (x2 - x1).abs(),
                        (y2 - y1).abs(),
                    );
                    let effective = self.clip_stack.last().map_or(next, |parent| {
                        let left = next.x.max(parent.rect.x);
                        let top = next.y.max(parent.rect.y);
                        let right = (next.x + next.width)
                            .min(parent.rect.x + parent.rect.width);
                        let bottom = (next.y + next.height)
                            .min(parent.rect.y + parent.rect.height);
                        Rect::new(left, top, (right - left).max(0.0), (bottom - top).max(0.0))
                    });
                    let mut radii = *border_radius;
                    for radius in &mut radii {
                        *radius *= sx;
                    }
                    self.clip_stack.push(ClipState {
                        rect: effective,
                        border_radius: radii,
                    });
                }
                DrawCommand::PopClip => {
                    self.clip_stack.pop();
                }
                DrawCommand::FillRect {
                    rect,
                    color,
                    border_radius,
                    border_width,
                    border_color,
                    outline_width,
                    outline_color,
                } => {
                    let (main, outline) = build_fill_rect_instances(
                        *rect,
                        *color,
                        *border_radius,
                        *border_width,
                        *border_color,
                        *outline_width,
                        *outline_color,
                        &current_transform,
                        current_scales.0,
                        current_scales.1,
                        self.clip_stack.last(),
                        alpha_state.current(),
                    );
                    if let Some(outline) = outline {
                        self.deferred_outlines.push(outline);
                    }
                    self.resolved.push(ResolvedCommand::Rect {
                        instance: main,
                        clear: false,
                    });
                }
                DrawCommand::ClearRect { rect } => {
                    let (x1, y1) = current_transform.transform_point(rect.x, rect.y);
                    let (x2, y2) = current_transform
                        .transform_point(rect.x + rect.width, rect.y + rect.height);
                    self.resolved.push(ResolvedCommand::Rect {
                        instance: RectInstance {
                            position: [x1.min(x2), y1.min(y2)],
                            size: [(x2 - x1).abs(), (y2 - y1).abs()],
                            color: Rgba8::TRANSPARENT,
                            border_radius: [0.0; 4],
                            border_width: [0.0; 4],
                            border_color: Rgba8::TRANSPARENT,
                            outline_width: [0.0; 4],
                            outline_color: Rgba8::TRANSPARENT,
                            clip_rect: clip_to_array(self.clip_stack.last()),
                            clip_border_radius: clip_border_radius(self.clip_stack.last()),
                            shadow_params: [0.0; 4],
                            shadow_color: Rgba8::TRANSPARENT,
                            shadow_flags: [0.0; 4],
                        },
                        clear: true,
                    });
                }
                DrawCommand::DrawText {
                    position,
                    text,
                    font_size,
                    color,
                    bounds_width,
                    bounds_height,
                    overflow,
                    horizontal_align,
                    font_family,
                    font_style,
                    font_weight,
                    shadow,
                    draw_glyphs,
                } => {
                    let (x, y) = current_transform.transform_point(position.x, position.y);
                    let request_index = self.text_requests.len();
                    self.text_requests.push(TextDrawRequest {
                        x,
                        y,
                        text: text.clone(),
                        font_size: *font_size,
                        color: Rgba8::from_unorm(apply_alpha(
                            color.to_array(),
                            alpha_state.current(),
                        )),
                        bounds_width: bounds_width.unwrap_or(width as f32 - x),
                        bounds_height: bounds_height.unwrap_or(height as f32 - y),
                        overflow: *overflow,
                        horizontal_align: *horizontal_align,
                        writing_mode: TextWritingMode::HorizontalTb,
                        line_height: None,
                        shadow: shadow.map(|value| {
                            transform_text_shadow(value, &current_transform, alpha_state.current())
                        }),
                        draw_glyphs: *draw_glyphs,
                        font_family: *font_family,
                        font_style: *font_style,
                        font_weight: Some(*font_weight),
                        language: current_language,
                        italic: current_italic,
                        clip_rect: clip_to_array(self.clip_stack.last()),
                        clip_border_radius: clip_border_radius(self.clip_stack.last()),
                        spans: Vec::new(),
                    });
                    self.resolved.push(ResolvedCommand::Text(request_index));
                }
                DrawCommand::DrawRichText {
                    position,
                    spans,
                    font_size,
                    color,
                    bounds_width,
                    bounds_height,
                    overflow,
                } => {
                    let (x, y) = current_transform.transform_point(position.x, position.y);
                    let request_index = self.text_requests.len();
                    self.text_requests.push(TextDrawRequest {
                        x,
                        y,
                        text: spans
                            .iter()
                            .map(|span| &*span.text)
                            .collect::<String>()
                            .into(),
                        font_size: *font_size,
                        color: Rgba8::from_unorm(apply_alpha(
                            color.to_array(),
                            alpha_state.current(),
                        )),
                        bounds_width: bounds_width.unwrap_or(width as f32 - x),
                        bounds_height: bounds_height.unwrap_or(height as f32 - y),
                        overflow: *overflow,
                        horizontal_align: TextHorizontalAlign::Left,
                        writing_mode: TextWritingMode::HorizontalTb,
                        line_height: None,
                        shadow: None,
                        draw_glyphs: true,
                        font_family: crate::font::FontFamily::SANS_SERIF,
                        font_style: crate::font::FontStyle::Normal,
                        font_weight: None,
                        language: current_language,
                        italic: false,
                        clip_rect: clip_to_array(self.clip_stack.last()),
                        clip_border_radius: clip_border_radius(self.clip_stack.last()),
                        spans: spans
                            .iter()
                            .map(|span| RichTextSpan {
                                text: span.text.clone(),
                                font_size: span.font_size,
                                color: span.color.map(|color| {
                                    Rgba8::from_unorm(apply_alpha(
                                        color.to_array(),
                                        alpha_state.current(),
                                    ))
                                }),
                                font_weight: span.font_weight,
                                italic: span.italic,
                            })
                            .collect(),
                    });
                    self.resolved.push(ResolvedCommand::Text(request_index));
                }
                DrawCommand::DrawTextDecoration {
                    rect,
                    color,
                    style,
                    thickness,
                    period,
                } => {
                    let (sx, sy) = current_scales;
                    let (x1, y1) = current_transform.transform_point(rect.x, rect.y);
                    let (x2, y2) = current_transform
                        .transform_point(rect.x + rect.width, rect.y + rect.height);
                    let index = self.decoration_requests.len();
                    self.decoration_requests.push(TextDecorationDraw {
                        x: x1.min(x2),
                        y: y1.min(y2),
                        width: (x2 - x1).abs(),
                        band_height: (y2 - y1).abs(),
                        thickness: (*thickness * sy).max(1.0),
                        period: (*period * sx).max(1.0),
                        style: *style,
                        color: Rgba8::from_unorm(apply_alpha(
                            color.to_array(),
                            alpha_state.current(),
                        )),
                        clip_rect: clip_to_array(self.clip_stack.last()),
                        clip_border_radius: clip_border_radius(self.clip_stack.last()),
                    });
                    self.resolved.push(ResolvedCommand::TextDecoration {
                        start: index,
                        end: index + 1,
                    });
                }
                DrawCommand::Svg {
                    scene,
                    destination,
                    overrides,
                } => {
                    let index = self.svg_items.len();
                    self.svg_items.push(resolve_svg_item(
                        scene.clone(),
                        *destination,
                        overrides.clone(),
                        current_transform,
                        self.clip_stack.last(),
                        alpha_state.current(),
                    ));
                    self.resolved.push(ResolvedCommand::Svg(index));
                }
                DrawCommand::SetTransform { matrix } => {
                    current_transform = matrix.pixel_aligned();
                    current_scales = transform_scales(&current_transform);
                }
                DrawCommand::SetAlpha { alpha } => alpha_state.set(*alpha),
                DrawCommand::RestoreAlpha => alpha_state.set(1.0),
                DrawCommand::SetItalic { italic } => current_italic = *italic,
                DrawCommand::SetTextLanguage { language } => current_language = *language,
                DrawCommand::DrawImage { rect, texture_id } => {
                    let (x1, y1) = current_transform.transform_point(rect.x, rect.y);
                    let (x2, y2) = current_transform
                        .transform_point(rect.x + rect.width, rect.y + rect.height);
                    self.resolved.push(ResolvedCommand::Image {
                        texture_id: *texture_id,
                        instance: ImageInstance {
                            position: [x1.min(x2), y1.min(y2)],
                            size: [(x2 - x1).abs(), (y2 - y1).abs()],
                            uv_offset: [0.0, 0.0],
                            uv_scale: [1.0, 1.0],
                            clip_rect: clip_to_array(self.clip_stack.last()),
                            clip_border_radius: clip_border_radius(self.clip_stack.last()),
                            alpha: alpha_state.current(),
                            source_premultiplied: 0.0,
                        },
                    });
                }
                DrawCommand::RetainedLayer { layer_id, rect, .. } => {
                    if let Some(layer) = self.retained_layers.get(layer_id) {
                        if let Some(bind_group) = layer.bind_group.clone() {
                            let (x1, y1) = current_transform.transform_point(rect.x, rect.y);
                            let (x2, y2) = current_transform
                                .transform_point(rect.x + rect.width, rect.y + rect.height);
                            self.resolved.push(ResolvedCommand::RetainedLayer {
                                bind_group,
                                instance: ImageInstance {
                                    position: [x1.min(x2), y1.min(y2)],
                                    size: [(x2 - x1).abs(), (y2 - y1).abs()],
                                    uv_offset: [0.0, 0.0],
                                    uv_scale: [1.0, 1.0],
                                    clip_rect: clip_to_array(self.clip_stack.last()),
                                    clip_border_radius: clip_border_radius(self.clip_stack.last()),
                                    alpha: alpha_state.current(),
                                    source_premultiplied: 1.0,
                                },
                            });
                        }
                    }
                }
                DrawCommand::LoadImage {
                    bytes,
                    texture_id,
                    width,
                    height,
                } => {
                    self.image_pipeline
                        .as_mut()
                        .expect("image uploads require the image pipeline")
                        .upload_if_absent_generic(
                        backend,
                        *texture_id,
                        *width,
                        *height,
                        bytes,
                    );
                }
                DrawCommand::LoadImageWithId {
                    texture_id,
                    bytes,
                    width,
                    height,
                } => self
                    .image_pipeline
                    .as_mut()
                    .expect("image uploads require the image pipeline")
                    .upload_image_with_id_generic(
                        backend,
                        *texture_id,
                        *width,
                        *height,
                        bytes,
                    ),
                DrawCommand::RemoveTexture { texture_id } => {
                    self.textures_to_remove.push(*texture_id);
                }
                DrawCommand::DrawShadowRect {
                    rect,
                    shadow_color,
                    shadow_params,
                    border_radius,
                    inset,
                    side_params,
                } => {
                    let (sx, sy) = current_scales;
                    let expand_x = shadow_params[2] + shadow_params[3].abs() + shadow_params[0].abs();
                    let expand_y = shadow_params[2] + shadow_params[3].abs() + shadow_params[1].abs();
                    let (x1, y1) = current_transform
                        .transform_point(rect.x - expand_x, rect.y - expand_y);
                    let (x2, y2) = current_transform.transform_point(
                        rect.x + rect.width + expand_x,
                        rect.y + rect.height + expand_y,
                    );
                    let mut radii = *border_radius;
                    for radius in &mut radii {
                        *radius *= sx;
                    }
                    self.resolved.push(ResolvedCommand::Rect {
                        instance: RectInstance {
                            position: [x1.min(x2), y1.min(y2)],
                            size: [(x2 - x1).abs(), (y2 - y1).abs()],
                            color: Rgba8::TRANSPARENT,
                            border_radius: radii,
                            border_width: [0.0; 4],
                            border_color: Rgba8::TRANSPARENT,
                            outline_width: [0.0; 4],
                            outline_color: Rgba8::TRANSPARENT,
                            clip_rect: clip_to_array(self.clip_stack.last()),
                            clip_border_radius: clip_border_radius(self.clip_stack.last()),
                            shadow_params: [
                                shadow_params[0] * sx,
                                shadow_params[1] * sy,
                                shadow_params[2] * sx,
                                shadow_params[3] * sx,
                            ],
                            shadow_color: packed_color(*shadow_color, alpha_state.current()),
                            shadow_flags: [
                                if *inset { 1.0 } else { 0.0 },
                                side_params[0],
                                side_params[1],
                                side_params[2],
                            ],
                        },
                        clear: false,
                    });
                }
                DrawCommand::Custom {
                    pipeline_name,
                    data,
                } => {
                    if pipeline_name == MATERIAL_PIPELINE_NAME
                        && self.material_pipeline_enabled
                        && !self.material_pipeline_initialized
                    {
                        let material_pipeline_started = dx12_stage_start!(startup_timing);
                        let mut pipeline = MaterialPipeline::<B>::new_generic(
                            backend,
                            self.format,
                            self.antialiasing,
                        );
                        pipeline.begin_frame();
                        self.custom_pipelines
                            .insert(0, CustomPipelineSlotGeneric::new(pipeline));
                        self.material_pipeline_initialized = true;
                        dx12_stage_finish!(
                            material_pipeline_started,
                            "lazy material pipeline initialization"
                        );
                    }
                    let Some(pipeline_index) = self
                        .custom_pipelines
                        .iter()
                        .position(|slot| slot.pipeline.name() == pipeline_name.as_str())
                    else {
                        continue;
                    };
                    let command_index = if pipeline_name == MATERIAL_PIPELINE_NAME {
                        let request = data
                            .downcast_ref::<Vec<u8>>()
                            .and_then(|bytes| crate::pipeline::material::MaterialRequest::decode(bytes).ok())
                            .map(|request| {
                                resolve_material_request(
                                    request,
                                    &current_transform,
                                    current_scales,
                                    self.clip_stack.last(),
                                    alpha_state.current(),
                                )
                            });
                        if let Some(request) = request {
                            self.custom_pipelines[pipeline_index]
                                .pipeline
                                .prepare_command(&request)
                        } else {
                            self.custom_pipelines[pipeline_index]
                                .pipeline
                                .prepare_command(data.as_ref())
                        }
                    } else {
                        self.custom_pipelines[pipeline_index]
                            .pipeline
                            .prepare_command(data.as_ref())
                    };
                    self.resolved.push(ResolvedCommand::Custom {
                        pipeline_index,
                        command_index,
                    });
                }
            }
        }
        for instance in self.deferred_outlines.iter().copied() {
            self.resolved.push(ResolvedCommand::Rect {
                instance,
                clear: false,
            });
        }
    }

    fn prepare_retained_layers(&mut self, backend: &B, draw_list: &DrawList, is_srgb: bool) {
        for command in draw_list.commands() {
            let DrawCommand::RetainedLayer {
                layer_id,
                rect,
                content,
            } = command
            else {
                continue;
            };
            self.prepare_retained_layer(backend, *layer_id, *rect, content, is_srgb);
        }
    }

    fn prepare_retained_layer(
        &mut self,
        backend: &B,
        layer_id: u64,
        rect: Rect,
        content: &Arc<RetainedLayerContent>,
        is_srgb: bool,
    ) {
        use crate::backend::GpuBackend;
        let Some((width, height)) =
            retained_layer_dimensions(rect, backend.limits().max_texture_dimension_2d)
        else {
            return;
        };
        let entry = self.retained_layers.entry(layer_id).or_insert_with(|| {
            RetainedLayerGeneric {
                content: content.clone(),
                target: PersistentTargetGeneric::default(),
                bind_group: None,
                width,
                height,
                bytes: width as u64 * height as u64 * 4,
                last_used_frame: self.frame_index,
                is_srgb,
            }
        });
        if entry.width != width || entry.height != height || entry.is_srgb != is_srgb {
            entry.width = width;
            entry.height = height;
            entry.bytes = width as u64 * height as u64 * 4;
            entry.is_srgb = is_srgb;
            entry.target.invalidate();
        }
        if !Arc::ptr_eq(&entry.content, content) {
            entry.content = content.clone();
            entry.target.invalidate();
        }
        entry.last_used_frame = self.frame_index;
        let key = PersistentTargetKey::new(width, height, 1.0, layer_id, 0, 0, TargetValidity::Valid);
        let result = entry.target.ensure(backend, self.format, key);
        if matches!(result, TargetEnsureResult::Created | TargetEnsureResult::Recreated) {
            if let Some(target_view) = entry.target.view() {
                entry.bind_group = Some(
                    self.image_pipeline
                        .as_ref()
                        .expect("retained layers require the image pipeline")
                        .create_external_bind_group_generic(backend, target_view),
                );
            }
        }
        if matches!(result, TargetEnsureResult::ReusedValid) {
            return;
        }
        let Some(target_view) = entry.target.view().cloned() else {
            return;
        };
        let layer_draw_list = content.to_draw_list();
        self.render_with_source_texture(
            backend,
            &target_view,
            None,
            width,
            height,
            is_srgb,
            &layer_draw_list,
        );
        if let Some(entry) = self.retained_layers.get_mut(&layer_id) {
            entry.target.mark_valid();
            if entry.bind_group.is_none() {
                if let Some(target_view) = entry.target.view() {
                    entry.bind_group = Some(
                        self.image_pipeline
                            .as_ref()
                            .expect("retained layers require the image pipeline")
                            .create_external_bind_group_generic(backend, target_view),
                    );
                }
            }
        }
    }

    fn reclaim_retained_layers(&mut self) {
        let mut total_bytes = self
            .retained_layers
            .values()
            .map(|layer| layer.bytes)
            .sum::<u64>();
        let mut candidates = self
            .retained_layers
            .iter()
            .map(|(&id, layer)| (id, layer.last_used_frame, layer.bytes))
            .collect::<Vec<_>>();
        candidates.sort_unstable_by_key(|(_, last_used, _)| *last_used);
        for (id, last_used, bytes) in candidates {
            let idle = self.frame_index.saturating_sub(last_used);
            if idle < RETAINED_LAYER_IDLE_FRAMES && total_bytes <= RETAINED_LAYER_CACHE_BUDGET_BYTES {
                break;
            }
            if self.retained_layers.remove(&id).is_some() {
                total_bytes = total_bytes.saturating_sub(bytes);
            }
        }
    }
}

fn flush_image_batch<'pass, B: GpuBackend>(
    backend: &B,
    pass: &mut B::RenderPass<'pass>,
    image_pipeline: Option<&mut ImagePipeline<B>>,
    image_batch: &mut Vec<ImageInstance>,
    texture_id: &mut Option<TextureId>,
) {
    if let Some(id) = texture_id.take() {
        if !image_batch.is_empty() {
            image_pipeline
                .expect("image draws require the image pipeline")
                .draw_batch_generic(backend, pass, id, image_batch);
            image_batch.clear();
        }
    }
}

fn draw_list_uses_material(draw_list: &DrawList) -> bool {
    draw_list.commands().iter().any(|command| {
        matches!(command, DrawCommand::Custom { pipeline_name, .. } if pipeline_name == MATERIAL_PIPELINE_NAME)
    })
}
