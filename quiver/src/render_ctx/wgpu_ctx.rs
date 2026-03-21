#[cfg(not(target_arch = "wasm32"))]
pub mod render_ctx {
    use cupid::canvas::CupidCanvas;
    use cupid::gpu_context::GpuContext;
    use cupid::renderer::Renderer;
    use winit::dpi::PhysicalSize;
    use winit::window::Window;

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
        pub fn initialize(&mut self, window: &Window, size: PhysicalSize<u32>) {
            if self.gpu.is_some() {
                return;
            }

            // SAFETY: The window is leaked to 'static in handler.rs (Box::leak),
            // so this transmute is sound for the GpuContext<'w> lifetime.
            let window_static: &'static Window = unsafe { std::mem::transmute(window) };

            let gpu = GpuContext::initialize(window_static, size);
            let canvas = CupidCanvas::new();
            let renderer = Renderer::new(&gpu.device, &gpu.queue, gpu.format, canvas.font_system());

            self.gpu = Some(gpu);
            self.renderer = Some(renderer);
            self.canvas = Some(canvas);
        }

        pub fn resize(&mut self, size: PhysicalSize<u32>) {
            if let Some(gpu) = &mut self.gpu {
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
                Some(f) => f,
                None => return,
            };

            let view = frame
                .texture
                .create_view(&Default::default());

            let width = gpu.width();
            let height = gpu.height();

            canvas.begin_frame();
            draw_fn(canvas, width, height);

            let draw_list = canvas.draw_list();
            renderer.render(&gpu.device, &gpu.queue, &view, width, height, &draw_list);

            gpu.end_frame(frame);
        }
    }
}
