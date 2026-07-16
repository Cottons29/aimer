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
}

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
pub trait CustomPipeline: Send + 'static {
    /// A unique name identifying this pipeline (used for debug labels and
    /// lookup).
    fn name(&self) -> &str;

    /// Called once per frame before the render pass begins.
    /// Use this to upload instance buffers, update uniforms, etc.
    fn prepare(&mut self, ctx: &RenderContext);

    /// Called during the render pass to issue draw calls.
    /// The render pass already has the correct color attachment set up. The
    /// render pipeline must use `RenderContext::sample_count`.
    fn render<'pass>(&'pass self, pass: &mut wgpu::RenderPass<'pass>);

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
