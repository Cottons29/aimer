use crate::widgets::{Circle, Square, Triangle, Widget};
use pixels::{Pixels, SurfaceTexture};
use skia_safe::{AlphaType, ColorType};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct App {
    window: Option<&'static Window>,
    pixels: Option<Pixels<'static>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window_attributes = Window::default_attributes().with_title("Oxidize Render");
        let window = event_loop.create_window(window_attributes).unwrap();
        let window: &'static Window = Box::leak(Box::new(window)); // Leak to static ref

        let size = window.inner_size();
        let surface_texture = SurfaceTexture::new(size.width, size.height, window);
        let pixels = Pixels::new(size.width, size.height, surface_texture).unwrap();

        self.window = Some(window);
        self.pixels = Some(pixels);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if let (Some(pixels), Some(window)) = (&mut self.pixels, &self.window) {
                    let width = window.inner_size().width;
                    let height = window.inner_size().height;

                    let frame = pixels.frame_mut();

                    let info = skia_safe::ImageInfo::new(
                        (width as i32, height as i32),
                        ColorType::RGBA8888,
                        AlphaType::Premul,
                        None,
                    );

                    let row_bytes = width as usize * 4;

                    {
                        if let Some(mut surface) =
                            skia_safe::surfaces::wrap_pixels(&info, frame, Some(row_bytes), None)
                        {
                            let canvas = surface.canvas();
                            canvas.clear(skia_safe::Color::WHITE);

                            let square = Square {
                                x: 50.0,
                                y: 50.0,
                                size: 100.0,
                                color: 0xFFFF0000,
                            };
                            square.draw(canvas);

                            let triangle = Triangle {
                                p1: (200.0, 200.0),
                                p2: (300.0, 200.0),
                                p3: (250.0, 100.0),
                                color: 0xFF00FF00,
                            };
                            triangle.draw(canvas);

                            let circle = Circle {
                                cx: 400.0,
                                cy: 300.0,
                                radius: 50.0,
                                color: 0xFF0000FF,
                            };
                            circle.draw(canvas);
                        }
                    }

                    if let Err(err) = pixels.render() {
                        eprintln!("pixels.render() failed: {err}");
                        event_loop.exit();
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(pixels) = &mut self.pixels {
                    let _ = pixels.resize_surface(size.width, size.height);
                    let _ = pixels.resize_buffer(size.width, size.height);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => (),
        }
    }
}

pub fn render() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        window: None,
        pixels: None,
    };
    let _ = event_loop.run_app(&mut app);
}
