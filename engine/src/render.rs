#[allow(unused)]
use crate::render;
use pixels::{Pixels, SurfaceTexture};
use skia_safe::{AlphaType, ColorType};
use tokio::runtime::Runtime;
use widget::base::{BuildContext, ResolvedSize, Vec2d};
use widget::{Element, ElementEvent, Widget, dispatch_event};
use winit::application::ApplicationHandler;
#[allow(unused)]
use winit::dpi::{LogicalSize, PhysicalSize, Position};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
#[allow(unused)]
use winit::monitor::MonitorHandle;
#[allow(unused)]
use winit::window::{self, Fullscreen, Window, WindowAttributes, WindowId};

pub struct App {
    pub window: Option<&'static Window>,
    pub pixels: Option<Pixels<'static>>,
    pub widget_root: Option<Box<dyn Element>>,
    pub pending_widget: Option<Box<dyn Widget>>,
    pub cursor_pos: Vec2d,
    pub window_scale: f64,
    pub native_window_size: Option<ResolvedSize>,
    pub pending_resize: Option<PhysicalSize<u32>>,
    pub async_runtime: Runtime,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "ios")]
        {
            match crate::utils::get_screen_resolution_pixels() {
                Some((width, height)) => {
                    self.native_window_size = Some(ResolvedSize { width: width as f32, height: height as f32 })
                }
                None => (),
            };
        }

        let window_attributes = {
            #[cfg(not(target_os = "ios"))]
            {
                WindowAttributes::default()
            }
            #[cfg(target_os = "ios")]
            {
                match crate::utils::get_screen_resolution_pixels() {
                    Some((w, h)) => {
                        let phy_size = PhysicalSize::new(w as u32, h as u32);
                        WindowAttributes::new().with_inner_size(phy_size)
                    }
                    None => WindowAttributes::default(),
                }
            }
        };

        let window = event_loop.create_window(window_attributes).unwrap();
        let window: &'static Window = Box::leak(Box::new(window)); // Leak to static ref

        let size = window.inner_size();

        println!("Window Size : {size:?}");
        let surface_texture = SurfaceTexture::new(size.width, size.height, window);

        let pixels = Pixels::new(size.width, size.height, surface_texture).unwrap();
        self.window = Some(window);
        self.pixels = Some(pixels);
        self.window_scale = window.scale_factor();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::Touch(item) => {
                let pos = Vec2d { x: item.location.x as f32, y: item.location.y as f32 };
                let event = match item.phase {
                    winit::event::TouchPhase::Started => Some(ElementEvent::PointerDown(pos)),
                    winit::event::TouchPhase::Moved => Some(ElementEvent::PointerMove(pos)),
                    winit::event::TouchPhase::Ended => Some(ElementEvent::PointerUp(pos)),
                    winit::event::TouchPhase::Cancelled => None,
                };
                #[allow(clippy::collapsible_if)]
                if let Some(event) = event {
                    if let Some(root) = &self.widget_root {
                        dispatch_event(root.as_ref(), pos, &event);
                    }
                }
            }
            // WindowEvent::Focused
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = Vec2d { x: position.x as f32, y: position.y as f32 };
                // println!("Cursor: {:?}", self.cursor_pos);
                if let Some(root) = &self.widget_root {
                    let event = ElementEvent::PointerMove(self.cursor_pos);
                    #[allow(clippy::collapsible_if)]
                    dispatch_event(root.as_ref(), self.cursor_pos, &event);
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                if button != winit::event::MouseButton::Left {
                    return;
                }

                let c = self.cursor_pos;
                let event = if state.is_pressed() {
                    ElementEvent::PointerDown(c)
                } else {
                    ElementEvent::PointerUp(c)
                };
                #[allow(clippy::collapsible_if)]
                if let Some(root) = &self.widget_root {
                    dispatch_event(root.as_ref(), c, &event);
                }
            }

            WindowEvent::RedrawRequested => self.render(event_loop),
            WindowEvent::Resized(size) => {
                self.pending_resize = Some(size);
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // if let Some(window) = &self.window {
        //     window.request_redraw();
        // }
    }
}
#[allow(dead_code)]
impl App {
    fn is_on_click(widget: &dyn Element, c: Vec2d) -> Option<&dyn Element> {
        let bounds = widget.pos_start_end();

        if let Some((start, end)) = bounds {
            let is_inside = c.x >= start.x && c.x <= end.x && c.y >= start.y && c.y <= end.y;
            if !is_inside {
                return None;
            }
        }

        let mut hit = None;
        widget.visit_children(&mut |child| {
            if hit.is_some() {
                return;
            } // already found
            if let Some(h) = Self::is_on_click(child, c) {
                hit = Some(h);
            }
        });

        // Let's collect children first.
        let mut children = Vec::new();
        widget.visit_children(&mut |child| children.push(child));

        for child in children.into_iter().rev() {
            if let Some(h) = Self::is_on_click(child, c) {
                return Some(h);
            }
        }

        if bounds.is_some() {
            return Some(widget);
        }

        None
    }

    fn render_widget_tree(widget: &dyn Element, ctx: &BuildContext) {
        ctx.canvas.save();
        widget.draw(ctx);
        let content = widget.content_size(ctx);
        let child_ctx = BuildContext {
            parent_size: content,
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: Default::default(),
            box_constraint: widget::style::BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: content.width,
                max_height: content.height,
            },
            window: ctx.window,
            async_handle: ctx.async_handle.clone(),
        };
        widget.visit_children(&mut |child| {
            Self::render_widget_tree(child, &child_ctx);
        });
        ctx.canvas.restore();
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        #[allow(clippy::collapsible_if)]
        if let Some(size) = self.pending_resize.take() {
            if let Some(pixels) = &mut self.pixels {
                let _ = pixels.resize_surface(size.width, size.height);
                let _ = pixels.resize_buffer(size.width, size.height);
            }
        }

        if let (Some(pixels), Some(window)) = (&mut self.pixels, &self.window) {
            let (width, height) = {
                #[cfg(not(target_os = "ios"))]
                {
                    let width = window.inner_size().width;
                    let height = window.inner_size().height;
                    (width, height)
                }
                #[cfg(target_os = "ios")]
                {
                    match self.native_window_size {
                        Some(item) => (item.width as u32, item.height as u32),  // ResolvedSize f32 -> u32 for pixel buffer
                        None => {
                            let width = window.inner_size().width;
                            let height = window.inner_size().height;
                            (width, height)
                        }
                    }
                }
            };

            // println!("Width {width}, height: {height}");

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
                    if self.window.is_none() {return;}
                    let ctx: BuildContext = BuildContext {
                        parent_size: ResolvedSize { width: width as f32, height: height as f32 },
                        canvas: surface.canvas(),
                        scale: self.window_scale as f32,
                        parent_pos: Default::default(),
                        box_constraint: widget::style::BoxConstraint {
                            min_width: 0.0,
                            min_height: 0.0,
                            max_width: width as f32,
                            max_height: height as f32,
                        },
                        window: self.window.unwrap(),
                        async_handle: self.async_runtime.handle().clone(),
                    };
                    ctx.canvas.clear(skia_safe::Color::WHITE);
                    #[allow(clippy::collapsible_if)]
                    if self.widget_root.is_none() {
                        if let Some(w) = self.pending_widget.take() {
                            self.widget_root = Some(w.to_element(&ctx));
                        }
                    }

                    if let Some(root) = &self.widget_root {
                        root.invalidate_layout();
                        Self::render_widget_tree(root.as_ref(), &ctx);
                    }
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
    use widget::Element;
    use widget::base::{BuildContext, Dimension, Size, Vec2d};

    struct MockWidget {
        pos: Option<Vec2d>,
        size: Option<Size>,
        children: Vec<Box<dyn Element>>,
    }

    impl Element for MockWidget {
        fn draw(&self, _ctx: &BuildContext) {}
        fn pos(&self) -> Option<Vec2d> {
            self.pos
        }
        fn size(&self) -> Option<Size> {
            self.size
        }
        fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
            for child in &self.children {
                visitor(child.as_ref());
            }
        }
    }

    #[test]
    fn test_is_on_click_wrapper() {
        let btn = MockWidget {
            pos: Some(Vec2d { x: 10.0, y: 10.0 }),
            size: Some(Size { width: Dimension::Px(20.0), height: Dimension::Px(20.0) }), // 10,10 to 30,30
            children: vec![],
        };

        let wrapper = MockWidget { pos: None, size: None, children: vec![Box::new(btn)] };

        // Click at 15, 15 (inside button)
        let hit = App::is_on_click(&wrapper, Vec2d { x: 15.0, y: 15.0 });
        assert!(hit.is_some());

    }

    #[test]
    fn test_is_on_click_outside() {
        let btn = MockWidget {
            pos: Some(Vec2d { x: 10.0, y: 10.0 }),
            size: Some(Size { width: Dimension::Px(20.0), height: Dimension::Px(20.0) }), // 10,10 to 30,30
            children: vec![],
        };

        let wrapper = MockWidget { pos: None, size: None, children: vec![Box::new(btn)] };

        // Click at 50, 50 (outside button)
        let hit = App::is_on_click(&wrapper, Vec2d { x: 50.0, y: 50.0 });
        assert!(hit.is_none());
    }
}
