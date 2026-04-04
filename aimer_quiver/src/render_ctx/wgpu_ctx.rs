#[cfg(not(target_arch = "wasm32"))]
pub mod render_ctx {
    use wgpu::{TextureViewDescriptor, TextureViewDimension};
    use aimer_cupid::canvas::CupidCanvas;
    use aimer_cupid::gpu_context::GpuContext;
    use aimer_cupid::renderer::Renderer;
    use winit::dpi::PhysicalSize;
    use winit::window::Window;
    use aimer_utils::debug;

    pub struct WgpuApi {
        gpu: Option<GpuContext<'static>>,
        renderer: Option<Renderer>,
        canvas: Option<CupidCanvas>,
    }

    impl Default for WgpuApi {
        fn default() -> Self {
            Self {
                gpu: None,
                renderer: None,
                canvas: None,
            }
        }
    }

    impl WgpuApi {
        pub fn initialize(&mut self, window: &'static Window, size: PhysicalSize<u32>) {
            if self.gpu.is_some() {
                self.resize(size);
                return;
            }

            // let gpu = GpuContext::initialize(window, size);
            let gpu = GpuContext::initialize(window, (1344, 2833).into());
            let canvas = CupidCanvas::new();
            let renderer = Renderer::new(&gpu.device, gpu.format);

            self.gpu = Some(gpu);
            self.renderer = Some(renderer);
            self.canvas = Some(canvas);
        }

        pub fn resize(&mut self, size: PhysicalSize<u32>) {
            if let Some(gpu) = &mut self.gpu {
                // debug!("WgpuApi::resize : Resizing GPU context to size: {} x {}", size.width, size.height);
                gpu.resize(size);
            }
        }

        /// Create a CupidCanvas, call `draw_fn` with it and dimensions,
        /// then flush the draw list through the renderer and present.
        pub fn render_frame(&mut self, draw_fn: impl FnOnce(&CupidCanvas, u32, u32)) {

            let (gpu, renderer, canvas) = match (&self.gpu, &mut self.renderer, &self.canvas) {
                (Some(g), Some(r), Some(c)) => (g, r, c),
                _ => return,
            };


            let frame = match gpu.begin_frame() {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
                _ => return,
            };


            let view = frame
                .texture
                .create_view(&Default::default());


            let width = gpu.width();
            let height = gpu.height();


            canvas.begin_frame();
            draw_fn(canvas, width, height);

            let draw_list = canvas.draw_list();
            renderer.render(&gpu.device, &gpu.queue, &view, width, height, gpu.is_srgb, &draw_list);

            gpu.end_frame(frame);
        }
    }
}
