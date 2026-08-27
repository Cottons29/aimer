#[cfg(any(target_os = "ios", target_os = "macos"))]
pub(crate) mod apple_fonts;
mod cache_key;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub(crate) mod core_text_raster;
mod deferred_preparation;
mod font_resolver;
pub mod glyph_atlas;
mod glyph_metrics;
mod glyph_outline;
pub mod glyph_rasterizer;
mod layout_cache;
mod preparation_batch;
pub(crate) mod system_fallback;
pub mod text_layout;

use std::sync::Arc;

use hashbrown::{HashMap, HashSet};

use aimer_utils::AnimInstant;
use bytemuck::{Pod, Zeroable};

use crate::font::{FontFamily, FontStyle, FontWeight, TextLanguage};
use crate::pipeline::frame_upload::FrameUpload;
use crate::pipeline::image_pipeline::InstanceBufferPolicy;
use crate::text_pipeline::cache_key::{
    LayoutCacheKey, LayoutInput, ShapingCacheKey, ShapingInput, span_layout_keys,
};
use crate::text_pipeline::deferred_preparation::{
    PREPARATION_BUDGET, PREPARATION_CHUNK, PreparationBudget, prepare_ahead_of_view,
    request_is_on_screen,
};
use crate::text_pipeline::font_resolver::warm_fallbacks_in_background;
use crate::text_pipeline::glyph_atlas::{BatchCapacityPlan, ColorGlyphAtlas, GlyphAtlas};
use crate::text_pipeline::glyph_rasterizer::{
    BOLD_WEIGHT_THRESHOLD, GlyphKey, GlyphPreparationContext, GlyphRasterizer, glyph_runs,
};
use crate::text_pipeline::layout_cache::LayoutCache;
use crate::text_pipeline::preparation_batch::{BatchExecutor, PreparationBatch};
use crate::text_pipeline::text_layout::{
    ShapedText, TextHorizontalAlign, layout_shaped_text, line_alignment_offsets,
    positioned_line_widths, prepare_shaped_text, shape_text_styled,
};

/// Per-instance data for one glyph quad.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct GlyphInstance {
    position: [f32; 2],
    size: [f32; 2],
    uv_rect: [f32; 4],
    color: [f32; 4],
    /// Clip rect: [x, y, width, height]. If width <= 0, no clip is applied.
    clip_rect: [f32; 4],
    /// Border radius for the clip rect: [top-left, top-right, bottom-right,
    /// bottom-left].
    clip_border_radius: [f32; 4],
    /// Horizontal shear factor for synthetic italic (tan of the slant angle).
    /// 0 = upright. The glyph shaders slant the quad by this, pinned at its
    /// bottom edge, so the advance/layout is unchanged.
    skew: f32,
    /// Exponent the glyph shader raises the atlas coverage to before blending.
    /// See [`coverage_exponent`]; `1.0` leaves the coverage untouched.
    coverage_exponent: f32,
    /// Padding to keep the struct 8-byte aligned for `Pod`/vertex upload.
    _pad: [f32; 2],
}

/// Gamma of the blend space text is composited in.
///
/// sRGB's transfer function is a 2.4 power with a linear toe; 2.2 is its
/// standard single-exponent approximation and the value every text stack that
/// corrects for this uses.
const TEXT_BLEND_GAMMA: f32 = 2.2;

/// Returns the exponent a glyph's coverage must be raised to before it is
/// blended in linear light.
///
/// A rasterizer's coverage is a *geometric* quantity: half a pixel covered by
/// black means "paint half way to black", which is a statement about the
/// picture, not about photons. Blending it on an sRGB target performs the mix
/// in linear light instead, and half way in linear light is far lighter than
/// half way in sRGB — so an antialiased edge loses weight, and a stroke looks
/// thinner the more of it falls on partially covered pixels.
///
/// Raising the coverage to `gamma^(2*luminance - 1)` puts the weight back:
/// dark text (luminance `0`) gets `1/2.2`, which strengthens partial coverage;
/// light text on a dark background gets `2.2`, which weakens it by the same
/// amount in the other direction; mid-luminance text — the one case linear
/// blending already renders correctly — gets exactly `1.0` and is untouched.
///
/// The background is not known here, so the text's own luminance stands in for
/// "which way the blend runs", which is the assumption every fixed-curve text
/// gamma uses and is right whenever text contrasts with what it sits on.
#[inline]
fn coverage_exponent(color: [f32; 4]) -> f32 {
    let channel = |value: f32| {
        if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            0.0
        }
    };
    let luminance =
        0.2126 * channel(color[0]) + 0.7152 * channel(color[1]) + 0.0722 * channel(color[2]);

    TEXT_BLEND_GAMMA.powf(2.0 * luminance - 1.0)
}

/// Returns the on-screen size of a glyph quad drawn from an atlas region of
/// `region_size` texels.
///
/// A glyph is a bitmap, not a shape. It reaches the screen unaltered only when
/// its quad is exactly as large as the region behind it, so that one quad pixel
/// maps to one texel and the linear sampler lands on texel centres. Any other
/// size resamples the whole glyph — every stroke is blended across two texels,
/// which reads as a loss of stroke weight rather than as a change of size,
/// because the ink the stroke carries is spread over more pixels than the
/// rasterizer put it on.
///
/// The region is therefore the only admissible source for the size. The
/// layout's own measurement of the glyph is not: it comes from a different
/// pass, and the two can disagree by a pixel.
#[inline]
fn glyph_quad_size(region_size: (u32, u32)) -> [f32; 2] {
    [region_size.0 as f32, region_size.1 as f32]
}

/// Snaps a glyph quad's top-left corner onto the device pixel grid.
///
/// Glyphs are rasterized at a single phase — [`GlyphKey`]'s `subpixel_x` and
/// `subpixel_y` are always `0` — so the bitmap in the atlas is drawn as if its
/// origin were a whole pixel. The quad's size already equals the bitmap's, so
/// placing that quad on a whole pixel makes every texel land on exactly one
/// pixel; placing it anywhere else makes the linear atlas sampler blend each
/// texel across two, which costs contrast and stroke weight.
///
/// Layout positions are fractional (advances, glyph bearings and a centred
/// line's alignment offset all are), so without this a piece of text renders
/// differently depending on where it happens to sit — most visibly for a label
/// centred in a box too narrow for it, which lands on a half pixel.
#[inline]
fn snap_to_pixel_grid(position: [f32; 2]) -> [f32; 2] {
    [position[0].round(), position[1].round()]
}

fn glyph_intersects_clip(position: [f32; 2], size: [f32; 2], clip: [f32; 4]) -> bool {
    if clip[2] <= 0.0 {
        return true;
    }

    let glyph_right = position[0] + size[0];
    let glyph_bottom = position[1] + size[1];
    let clip_right = clip[0] + clip[2];
    let clip_bottom = clip[1] + clip[3];

    glyph_right > clip[0]
        && position[0] < clip_right
        && glyph_bottom > clip[1]
        && position[1] < clip_bottom
}

#[inline]
fn shadow_padding(shadow: TextShadowRequest) -> f32 {
    let offset_x = shadow
        .offset_x
        .is_finite()
        .then_some(shadow.offset_x.abs())
        .unwrap_or(0.0);
    let offset_y = shadow
        .offset_y
        .is_finite()
        .then_some(shadow.offset_y.abs())
        .unwrap_or(0.0);
    let blur = shadow
        .blur
        .is_finite()
        .then_some(shadow.blur.max(0.0))
        .unwrap_or(0.0);
    offset_x.max(offset_y) + blur
}

#[inline]
fn shadow_intersects_clip(
    position: [f32; 2],
    size: [f32; 2],
    shadow: TextShadowRequest,
    clip: [f32; 4],
) -> bool {
    let offset_x = shadow
        .offset_x
        .is_finite()
        .then_some(shadow.offset_x)
        .unwrap_or(0.0);
    let offset_y = shadow
        .offset_y
        .is_finite()
        .then_some(shadow.offset_y)
        .unwrap_or(0.0);
    let blur = shadow
        .blur
        .is_finite()
        .then_some(shadow.blur.max(0.0))
        .unwrap_or(0.0);
    glyph_intersects_clip(
        [position[0] + offset_x - blur, position[1] + offset_y - blur],
        [size[0] + 2.0 * blur, size[1] + 2.0 * blur],
        clip,
    )
}

#[inline]
fn shadow_is_visible(color: [f32; 4]) -> bool {
    color.get(3).copied().unwrap_or(0.0).is_finite()
        && color.get(3).copied().unwrap_or(0.0) > 0.0
}

#[inline]
fn normalize_pixel_uv_rect(pixel_rect: [f32; 4], atlas_width: u32, atlas_height: u32) -> [f32; 4] {
    let width = atlas_width as f32;
    let height = atlas_height as f32;
    [
        pixel_rect[0] / width,
        pixel_rect[1] / height,
        pixel_rect[2] / width,
        pixel_rect[3] / height,
    ]
}

impl GlyphInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32,
        7 => Float32,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<GlyphInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Per-instance data for one decoration line quad (underline/overline/strike).
/// The line geometry is a plain quad; the actual stroke (and its dotted/dashed/
/// wavy shape) is produced procedurally by `text_decoration.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct DecorationInstance {
    /// Top-left of the band quad, screen space.
    position: [f32; 2],
    /// Band size: [width, band_height].
    size: [f32; 2],
    color: [f32; 4],
    clip_rect: [f32; 4],
    clip_border_radius: [f32; 4],
    /// [style_id, thickness_px, period_px, band_height_px].
    params: [f32; 4],
}

impl DecorationInstance {
    const ATTRIBS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<DecorationInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// A single styled decoration line to render, in final screen-space geometry.
/// The producer (widget/renderer) computes where the line sits from the text
/// metrics; the engine only rasterizes the styled stroke inside the band.
#[derive(Clone, Copy, Debug)]
pub struct TextDecorationDraw {
    /// Top-left of the band quad.
    pub x: f32,
    pub y: f32,
    /// Band width (line length).
    pub width: f32,
    /// Band height — tall enough to hold the stroke plus wave/double spacing.
    pub band_height: f32,
    /// Stroke thickness in pixels.
    pub thickness: f32,
    /// Repeat period for dotted/dashed/wavy styles (pixels).
    pub period: f32,
    /// Style id, matching `aimer_style::TextDecorationStyle::id`.
    pub style: u32,
    pub color: [f32; 4],
    pub clip_rect: [f32; 4],
    pub clip_border_radius: [f32; 4],
}

impl TextDecorationDraw {
    fn to_instance(self) -> DecorationInstance {
        DecorationInstance {
            position: [self.x, self.y],
            size: [self.width, self.band_height],
            color: self.color,
            clip_rect: self.clip_rect,
            clip_border_radius: self.clip_border_radius,
            params: [
                self.style as f32,
                self.thickness,
                self.period,
                self.band_height,
            ],
        }
    }
}

/// Paint data for one glyph shadow. The text pipeline expands blurred shadows
/// into a small, bounded sample set so the same atlas, clip, opacity, and
/// transform path is used as the foreground glyphs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextShadowRequest {
    /// Horizontal offset in physical pixels.
    pub offset_x: f32,
    /// Vertical offset in physical pixels.
    pub offset_y: f32,
    /// Blur radius in physical pixels.
    pub blur: f32,
    /// RGBA paint color.
    pub color: [f32; 4],
}

pub struct TextDrawRequest {
    pub x: f32,
    pub y: f32,
    // Reference-counted so cloning the request per frame (and from the draw
    // list) is a cheap refcount bump rather than a fresh string allocation.
    pub text: Arc<str>,
    pub font_size: f32,
    pub color: [f32; 4],
    pub bounds_width: f32,
    pub bounds_height: f32,
    pub overflow: TextOverflowMode,
    pub horizontal_align: TextHorizontalAlign,
    pub line_height: Option<f32>,
    /// Optional glyph shadow painted before the foreground run.
    pub shadow: Option<TextShadowRequest>,
    /// Whether this request also paints its foreground glyphs. Shadow-only
    /// requests are used by the canvas convenience API.
    pub draw_glyphs: bool,
    pub font_family: FontFamily,
    pub font_style: FontStyle,
    pub font_weight: Option<u16>,
    /// The language this text is written in, when the producer knows it.
    ///
    /// Han is unified, so a run of ideographs does not say whether it wants a
    /// Chinese or a Japanese face and stays on whichever the platform's
    /// cascade prefers — until a character only one language writes is typed
    /// and the whole word changes typeface. A text field knows the keyboard it
    /// is edited with and says so here; `None` leaves the run judged on its
    /// own characters.
    pub language: Option<TextLanguage>,
    pub italic: bool,
    pub clip_rect: [f32; 4],
    pub clip_border_radius: [f32; 4],
    pub spans: Vec<RichTextSpan>,
}

#[derive(Clone, Debug)]
pub struct RichTextSpan {
    pub text: Arc<str>,
    pub font_size: Option<f32>,
    pub color: Option<[f32; 4]>,
    pub font_weight: Option<u16>,
    pub italic: Option<bool>,
}

impl RichTextSpan {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self {
            text: text.into(),
            font_size: None,
            color: None,
            font_weight: None,
            italic: None,
        }
    }

    pub fn with_style(mut self, font_size: Option<f32>, color: Option<[f32; 4]>) -> Self {
        self.font_size = font_size;
        self.color = color;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextOverflowMode {
    #[default]
    Clip,
    Wrap,
    Ellipsis,
}

/// Glyph-instance ranges owned by a single text request. `[alpha_start,
/// alpha_end)` indexes `instances` and `[color_start, color_end)` indexes
/// `color_instances`. `prepare` fills both lists in request order, so each
/// request owns a contiguous slice of each and can be drawn on its own at the
/// right z-position in the draw stream.
#[derive(Clone, Copy, Default)]
struct TextRequestRange {
    alpha_start: u32,
    alpha_end: u32,
    color_start: u32,
    color_end: u32,
}

pub struct TextPipelineV2 {
    rasterizer: GlyphRasterizer,
    executor: BatchExecutor,
    /// Whether the last frame ran out of budget before preparing everything a
    /// viewport asked for ahead of itself. See [`has_postponed_preparation`].
    ///
    /// [`has_postponed_preparation`]: TextPipelineV2::has_postponed_preparation
    postponed_preparation: bool,
    /// Alpha-coverage atlas (R8Unorm) for monochrome glyphs.
    atlas: GlyphAtlas,
    /// RGBA8 atlas for sbix color emoji bitmaps (Apple Color Emoji et al.).
    color_atlas: ColorGlyphAtlas,
    pipeline: wgpu::RenderPipeline,
    /// Pipeline that samples the RGBA color atlas instead of the alpha atlas.
    color_pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    /// Shared layout used by both the alpha and color bind groups (the binding
    /// shape — uniform + texture_2d<f32> + sampler — is identical).
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    color_bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    instance_buffer: wgpu::Buffer,
    instance_policy: InstanceBufferPolicy,
    instances: Vec<GlyphInstance>,
    /// Skips the alpha-glyph upload when the buffer already holds the frame's
    /// exact bytes — static text costs no upload at all.
    instance_upload: FrameUpload<GlyphInstance>,
    /// Sibling buffer + scratch list for color glyph quads (drawn in a second
    /// pass after the alpha glyphs so they layer on top of the same line).
    color_instance_buffer: wgpu::Buffer,
    color_instance_policy: InstanceBufferPolicy,
    color_instances: Vec<GlyphInstance>,
    /// Upload skip gate for `color_instance_buffer`.
    color_instance_upload: FrameUpload<GlyphInstance>,
    /// Decoration-line pipeline + its own instance list/buffer. Decoration
    /// quads are drawn after the glyphs (see `render`) so lines layer with
    /// their text.
    decoration_pipeline: wgpu::RenderPipeline,
    decoration_instance_buffer: wgpu::Buffer,
    decoration_instance_policy: InstanceBufferPolicy,
    decoration_instances: Vec<DecorationInstance>,
    /// Upload skip gate for `decoration_instance_buffer`.
    decoration_instance_upload: FrameUpload<DecorationInstance>,
    /// Track atlas generation to only rebuild bind group when atlas texture
    /// changes.
    atlas_generation: u64,
    color_atlas_generation: u64,
    /// Cached viewport dimensions to skip redundant uniform writes.
    last_viewport: (u32, u32),
    /// Surface size the previous [`prepare`] ran against. A frame whose size
    /// differs is a live-resize frame: every wrapped layout is keyed by a
    /// width that will be different again next frame, so `prepare` postpones
    /// the ahead-of-view work instead of computing it at a transient width.
    ///
    /// [`prepare`]: TextPipelineV2::prepare
    last_prepared_surface: (u32, u32),
    /// Layout cache: maps a stable key derived from text content + render
    /// parameters to the pre-computed `Vec<PositionedGlyph>`. Entries are
    /// evicted by recency once the cache outgrows its capacity (see
    /// [`LayoutCache`]), so a resize's flood of transient-width layouts is
    /// shed without ever taking the current screen's layouts with it.
    layout_cache: LayoutCache,
    /// Width-independent shaping cache.  Resize may invalidate final positions
    /// for wrapping/ellipsis text, but shaped glyph ids and advances only
    /// depend on text content and font size.
    shaping_cache: HashMap<ShapingCacheKey, ShapedText>,
    /// Per-request glyph ranges recorded during `prepare` so the renderer can
    /// draw a single text request at its own z-position (interleaved with
    /// rects/images) instead of drawing all text in one final pass — the
    /// latter made text ignore z-order (e.g. a `Stack`'s upper layer could not
    /// cover text belonging to a lower layer).
    request_ranges: Vec<TextRequestRange>,
}

impl TextPipelineV2 {
    const INITIAL_CAPACITY: usize = 64;
    /// Soft upper bound on the number of cached positioned-glyph layouts.
    /// The cache is kept persistent across frames/screens; once it outgrows
    /// this bound, entries no recent frame read are evicted at the start of
    /// the next frame (see [`LayoutCache`]), so shaping/layout work is reused
    /// instead of being thrown away on every screen transition.
    const LAYOUT_CACHE_CAPACITY: usize = 4096;
    /// Absolute upper bound on the number of cached shaped strings. Shaped
    /// results are width-independent and tiny, so this can be generous.
    const SHAPING_CACHE_CAPACITY: usize = 4096;

    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        pipeline_cache: Option<&wgpu::PipelineCache>,
        antialiasing: crate::AntiAlias,
    ) -> Self {
        // The pipeline is built once, while the first frame is still being
        // laid out, so this is the last moment at which the platform fallback
        // chain can be built without a frame waiting on it.
        warm_fallbacks_in_background();

        let rasterizer = GlyphRasterizer::new();
        let atlas = GlyphAtlas::new(device);
        let color_atlas = ColorGlyphAtlas::new(device);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./shaders/text.wgsl").into()),
        });
        let color_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text color shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./shaders/text_color.wgsl").into()),
        });
        let decoration_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text decoration shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./shaders/text_decoration.wgsl").into()),
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("text atlas sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text viewport uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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

        let bind_group = Self::create_bind_group(
            device,
            &bind_group_layout,
            &viewport_buffer,
            &atlas.view,
            &sampler,
        );
        let color_bind_group = Self::create_bind_group(
            device,
            &bind_group_layout,
            &viewport_buffer,
            &color_atlas.view,
            &sampler,
        );

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text pipeline v2"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(GlyphInstance::layout())],
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

        let color_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text color pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &color_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(GlyphInstance::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &color_shader,
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

        let decoration_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text decoration pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &decoration_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(DecorationInstance::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &decoration_shader,
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
            label: Some("text instance buffer"),
            size: (Self::INITIAL_CAPACITY * size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let color_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text color instance buffer"),
            size: (Self::INITIAL_CAPACITY * size_of::<GlyphInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let decoration_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("text decoration instance buffer"),
            size: (Self::INITIAL_CAPACITY * size_of::<DecorationInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            rasterizer,
            executor: BatchExecutor::new(),
            postponed_preparation: false,
            atlas,
            color_atlas,
            pipeline,
            color_pipeline,
            viewport_buffer,
            bind_group_layout,
            bind_group,
            color_bind_group,
            sampler,
            instance_buffer,
            instance_policy: InstanceBufferPolicy::new(Self::INITIAL_CAPACITY),
            instances: Vec::new(),
            instance_upload: FrameUpload::new(),
            color_instance_buffer,
            color_instance_policy: InstanceBufferPolicy::new(Self::INITIAL_CAPACITY),
            color_instances: Vec::new(),
            color_instance_upload: FrameUpload::new(),
            decoration_pipeline,
            decoration_instance_buffer,
            decoration_instance_policy: InstanceBufferPolicy::new(Self::INITIAL_CAPACITY),
            decoration_instances: Vec::new(),
            decoration_instance_upload: FrameUpload::new(),
            atlas_generation: 0,
            color_atlas_generation: 0,
            last_viewport: (0, 0),
            last_prepared_surface: (0, 0),
            layout_cache: LayoutCache::new(Self::LAYOUT_CACHE_CAPACITY),
            shaping_cache: HashMap::new(),
            request_ranges: Vec::new(),
        }
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        viewport_buffer: &wgpu::Buffer,
        atlas_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: viewport_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    pub fn preload_text(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        font_size: f32,
    ) {
        for (key, glyph) in self.rasterizer.preload_text(text, font_size, None) {
            self.insert_rasterized_glyph(
                device,
                queue,
                key,
                glyph.is_color,
                glyph.width,
                glyph.height,
                &glyph.bitmap,
            );
            self.rasterizer.release_bitmap(key);
        }

        self.flush_atlas(device, queue);
    }

    /// Common glyph set warmed by [`warm_glyph_set`](Self::warm_glyph_set):
    /// the space, digits, lowercase and uppercase ASCII letters, and the
    /// printable ASCII punctuation. Rasterizing this set fills the glyph atlas
    /// (the heavier of the two per-glyph costs) so even brand-new, never-seen
    /// strings only pay HarfRust shaping and never glyph rasterization.
    const COMMON_GLYPH_SET: &'static str = " 0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~";

    /// Insert a single rasterized glyph bitmap into the matching atlas,
    /// skipping empty (zero-area) glyphs and glyphs already present.
    #[allow(clippy::too_many_arguments)]
    fn insert_rasterized_glyph(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: GlyphKey,
        is_color: bool,
        width: u32,
        height: u32,
        bitmap: &[u8],
    ) {
        if width == 0 || height == 0 {
            return;
        }
        if is_color {
            if self.color_atlas.get(&key).is_none() {
                self.color_atlas
                    .get_or_insert(device, queue, key, width, height, bitmap);
            }
        } else if self.atlas.get(&key).is_none() {
            self.atlas
                .get_or_insert(device, queue, key, width, height, bitmap);
        }
    }

    /// Upload any pending atlas changes to the GPU and rebuild the bind groups
    /// if either atlas texture was reallocated (generation changed). Shared by
    /// the warm-up paths and `preload_text`.
    fn flush_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.atlas.upload(queue);
        self.color_atlas.upload(queue);

        let atlas_gen = self.atlas.generation();
        if atlas_gen != self.atlas_generation {
            self.atlas_generation = atlas_gen;
            self.bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.viewport_buffer,
                &self.atlas.view,
                &self.sampler,
            );
        }

        let color_gen = self.color_atlas.generation();
        if color_gen != self.color_atlas_generation {
            self.color_atlas_generation = color_gen;
            self.color_bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.viewport_buffer,
                &self.color_atlas.view,
                &self.sampler,
            );
        }
    }

    /// Level 2 warm-up — pre-rasterize the common ASCII glyph set at each of
    /// the supplied font sizes so the glyph atlas is already populated
    /// before the first frame is drawn. Because rasterization (not shaping)
    /// is the heavier per-glyph cost, this keeps even brand-new strings
    /// (numbers, usernames, live text) cheap: they only pay shaping, never
    /// glyph rasterization.
    pub fn warm_glyph_set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        font_sizes: &[f32],
    ) {
        for &font_size in font_sizes {
            for (key, glyph) in self
                .rasterizer
                .preload_text(Self::COMMON_GLYPH_SET, font_size, None)
            {
                self.insert_rasterized_glyph(
                    device,
                    queue,
                    key,
                    glyph.is_color,
                    glyph.width,
                    glyph.height,
                    &glyph.bitmap,
                );
                self.rasterizer.release_bitmap(key);
            }
        }

        self.flush_atlas(device, queue);
    }

    /// Level 1 warm-up — pre-shape and lay out a known static string at the
    /// given font size, populating the shaping cache, the layout cache, and the
    /// glyph atlas. After this, the string renders on the ~1 ms cache-hit path
    /// from the very first frame instead of paying the cold HarfRust
    /// shaping + rasterization cost (the 27–86 ms spikes) on first paint.
    ///
    /// `layout_width` must match the wrapping width the string will be drawn
    /// with (0.0 for non-wrapping `Clip` text) for the layout cache to hit;
    /// even if it differs the width-independent shaping cache still hits,
    /// so the expensive shaping work is warmed regardless.
    pub fn warm_text(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        font_size: f32,
        layout_width: f32,
    ) {
        self.warm_layout(device, queue, text, font_size, layout_width);
        self.flush_atlas(device, queue);
    }

    /// Shared core of [`warm_text`](Self::warm_text): shape + lay out `text`,
    /// populating both caches exactly like `prepare` does, then rasterize every
    /// positioned glyph into the atlas. Does not upload/flush the atlas
    /// (callers batch a single `flush_atlas` afterwards).
    fn warm_layout(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        text: &str,
        font_size: f32,
        layout_width: f32,
    ) {
        let cache_key = LayoutCacheKey::new(
            text,
            font_size,
            layout_width,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
            None,
        );

        // Populate (or reuse) the layout/shaping caches, mirroring the hot path
        // in `prepare`, then snapshot the glyph keys so the cache borrow ends
        // before we touch the rasterizer/atlas again.
        if !self.layout_cache.touch(&cache_key) {
            let shaping_cache = &mut self.shaping_cache;
            let rasterizer = &mut self.rasterizer;
            let shaped_key = ShapingCacheKey::new(
                text,
                font_size,
                FontFamily::SANS_SERIF,
                FontStyle::Normal,
                FontWeight::Normal.numeric(),
                None,
            );
            let shaped_text = shaping_cache.entry(shaped_key).or_insert_with(|| {
                shape_text_styled(
                    rasterizer,
                    text,
                    font_size,
                    FontFamily::SANS_SERIF,
                    FontWeight::Normal,
                    FontStyle::Normal,
                    None,
                )
            });
            let positioned = layout_shaped_text(shaped_text, 0.0, 0.0, layout_width);
            self.layout_cache.insert(cache_key.clone(), positioned);
        }
        let glyphs: Vec<(GlyphKey, f32)> = self
            .layout_cache
            .peek(&cache_key)
            .expect("layout warmed above")
            .iter()
            .map(|pg| (pg.glyph_key, pg.font_size))
            .collect();

        for (key, glyph_font_size) in glyphs {
            let (is_color, width, height) = {
                let rg = self.rasterizer.rasterize_key(key, glyph_font_size);
                (rg.is_color, rg.width, rg.height)
            };
            if width == 0 || height == 0 {
                continue;
            }
            if is_color {
                if self.color_atlas.get(&key).is_none() {
                    let rg = self.rasterizer.rasterize_bitmap_key(key, glyph_font_size);
                    self.color_atlas
                        .get_or_insert(device, queue, key, rg.width, rg.height, &rg.bitmap);
                    self.rasterizer.release_bitmap(key);
                }
            } else if self.atlas.get(&key).is_none() {
                let rg = self.rasterizer.rasterize_bitmap_key(key, glyph_font_size);
                self.atlas
                    .get_or_insert(device, queue, key, rg.width, rg.height, &rg.bitmap);
                self.rasterizer.release_bitmap(key);
            }
        }
    }
    /// Whether the last frame left text unprepared because it ran out of
    /// budget.
    ///
    /// Only text that could not show a pixel is ever left behind, so a frame
    /// that reports `true` rendered exactly what it owed the screen — but the
    /// content a viewport asked for ahead of itself is not ready yet, and will
    /// cost its owner the arrival frame unless another frame picks it up. A
    /// presenter may use this as a hint while another frame source, such as
    /// active scrolling, is already keeping the render loop alive.
    #[inline]
    pub fn has_postponed_preparation(&self) -> bool {
        self.postponed_preparation
    }

    /// Returns whether an off-screen request is missing a final positioned
    /// layout. A layout hit needs no ahead-of-view executor work; visible
    /// requests still go through the mandatory path below because their glyphs
    /// may have to be prepared for this frame.
    #[inline]
    fn request_has_layout_miss(&self, req: &TextDrawRequest) -> bool {
        let synthesized: [RichTextSpan; 1];
        let spans: &[RichTextSpan] = if req.spans.is_empty() {
            synthesized = [RichTextSpan::new(req.text.clone())];
            &synthesized
        } else {
            &req.spans
        };

        spans.iter().any(|span| {
            let font_size = span.font_size.unwrap_or(req.font_size);
            let font_weight = span
                .font_weight
                .or(req.font_weight)
                .unwrap_or(FontWeight::Normal.numeric());
            let layout_width = match req.overflow {
                TextOverflowMode::Wrap | TextOverflowMode::Ellipsis => req.bounds_width,
                TextOverflowMode::Clip => 0.0,
            };
            let keys = span_layout_keys(
                &self.shaping_cache,
                &span.text,
                font_size,
                req.font_family,
                req.font_style,
                font_weight,
                req.language,
                layout_width,
            );
            self.layout_cache
                .peek_with_fallback(&keys.primary, keys.fallback.as_ref())
                .is_none()
        })
    }

    /// Shapes, lays out and rasterizes everything `requests` need that the
    /// caches do not already hold, and commits the results.
    ///
    /// Returns `false` when a stage could not complete — a poisoned worker, a
    /// batch that lost a result — in which case nothing was committed for
    /// these requests and the caller must not assume their layouts exist.
    fn prepare_content(&mut self, requests: &[&TextDrawRequest]) -> bool {
        let clear_shaping_cache = self.shaping_cache.len() > Self::SHAPING_CACHE_CAPACITY;

        let mut shaping_batch = PreparationBatch::new();
        let mut layout_batch = PreparationBatch::new();
        for req in requests {
            let synthesized: [RichTextSpan; 1];
            let spans: &[RichTextSpan] = if req.spans.is_empty() {
                synthesized = [RichTextSpan::new(req.text.clone())];
                &synthesized
            } else {
                &req.spans
            };

            for span in spans {
                let font_size = span.font_size.unwrap_or(req.font_size);
                let font_weight = span
                    .font_weight
                    .or(req.font_weight)
                    .unwrap_or(FontWeight::Normal.numeric());
                let layout_width = match req.overflow {
                    TextOverflowMode::Wrap | TextOverflowMode::Ellipsis => req.bounds_width,
                    TextOverflowMode::Clip => 0.0,
                };
                let keys = span_layout_keys(
                    &self.shaping_cache,
                    &span.text,
                    font_size,
                    req.font_family,
                    req.font_style,
                    font_weight,
                    req.language,
                    layout_width,
                );
                if self
                    .layout_cache
                    .get_with_fallback(&keys.primary, keys.fallback.as_ref())
                    .is_some()
                {
                    continue;
                }

                if clear_shaping_cache || !self.shaping_cache.contains_key(&keys.shaping_key) {
                    shaping_batch.push(
                        keys.shaping_key.clone(),
                        ShapingInput {
                            text: span.text.clone(),
                            font_size,
                            font_family: req.font_family,
                            font_style: req.font_style,
                            font_weight,
                            language: req.language,
                        },
                    );
                }
                layout_batch.push(
                    keys.primary,
                    LayoutInput {
                        shaping_key: keys.shaping_key,
                        layout_width: keys.layout_width,
                    },
                );
            }
        }

        let font_snapshot = self.rasterizer.font_snapshot();
        let shaping_results = self.executor.execute_with_context(
            shaping_batch.jobs(),
            || GlyphPreparationContext::new(font_snapshot.clone()),
            |context, job| {
                let input = &job.input;
                Some(prepare_shaped_text(
                    context,
                    &input.text,
                    input.font_size,
                    input.font_family,
                    FontWeight::Value(u32::from(input.font_weight)),
                    input.font_style,
                    input.language,
                ))
            },
        );
        let Ok(shaping_results) = shaping_results else {
            return false;
        };
        let Ok(prepared_shaping) = shaping_batch.merge(shaping_results) else {
            return false;
        };
        let prepared_shaping = prepared_shaping.into_iter().collect::<HashMap<_, _>>();

        // Positioning is pure arithmetic over the shaped clusters — the pixel
        // box of every glyph was baked in at shaping time — so the layout jobs
        // need no rasterizer, no fonts and no per-worker context.
        let layout_results = self.executor.execute_with_context(
            layout_batch.jobs(),
            || (),
            |(), job| {
                let input = &job.input;
                let shaped = prepared_shaping.get(&input.shaping_key).or_else(|| {
                    (!clear_shaping_cache)
                        .then(|| self.shaping_cache.get(&input.shaping_key))
                        .flatten()
                })?;
                Some(layout_shaped_text(shaped, 0.0, 0.0, input.layout_width))
            },
        );
        let Ok(layout_results) = layout_results else {
            return false;
        };
        let Ok(prepared_layouts) = layout_batch.merge(layout_results) else {
            return false;
        };
        let prepared_layouts = prepared_layouts.into_iter().collect::<HashMap<_, _>>();

        let mut fresh_glyphs = Vec::new();
        let mut queued_glyphs = HashSet::new();
        for req in requests {
            let synthesized: [RichTextSpan; 1];
            let spans: &[RichTextSpan] = if req.spans.is_empty() {
                synthesized = [RichTextSpan::new(req.text.clone())];
                &synthesized
            } else {
                &req.spans
            };

            for span in spans {
                let font_size = span.font_size.unwrap_or(req.font_size);
                let font_weight = span
                    .font_weight
                    .or(req.font_weight)
                    .unwrap_or(FontWeight::Normal.numeric());
                let layout_width = match req.overflow {
                    TextOverflowMode::Wrap | TextOverflowMode::Ellipsis => req.bounds_width,
                    TextOverflowMode::Clip => 0.0,
                };
                let keys = span_layout_keys(
                    &self.shaping_cache,
                    &span.text,
                    font_size,
                    req.font_family,
                    req.font_style,
                    font_weight,
                    req.language,
                    layout_width,
                );
                let Some(positioned) = prepared_layouts
                    .get(&keys.primary)
                    .map(Vec::as_slice)
                    .or_else(|| {
                        self.layout_cache
                            .peek_with_fallback(&keys.primary, keys.fallback.as_ref())
                    })
                else {
                    return false;
                };

                for glyph in positioned {
                    let key = glyph.glyph_key;
                    let needs_bitmap =
                        self.atlas.get(&key).is_none() && self.color_atlas.get(&key).is_none();
                    if self.rasterizer.needs_prepared_glyph(key, needs_bitmap)
                        && queued_glyphs.insert(key)
                    {
                        fresh_glyphs.push((key, glyph.font_size));
                    }
                }
            }
        }

        // Glyphs are prepared in runs sharing a face and a size, so the work
        // that belongs to the face — mapping the file, parsing its tables,
        // building the scaler and reading the metrics — is done once per run
        // instead of once per glyph. The runs are bounded, so a page of one
        // face at one size still spreads across the workers.
        let mut glyph_batch = PreparationBatch::new();
        for (order, run) in glyph_runs(fresh_glyphs).into_iter().enumerate() {
            glyph_batch.push(order, run);
        }

        let glyph_results = self.executor.execute_with_context(
            glyph_batch.jobs(),
            || GlyphPreparationContext::new(font_snapshot.clone()),
            |context, job| Some(context.prepare_glyph_run(&job.input)),
        );
        let Ok(glyph_results) = glyph_results else {
            return false;
        };
        let Ok(prepared_glyphs) = glyph_batch.merge(glyph_results) else {
            return false;
        };

        if clear_shaping_cache {
            self.shaping_cache.clear();
        }
        self.shaping_cache.extend(prepared_shaping);
        self.layout_cache.extend(prepared_layouts);
        for (_, glyphs) in prepared_glyphs {
            for (key, glyph) in glyphs {
                self.rasterizer.commit_prepared_glyph(key, glyph);
            }
        }

        true
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        is_srgb: bool,
        requests: &[TextDrawRequest],
        decorations: &[TextDecorationDraw],
    ) {
        let started = AnimInstant::now();

        // A scroll viewport hands its child more than it can show, so that a
        // line is prepared a few frames before it scrolls in instead of on the
        // frame its edge crosses the boundary. That moves the work rather than
        // removing it: during a fling fresh content arrives every frame, and
        // preparing all of it turns one visible stall into a continuous one.
        // What the frame owes the screen is prepared whatever it costs; the
        // rest is offered the time that is left, and what does not fit waits
        // for the frame this asks for. Dropping a visible glyph shows a blank,
        // dropping one the clip discards shows nothing at all.
        //
        // The same split bounds the *drawing* below: a request that cannot
        // show a pixel builds no glyph instances and reserves no atlas room,
        // so scrolling a text-heavy document costs the frame what is on
        // screen, not what the viewport asked for ahead of itself.

        // A layout the frame reads is stamped as used; opening the frame is
        // also when a cache that outgrew its capacity sheds the entries no
        // recent frame read — a resize's flood of transient widths, a closed
        // screen's lines — without touching the current screen's layouts.
        self.layout_cache.begin_frame();

        // A frame whose surface size changed is one step of a live resize:
        // every wrapped layout is keyed by a width that will be different
        // again next frame.
        let resizing = self.last_prepared_surface != (width, height);
        self.last_prepared_surface = (width, height);

        let surface = (width as f32, height as f32);
        let mut on_screen = Vec::with_capacity(requests.len());
        let mut ahead_of_view = Vec::new();
        let mut visible = vec![true; requests.len()];
        for (index, req) in requests.iter().enumerate() {
            // Glyphs can render slightly outside the box the request declares:
            // an ascender or italic left-bearing reaches above/left of the
            // origin, and a tight box can clip glyphs that overflow it. The
            // coarse on-screen test below would then drop a request whose
            // glyphs have already scrolled into (or not yet out of) the clip.
            // Inflate the box by the request's largest font size so the test
            // only ever errs toward keeping text — `glyph_intersects_clip`
            // does the precise per-glyph check afterwards. Non-positive extents
            // are left alone so `known_extent` still reads them as unbounded.
            let glyph_pad = req
                .spans
                .iter()
                .map(|span| span.font_size.unwrap_or(req.font_size))
                .fold(req.font_size, f32::max);
            let shadow_pad = req.shadow.map_or(0.0, shadow_padding);
            let pad = glyph_pad + shadow_pad;
            let bounds = [
                req.x - pad,
                req.y - pad,
                if req.bounds_width > 0.0 {
                    req.bounds_width + 2.0 * pad
                } else {
                    req.bounds_width
                },
                if req.bounds_height > 0.0 {
                    req.bounds_height + 2.0 * pad
                } else {
                    req.bounds_height
                },
            ];
            if request_is_on_screen(bounds, req.clip_rect, surface) {
                on_screen.push(req);
            } else {
                visible[index] = false;
                // The scroll cache window deliberately includes nearby
                // content, but a cache hit must not start shaping/layout
                // executor work just because it is off-screen.
                if self.request_has_layout_miss(req) {
                    ahead_of_view.push((index, req));
                }
            }
        }

        if !self.prepare_content(&on_screen) {
            return;
        }

        self.postponed_preparation = false;
        if !ahead_of_view.is_empty() {
            if resizing {
                // Laying the tail out now would spend the budget on layouts
                // keyed by a width the next resize frame invalidates, and
                // flood the layout cache with entries nothing will read. The
                // tail is postponed instead: a later real frame prepares it
                // at the width that will actually be drawn.
                self.postponed_preparation = true;
            } else {
                let mut chunk_requests = Vec::with_capacity(PREPARATION_CHUNK);
                let prepared = prepare_ahead_of_view(
                    &ahead_of_view,
                    PreparationBudget::starting_at(started, PREPARATION_BUDGET),
                    AnimInstant::now,
                    |chunk| {
                        chunk_requests.clear();
                        chunk_requests.extend(chunk.iter().map(|(_, req)| *req));
                        self.prepare_content(&chunk_requests)
                    },
                );

                self.postponed_preparation = prepared < ahead_of_view.len();
            }
        }

        let mut alpha_glyphs = Vec::new();
        let mut color_glyphs = Vec::new();
        let mut seen_glyphs = HashSet::new();
        for (index, req) in requests.iter().enumerate() {
            // An off-screen request contributes no pixel this frame, so it
            // reserves no atlas room either; its glyphs claim theirs on the
            // frame they scroll in, from the bitmaps preparation cached.
            if !visible[index] {
                continue;
            }

            let synthesized: [RichTextSpan; 1];
            let spans: &[RichTextSpan] = if req.spans.is_empty() {
                synthesized = [RichTextSpan::new(req.text.clone())];
                &synthesized
            } else {
                &req.spans
            };

            for span in spans {
                let font_size = span.font_size.unwrap_or(req.font_size);
                let font_weight = span
                    .font_weight
                    .or(req.font_weight)
                    .unwrap_or(FontWeight::Normal.numeric());
                let layout_width = match req.overflow {
                    TextOverflowMode::Wrap | TextOverflowMode::Ellipsis => req.bounds_width,
                    TextOverflowMode::Clip => 0.0,
                };
                let keys = span_layout_keys(
                    &self.shaping_cache,
                    &span.text,
                    font_size,
                    req.font_family,
                    req.font_style,
                    font_weight,
                    req.language,
                    layout_width,
                );
                // The stamping read: this is the loop that walks exactly the
                // layouts the frame draws, so it is where the working set is
                // marked as alive.
                let Some(positioned) = self
                    .layout_cache
                    .get_with_fallback(&keys.primary, keys.fallback.as_ref())
                else {
                    return;
                };

                for glyph in positioned {
                    let key = glyph.glyph_key;
                    if !seen_glyphs.insert(key) {
                        continue;
                    }
                    let Some((is_color, glyph_width, glyph_height)) =
                        self.rasterizer.cached_glyph_descriptor(key)
                    else {
                        return;
                    };
                    let descriptor = (key, glyph_width, glyph_height);
                    if is_color {
                        color_glyphs.push(descriptor);
                    } else {
                        alpha_glyphs.push(descriptor);
                    }
                }
            }
        }

        let alpha_plan = self.atlas.plan_batch(&alpha_glyphs);
        let color_plan = self.color_atlas.plan_batch(&color_glyphs);
        if alpha_plan == BatchCapacityPlan::Reject || color_plan == BatchCapacityPlan::Reject {
            return;
        }
        self.atlas.apply_batch_plan(alpha_plan);
        self.color_atlas.apply_batch_plan(color_plan);

        self.instances.clear();
        self.color_instances.clear();
        self.decoration_instances.clear();
        self.decoration_instances.extend(
            decorations
                .iter()
                .map(|decoration| decoration.to_instance()),
        );
        self.request_ranges.clear();
        self.request_ranges.reserve(requests.len());

        for (index, req) in requests.iter().enumerate() {
            // Record the glyph ranges this request will own. Both instance lists
            // are appended to in request order, so the slice for this request is
            // `[start, len_after)` in each list.
            let alpha_start = self.instances.len() as u32;
            let color_start = self.color_instances.len() as u32;

            // Off-screen text draws nothing this frame. Its range stays empty
            // so the ranges still line up with `requests`, which is what lets
            // a caller address one request's instances by index.
            if !visible[index] {
                self.request_ranges.push(TextRequestRange {
                    alpha_start,
                    alpha_end: alpha_start,
                    color_start,
                    color_end: color_start,
                });
                continue;
            }

            // Avoid cloning the span list on every frame (it ran even on a pure
            // cache hit). Borrow `req.spans` directly when present and only
            // allocate a one-element fallback when the request has no spans.
            let synthesized: [RichTextSpan; 1];
            let spans: &[RichTextSpan] = if req.spans.is_empty() {
                synthesized = [RichTextSpan::new(req.text.clone())];
                &synthesized
            } else {
                &req.spans
            };

            let mut cursor_x = req.x;
            let mut cursor_y = req.y;

            for span in spans {
                let font_size = span.font_size.unwrap_or(req.font_size);
                let color = span.color.unwrap_or(req.color);
                let font_weight = span
                    .font_weight
                    .or(req.font_weight)
                    .unwrap_or(FontWeight::Normal.numeric());
                // A weight of 600+ (semi-bold and up) is rendered bold. Which
                // glyphs of the span still have to be *drawn* bold by hand is
                // decided per glyph below: a line mixing scripts reaches its
                // stroke by two routes at once, and the glyphs already drawn
                // at the requested weight must not be emboldened again.
                let is_bold = font_weight >= BOLD_WEIGHT_THRESHOLD;
                // ponytail: synthetic (faux) italic via a horizontal shear in
                // the glyph shaders (0.25 ≈ 14°). Ceiling: not a real italic
                // face (no cursive glyph forms, advances unchanged). Upgrade
                // path: load a real italic/oblique face and key the atlas by it.
                let skew = if span.italic.unwrap_or(req.italic) {
                    0.25
                } else {
                    0.0
                };
                let layout_width = match req.overflow {
                    TextOverflowMode::Wrap | TextOverflowMode::Ellipsis => req.bounds_width,
                    TextOverflowMode::Clip => 0.0,
                };
                let keys = span_layout_keys(
                    &self.shaping_cache,
                    &span.text,
                    font_size,
                    req.font_family,
                    req.font_style,
                    font_weight,
                    req.language,
                    layout_width,
                );
                // Layout is always computed at origin (0, 0) so the cached
                // positions are purely relative and can be shifted cheaply.
                // A peek, not a get: the planning loop above already stamped
                // this frame's layouts.
                let positioned = self
                    .layout_cache
                    .peek_with_fallback(&keys.primary, keys.fallback.as_ref())
                    .expect("collected text layout must be committed before rendering");
                // Left-aligned lines — the overwhelmingly common case — all
                // start at the origin, so the per-line offset table (two
                // vectors rebuilt per span per frame) is only computed when
                // an alignment actually shifts something.
                let line_offsets = match req.horizontal_align {
                    TextHorizontalAlign::Left => None,
                    _ => Some(line_alignment_offsets(
                        &positioned_line_widths(positioned),
                        req.bounds_width,
                        req.horizontal_align,
                    )),
                };
                // The blend-space correction depends only on the span's
                // color; one `powf` serves every glyph of the span.
                let span_coverage_exponent = coverage_exponent(color);
                for pg in positioned {
                    let key = pg.glyph_key;
                    // One atlas probe answers both "where is the bitmap" and
                    // "is it color": a rasterized glyph lives in exactly one
                    // of the two atlases. Only a miss on both asks the
                    // rasterizer — a glyph arriving from the ahead-of-view
                    // cache, whose bitmap insert is a cache hit.
                    let (region, target_color_list) = if let Some(region) = self.atlas.get(&key) {
                        (region, false)
                    } else if let Some(region) = self.color_atlas.get(&key) {
                        (region, true)
                    } else {
                        let rg = self.rasterizer.rasterize_bitmap_key(key, pg.font_size);
                        let (is_color, glyph_width, glyph_height) =
                            (rg.is_color, rg.width, rg.height);
                        let region = if is_color {
                            self.color_atlas.get_or_insert(
                                device,
                                queue,
                                key,
                                glyph_width,
                                glyph_height,
                                &rg.bitmap,
                            )
                        } else {
                            self.atlas.get_or_insert(
                                device,
                                queue,
                                key,
                                glyph_width,
                                glyph_height,
                                &rg.bitmap,
                            )
                        };
                        self.rasterizer.release_bitmap(key);
                        (region, is_color)
                    };

                    let size = glyph_quad_size((region.width, region.height));
                    let line_offset = line_offsets
                        .as_ref()
                        .map_or(0.0, |offsets| offsets[pg.line_index]);
                    let position =
                        snap_to_pixel_grid([pg.x + cursor_x + line_offset, pg.y + cursor_y]);
                    let foreground_visible = glyph_intersects_clip(position, size, req.clip_rect);
                    let shadow_visible = req.shadow.is_some_and(|shadow| {
                        shadow_intersects_clip(position, size, shadow, req.clip_rect)
                    });
                    if !foreground_visible && !shadow_visible {
                        continue;
                    }
                    let instance = GlyphInstance {
                        position,
                        size,
                        uv_rect: [
                            region.x as f32,
                            region.y as f32,
                            (region.x + region.width) as f32,
                            (region.y + region.height) as f32,
                        ],
                        color,
                        clip_rect: req.clip_rect,
                        clip_border_radius: req.clip_border_radius,
                        skew,
                        coverage_exponent: span_coverage_exponent,
                        _pad: [0.0; 2],
                    };

                    if let Some(shadow) = req.shadow
                        && shadow_visible
                        && shadow_is_visible(shadow.color)
                    {
                        let offset_x = shadow
                            .offset_x
                            .is_finite()
                            .then_some(shadow.offset_x)
                            .unwrap_or(0.0);
                        let offset_y = shadow
                            .offset_y
                            .is_finite()
                            .then_some(shadow.offset_y)
                            .unwrap_or(0.0);
                        let blur = shadow
                            .blur
                            .is_finite()
                            .then_some(shadow.blur.max(0.0))
                            .unwrap_or(0.0);
                        let sample_count = if blur == 0.0 { 1 } else { 8 };
                        let shadow_coverage_exponent = coverage_exponent(shadow.color);
                        for sample in 0..sample_count {
                            let (blur_x, blur_y) = if sample_count == 1 {
                                (0.0, 0.0)
                            } else {
                                let angle =
                                    sample as f32 * std::f32::consts::TAU / sample_count as f32;
                                (angle.cos() * blur, angle.sin() * blur)
                            };
                            let mut shadow_instance = instance;
                            shadow_instance.position[0] += offset_x + blur_x;
                            shadow_instance.position[1] += offset_y + blur_y;
                            shadow_instance.color = shadow.color;
                            shadow_instance.coverage_exponent = shadow_coverage_exponent;
                            if target_color_list {
                                self.color_instances.push(shadow_instance);
                            } else {
                                self.instances.push(shadow_instance);
                            }
                        }
                    }

                    if req.draw_glyphs && foreground_visible {
                        if target_color_list {
                            self.color_instances.push(instance);
                        } else {
                            self.instances.push(instance);
                            if is_bold
                                && self
                                    .rasterizer
                                    .glyph_needs_synthetic_bold(key, font_weight)
                            {
                                let mut bold = instance;
                                bold.position[0] += (pg.font_size * 0.03).max(0.5);
                                self.instances.push(bold);
                            }
                        }
                    }
                }

                if let Some(last) = positioned.last() {
                    cursor_x += last.x + last.width as f32;
                    cursor_y += last.y;
                }
            }

            self.request_ranges.push(TextRequestRange {
                alpha_start,
                alpha_end: self.instances.len() as u32,
                color_start,
                color_end: self.color_instances.len() as u32,
            });
        }

        // Now that every glyph has been inserted, the atlases have reached
        // their final dimensions for this frame. Resolve UVs against those
        // final dimensions so glyphs inserted before a mid-frame `grow()` are
        // not left referencing stale (smaller) atlas sizes.
        let (aw, ah) = (self.atlas.width, self.atlas.height);
        for instance in &mut self.instances {
            instance.uv_rect = normalize_pixel_uv_rect(instance.uv_rect, aw, ah);
        }
        let (cw, ch) = (self.color_atlas.width, self.color_atlas.height);
        for instance in &mut self.color_instances {
            instance.uv_rect = normalize_pixel_uv_rect(instance.uv_rect, cw, ch);
        }

        // Upload both atlases if new glyphs were added.
        self.atlas.upload(queue);
        self.color_atlas.upload(queue);

        // Rebuild bind groups only when their atlas texture was recreated (grow).
        let atlas_gen = self.atlas.generation();
        if atlas_gen != self.atlas_generation {
            self.atlas_generation = atlas_gen;
            self.bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.viewport_buffer,
                &self.atlas.view,
                &self.sampler,
            );
        }
        let color_gen = self.color_atlas.generation();
        if color_gen != self.color_atlas_generation {
            self.color_atlas_generation = color_gen;
            self.color_bind_group = Self::create_bind_group(
                device,
                &self.bind_group_layout,
                &self.viewport_buffer,
                &self.color_atlas.view,
                &self.sampler,
            );
        }
        #[cfg(target_os = "android")]
        let is_srgb_f32 = 2.0_f32;
        #[cfg(not(target_os = "android"))]
        let is_srgb_f32 = if is_srgb { 1.0_f32 } else { 0.0 };
        if self.last_viewport != (width, height) {
            self.last_viewport = (width, height);
            queue.write_buffer(
                &self.viewport_buffer,
                0,
                bytemuck::cast_slice(&[width as f32, height as f32, is_srgb_f32, 0.0]),
            );
        }

        let previous_capacity = self.instance_policy.capacity();
        self.instance_policy.record_usage(self.instances.len());
        if self.instance_policy.capacity() != previous_capacity {
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text instance buffer"),
                size: (self.instance_policy.capacity() * size_of::<GlyphInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_upload.invalidate();
        }

        let previous_color_capacity = self.color_instance_policy.capacity();
        self.color_instance_policy
            .record_usage(self.color_instances.len());
        if self.color_instance_policy.capacity() != previous_color_capacity {
            self.color_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text color instance buffer"),
                size: (self.color_instance_policy.capacity() * size_of::<GlyphInstance>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.color_instance_upload.invalidate();
        }

        let previous_decoration_capacity = self.decoration_instance_policy.capacity();
        self.decoration_instance_policy
            .record_usage(self.decoration_instances.len());
        if self.decoration_instance_policy.capacity() != previous_decoration_capacity {
            self.decoration_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("text decoration instance buffer"),
                size: (self.decoration_instance_policy.capacity() * size_of::<DecorationInstance>())
                    as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.decoration_instance_upload.invalidate();
        }

        // Upload the instance lists — each in one write, and only when their
        // bytes differ from what the GPU buffer already holds. Static text is
        // the common case, and it now costs no upload at all.
        self.instance_upload
            .upload(queue, &self.instance_buffer, &self.instances);
        self.color_instance_upload
            .upload(queue, &self.color_instance_buffer, &self.color_instances);
        self.decoration_instance_upload.upload(
            queue,
            &self.decoration_instance_buffer,
            &self.decoration_instances,
        );
    }

    /// How many glyph quads the last [`prepare`] committed for drawing, as
    /// `(alpha, color)` counts.
    ///
    /// Diagnostics: lets tests and tooling observe how much of a frame's text
    /// actually reached the instance buffers — a request that cannot show a
    /// pixel must contribute zero.
    ///
    /// [`prepare`]: TextPipelineV2::prepare
    pub fn frame_glyph_instances(&self) -> (usize, usize) {
        (self.instances.len(), self.color_instances.len())
    }

    /// How many positioned layouts the cache currently holds.
    ///
    /// Diagnostics: lets tests and tooling observe how layout work scales —
    /// a resize that wraps nothing must reuse its layouts instead of minting
    /// a fresh set per width.
    pub fn layout_cache_entries(&self) -> usize {
        self.layout_cache.len()
    }

    pub fn instance_buffer_bytes(&self) -> u64 {
        (self.instance_policy.capacity() * size_of::<GlyphInstance>()) as u64
            + (self.color_instance_policy.capacity() * size_of::<GlyphInstance>()) as u64
            + (self.decoration_instance_policy.capacity() * size_of::<DecorationInstance>()) as u64
    }

    pub fn glyph_bitmap_cache_bytes(&self) -> usize {
        self.rasterizer.bitmap_cache_bytes()
    }

    pub fn glyph_atlas_bytes(&self) -> u64 {
        self.atlas.memory_bytes() + self.color_atlas.memory_bytes()
    }

    pub fn cached_glyph_count(&self) -> usize {
        self.rasterizer.cached_glyph_count()
    }

    /// Draw a single text request's glyphs at the current point in the render
    /// pass: alpha-coverage glyphs first, then color emoji so they ride on top
    /// of any monochrome glyphs sharing the same line. Drawing per request —
    /// instead of all text in one final pass — is what lets text obey z-order
    /// against rects/images (e.g. a `Stack`'s upper layer can now cover text
    /// belonging to a lower layer). `index` matches the request order passed to
    /// `prepare`.
    pub fn render_request(&self, pass: &mut wgpu::RenderPass<'_>, index: usize) {
        let Some(range) = self.request_ranges.get(index) else {
            return;
        };

        if range.alpha_end > range.alpha_start {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
            pass.draw(0..6, range.alpha_start..range.alpha_end);
        }

        if range.color_end > range.color_start {
            pass.set_pipeline(&self.color_pipeline);
            pass.set_bind_group(0, &self.color_bind_group, &[]);
            pass.set_vertex_buffer(0, self.color_instance_buffer.slice(..));
            pass.draw(0..6, range.color_start..range.color_end);
        }
    }

    /// Draw a single decoration line (underline/overline/strike) at its
    /// position in the draw stream so it layers with its text. One
    /// decoration request maps to exactly one instance. Reuses the alpha
    /// `bind_group` (it only needs the viewport uniform).
    pub fn render_decoration(&self, pass: &mut wgpu::RenderPass<'_>, index: usize) {
        if index >= self.decoration_instances.len() {
            return;
        }
        pass.set_pipeline(&self.decoration_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.decoration_instance_buffer.slice(..));
        let start = index as u32;
        pass.draw(0..6, start..start + 1);
    }

    /// Measure text width using the rasterizer.
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> f32 {
        self.rasterizer.measure_text(text, font_size)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TextDecorationDraw, TextShadowRequest, coverage_exponent, glyph_intersects_clip,
        glyph_quad_size, normalize_pixel_uv_rect, shadow_intersects_clip, shadow_is_visible,
        shadow_padding, snap_to_pixel_grid,
    };

    /// How many atlas texels one pixel of `quad_size` spans.
    ///
    /// The pipeline keeps this at exactly `[1.0, 1.0]`; anything else means the
    /// sampler is rescaling the glyph.
    fn texel_scale(quad_size: [f32; 2], region_size: (u32, u32)) -> [f32; 2] {
        let axis = |quad: f32, region: u32| {
            if region == 0 {
                0.0
            } else {
                region as f32 / quad
            }
        };

        [
            axis(quad_size[0], region_size.0),
            axis(quad_size[1], region_size.1),
        ]
    }

    #[test]
    fn glyph_culling_keeps_unclipped_and_partially_visible_glyphs() {
        assert!(glyph_intersects_clip(
            [500.0, 500.0],
            [20.0, 20.0],
            [0.0, 0.0, -1.0, 0.0],
        ));
        assert!(glyph_intersects_clip(
            [90.0, 90.0],
            [20.0, 20.0],
            [0.0, 0.0, 100.0, 100.0],
        ));
        assert!(glyph_intersects_clip(
            [-10.0, 10.0],
            [20.0, 20.0],
            [0.0, 0.0, 100.0, 100.0],
        ));
    }

    #[test]
    fn glyph_culling_rejects_glyphs_fully_outside_clip() {
        assert!(!glyph_intersects_clip(
            [101.0, 10.0],
            [20.0, 20.0],
            [0.0, 0.0, 100.0, 100.0],
        ));
        assert!(!glyph_intersects_clip(
            [10.0, -21.0],
            [20.0, 20.0],
            [0.0, 0.0, 100.0, 100.0],
        ));
    }

    #[test]
    fn shadow_culling_keeps_a_shadow_inside_an_otherwise_outside_clip() {
        let shadow = TextShadowRequest {
            offset_x: -24.0,
            offset_y: 0.0,
            blur: 2.0,
            color: [0.0, 0.0, 0.0, 0.5],
        };

        assert!(shadow_intersects_clip(
            [105.0, 10.0],
            [8.0, 12.0],
            shadow,
            [0.0, 0.0, 100.0, 100.0],
        ));
        assert_eq!(shadow_padding(shadow), 26.0);
    }

    #[test]
    fn transparent_or_non_finite_shadow_alpha_paints_nothing() {
        assert!(!shadow_is_visible([0.0, 0.0, 0.0, 0.0]));
        assert!(!shadow_is_visible([0.0, 0.0, 0.0, f32::NAN]));
        assert!(shadow_is_visible([0.0, 0.0, 0.0, 0.5]));
    }

    #[test]
    fn pixel_uv_rect_is_normalized_against_the_final_atlas_size() {
        let pixel_rect = [64.0, 32.0, 96.0, 64.0];

        assert_eq!(
            normalize_pixel_uv_rect(pixel_rect, 256, 128),
            [0.25, 0.25, 0.375, 0.5]
        );
        assert_eq!(
            normalize_pixel_uv_rect(pixel_rect, 512, 256),
            [0.125, 0.125, 0.1875, 0.25]
        );
    }

    // A glyph's coverage is written as an alpha the GPU blends in *linear*
    // light on an sRGB target. Blending there is not what the rasterizer meant:
    // half coverage of black over a light background comes out far lighter than
    // half way to black, so a stroke's apparent weight depends on how much of it
    // sits on partially covered pixels. The exponent below is what puts the
    // weight back.
    #[test]
    fn dark_text_has_its_partial_coverage_strengthened() {
        let exponent = coverage_exponent([0.0, 0.0, 0.0, 1.0]);

        assert!(exponent < 1.0, "dark text must gain coverage: {exponent}");
        assert!((0.5_f32.powf(exponent) - 0.729).abs() < 0.01);
    }

    #[test]
    fn light_text_has_its_partial_coverage_weakened() {
        let exponent = coverage_exponent([1.0, 1.0, 1.0, 1.0]);

        assert!(exponent > 1.0, "light text must lose coverage: {exponent}");
        assert!((0.5_f32.powf(exponent) - 0.217).abs() < 0.01);
    }

    // Mid-luminance text is the one case linear blending already gets right,
    // and the correction must not disturb it — otherwise every mid-tone label
    // in the framework changes weight for nothing.
    #[test]
    fn mid_luminance_text_is_left_alone() {
        let exponent = coverage_exponent([0.5, 0.5, 0.5, 1.0]);

        assert!((exponent - 1.0).abs() < 0.05, "{exponent}");
    }

    // Luminance, not the plain channel average: pure blue is dark, pure green
    // is light, and they must be corrected in opposite directions.
    #[test]
    fn the_exponent_follows_perceived_luminance() {
        assert!(coverage_exponent([0.0, 0.0, 1.0, 1.0]) < 1.0);
        assert!(coverage_exponent([0.0, 1.0, 0.0, 1.0]) > 1.0);
    }

    #[test]
    fn colors_outside_the_unit_range_are_still_answered() {
        for color in [
            [-1.0, -1.0, -1.0, 1.0],
            [2.0, 2.0, 2.0, 1.0],
            [f32::NAN, 0.0, 0.0, 1.0],
        ] {
            let exponent = coverage_exponent(color);
            assert!(
                exponent.is_finite() && exponent > 0.0,
                "{color:?} produced {exponent}"
            );
        }
    }

    // The defect in one number: black text at half coverage over a light grey
    // background. Gamma-space compositing — what the rasterizer's coverage
    // means, and what a native text stack produces — lands at half the
    // background's sRGB value; blending the raw coverage in linear light lands
    // far lighter. The correction has to close most of that gap.
    #[test]
    fn corrected_coverage_lands_near_gamma_space_compositing() {
        let background = 0.647_f32;
        let target = background * 0.5;

        let composite = |alpha: f32| {
            let background_linear = srgb_to_linear(background);
            linear_to_srgb(background_linear * (1.0 - alpha))
        };

        let raw = composite(0.5);
        let corrected = composite(0.5_f32.powf(coverage_exponent([0.0, 0.0, 0.0, 1.0])));

        assert!(raw > target + 0.1, "the defect must be visible: {raw}");
        assert!(
            (corrected - target).abs() < (raw - target).abs() * 0.5,
            "correction left {corrected}, target {target}, was {raw}"
        );
    }

    fn srgb_to_linear(channel: f32) -> f32 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    fn linear_to_srgb(channel: f32) -> f32 {
        if channel <= 0.0031308 {
            channel * 12.92
        } else {
            1.055 * channel.powf(1.0 / 2.4) - 0.055
        }
    }

    // A glyph's quad is sized by the pipeline, but the pixels inside it come
    // from the atlas region its UVs address. The two are produced by different
    // passes — layout measures the glyph, the atlas stores whatever bitmap was
    // rasterized for its key — so nothing but this invariant keeps them equal,
    // and a single pixel of disagreement resamples the whole glyph.
    #[test]
    fn a_glyph_quad_samples_one_texel_per_pixel() {
        for region in [(38, 38), (1, 1), (12, 27)] {
            assert_eq!(texel_scale(glyph_quad_size(region), region), [1.0, 1.0]);
        }
    }

    // The defect this invariant exists for: the layout measured the glyph one
    // pixel smaller than the bitmap the atlas holds. Sizing the quad by that
    // measurement rescales every stroke through the sampler.
    #[test]
    fn a_quad_disagreeing_with_its_region_resamples_the_glyph() {
        let scale = texel_scale([38.0, 38.0], (39, 39));

        assert!(scale[0] != 1.0 && scale[1] != 1.0, "{scale:?}");
    }

    // An empty region has nothing to sample; the scale must stay finite rather
    // than divide its way to infinity.
    #[test]
    fn an_empty_region_has_no_scale() {
        assert_eq!(texel_scale(glyph_quad_size((0, 0)), (0, 0)), [0.0, 0.0]);
    }

    #[test]
    fn a_glyph_quad_is_placed_on_a_whole_pixel() {
        assert_eq!(snap_to_pixel_grid([10.4, 20.6]), [10.0, 21.0]);
        assert_eq!(snap_to_pixel_grid([-3.4, -0.2]), [-3.0, -0.0]);
        assert_eq!(snap_to_pixel_grid([7.0, 9.0]), [7.0, 9.0]);
    }

    // The defect this guards: the same label rendered twice, once at an integer
    // origin and once shifted by half a pixel — as a line centred in a box too
    // narrow for it is — must produce the same quads, or the linear atlas
    // sampler blurs one of them and its strokes lose weight.
    #[test]
    fn the_same_text_lands_on_the_same_pixels_wherever_its_line_starts() {
        let glyph_offsets = [0.0_f32, 6.34, 12.68, 19.02];
        let crisp: Vec<[f32; 2]> = glyph_offsets
            .iter()
            .map(|offset| snap_to_pixel_grid([40.0 + offset, 12.0]))
            .collect();
        let shifted: Vec<[f32; 2]> = glyph_offsets
            .iter()
            .map(|offset| snap_to_pixel_grid([40.5 + offset, 12.0]))
            .collect();

        for position in crisp.iter().chain(&shifted) {
            assert_eq!(position[0], position[0].round());
            assert_eq!(position[1], position[1].round());
        }
        let phases: Vec<f32> = shifted
            .iter()
            .zip(&crisp)
            .map(|(shifted, crisp)| shifted[0] - crisp[0])
            .collect();
        assert!(
            phases.iter().all(|phase| phase.abs() <= 1.0),
            "snapping moved a glyph by more than a pixel: {phases:?}"
        );
    }

    // Guards the CPU->GPU packing of a decoration line: `params` must be
    // [style_id, thickness, period, band_height] and geometry must map to the
    // instance's position/size, matching what `text_decoration.wgsl` reads.
    #[test]
    fn decoration_instance_packing() {
        let draw = TextDecorationDraw {
            x: 10.0,
            y: 20.0,
            width: 120.0,
            band_height: 6.0,
            thickness: 2.0,
            period: 8.0,
            style: 4, // Wavy
            color: [1.0, 0.0, 0.0, 1.0],
            clip_rect: [0.0, 0.0, -1.0, 0.0],
            clip_border_radius: [0.0; 4],
        };
        let inst = draw.to_instance();
        assert_eq!(inst.position, [10.0, 20.0]);
        assert_eq!(inst.size, [120.0, 6.0]);
        assert_eq!(inst.color, [1.0, 0.0, 0.0, 1.0]);
        // params: style, thickness, period, band_height (band_height duplicated
        // so the fragment shader has it without relying on the interpolated size).
        assert_eq!(inst.params, [4.0, 2.0, 8.0, 6.0]);
    }
}
