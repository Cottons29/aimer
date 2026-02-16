use pixels::{Pixels, SurfaceTexture};
use skia_safe::{AlphaType, ColorType};
use widget::Widget;
use widget::base::{BuildContext, Size, Vec2d};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

pub struct App<T: Widget> {
    pub window: Option<&'static Window>,
    pub pixels: Option<Pixels<'static>>,
    pub widget_root: T,
    pub cursor_pos: Vec2d,
}

impl<T: Widget> ApplicationHandler for App<T> {
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

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            // WindowEvent::Focused
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = Vec2d { x: position.x as f32, y: position.y as f32 };
            }

            WindowEvent::MouseInput { state, button, .. } => {
                if !state.is_pressed() || button != winit::event::MouseButton::Left {
                    return;
                }

                // println!("Mouse Clicked : {:?}", self.cursor_pos);

                let c = self.cursor_pos;
                #[allow(clippy::collapsible_if)]
                if let Some(widget) = Self::is_on_click(&self.widget_root, c) {
                    if let Some(on_click) = widget.on_click() {
                        on_click();
                        if let Some(window) = &self.window {
                            window.request_redraw();
                        }
                    }
                }
            }

            WindowEvent::RedrawRequested => self.render(event_loop),
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

impl<T: Widget> App<T> {
    fn is_on_click(widget: & dyn Widget, c: Vec2d) -> Option<&dyn Widget> {
        let bounds = widget.pos_start_end();

        if let Some((start, end)) = bounds {
            let is_inside = c.x >= start.x && c.x <= end.x && c.y >= start.y && c.y <= end.y;
            if !is_inside {
                return None;
            }
        }

        for child in widget.child().iter().rev() {
            if let Some(hit) = Self::is_on_click(child.as_ref(), c) {
                return Some(hit);
            }
        }

        if bounds.is_some() {
            return Some(widget);
        }

        None
    }

    fn render_widget_tree(widget: &dyn Widget, ctx: &BuildContext) {
        widget.draw(ctx);
        for item in widget.child() {
            Self::render_widget_tree(item.as_ref(), ctx);
        }
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
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
                    let ctx: BuildContext =
                        BuildContext { size: Size { width, height }, canvas: surface.canvas() };
                    ctx.canvas.clear(skia_safe::Color::WHITE);

                    Self::render_widget_tree(&self.widget_root, &ctx);
                }
            }

            if let Err(err) = pixels.render() {
                eprintln!("pixels.render() failed: {err}");
                event_loop.exit();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use widget::Widget;
    use widget::base::{BuildContext, Size, Vec2d};

    struct MockWidget {
        pos: Option<Vec2d>,
        size: Option<Size>,
        children: Vec<Box<dyn Widget>>,
    }

    impl Widget for MockWidget {
        fn draw(&self, _ctx: &BuildContext) {}
        fn pos(&self) -> Option<Vec2d> {
            self.pos
        }
        fn size(&self) -> Option<Size> {
            self.size
        }
        fn child(&self) -> &[Box<dyn Widget>] {
            &self.children
        }
    }

    #[test]
    fn test_is_on_click_wrapper() {
        let btn = MockWidget {
            pos: Some(Vec2d { x: 10.0, y: 10.0 }),
            size: Some(Size { width: 20, height: 20 }), // 10,10 to 30,30
            children: vec![],
        };

        let wrapper = MockWidget { pos: None, size: None, children: vec![Box::new(btn)] };

        // Click at 15, 15 (inside button)
        let hit = App::<MockWidget>::is_on_click(&wrapper, Vec2d { x: 15.0, y: 15.0 });
        assert!(hit.is_some());

        // Verify we get a hit even if the wrapper has no bounds.
        // With the old logic, this would fail (return None).
    }

    #[test]
    fn test_is_on_click_outside() {
        let btn = MockWidget {
            pos: Some(Vec2d { x: 10.0, y: 10.0 }),
            size: Some(Size { width: 20, height: 20 }), // 10,10 to 30,30
            children: vec![],
        };

        let wrapper = MockWidget { pos: None, size: None, children: vec![Box::new(btn)] };

        // Click at 50, 50 (outside button)
        let hit = App::<MockWidget>::is_on_click(&wrapper, Vec2d { x: 50.0, y: 50.0 });
        assert!(hit.is_none());
    }
}
