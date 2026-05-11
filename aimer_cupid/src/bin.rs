use aimer_cupid::canvas::CupidCanvas;
use aimer_cupid::gpu_context::GpuContext;
use aimer_cupid::renderer::Renderer;
use aimer_cupid::utilities::Color;
use aimer_utils::{debug, ExecTimes};
use std::path::PathBuf;
use std::sync::OnceLock;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

static MY_EVENT_PROXY: OnceLock<EventLoopProxy<MyWindowEvent>> = OnceLock::new();

pub fn time_consume(func: impl FnOnce()) {
    let start = std::time::Instant::now();
    func();
    let elapsed = start.elapsed();
    println!("Time elapsed: {} ms", elapsed.as_millis());
}

struct App<'w> {
    gpu: Option<GpuContext<'w>>,
    renderer: Option<Renderer>,
    canvas: CupidCanvas,
    window: Option<Window>,
    texture_id: Option<u32>,
    frame_count: usize,
}

enum MyWindowEvent {
    FirstFrame,
}

impl<'w> App<'w> {
    fn new() -> Self {
        Self { gpu: None, renderer: None, canvas: CupidCanvas::new(), window: None, texture_id: None, frame_count: 200 }
    }
}

impl<'w> ApplicationHandler<MyWindowEvent> for App<'w> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let title = "Cupid Render Engine — Test";
        let attrs = Window::default_attributes()
            .with_title(title)
            .with_inner_size(winit::dpi::LogicalSize::new(800, 600));
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;

            attrs
                .with_decorations(true)
                .with_titlebar_hidden(false)
                .with_titlebar_transparent(false)
                .with_title_hidden(false)
                .with_titlebar_buttons_hidden(false)
                .with_fullsize_content_view(false)
        };
        let window = event_loop.create_window(attrs).unwrap();
        window.set_title(title);

        let size = window.inner_size();

        // SAFETY: We store the window in self and the GpuContext borrows it.
        // The window outlives the GpuContext because we drop gpu before window.
        let window_ref: &'w Window = unsafe { &*(&window as *const Window) };
        let gpu = GpuContext::initialize(window_ref, size);

        // Upload test image from cupid/image.png.
        // Glyphs are rasterized lazily on first use in TextPipelineV2::prepare,
        // so we no longer call preload_text on the resume path — it serializes
        // first-frame for ~5 ms of work that the lazy path already handles.

        debug!("Initializing GPU context and loading test image");
        let mut img_renderer = Renderer::new(&gpu.device, gpu.format);
        debug!("Initialized GPU context");
        let image_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("image.png");
        debug!("Loading test image from {}", image_path.display());
        let img = image::open(&image_path)
            .unwrap_or_else(|e| panic!("Failed to load {}: {e}", image_path.display()))
            .into_rgba8();
        let (img_w, img_h) = img.dimensions();
        let tex_id = img_renderer
            .image_pipeline
            .upload_image(&gpu.device, &gpu.queue, img_w, img_h, img.as_raw());
        debug!("Uploaded image to GPU");

        self.texture_id = Some(tex_id);
        debug!("Test image uploaded");
        self.renderer = Some(img_renderer);
        debug!("Renderer initialized");
        self.gpu = Some(gpu);
        debug!("GPU context initialized");
        self.window = Some(window);
        debug!("Window initialized");
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        debug!("App resumed");
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: MyWindowEvent) {
        match event {
            MyWindowEvent::FirstFrame => {
                self.window.as_ref().unwrap().request_redraw();
            }
        }
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

            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
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
                self.canvas
                    .fill_rect(20.0, 20.0, 300.0, 200.0, Color::new(0.2, 0.4, 0.8, 1.0), [10.0; 4]);

                // Draw a red rect
                self.canvas
                    .fill_rect(50.0, 50.0, 150.0, 80.0, Color::red(), [20.0; 4]);

                // Draw a green rounded rect
                self.canvas
                    .fill_rect(200.0, 100.0, 180.0, 120.0, Color::green(), [20.0; 4]);
                //
                // // Draw a rect with border
                // self.canvas.fill_rect_with_border(
                //     420.0, 300.0, 160.0, 100.0,
                //     Color::red(),
                //     [12.0; 4],
                //     3.0,
                //     Color::new(0.2, 0.2, 0.8, 1.0),
                // );
                //
                // // Draw a border-only rect (transparent fill)
                // self.canvas.fill_rect_with_border(
                //     420.0, 420.0, 460.0, 480.0,
                //     Color::blue(),
                //     [28.0; 4],
                //     0.0,
                //     Color::red(),
                // );
                //
                // // Test clipping
                // self.canvas.set_clip(50.0, 400.0, 200.0, 100.0);
                // self.canvas.fill_rect(
                //     30.0, 380.0, 300.0, 150.0,
                //     Color::red(),
                //     [0.0; 4],
                // );
                //
                // // Test save/translate/restore
                // self.canvas.save();
                // self.canvas.translate(400.0, 50.0);
                // self.canvas.fill_rect(
                //     0.0, 0.0, 500.0, 450.0,
                //     Color::new(0.8, 0.2, 0.8, 1.0).set_alpha(128),
                //     [5.0; 4],
                // );
                // self.canvas.restore();

                // Draw text
                // self.canvas.draw_text(30.0, 250.0, "Hello from Cupid!", 32.0, Color::black());
                //
                // self.canvas
                //     .draw_text(30.0, 300.0, "Wgpu-powered UI render engine", 20.0, Color::black());

                // Mixed CJK + color emoji line — verifies fixes A (no first-frame
                // stall on CJK) and B/C (AppleColorEmoji renders alongside CJK).
                self.canvas
                    .draw_text(30.0, 340.0, "你好,世界 😀 👍 🙂", 24.0, Color::black());

                // Draw test image if available
                // if let Some(tex_id) = self.texture_id {
                //     self.canvas.draw_image(500.0, 200.0, 300.0, 300.0, tex_id);
                // }

                debug!("I Should be stop here");

                self.canvas.clear_clip();

                renderer.render(&gpu.device, &gpu.queue, &view, width, height, gpu.is_srgb, &self.canvas.draw_list());

                ExecTimes::no_param("Render FirstFrame", || gpu.end_frame(frame));
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.frame_count > 0 {
            self.frame_count -= 1;
            debug!("Render Frame: {}", self.frame_count);
            self.window.as_ref().unwrap().request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::<MyWindowEvent>::with_user_event()
        .build()
        .expect("Failed to create event loop");

    MY_EVENT_PROXY.set(event_loop.create_proxy()).ok();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
