//! Pluggable GPU backends for the Cupid rendering engine.
//!
//! When the `pluggable-backend-exp` feature is enabled, the renderer and all
//! pipelines are generic over a [`GpuBackend`] type parameter. This module
//! defines the trait surface: the operations each backend must provide, the
//! descriptor types they consume, and the render-pass operations a backend's
//! temporary pass object must support.
//!
//! # Stability
//!
//! This API is **experimental** (`pluggable-backend-exp`). It will evolve as
//! backends beyond the wgpu adapter mature.
//!
//! # Layout
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`self`] | `GpuBackend` trait, `GpuRenderPass` trait, shared descriptors |
//! | [`wgpu`] | `WgpuBackend` — delegates every operation to the `wgpu` crate |
//! | `metal` | `MetalBackend` — native Metal via `objc2-metal` (future) |

use std::num::NonZeroU32;
use std::ops::Range;

// Re-export the wgpu adapter when the feature is active.
#[cfg(feature = "pluggable-backend-exp")]
pub mod wgpu;

// ──────────────────────────────────────────────
//  Descriptor types shared by all backends
// ──────────────────────────────────────────────

/// Buffer usage categories portable across backends.
///
/// Each backend maps these to its native resource-option flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferUsage {
    Vertex,
    Index,
    Uniform,
    Storage,
    ReadOnlyStorage,
    Indirect,
    CopySrc,
    CopyDst,
    MapRead,
    MapWrite,
}

/// Size and flags for a GPU buffer.
#[derive(Clone, Debug)]
pub struct BufferDescriptor {
    pub label: Option<String>,
    pub size: u64,
    /// Operations the buffer supports (portable across backends).
    pub usage: Vec<BufferUsage>,
}

/// Dimension of a GPU texture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureDimension {
    D1,
    D2,
    D3,
}

/// Texture usage categories portable across backends.
///
/// Each backend maps these to its native texture-usage flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureUsage {
    TextureBinding,
    StorageBinding,
    RenderAttachment,
    CopySrc,
    CopyDst,
}

/// Descriptor for creating a GPU texture.
#[derive(Clone, Debug)]
pub struct TextureDescriptor<F: Copy + Eq + Send + Sync> {
    pub label: Option<String>,
    pub size: (u32, u32, u32), // (width, height, depth_or_array_layers)
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub dimension: TextureDimension,
    pub format: F,
    /// Operations the texture supports (portable across backends).
    pub usage: Vec<TextureUsage>,
}

/// Descriptor for creating a GPU sampler.
#[derive(Clone, Debug)]
pub struct SamplerDescriptor {
    pub label: Option<String>,
    pub address_mode_u: AddressMode,
    pub address_mode_v: AddressMode,
    pub address_mode_w: AddressMode,
    pub mag_filter: FilterMode,
    pub min_filter: FilterMode,
    pub mipmap_filter: FilterMode,
    pub lod_min_clamp: f32,
    pub lod_max_clamp: f32,
    pub compare: Option<CompareFunction>,
    pub max_anisotropy: u16,
}

/// How a texture coordinate outside [0, 1] is handled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

/// Texel filter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterMode {
    Nearest,
    Linear,
}

// ── Texture copy primitives ────────────────────────────────────────────────

/// 3D offset / origin for texture copy operations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Origin3d {
    pub x: u32,
    pub y: u32,
    pub z: u32,
}

impl Origin3d {
    pub const ZERO: Self = Self { x: 0, y: 0, z: 0 };
}

/// 3D extent for texture copy operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Extent3d {
    pub width: u32,
    pub height: u32,
    pub depth_or_array_layers: u32,
}

/// Which aspects of a depth/stencil texture to access during a copy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureAspect {
    All,
    DepthOnly,
    StencilOnly,
}

/// Layout of buffer-row data for texture copy operations.
#[derive(Clone, Copy, Debug)]
pub struct TexelCopyBufferLayout {
    pub offset: u64,
    pub bytes_per_row: Option<u32>,
    pub rows_per_image: Option<u32>,
}

// ── Bind-group model ───────────────────────────────────────────────────────

/// Shader stages a binding is visible to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}

/// One entry in a bind group layout.
#[derive(Clone, Debug)]
pub struct BindGroupLayoutEntry {
    pub binding: u32,
    /// Shader stages this binding is visible to (portable across backends).
    pub visibility: Vec<ShaderStage>,
    pub ty: BindingType,
    pub count: Option<NonZeroU32>,
}

/// The type of resource bound at a binding point.
#[derive(Clone, Debug)]
pub enum BindingType {
    Buffer {
        ty: BufferBindingType,
        has_dynamic_offset: bool,
        min_binding_size: Option<u64>,
    },
    Texture {
        multisampled: bool,
        view_dimension: TextureViewDimension,
        sample_type: TextureSampleType,
    },
    Sampler(SamplerBindingType),
    StorageTexture {
        access: StorageTextureAccess,
        format: u32,
        view_dimension: TextureViewDimension,
    },
}

/// Category of buffer binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferBindingType {
    Uniform,
    Storage,
    ReadOnlyStorage,
}

/// Dimension of a texture view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureViewDimension {
    D1,
    D2,
    D2Array,
    Cube,
    CubeArray,
    D3,
}

/// Element type of a sampled texture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureSampleType {
    Float { filterable: bool },
    Depth,
    Uint,
    Sint,
}

/// Sampler binding kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SamplerBindingType {
    Filtering,
    NonFiltering,
    Comparison,
}

/// Access mode for storage textures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageTextureAccess {
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

/// One entry in a bind group — maps a binding slot to a concrete resource.
pub struct BindGroupEntry<'a, B: GpuBackend> {
    pub binding: u32,
    pub resource: BindingResource<'a, B>,
}

/// The concrete resource behind a bind-group entry.
pub enum BindingResource<'a, B: GpuBackend> {
    Buffer(&'a B::Buffer),
    BufferRange(&'a B::Buffer, u64, u64), // buffer, offset, size
    TextureView(&'a B::TextureView),
    Sampler(&'a B::Sampler),
}

// ── Pipeline creation ──────────────────────────────────────────────────────

/// Description of a render pipeline state object.
pub struct RenderPipelineDescriptor<'a, B: GpuBackend> {
    pub label: Option<String>,
    pub layout: Option<&'a B::PipelineLayout>,
    pub vertex: VertexState<'a, B>,
    pub fragment: Option<FragmentState<'a, B>>,
    pub primitive: PrimitiveState,
    pub depth_stencil: Option<DepthStencilState<B::TextureFormat>>,
    pub multisample: MultisampleState,
}

/// The vertex-shader side of a pipeline.
pub struct VertexState<'a, B: GpuBackend> {
    pub module: &'a B::ShaderModule,
    pub entry_point: &'a str,
    pub buffers: &'a [Option<VertexBufferLayout<'a>>],
}

/// The fragment-shader side of a pipeline.
pub struct FragmentState<'a, B: GpuBackend> {
    pub module: &'a B::ShaderModule,
    pub entry_point: &'a str,
    pub targets: &'a [Option<ColorTargetState<B::TextureFormat>>],
}

/// Layout of one vertex buffer.
#[derive(Clone)]
pub struct VertexBufferLayout<'a> {
    pub array_stride: u64,
    pub step_mode: VertexStepMode,
    pub attributes: &'a [VertexAttribute],
}

/// Whether a vertex buffer advances per vertex or per instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VertexStepMode {
    Vertex,
    Instance,
}

/// One vertex attribute within a buffer.
#[derive(Clone, Copy, Debug)]
pub struct VertexAttribute {
    pub format: VertexFormat,
    pub offset: u64,
    pub shader_location: u32,
}

/// Vertex element format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VertexFormat {
    Uint8x2,
    Uint8x4,
    Sint8x2,
    Sint8x4,
    Unorm8x2,
    Unorm8x4,
    Snorm8x2,
    Snorm8x4,
    Uint16x2,
    Uint16x4,
    Sint16x2,
    Sint16x4,
    Unorm16x2,
    Unorm16x4,
    Snorm16x2,
    Snorm16x4,
    Float16x2,
    Float16x4,
    Float32,
    Float32x2,
    Float32x3,
    Float32x4,
    Uint32,
    Uint32x2,
    Uint32x3,
    Uint32x4,
    Sint32,
    Sint32x2,
    Sint32x3,
    Sint32x4,
}

/// Per-channel mask for color writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColorWriteMask {
    pub red: bool,
    pub green: bool,
    pub blue: bool,
    pub alpha: bool,
}

impl ColorWriteMask {
    /// No color channels written.
    pub const NONE: Self = Self { red: false, green: false, blue: false, alpha: false };
    /// All four channels written.
    pub const ALL: Self = Self { red: true, green: true, blue: true, alpha: true };
    /// Red, green, and blue (without alpha).
    pub const RGB: Self = Self { red: true, green: true, blue: true, alpha: false };
}

/// Color-target blend state.
pub struct ColorTargetState<F: Copy + Eq + Send + Sync> {
    pub format: F,
    pub blend: Option<BlendState>,
    /// Per-channel write mask.
    pub write_mask: ColorWriteMask,
}

/// Per-component blend equation.
#[derive(Clone, Debug)]
pub struct BlendState {
    pub color: BlendComponent,
    pub alpha: BlendComponent,
}

impl BlendState {
    /// Premultiplied alpha blending, matching `wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING`:
    /// `src * One + dst * (1 - src_alpha)`.
    pub const PREMULTIPLIED_ALPHA_BLENDING: Self = Self {
        color: BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        },
        alpha: BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        },
    };
}

/// One side of a blend equation.
#[derive(Clone, Debug)]
pub struct BlendComponent {
    pub src_factor: BlendFactor,
    pub dst_factor: BlendFactor,
    pub operation: BlendOperation,
}

/// Blend factor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlendFactor {
    Zero,
    One,
    Src,
    OneMinusSrc,
    SrcAlpha,
    OneMinusSrcAlpha,
    Dst,
    OneMinusDst,
    DstAlpha,
    OneMinusDstAlpha,
    SrcAlphaSaturated,
    Constant,
    OneMinusConstant,
}

/// Blend operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlendOperation {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
}

// ── Primitive / depth / multisample state ──────────────────────────────────

/// Primitive assembly state.
#[derive(Clone, Copy, Debug)]
pub struct PrimitiveState {
    pub topology: PrimitiveTopology,
    pub strip_index_format: Option<IndexFormat>,
    pub front_face: FrontFace,
    pub cull_mode: Option<Face>,
    pub unclipped_depth: bool,
    pub polygon_mode: PolygonMode,
    pub conservative: bool,
}

impl Default for PrimitiveState {
    fn default() -> Self {
        Self {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        }
    }
}

/// How the GPU assembles primitives from vertices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimitiveTopology {
    PointList,
    LineList,
    LineStrip,
    TriangleList,
    TriangleStrip,
}

/// Index buffer element size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexFormat {
    Uint16,
    Uint32,
}

/// Triangle front-face winding order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontFace {
    Ccw,
    Cw,
}

/// Which face to cull.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Face {
    Front,
    Back,
}

/// Polygon rasterization mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolygonMode {
    Fill,
    Line,
    Point,
}

/// Depth-stencil attachment state used in pipeline creation.
pub struct DepthStencilState<F: Copy + Eq + Send + Sync> {
    pub format: F,
    pub depth_write_enabled: bool,
    pub depth_compare: CompareFunction,
    pub stencil: StencilState,
    pub bias: DepthBiasState,
}

/// Front/back stencil operations.
#[derive(Clone, Copy, Debug)]
pub struct StencilState {
    pub front: StencilFaceState,
    pub back: StencilFaceState,
    pub read_mask: u32,
    pub write_mask: u32,
}

/// Stencil operation for one face.
#[derive(Clone, Copy, Debug)]
pub struct StencilFaceState {
    pub compare: CompareFunction,
    pub fail_op: StencilOperation,
    pub depth_fail_op: StencilOperation,
    pub pass_op: StencilOperation,
}

/// What to do when a stencil test passes or fails.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StencilOperation {
    Keep,
    Zero,
    Replace,
    IncrementClamp,
    DecrementClamp,
    Invert,
    IncrementWrap,
    DecrementWrap,
}

/// Comparison operator used in depth and stencil tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompareFunction {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

/// Depth-bias configuration.
#[derive(Clone, Copy, Debug)]
pub struct DepthBiasState {
    pub constant: i32,
    pub slope_scale: f32,
    pub clamp: f32,
}

/// Multisample state.
#[derive(Clone, Copy, Debug)]
pub struct MultisampleState {
    pub count: u32,
    pub mask: u64,
    pub alpha_to_coverage_enabled: bool,
}

impl Default for MultisampleState {
    fn default() -> Self {
        Self {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        }
    }
}

// ── Render pass descriptors ────────────────────────────────────────────────

/// Describes one render pass (color attachments + optional depth-stencil).
pub struct RenderPassDescriptor<'a, B: GpuBackend> {
    pub label: Option<String>,
    pub color_attachments: &'a [RenderPassColorAttachment<'a, B>],
    pub depth_stencil_attachment: Option<RenderPassDepthStencilAttachment<'a, B>>,
}

/// One color attachment inside a render pass.
pub struct RenderPassColorAttachment<'a, B: GpuBackend> {
    pub view: &'a B::TextureView,
    pub resolve_target: Option<&'a B::TextureView>,
    pub ops: Operations<[f64; 4]>,
}

/// Generic load/store ops with backend‑specific clear value type `T`.
///
/// For color attachments `T = [f64; 4]` (RGBA), for depth `T = f32`, for
/// stencil `T = u32`.
#[derive(Clone, Copy, Debug)]
pub struct Operations<T: Copy> {
    pub load: LoadOp<T>,
    pub store: StoreOp,
}

/// How an attachment is loaded at the start of the render pass.
#[derive(Clone, Copy, Debug)]
pub enum LoadOp<T: Copy> {
    Load,
    Clear(T),
}

/// How an attachment is stored at the end of the render pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreOp {
    Store,
    Discard,
}

/// Depth-stencil attachment inside a render pass.
pub struct RenderPassDepthStencilAttachment<'a, B: GpuBackend> {
    pub view: &'a B::TextureView,
    pub depth_ops: Option<Operations<f32>>,
    pub stencil_ops: Option<Operations<u32>>,
}

// ── Limits ─────────────────────────────────────────────────────────────────

/// Hardware-imposed limits reported by a backend.
#[derive(Clone, Copy, Debug)]
pub struct GpuLimits {
    pub max_texture_dimension_2d: u32,
    /// Minimum alignment (in bytes) for the start offset of a uniform buffer
    /// binding with dynamic offset.
    pub min_uniform_buffer_offset_alignment: u32,
}

// ── Texture copy descriptors ───────────────────────────────────────────────

/// Source texture for copy operations.
pub struct TexelCopyTextureInfo<'a, B: GpuBackend> {
    pub texture: &'a B::Texture,
    pub mip_level: u32,
    pub origin: Origin3d,
    pub aspect: TextureAspect,
}

/// Source buffer for buffer-to-texture copy operations.
pub struct TexelCopyBufferInfo<'a, B: GpuBackend> {
    pub buffer: &'a B::Buffer,
    pub layout: TexelCopyBufferLayout,
}

/// Descriptor for a queue-level write_texture operation.
pub struct WriteTextureDescriptor<'a, B: GpuBackend> {
    pub texture: &'a B::Texture,
    pub mip_level: u32,
    pub origin: Origin3d,
    pub aspect: TextureAspect,
    pub data: &'a [u8],
    pub buffer_layout: TexelCopyBufferLayout,
    pub extent: Extent3d,
}

// ── GpuRenderPass trait ───────────────────────────────────────────────────

/// Operations that can be encoded into a render pass.
///
/// Every method maps to the corresponding `wgpu::RenderPass` operation with
/// the same name and arguments, adapted for the backend's type hierarchy.
pub trait GpuRenderPass<B: GpuBackend> {
    fn set_pipeline(&mut self, pipeline: &B::RenderPipeline);
    fn set_bind_group(&mut self, index: u32, bind_group: &B::BindGroup, dynamic_offsets: &[u32]);
    fn set_vertex_buffer(&mut self, slot: u32, buffer: &B::Buffer, offset: u64);
    fn set_index_buffer(&mut self, buffer: &B::Buffer, index_format: IndexFormat, offset: u64);
    fn set_scissor_rect(&mut self, x: u32, y: u32, width: u32, height: u32);
    fn draw(&mut self, vertices: Range<u32>, instances: Range<u32>);
    fn draw_indexed(&mut self, indices: Range<u32>, base_vertex: i32, instances: Range<u32>);
}

// ── GpuBackend trait ──────────────────────────────────────────────────────

/// A pluggable GPU backend.
///
/// Implementations own the device handle, a command queue, and produce all GPU
/// resources needed by the Cupid pipelines.
///
/// The associated `RenderPass` type uses a GAT so that each pass object borrows
/// its command encoder for the correct lifetime — the same pattern `wgpu` uses.
pub trait GpuBackend: Sized + 'static {
    // ── Associated types ────────────────────────────────────────────────

    /// A GPU buffer (vertex, index, uniform, staging, …).
    type Buffer;
    /// A GPU texture (color target, image, …).
    type Texture;
    /// A view into a GPU texture.
    type TextureView;
    /// A bind group layout describing the set of bound resources.
    type BindGroupLayout;
    /// A concrete bind group.
    type BindGroup;
    /// A pipeline layout describing bind group layouts.
    type PipelineLayout;
    /// A fully compiled render pipeline state object.
    type RenderPipeline;
    /// A texture sampler.
    type Sampler;
    /// A compiled shader module.
    type ShaderModule;
    /// A command encoder for building GPU command streams.
    type CommandEncoder;
    /// The native texture format type (must be cheaply copyable and comparable).
    type TextureFormat: Copy + Eq + Send + Sync;

    /// The backend's render-pass type, parameterized by the encoder lifetime.
    type RenderPass<'a>: GpuRenderPass<Self>
    where
        Self: 'a;

    // ── Resource creation ───────────────────────────────────────────────

    /// Compile a shader module from SPIR-V / WGSL / MSL source bytes.
    fn create_shader_module(&self, source: &[u8], label: &str) -> Self::ShaderModule;

    /// Create a GPU buffer.
    fn create_buffer(&self, desc: &BufferDescriptor) -> Self::Buffer;

    /// Create a GPU texture.
    fn create_texture(&self, desc: &TextureDescriptor<Self::TextureFormat>) -> Self::Texture;

    /// Create a texture view (default view covering the whole texture).
    fn create_texture_view(&self, texture: &Self::Texture, label: &str) -> Self::TextureView;

    /// Create a texture sampler.
    fn create_sampler(&self, desc: &SamplerDescriptor) -> Self::Sampler;

    /// Create a bind group layout.
    fn create_bind_group_layout(
        &self,
        entries: &[BindGroupLayoutEntry],
    ) -> Self::BindGroupLayout;

    /// Create a bind group from a layout and resource entries.
    fn create_bind_group(
        &self,
        layout: &Self::BindGroupLayout,
        entries: &[BindGroupEntry<Self>],
    ) -> Self::BindGroup;

    /// Create a pipeline layout from a slice of bind group layouts.
    fn create_pipeline_layout(&self, layouts: &[&Self::BindGroupLayout]) -> Self::PipelineLayout;

    /// Create a render pipeline state object.
    fn create_render_pipeline(
        &self,
        desc: &RenderPipelineDescriptor<Self>,
    ) -> Self::RenderPipeline;

    // ── Data upload ─────────────────────────────────────────────────────

    /// Write data into a buffer at the given offset.
    fn write_buffer(&self, buffer: &Self::Buffer, offset: u64, data: &[u8]);

    /// Write pixel data into a texture sub-region.
    ///
    /// The descriptor specifies which sub-region of the texture is written
    /// (via `origin` and `extent`), the byte layout of the source data (via
    /// `buffer_layout`), and which texture aspect is affected. This supports
    /// sub-region uploads (e.g. glyph atlas updates), single-channel
    /// textures (R8 coverage), and custom row strides.
    fn write_texture(&self, desc: &WriteTextureDescriptor<Self>);

    // ── Command encoding ────────────────────────────────────────────────

    /// Create a command encoder.
    fn create_command_encoder(&self, label: &str) -> Self::CommandEncoder;

    /// Begin a render pass on the given encoder.
    fn begin_render_pass<'a>(
        &self,
        encoder: &'a mut Self::CommandEncoder,
        desc: &RenderPassDescriptor<Self>,
    ) -> Self::RenderPass<'a>;

    // ── Copy operations ─────────────────────────────────────────────────

    /// Copy pixel data from a buffer into a texture sub-region.
    fn copy_buffer_to_texture(
        &self,
        encoder: &mut Self::CommandEncoder,
        src: &TexelCopyBufferInfo<Self>,
        dst: &TexelCopyTextureInfo<Self>,
        extent: Extent3d,
    );

    /// Copy pixel data between two textures.
    fn copy_texture_to_texture(
        &self,
        encoder: &mut Self::CommandEncoder,
        src: &TexelCopyTextureInfo<Self>,
        dst: &TexelCopyTextureInfo<Self>,
        extent: Extent3d,
    );

    // ── Submission ──────────────────────────────────────────────────────

    /// Finish the command encoder and submit the resulting command buffer to
    /// the GPU.
    fn submit(&self, encoder: Self::CommandEncoder);

    // ── Queries ─────────────────────────────────────────────────────────

    /// Return backend limits.
    fn limits(&self) -> GpuLimits;

    /// Returns whether a texture format uses an sRGB color space.
    fn format_is_srgb(format: Self::TextureFormat) -> bool;
}