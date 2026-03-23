use cupid::canvas::CupidCanvas;
use cupid::gpu_context::GpuContext;
use cupid::renderer::Renderer;
use cupid::utilities::Color;
use std::path::PathBuf;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

struct App<'w> {
    gpu: Option<GpuContext<'w>>,
    renderer: Option<Renderer>,
    canvas: CupidCanvas,
    window: Option<Window>,
    texture_id: Option<u32>,
}

impl<'w> App<'w> {
    fn new() -> Self {
        Self {
            gpu: None,
            renderer: None,
            canvas: CupidCanvas::new(),
            window: None,
            texture_id: None,
        }
    }
}

impl<'w> ApplicationHandler for App<'w> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("Cupid Render Engine — Test")
            .with_inner_size(winit::dpi::LogicalSize::new(800, 600));
        let window = event_loop.create_window(attrs).unwrap();

        let size = window.inner_size();

        // SAFETY: We store the window in self and the GpuContext borrows it.
        // The window outlives the GpuContext because we drop gpu before window.
        let window_ref: &'w Window = unsafe { &*(&window as *const Window) };
        let gpu = GpuContext::initialize(window_ref, size);

        // Upload test image from cupid/image.png
        let mut img_renderer = Renderer::new(&gpu.device, gpu.format);
        let image_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("image.png");
        let img = image::open(&image_path)
            .unwrap_or_else(|e| panic!("Failed to load {}: {e}", image_path.display()))
            .into_rgba8();
        let (img_w, img_h) = img.dimensions();
        let tex_id = img_renderer.image_pipeline.upload_image(
            &gpu.device,
            &gpu.queue,
            img_w,
            img_h,
            img.as_raw(),
        );

        self.texture_id = Some(tex_id);
        self.renderer = Some(img_renderer);
        self.gpu = Some(gpu);
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(new_size);
                }
            }
            WindowEvent::RedrawRequested => {
                let gpu = match &self.gpu {
                    Some(g) => g,
                    None => return,
                };
                let renderer = match &mut self.renderer {
                    Some(r) => r,
                    None => return,
                };

                let frame = match gpu.begin_frame() {
                    wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                    _ => return,
                };

                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let width = gpu.width();
                let height = gpu.height();

                // Build draw commands using CupidCanvas
                self.canvas.begin_frame();

                // Draw a blue background rect
                self.canvas.fill_rect(
                    20.0, 20.0, 300.0, 200.0,
                    Color::new(0.2, 0.4, 0.8, 1.0),
                    10.0,
                );

                // Draw a red rect
                self.canvas.fill_rect(
                    50.0, 50.0, 150.0, 80.0,
                    Color::red(),
                    0.0,
                );

                // Draw a green rounded rect
                self.canvas.fill_rect(
                    200.0, 100.0, 180.0, 120.0,
                    Color::green(),
                    20.0,
                );

                // Draw a rect with border
                self.canvas.fill_rect_with_border(
                    420.0, 300.0, 160.0, 100.0,
                    Color::white(),
                    12.0,
                    3.0,
                    Color::new(0.2, 0.2, 0.8, 1.0),
                );

                // Draw a border-only rect (transparent fill)
                self.canvas.fill_rect_with_border(
                    420.0, 420.0, 160.0, 80.0,
                    Color::transparent(),
                    8.0,
                    2.0,
                    Color::red(),
                );

                // Test clipping
                self.canvas.set_clip(50.0, 400.0, 200.0, 100.0);
                self.canvas.fill_rect(
                    30.0, 380.0, 300.0, 150.0,
                    Color::red(),
                    0.0,
                );

                // Test save/translate/restore
                self.canvas.save();
                self.canvas.translate(400.0, 50.0);
                self.canvas.fill_rect(
                    0.0, 0.0, 500.0, 450.0,
                    Color::new(0.8, 0.2, 0.8, 1.0).set_alpha(128),
                    5.0,
                );
                self.canvas.restore();

                // Draw text
                self.canvas.draw_text(
                    30.0, 250.0,
                    "Hello from Cupid!",
                    32.0,
                    Color::black(),
                );

                self.canvas.draw_text(
                    30.0, 300.0,
                    "WGPU-powered UI render engine",
                    20.0,
                    Color::new(0.3, 0.3, 0.3, 1.0),
                );

                // Draw test image if available
                if let Some(tex_id) = self.texture_id {
                    self.canvas.draw_image(500.0, 200.0, 300.0, 300.0, tex_id);
                }

                self.canvas.clear_clip();

                renderer.render(
                    &gpu.device,
                    &gpu.queue,
                    &view,
                    width,
                    height,
                    &self.canvas.draw_list(),
                );

                gpu.end_frame(frame);
            }
            _ => {}
        }

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}


fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}