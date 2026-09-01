use std::any::Any;

/// Context passed to custom pipelines during rendering.
/// Provides access to GPU resources and the current frame's viewport info.
pub struct RenderContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub width: u32,
    pub height: u32,
    pub is_srgb: bool,
    pub format: wgpu::TextureFormat,
    /// Sample count used by the renderer's color attachment. Custom render
    /// pipelines must use the same count.
    pub sample_count: u32,
    /// The frame's source texture when the renderer can snapshot the current
    /// backdrop. Custom pipelines should treat this as unavailable on targets
    /// whose presentation surface cannot be copied.
    pub source_texture: Option<&'a wgpu::Texture>,
}

/// The thread-affinity a custom pipeline has to satisfy.
///
/// A native renderer is owned by whichever thread encodes frames, so every
/// pipeline it holds has to be `Send`. On `wasm32` the WebGPU backend's
/// resources are `Rc`-based and therefore never `Send`, and the renderer never
/// leaves the thread that created it, so the bound is dropped there.
#[cfg(not(target_arch = "wasm32"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> MaybeSend for T {}

#[cfg(target_arch = "wasm32")]
pub trait MaybeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> MaybeSend for T {}

/// Trait for user-defined render pipelines that can be plugged into the main
/// renderer.
///
/// Custom pipelines manage their own GPU resources (shader modules, bind
/// groups, instance buffers, etc.) and are invoked during the render pass at
/// the correct z-order position whenever a `DrawCommand::Custom` targets them
/// by name.
///
/// # Usage
///
/// 1. Implement this trait on your pipeline struct.
/// 2. Register it with `renderer.register_custom_pipeline(my_pipeline)`.
/// 3. Push your per-frame data into the pipeline before rendering (e.g. via a
///    method on your struct, or through shared state).
/// 4. Emit `draw_list.draw_custom("my_pipeline", ())` at the desired z-order
///    position in your draw list.
///
/// # Example
///
/// ```ignore
/// struct GlowPipeline {
///     render_pipeline: wgpu::RenderPipeline,
///     instances: Vec<GlowInstance>,
///     // ...
/// }
///
/// impl CustomPipeline for GlowPipeline {
///     fn name(&self) -> &str { "glow" }
///
///     fn prepare(&mut self, ctx: &RenderContext) {
///         // Upload instance buffers, update uniforms, etc.
///     }
///
///     fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>) {
///         if self.instances.is_empty() { return; }
///         pass.set_pipeline(&self.render_pipeline);
///         // set bind groups, vertex buffers, draw...
///     }
/// }
/// ```
pub trait CustomPipeline: MaybeSend + 'static {
    /// A unique name identifying this pipeline (used for debug labels and
    /// lookup).
    fn name(&self) -> &str;

    /// Called once per frame before the render pass begins.
    /// Use this to upload instance buffers, update uniforms, etc.
    fn prepare(&mut self, ctx: &RenderContext);

    /// Starts a new ordered draw-command frame.
    ///
    /// The default is a no-op so existing custom pipelines that manage their
    /// own per-frame state remain source-compatible. Pipelines that consume
    /// command payloads can clear their decoded work here before
    /// [`Self::prepare`] is called.
    fn begin_frame(&mut self) {}

    /// Accepts the payload attached to one custom draw command.
    ///
    /// Returning an index lets a pipeline keep one prepared GPU buffer while
    /// the renderer still invokes it at the command's original z-order. A
    /// `None` result means that command has no renderable payload. Existing
    /// custom pipelines do not need to implement this hook.
    fn prepare_command(&mut self, _data: &(dyn Any + Send)) -> Option<usize> {
        None
    }

    /// Called during the render pass to issue draw calls.
    /// The render pass already has the correct color attachment set up. The
    /// render pipeline must use `RenderContext::sample_count`.
    fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>);

    /// Renders one prepared command at its original position in the draw list.
    ///
    /// The default preserves the original custom-pipeline behavior, where a
    /// pipeline owns one aggregate draw and ignores command indices.
    fn render_command<'pass>(
        &'pass self,
        _command_index: Option<usize>,
        pass: &mut wgpu::RenderPass<'pass>,
    ) {
        self.render(pass);
    }

    /// Whether the command needs the already-rendered backdrop copied into a
    /// pipeline-owned texture before it is drawn.
    fn needs_backdrop(&self, _command_index: Option<usize>) -> bool {
        false
    }

    /// Copies the current frame into the pipeline's backdrop texture.
    ///
    /// The renderer ends the active render pass before calling this hook and
    /// resumes it with a load operation afterwards, preserving command order.
    fn capture_backdrop(
        &self,
        _encoder: &mut wgpu::CommandEncoder,
        _source_texture: &wgpu::Texture,
        _width: u32,
        _height: u32,
    ) {
    }

    /// Copies the backdrop needed by one prepared command.
    ///
    /// Pipelines with command-local capture regions override this hook. The
    /// default preserves the original full-frame capture contract for existing
    /// custom pipelines.
    fn capture_backdrop_command(
        &self,
        _command_index: Option<usize>,
        encoder: &mut wgpu::CommandEncoder,
        source_texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) {
        self.capture_backdrop(encoder, source_texture, width, height);
    }

    /// Whether this pipeline has any work to do this frame.
    /// Default returns true; override to skip the render call when idle.
    fn has_work(&self) -> bool {
        true
    }
}

/// Wrapper that holds a custom pipeline instance.
pub(crate) struct CustomPipelineSlot {
    pub pipeline: Box<dyn CustomPipeline>,
}

impl CustomPipelineSlot {
    pub fn new(pipeline: impl CustomPipeline) -> Self {
        let pipeline = Box::new(pipeline);
        Self { pipeline }
    }
}
