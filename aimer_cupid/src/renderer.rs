use std::sync::Arc;

use aimer_utils::debug;

use crate::custom_pipeline::{CustomPipeline, CustomPipelineSlot, RenderContext};
use crate::draw_cmd::{DrawCommand, DrawList};
use crate::image_pipeline::{ImageInstance, ImagePipeline};
use crate::pipeline_cache;
use crate::rect_pipeline::{RectInstance, RectPipeline};
use crate::svg::{SvgNodeStyleOverride, SvgScene};
use crate::svg_pipeline::SvgPipeline;
use crate::text_pipeline::{
    RichTextSpan, TextDecorationDraw, TextDrawRequest, TextPipelineV2, TextShadowRequest,
};
use crate::utilities::{Color, Mat3, Rect};

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

struct AlphaState {
    current: f32,
    stack: Vec<f32>,
}

impl AlphaState {
    fn new() -> Self {
        Self {
            current: 1.0,
            stack: Vec::new(),
        }
    }

    fn current(&self) -> f32 {
        self.current
    }

    fn set(&mut self, alpha: f32) {
        self.current = alpha.clamp(0.0, 1.0);
    }

    fn save(&mut self) {
        self.stack.push(self.current);
    }

    fn restore(&mut self) {
        self.current = self.stack.pop().unwrap_or(1.0);
    }
}

impl Default for AlphaState {
    fn default() -> Self {
        Self::new()
    }
}

fn apply_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] *= alpha;
    color
}

fn transform_text_shadow(
    shadow: TextShadowRequest,
    transform: &Mat3,
    alpha: f32,
) -> TextShadowRequest {
    let offset_x = transform.cols[0][0] * shadow.offset_x
        + transform.cols[1][0] * shadow.offset_y;
    let offset_y = transform.cols[0][1] * shadow.offset_x
        + transform.cols[1][1] * shadow.offset_y;
    TextShadowRequest {
        offset_x,
        offset_y,
        color: apply_alpha(shadow.color, alpha),
        ..shadow
    }
}

/// Builds the GPU rect instance(s) for a single `FillRect` command.
///
/// Returns the box's own instance (background + border, its outline fields
/// always zeroed) and, when the command carries a visible outline, a second
/// instance covering only the outline ring at its expanded bounds (its fill
/// and border fields always zeroed, so it paints nothing but the ring).
///
/// An outline is drawn just outside its box's own edge, so it can overlap
/// whatever is painted right after it — an adjacent sibling in a `Row`, say.
/// Splitting it into its own instance lets the caller defer it: see
/// [`Renderer::deferred_outlines`].
#[allow(clippy::too_many_arguments)]
fn build_fill_rect_instances(
    rect: Rect,
    color: Color,
    border_radius: [f32; 4],
    border_width: [f32; 4],
    border_color: Color,
    outline_width: [f32; 4],
    outline_color: Color,
    current_transform: &Mat3,
    clip: Option<&ClipState>,
    alpha: f32,
) -> (RectInstance, Option<RectInstance>) {
    let sx = (current_transform.cols[0][0].powi(2) + current_transform.cols[0][1].powi(2)).sqrt();
    let sy = (current_transform.cols[1][0].powi(2) + current_transform.cols[1][1].powi(2)).sqrt();

    let ol = outline_width[3]; // left
    let or = outline_width[1]; // right
    let ot = outline_width[0]; // top
    let ob = outline_width[2]; // bottom
    let has_outline = ol > 0.0 || or > 0.0 || ot > 0.0 || ob > 0.0;

    // Transform the top-left and bottom-right corners of the box itself. This
    // correctly handles translation and scaling.
    let (p1x, p1y) = current_transform.transform_point(rect.x, rect.y);
    let (p2x, p2y) =
        current_transform.transform_point(rect.x + rect.width, rect.y + rect.height);

    let mut scaled_br = border_radius;
    for r in &mut scaled_br {
        *r *= sx;
    } // Assuming uniform scale for simplicity, or use sx

    let mut scaled_bw = border_width;
    scaled_bw[0] *= sy; // top
    scaled_bw[1] *= sx; // right
    scaled_bw[2] *= sy; // bottom
    scaled_bw[3] *= sx; // left

    let clip_rect = clip_to_array(clip);
    let clip_radii = clip_border_radius(clip);

    let outline_instance = has_outline.then(|| {
        // Transform the top-left and bottom-right corners of the quad expanded
        // by the outline width, so the ring is visible.
        let (ep1x, ep1y) = current_transform.transform_point(rect.x - ol, rect.y - ot);
        let (ep2x, ep2y) = current_transform
            .transform_point(rect.x + rect.width + or, rect.y + rect.height + ob);

        let mut scaled_ow = outline_width;
        scaled_ow[0] *= sy; // top
        scaled_ow[1] *= sx; // right
        scaled_ow[2] *= sy; // bottom
        scaled_ow[3] *= sx; // left

        RectInstance {
            position: [ep1x.min(ep2x), ep1y.min(ep2y)],
            size: [(ep2x - ep1x).abs(), (ep2y - ep1y).abs()],
            color: [0.0, 0.0, 0.0, 0.0],
            border_radius: scaled_br,
            border_width: [0.0; 4],
            border_color: [0.0, 0.0, 0.0, 0.0],
            outline_width: scaled_ow,
            outline_color: apply_alpha(outline_color.to_array(), alpha),
            clip_rect,
            clip_border_radius: clip_radii,
            shadow_params: [0.0; 4],
            shadow_color: [0.0; 4],
            shadow_flags: [0.0; 4],
        }
    });

    let main_instance = RectInstance {
        position: [p1x.min(p2x), p1y.min(p2y)],
        size: [(p2x - p1x).abs(), (p2y - p1y).abs()],
        color: apply_alpha(color.to_array(), alpha),
        border_radius: scaled_br,
        border_width: scaled_bw,
        border_color: apply_alpha(border_color.to_array(), alpha),
        outline_width: [0.0; 4],
        outline_color: [0.0, 0.0, 0.0, 0.0],
        clip_rect,
        clip_border_radius: clip_radii,
        shadow_params: [0.0; 4],
        shadow_color: [0.0; 4],
        shadow_flags: [0.0; 4],
    };

    (main_instance, outline_instance)
}

fn has_renderable_dimensions(width: u32, height: u32) -> bool {
    width > 0 && height > 0
}

struct ResolvedCmd {
    kind: ResolvedKind,
}

enum ResolvedKind {
    Rect(RectInstance),
    Image {
        texture_id: u32,
        instance: ImageInstance,
    },
    /// Index into `text_requests` (and the text pipeline's per-request ranges).
    /// Kept in draw order so text is painted at its own z-position instead of
    /// on top of everything at the end.
    Text(usize),
    /// Index into `decoration_requests` (one instance per decoration).
    TextDecoration(usize),
    Svg(usize),
    Custom {
        pipeline_index: usize,
    },
}

pub struct SvgRenderItem {
    pub scene: Arc<SvgScene>,
    pub destination: Rect,
    pub overrides: Arc<[SvgNodeStyleOverride]>,
    pub world_transform: Mat3,
    pub clip_rect: [f32; 4],
    pub clip_border_radius: [f32; 4],
    pub opacity: f32,
}

fn resolve_svg_item(
    scene: Arc<SvgScene>,
    destination: Rect,
    overrides: Arc<[SvgNodeStyleOverride]>,
    world_transform: Mat3,
    clip: Option<&ClipState>,
    opacity: f32,
) -> SvgRenderItem {
    SvgRenderItem {
        scene,
        destination,
        overrides,
        world_transform,
        clip_rect: clip_to_array(clip),
        clip_border_radius: clip_border_radius(clip),
        opacity,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RendererMemoryStats {
    pub image_texture_count: usize,
    pub image_texture_bytes: u64,
    pub glyph_atlas_bytes: u64,
    pub glyph_bitmap_cache_entries: usize,
    pub glyph_bitmap_cache_bytes: usize,
    pub instance_buffer_bytes: u64,
    pub svg_geometry_cpu_bytes: u64,
    pub svg_geometry_gpu_bytes: u64,
    pub svg_instance_buffer_bytes: u64,
    pub multisample_target_bytes: u64,
}

struct MultisampleTarget {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    sample_count: u32,
}

impl MultisampleTarget {
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        sample_count: u32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cupid multisample color target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            _texture: texture,
            view,
            width,
            height,
            sample_count,
        }
    }

    fn bytes(&self) -> u64 {
        self.width as u64 * self.height as u64 * self.sample_count as u64 * 4
    }
}

pub struct Renderer {
    pub rect_pipeline: RectPipeline,
    pub text_pipeline: TextPipelineV2,
    pub image_pipeline: ImagePipeline,
    pub svg_pipeline: SvgPipeline,
    pipeline_cache: Option<wgpu::PipelineCache>,
    custom_pipelines: Vec<CustomPipelineSlot>,
    surface_format: wgpu::TextureFormat,
    // Reusable per-frame scratch buffers to avoid allocations.
    transform_stack: Vec<Mat3>,
    clip_stack: Vec<ClipState>,
    text_requests: Vec<TextDrawRequest>,
    decoration_requests: Vec<TextDecorationDraw>,
    svg_items: Vec<SvgRenderItem>,
    resolved: Vec<ResolvedCmd>,
    /// Outline rings pulled out of this frame's `FillRect` commands.
    ///
    /// An outline is painted just outside its own box's edge, so it can
    /// overlap whatever sits next to that box (e.g. an adjacent panel in a
    /// `Row`). Holding these back and appending them to `resolved` only
    /// after every in-order command of the frame keeps every outline on top
    /// instead of being silently painted over by a later sibling.
    deferred_outlines: Vec<RectInstance>,
    textures_to_remove: Vec<u32>,
    multisample_target: Option<MultisampleTarget>,
    antialiasing: crate::AntiAlias,
}

impl Renderer {
    /// Creates a renderer using lightweight analytic antialiasing.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::with_antialiasing(device, format, crate::AntiAlias::default())
    }

    /// Creates a renderer with the requested antialiasing strategy.
    pub fn with_antialiasing(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        antialiasing: crate::AntiAlias,
    ) -> Self {
        let start = aimer_utils::AnimInstant::now();

        let cache = pipeline_cache::create_pipeline_cache(device);

        let renderer = Self {
            rect_pipeline: RectPipeline::new(device, format, cache.as_ref(), antialiasing),
            text_pipeline: TextPipelineV2::new(device, format, cache.as_ref(), antialiasing),
            image_pipeline: ImagePipeline::new(device, format, cache.as_ref(), antialiasing),
            svg_pipeline: SvgPipeline::new(device, format, cache.as_ref(), antialiasing),
            pipeline_cache: cache,
            custom_pipelines: Vec::new(),
            surface_format: format,
            transform_stack: Vec::new(),
            clip_stack: Vec::new(),
            text_requests: Vec::new(),
            decoration_requests: Vec::new(),
            svg_items: Vec::new(),
            resolved: Vec::new(),
            deferred_outlines: Vec::new(),
            textures_to_remove: Vec::new(),
            multisample_target: None,
            antialiasing,
        };

        debug!(
            "Renderer initialization ready {}ms",
            start.elapsed().as_millis()
        );
        renderer
    }

    /// Register a user-defined custom pipeline.
    /// The pipeline will participate in the render loop whenever
    /// `DrawCommand::Custom` commands target it by name.
    pub fn register_custom_pipeline(&mut self, pipeline: impl CustomPipeline) {
        self.custom_pipelines
            .push(CustomPipelineSlot::new(pipeline));
    }

    /// Returns the surface texture format (useful for creating custom
    /// pipelines).
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    pub fn memory_stats(&self) -> RendererMemoryStats {
        RendererMemoryStats {
            image_texture_count: self.image_pipeline.texture_count(),
            image_texture_bytes: self.image_pipeline.texture_bytes(),
            glyph_atlas_bytes: self.text_pipeline.glyph_atlas_bytes(),
            glyph_bitmap_cache_entries: self.text_pipeline.cached_glyph_count(),
            glyph_bitmap_cache_bytes: self.text_pipeline.glyph_bitmap_cache_bytes(),
            instance_buffer_bytes: self.rect_pipeline.instance_buffer_bytes()
                + self.image_pipeline.instance_buffer_bytes()
                + self.text_pipeline.instance_buffer_bytes()
                + self.svg_pipeline.instance_buffer_bytes(),
            svg_geometry_cpu_bytes: self.svg_pipeline.cpu_geometry_bytes(),
            svg_geometry_gpu_bytes: self.svg_pipeline.gpu_geometry_bytes(),
            svg_instance_buffer_bytes: self.svg_pipeline.instance_buffer_bytes(),
            multisample_target_bytes: self
                .multisample_target
                .as_ref()
                .map_or(0, MultisampleTarget::bytes),
        }
    }

    pub fn clear_svg_resources(&mut self) {
        self.svg_pipeline.clear_resources();
    }

    /// Whether the frame just rendered left text unprepared for lack of time.
    ///
    /// Only text that could not reach a pixel this frame is ever left behind —
    /// a scroll viewport prepares beyond its own edges so a line is ready
    /// before it scrolls in, and a frame spends only what it can spare on
    /// that. What did not fit is not lost, but it is not ready either: the
    /// caller is expected to schedule another frame, which is where it is
    /// picked up.
    #[inline]
    pub fn has_postponed_text_preparation(&self) -> bool {
        self.text_pipeline.has_postponed_preparation()
    }

    pub fn preload_text(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        font_size: f32,
    ) {
        self.text_pipeline
            .preload_text(device, queue, text, font_size);
    }

    /// Level 2 warm-up — pre-rasterize the common ASCII glyph set at the given
    /// font sizes so the glyph atlas is populated before the first frame. This
    /// keeps even brand-new strings cheap (shaping only, no rasterization).
    pub fn warm_glyph_set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        font_sizes: &[f32],
    ) {
        self.text_pipeline.warm_glyph_set(device, queue, font_sizes);
    }

    /// Level 1 warm-up — pre-shape and lay out a known static string so the
    /// shaping/layout caches and atlas are warm, and the string renders on the
    /// fast cache-hit path from the very first frame. `layout_width` is the
    /// wrap width it will be drawn with (0.0 for non-wrapping text).
    pub fn warm_text(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        font_size: f32,
        layout_width: f32,
    ) {
        self.text_pipeline
            .warm_text(device, queue, text, font_size, layout_width);
    }

    /// Save the pipeline cache to disk for faster startup on next launch.
    /// Called automatically on drop, or can be called manually on suspend.
    pub fn save_pipeline_cache(&self) {
        if let Some(ref cache) = self.pipeline_cache {
            pipeline_cache::save_pipeline_cache(cache);
        }
    }

    /// Process a DrawList into pipeline-specific batches and render in a single
    /// pass.
    #[allow(clippy::too_many_arguments)]
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
        self.clip_stack.clear();
        self.text_requests.clear();
        self.decoration_requests.clear();
        self.svg_items.clear();
        self.resolved.clear();
        self.deferred_outlines.clear();
        self.textures_to_remove.clear();

        if !has_renderable_dimensions(width, height) {
            return;
        }

        let mut current_transform = Mat3::identity();
        let mut alpha_state = AlphaState::new();
        // Canvas-level italic state applied to plain `DrawText` (rich text carries
        // italic per span). Reset each frame; toggled by `SetItalic`.
        let mut current_italic = false;
        // Canvas-level written language applied to the text after it, which
        // decides whether a run of Han is drawn in a Chinese or a Japanese
        // face. Reset each frame; set by `SetTextLanguage`.
        let mut current_language = None;

        for cmd in draw_list.commands() {
            match cmd {
                DrawCommand::PushTransform { matrix } => {
                    self.transform_stack.push(current_transform);
                    alpha_state.save();
                    current_transform = matrix.pixel_aligned();
                }
                DrawCommand::PopTransform => {
                    if let Some(prev) = self.transform_stack.pop() {
                        current_transform = prev;
                    }
                    alpha_state.restore();
                }
                DrawCommand::PushClip {
                    rect,
                    border_radius,
                } => {
                    let (p1x, p1y) = current_transform.transform_point(rect.x, rect.y);
                    let (p2x, p2y) = current_transform
                        .transform_point(rect.x + rect.width, rect.y + rect.height);
                    let sx = (current_transform.cols[0][0].powi(2)
                        + current_transform.cols[0][1].powi(2))
                    .sqrt();

                    let new_rect = Rect::new(
                        p1x.min(p2x),
                        p1y.min(p2y),
                        (p2x - p1x).abs(),
                        (p2y - p1y).abs(),
                    );

                    let effective_clip = if let Some(parent) = self.clip_stack.last() {
                        let x = new_rect.x.max(parent.rect.x);
                        let y = new_rect.y.max(parent.rect.y);
                        let r =
                            (new_rect.x + new_rect.width).min(parent.rect.x + parent.rect.width);
                        let b =
                            (new_rect.y + new_rect.height).min(parent.rect.y + parent.rect.height);
                        Rect::new(x, y, (r - x).max(0.0), (b - y).max(0.0))
                    } else {
                        new_rect
                    };

                    let mut scaled_br = *border_radius;
                    for r in &mut scaled_br {
                        *r *= sx;
                    }

                    self.clip_stack.push(ClipState {
                        rect: effective_clip,
                        border_radius: scaled_br,
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
                    let (main_instance, outline_instance) = build_fill_rect_instances(
                        *rect,
                        *color,
                        *border_radius,
                        *border_width,
                        *border_color,
                        *outline_width,
                        *outline_color,
                        &current_transform,
                        self.clip_stack.last(),
                        alpha_state.current(),
                    );

                    // An outline is painted just outside its box's own edge, so it can
                    // overlap whatever sits next to that box (e.g. an adjacent panel in
                    // a `Row`, drawn right after it at the same layer). Deferring it —
                    // later appended to `resolved` after every in-order command of this
                    // frame — keeps every outline on top instead of letting a later
                    // sibling silently paint over it.
                    if let Some(outline_instance) = outline_instance {
                        self.deferred_outlines.push(outline_instance);
                    }

                    self.resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Rect(main_instance),
                    });
                }
                DrawCommand::ClearRect { rect } => {
                    let (p1x, p1y) = current_transform.transform_point(rect.x, rect.y);
                    let (p2x, p2y) = current_transform
                        .transform_point(rect.x + rect.width, rect.y + rect.height);
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
                    let (tx, ty) = current_transform.transform_point(position.x, position.y);
                    let shadow = shadow.map(|shadow| {
                        transform_text_shadow(shadow, &current_transform, alpha_state.current())
                    });
                    let idx = self.text_requests.len();
                    self.text_requests.push(TextDrawRequest {
                        x: tx,
                        y: ty,
                        text: text.clone(),
                        font_size: *font_size,
                        color: apply_alpha(color.to_array(), alpha_state.current()),
                        bounds_width: bounds_width.unwrap_or(width as f32 - tx),
                        bounds_height: bounds_height.unwrap_or(height as f32 - ty),
                        overflow: *overflow,
                        horizontal_align: *horizontal_align,
                        line_height: None,
                        shadow,
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
                    self.resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Text(idx),
                    });
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
                    let (tx, ty) = current_transform.transform_point(position.x, position.y);
                    let idx = self.text_requests.len();
                    self.text_requests.push(TextDrawRequest {
                        x: tx,
                        y: ty,
                        text: spans
                            .iter()
                            .map(|span| &*span.text)
                            .collect::<String>()
                            .into(),
                        font_size: *font_size,
                        color: apply_alpha(color.to_array(), alpha_state.current()),
                        bounds_width: bounds_width.unwrap_or(width as f32 - tx),
                        bounds_height: bounds_height.unwrap_or(height as f32 - ty),
                        overflow: *overflow,
                        horizontal_align:
                            crate::text_pipeline::text_layout::TextHorizontalAlign::Left,
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
                                    apply_alpha(color.to_array(), alpha_state.current())
                                }),
                                font_weight: span.font_weight,
                                italic: span.italic,
                            })
                            .collect(),
                    });
                    self.resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Text(idx),
                    });
                }
                DrawCommand::DrawTextDecoration {
                    rect,
                    color,
                    style,
                    thickness,
                    period,
                } => {
                    // The band is authored in local coordinates; transform its
                    // top-left and scale the extents so decoration follows any
                    // active scale/translation just like the text it underlines.
                    let sx = (current_transform.cols[0][0].powi(2)
                        + current_transform.cols[0][1].powi(2))
                    .sqrt();
                    let sy = (current_transform.cols[1][0].powi(2)
                        + current_transform.cols[1][1].powi(2))
                    .sqrt();
                    let (p1x, p1y) = current_transform.transform_point(rect.x, rect.y);
                    let (p2x, p2y) = current_transform
                        .transform_point(rect.x + rect.width, rect.y + rect.height);
                    let deco_idx = self.decoration_requests.len();
                    self.decoration_requests.push(TextDecorationDraw {
                        x: p1x.min(p2x),
                        y: p1y.min(p2y),
                        width: (p2x - p1x).abs(),
                        band_height: (p2y - p1y).abs(),
                        thickness: (*thickness * sy).max(1.0),
                        period: (*period * sx).max(1.0),
                        style: *style,
                        color: apply_alpha(color.to_array(), alpha_state.current()),
                        clip_rect: clip_to_array(self.clip_stack.last()),
                        clip_border_radius: clip_border_radius(self.clip_stack.last()),
                    });
                    self.resolved.push(ResolvedCmd {
                        kind: ResolvedKind::TextDecoration(deco_idx),
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
                    self.resolved.push(ResolvedCmd {
                        kind: ResolvedKind::Svg(index),
                    });
                }
                DrawCommand::SetTransform { matrix } => {
                    current_transform = matrix.pixel_aligned();
                }
                DrawCommand::SetAlpha { alpha } => {
                    alpha_state.set(*alpha);
                }
                DrawCommand::RestoreAlpha => {
                    alpha_state.set(1.0);
                }
                DrawCommand::SetItalic { italic } => {
                    current_italic = *italic;
                }
                DrawCommand::SetTextLanguage { language } => {
                    current_language = *language;
                }
                DrawCommand::DrawImage { rect, texture_id } => {
                    let (p1x, p1y) = current_transform.transform_point(rect.x, rect.y);
                    let (p2x, p2y) = current_transform
                        .transform_point(rect.x + rect.width, rect.y + rect.height);
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
                                alpha: alpha_state.current(),
                            },
                        },
                    });
                }
                DrawCommand::LoadImage {
                    bytes,
                    texture_id,
                    width,
                    height,
                } => {
                    self.image_pipeline.upload_if_absent(
                        device,
                        queue,
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
                } => {
                    self.image_pipeline.upload_image_with_id(
                        device,
                        queue,
                        *texture_id,
                        *width,
                        *height,
                        bytes,
                    );
                }
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
                    let sx = (current_transform.cols[0][0].powi(2)
                        + current_transform.cols[0][1].powi(2))
                    .sqrt();
                    let sy = (current_transform.cols[1][0].powi(2)
                        + current_transform.cols[1][1].powi(2))
                    .sqrt();

                    let offset_x = shadow_params[0];
                    let offset_y = shadow_params[1];
                    let blur = shadow_params[2];
                    let spread = shadow_params[3];

                    // Expand the rect per-axis to encompass the full shadow extent
                    let expand_x = blur + spread.abs() + offset_x.abs();
                    let expand_y = blur + spread.abs() + offset_y.abs();

                    let (p1x, p1y) =
                        current_transform.transform_point(rect.x - expand_x, rect.y - expand_y);
                    let (p2x, p2y) = current_transform.transform_point(
                        rect.x + rect.width + expand_x,
                        rect.y + rect.height + expand_y,
                    );

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
                            shadow_color: apply_alpha(
                                shadow_color.to_array(),
                                alpha_state.current(),
                            ),
                            shadow_flags: [
                                if *inset { 1.0 } else { 0.0 },
                                side_params[0],
                                side_params[1],
                                side_params[2],
                            ],
                        }),
                    });
                }
                DrawCommand::Custom {
                    pipeline_name,
                    data: _,
                } => {
                    if let Some(idx) = self
                        .custom_pipelines
                        .iter()
                        .position(|s| s.pipeline.name() == pipeline_name.as_str())
                    {
                        self.resolved.push(ResolvedCmd {
                            kind: ResolvedKind::Custom {
                                pipeline_index: idx,
                            },
                        });
                    }
                }
            }
        }

        // Every outline recorded this frame paints last, on top of every
        // in-order command above — including a sibling drawn after the
        // outlined box that would otherwise cover the ring bleeding past its
        // edge (see `deferred_outlines`).
        for instance in self.deferred_outlines.drain(..) {
            self.resolved.push(ResolvedCmd {
                kind: ResolvedKind::Rect(instance),
            });
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
                sample_count: self.antialiasing.sample_count(),
            };
            for slot in &mut self.custom_pipelines {
                if slot.pipeline.has_work() {
                    slot.pipeline.prepare(&render_ctx);
                }
            }
        }

        if !self.text_requests.is_empty() || !self.decoration_requests.is_empty() {
            self.text_pipeline.prepare(
                device,
                queue,
                width,
                height,
                is_srgb,
                &self.text_requests,
                &self.decoration_requests,
            );
        }

        self.svg_pipeline
            .prepare(device, queue, &self.svg_items, width, height, is_srgb);

        // Create encoder and render pass
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("cupid render encoder"),
        });

        let sample_count = self.antialiasing.sample_count();
        let (render_view, resolve_target, store) = if self.antialiasing.uses_multisampling() {
            let target = self.multisample_target.get_or_insert_with(|| {
                MultisampleTarget::new(device, self.surface_format, width, height, sample_count)
            });
            if target.width != width || target.height != height {
                *target = MultisampleTarget::new(
                    device,
                    self.surface_format,
                    width,
                    height,
                    sample_count,
                );
            }
            (&target.view, Some(view), wgpu::StoreOp::Discard)
        } else {
            self.multisample_target = None;
            (view, None, wgpu::StoreOp::Store)
        };

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cupid render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: render_view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Render commands in draw order to preserve correct z-ordering
            // across rects, images, text and text decorations. Consecutive
            // same-type commands are batched; switching type flushes the pending
            // batch first so nothing is reordered. Text used to be drawn in a
            // single pass at the very end, which made it float above every rect
            // regardless of z-order (e.g. a `Stack`'s upper layer could not cover
            // text drawn by a lower layer) — it is now interleaved like the rest.
            //
            // Size the rect and image instance buffers for *all* instances of
            // this frame up-front. Each flush/batch then draws from a distinct
            // region of its shared buffer (batches in one pass must not alias),
            // and the whole frame's data reaches the GPU in one deferred write
            // per pipeline at `end_frame` — instead of one `write_buffer` (a
            // staging allocation plus a blit) per z-order split.
            let mut total_rect_instances = 0;
            let mut total_image_instances = 0;
            for cmd in &self.resolved {
                match cmd.kind {
                    ResolvedKind::Rect(_) => total_rect_instances += 1,
                    ResolvedKind::Image { .. } => total_image_instances += 1,
                    _ => {}
                }
            }
            self.rect_pipeline.begin_frame(
                device,
                queue,
                total_rect_instances,
                width,
                height,
                is_srgb,
            );
            self.image_pipeline.begin_frame(
                device,
                queue,
                total_image_instances,
                width,
                height,
                is_srgb,
            );

            let mut image_batch: Vec<ImageInstance> = Vec::new();
            let mut current_texture_id: Option<u32> = None;

            for i in 0..self.resolved.len() {
                match &self.resolved[i].kind {
                    ResolvedKind::Rect(inst) => {
                        // Flush any pending image batch before switching to rects
                        if let Some(tid) = current_texture_id.take()
                            && !image_batch.is_empty()
                        {
                            self.image_pipeline.draw_batch(
                                device,
                                queue,
                                &mut pass,
                                tid,
                                &image_batch,
                            );
                            image_batch.clear();
                        }
                        self.rect_pipeline.push(*inst);
                    }
                    ResolvedKind::Image {
                        texture_id,
                        instance,
                    } => {
                        // Flush any pending rects before switching to images
                        self.rect_pipeline.flush(&mut pass);

                        if current_texture_id.is_some() && current_texture_id != Some(*texture_id) {
                            // Flush current image batch for previous texture
                            let Some(tid) = current_texture_id.take() else {
                                continue;
                            };
                            if !image_batch.is_empty() {
                                self.image_pipeline.draw_batch(
                                    device,
                                    queue,
                                    &mut pass,
                                    tid,
                                    &image_batch,
                                );
                                image_batch.clear();
                            }
                        }
                        current_texture_id = Some(*texture_id);
                        image_batch.push(*instance);
                    }
                    ResolvedKind::Text(index) => {
                        let index = *index;
                        // Flush everything drawn before this text so the text
                        // lands on top of it, and anything drawn after this text
                        // lands on top of the text.
                        self.rect_pipeline.flush(&mut pass);
                        if let Some(tid) = current_texture_id.take()
                            && !image_batch.is_empty()
                        {
                            self.image_pipeline.draw_batch(
                                device,
                                queue,
                                &mut pass,
                                tid,
                                &image_batch,
                            );
                            image_batch.clear();
                        }
                        self.text_pipeline.render_request(&mut pass, index);
                    }
                    ResolvedKind::TextDecoration(index) => {
                        let index = *index;
                        self.rect_pipeline.flush(&mut pass);
                        if let Some(tid) = current_texture_id.take()
                            && !image_batch.is_empty()
                        {
                            self.image_pipeline.draw_batch(
                                device,
                                queue,
                                &mut pass,
                                tid,
                                &image_batch,
                            );
                            image_batch.clear();
                        }
                        self.text_pipeline.render_decoration(&mut pass, index);
                    }
                    ResolvedKind::Svg(index) => {
                        self.rect_pipeline.flush(&mut pass);
                        if let Some(tid) = current_texture_id.take()
                            && !image_batch.is_empty()
                        {
                            self.image_pipeline.draw_batch(
                                device,
                                queue,
                                &mut pass,
                                tid,
                                &image_batch,
                            );
                            image_batch.clear();
                        }
                        self.svg_pipeline.draw_item(&mut pass, *index);
                    }
                    ResolvedKind::Custom { pipeline_index } => {
                        // Flush pending built-in batches to maintain z-order
                        self.rect_pipeline.flush(&mut pass);
                        if let Some(tid) = current_texture_id.take()
                            && !image_batch.is_empty()
                        {
                            self.image_pipeline.draw_batch(
                                device,
                                queue,
                                &mut pass,
                                tid,
                                &image_batch,
                            );
                            image_batch.clear();
                        }
                        // Render the custom pipeline
                        if let Some(slot) = self.custom_pipelines.get(*pipeline_index) {
                            slot.pipeline.render(&mut pass);
                        }
                    }
                }
            }

            // Flush remaining image batch
            if let Some(tid) = current_texture_id
                && !image_batch.is_empty()
            {
                self.image_pipeline
                    .draw_batch(device, queue, &mut pass, tid, &image_batch);
            }

            // Flush remaining rects
            self.rect_pipeline.flush(&mut pass);
        }

        // The frame's instance data travels in one deferred write per pipeline
        // (skipped outright when the bytes did not change). `write_buffer` is
        // applied on the queue timeline before the submitted pass executes, so
        // the draws recorded above read exactly this data.
        self.rect_pipeline.end_frame(queue);
        self.image_pipeline.end_frame(queue);

        queue.submit(std::iter::once(encoder.finish()));
        for texture_id in self.textures_to_remove.drain(..) {
            self.image_pipeline.remove_texture(texture_id);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.save_pipeline_cache();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::svg::{SvgScene, SvgViewport};

    /// The renderer is owned by whichever thread encodes frames. Moving it to a
    /// raster thread requires it — and every pipeline it owns, including the text
    /// pipeline's glyph atlas — to be `Send`.
    #[test]
    fn renderer_can_be_owned_by_a_raster_thread() {
        fn assert_send<T: Send>() {}
        assert_send::<Renderer>();
    }

    #[test]
    fn alpha_state_restores_nested_saved_values() {
        let mut state = AlphaState::default();
        state.set(0.8);
        state.save();
        state.set(0.25);
        state.save();
        state.set(0.1);

        state.restore();
        assert_eq!(state.current(), 0.25);
        state.restore();
        assert_eq!(state.current(), 0.8);
    }

    #[test]
    fn apply_alpha_multiplies_existing_color_opacity() {
        assert_eq!(
            apply_alpha([0.1, 0.2, 0.3, 0.8], 0.25),
            [0.1, 0.2, 0.3, 0.2]
        );
    }

    #[test]
    fn text_shadow_transform_scales_offset_and_canvas_alpha() {
        let shadow = transform_text_shadow(
            TextShadowRequest {
                offset_x: 2.0,
                offset_y: -1.0,
                blur: 3.0,
                color: [0.1, 0.2, 0.3, 0.8],
            },
            &Mat3::scale(2.0, 3.0),
            0.25,
        );

        assert_eq!(shadow.offset_x, 4.0);
        assert_eq!(shadow.offset_y, -3.0);
        assert_eq!(shadow.blur, 3.0);
        assert_eq!(shadow.color, [0.1, 0.2, 0.3, 0.2]);
    }

    #[test]
    fn render_dimensions_require_nonzero_width_and_height() {
        assert!(has_renderable_dimensions(1, 1));
        assert!(!has_renderable_dimensions(0, 1));
        assert!(!has_renderable_dimensions(1, 0));
        assert!(!has_renderable_dimensions(0, 0));
    }

    #[test]
    fn alpha_state_clamps_invalid_values() {
        let mut state = AlphaState::default();
        state.set(2.0);
        assert_eq!(state.current(), 1.0);
        state.set(-1.0);
        assert_eq!(state.current(), 0.0);
    }

    // Regression: `BoxOutline` is meant to bleed just outside its own box,
    // like a CSS `outline` — but the outline used to be baked into the very
    // same rect instance as the background/border, all sharing one draw call
    // in submission order. A sibling painted right after that box (e.g. the
    // next panel in a `Row`) landed on top of the whole instance, including
    // the outline ring bleeding into the sibling's space, making the outline
    // look like it "never rendered". Splitting the outline into its own
    // instance is what lets the renderer defer and paint it last.
    #[test]
    fn build_fill_rect_instances_returns_no_outline_instance_when_outline_is_zero() {
        let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
        let (main_instance, outline_instance) = build_fill_rect_instances(
            rect,
            Color::white(),
            [0.0; 4],
            [0.0; 4],
            Color::black(),
            [0.0; 4],
            Color::red(),
            &Mat3::identity(),
            None,
            1.0,
        );

        assert!(outline_instance.is_none());
        assert_eq!(main_instance.position, [10.0, 20.0]);
        assert_eq!(main_instance.size, [100.0, 50.0]);
    }

    #[test]
    fn build_fill_rect_instances_splits_outline_ring_from_the_box_own_bounds() {
        let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
        // [top, right, bottom, left]
        let outline_width = [2.0, 3.0, 4.0, 5.0];

        let (main_instance, outline_instance) = build_fill_rect_instances(
            rect,
            Color::white(),
            [0.0; 4],
            [0.0; 4],
            Color::black(),
            outline_width,
            Color::red(),
            &Mat3::identity(),
            None,
            1.0,
        );

        // The box's own instance stays at its own bounds and carries no
        // outline of its own — otherwise it would double-draw the ring.
        assert_eq!(main_instance.position, [10.0, 20.0]);
        assert_eq!(main_instance.size, [100.0, 50.0]);
        assert_eq!(main_instance.outline_width, [0.0; 4]);
        assert_eq!(main_instance.outline_color, [0.0, 0.0, 0.0, 0.0]);

        // The outline instance covers the box expanded by its outline width on
        // every side, and paints nothing but the ring.
        let outline_instance = outline_instance.expect("outline width is non-zero");
        assert_eq!(outline_instance.position, [5.0, 18.0]);
        assert_eq!(outline_instance.size, [108.0, 56.0]);
        assert_eq!(outline_instance.color, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(outline_instance.border_width, [0.0; 4]);
        assert_eq!(outline_instance.border_color, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(outline_instance.outline_width, outline_width);
        assert_eq!(outline_instance.outline_color, Color::red().to_array());
    }

    #[test]
    fn svg_item_captures_transform_clip_alpha_and_destination() {
        let scene = Arc::new(SvgScene {
            viewport: SvgViewport {
                width: 24.0,
                height: 12.0,
            },
            nodes: Arc::from([]),
            geometries: Arc::from([]),
        });
        let transform = Mat3::translate(10.4, 20.6)
            .mul(&Mat3::scale(2.0, 3.0))
            .pixel_aligned();
        let clip = ClipState {
            rect: Rect::new(4.0, 5.0, 30.0, 40.0),
            border_radius: [3.0; 4],
        };
        let destination = Rect::new(1.0, 2.0, 48.0, 24.0);

        let item = resolve_svg_item(
            scene.clone(),
            destination,
            Arc::from([]),
            transform,
            Some(&clip),
            0.4,
        );

        assert!(Arc::ptr_eq(&item.scene, &scene));
        assert_eq!(
            [
                item.destination.x,
                item.destination.y,
                item.destination.width,
                item.destination.height
            ],
            [1.0, 2.0, 48.0, 24.0]
        );
        assert_eq!(item.world_transform.cols, transform.cols);
        assert_eq!(item.clip_rect, [4.0, 5.0, 30.0, 40.0]);
        assert_eq!(item.clip_border_radius, [3.0; 4]);
        assert_eq!(item.opacity, 0.4);
    }
}
