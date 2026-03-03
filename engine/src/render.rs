#[allow(unused)]
use crate::render;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
#[cfg(not(target_arch = "wasm32"))]
use pixels::{Pixels, SurfaceTexture};
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
use widget::base::BuildContext;
use widget::{dispatch_event, Element, ElementEvent, Widget};
use winit::application::ApplicationHandler;
#[allow(unused)]
use winit::dpi::{LogicalSize, PhysicalSize, Position};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
#[allow(unused)]
use winit::monitor::MonitorHandle;
#[allow(unused)]
use winit::window::{self, Fullscreen, Window, WindowAttributes, WindowId};
use utils::{debug, info};

#[cfg(target_arch = "wasm32")]
type FLOAT = f64;
#[cfg(not(target_arch = "wasm32"))]
type FLOAT = f32;

pub struct App {
    pub window: Option<&'static Window>,
    #[cfg(not(target_arch = "wasm32"))]
    pub pixels: Option<Pixels<'static>>,
    #[cfg(target_arch = "wasm32")]
    pub canvas_ctx: Option<web_sys::CanvasRenderingContext2d>,
    pub widget_root: Option<Box<dyn Element>>,
    pub pending_widget: Option<Box<dyn Widget>>,
    pub cursor_pos: Vec2d,
    pub window_scale: f64,
    pub native_window_size: Option<ResolvedSize>,
    pub pending_resize: Option<PhysicalSize<u32>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_runtime: Runtime,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "ios")]
        {
            match crate::ios_screen::get_screen_resolution_pixels() {
                Some((width, height)) => {
                    println!("IOS TARGET NATIVE Window Size : {width}x{height}");
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
                match crate::ios_screen::get_screen_resolution_pixels() {
                    Some((w, h)) => {
                        println!("IOS TARGET Window Size : {w}x{h}");
                        let phy_size = PhysicalSize::new(w as u32, h as u32);
                        WindowAttributes::default().with_inner_size(phy_size)
                    }
                    None => WindowAttributes::default(),
                }
            }
        };

        let window = event_loop.create_window(window_attributes).unwrap();
        let window: &'static Window = Box::leak(Box::new(window)); // Leak to static ref

        let size = window.inner_size();

        println!("Window Size : {size:?}");
        
        #[cfg(not(target_arch = "wasm32"))]
        {
            let surface_texture = SurfaceTexture::new(size.width, size.height, window);
            let pixels = Pixels::new(size.width, size.height, surface_texture).unwrap();
            self.pixels = Some(pixels);
        }

        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            use wasm_bindgen::JsCast;
            let canvas = window.canvas().unwrap();
            let web_window = web_sys::window().unwrap();
            let document = web_window.document().unwrap();
            let body = document.body().unwrap();
            utils::info!("Creating canvas...");
            body.append_child(&canvas).unwrap();
            utils::info!("Canvas created.");

            canvas.set_attribute("id", "oxidize_app").unwrap();
            // canvas.set_attribute("width", "100%").unwrap();
            // canvas.set_attribute("height", "100%").unwrap();

            utils::info!("Getting canvas context...");
            let ctx = canvas
                .get_context("2d")
                .unwrap()
                .unwrap()
                .dyn_into::<web_sys::CanvasRenderingContext2d>()
                .unwrap();
            utils::info!("Canvas context obtained.");
            self.canvas_ctx = Some(ctx);
        }

        self.window = Some(window);
        self.window_scale = window.scale_factor();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }

            WindowEvent::Touch(item) => {
                let pos = Vec2d { x: item.location.x as FLOAT, y: item.location.y as FLOAT };
                info!("Touch: {:?}", pos);
                let event = match item.phase {
                    winit::event::TouchPhase::Started => Some(ElementEvent::PointerDown(pos)),
                    winit::event::TouchPhase::Moved => Some(ElementEvent::PointerMove(pos)),
                    winit::event::TouchPhase::Ended => Some(ElementEvent::PointerUp(pos)),
                    winit::event::TouchPhase::Cancelled => Some(ElementEvent::Cancel),
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
                self.cursor_pos = Vec2d { x: position.x as FLOAT, y: position.y as FLOAT };
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
            cursor_pos: ctx.cursor_pos,
            box_constraint: widget::style::BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: content.width,
                max_height: content.height,
            },
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
        };
        widget.visit_children(&mut |child| {
            Self::render_widget_tree(child, &child_ctx);
        });
        ctx.canvas.restore();
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {

        // debug!("Rendering is starting...");

        #[allow(clippy::collapsible_if)]
        if let Some(size) = self.pending_resize.take() {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(pixels) = &mut self.pixels {
                let _ = pixels.resize_surface(size.width, size.height);
                let _ = pixels.resize_buffer(size.width, size.height);
            }
            #[cfg(target_arch = "wasm32")]
            if let (Some(_ctx), Some(window)) = (&self.canvas_ctx, &self.window) {
                use winit::platform::web::WindowExtWebSys;
                if let Some(canvas) = window.canvas() {
                    canvas.set_width(size.width);
                    canvas.set_height(size.height);
                }
            }
        }

        #[cfg(not(target_arch = "wasm32"))]
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
                        Some(item) => {
                            utils::info!("IOS TARGET NATIVE Window Size : {}x{}", item.width, item.height);
                            (item.width as u32, item.height as u32)
                        },  // ResolvedSize f32 -> u32 for pixel buffer
                        None => {

                            let width = window.inner_size().width;
                            let height = window.inner_size().height;
                            utils::info!("Not found the native window size : {width}x{height}");
                            (width, height)
                        }
                    }
                }
            };

            // println!("Width {width}, height: {height}");

            let frame = pixels.frame_mut();

            let info = skia_safe::ImageInfo::new(
                (width as i32, height as i32),
                skia_safe::ColorType::RGBA8888,
                skia_safe::AlphaType::Premul,
                None,
            );

            let row_bytes = width as usize * 4;

            {
                if let Some(mut surface) =
                    skia_safe::surfaces::wrap_pixels(&info, frame, Some(row_bytes), None)
                {
                    if self.window.is_none() {return;}
                    let ctx: BuildContext = BuildContext {
                        parent_size: ResolvedSize { width: width as FLOAT, height: height as FLOAT },
                        canvas: surface.canvas(),
                        scale: self.window_scale as FLOAT,
                        parent_pos: Default::default(),
                        cursor_pos: self.cursor_pos,
                        box_constraint: widget::style::BoxConstraint {
                            min_width: 0.0,
                            min_height: 0.0,
                            max_width: width as FLOAT,
                            max_height: height as FLOAT,
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
                        Self::render_widget_tree(root.as_ref(), &ctx);
                    }
                }
            }

            if let Err(err) = pixels.render() {
                eprintln!("pixels.render() failed: {err}");
                event_loop.exit();
            }
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            // utils::info!("Setting up canvas context...");
            if let (Some(ctx), Some(window)) = (&self.canvas_ctx, &self.window) {
                // utils::info!("Stating render loop...");
                let width = window.inner_size().width;
                let height = window.inner_size().height;

                ctx.clear_rect(0.0, 0.0, width as f64, height as f64);

                let build_ctx = BuildContext {
                    parent_size: ResolvedSize { width: width as f64, height: height as f64 },
                    canvas: ctx,
                    scale: self.window_scale,
                    parent_pos: Default::default(),
                    cursor_pos: self.cursor_pos,
                    box_constraint: widget::style::BoxConstraint {
                        min_width: 0.0,
                        min_height: 0.0,
                        max_width: width as f64,
                        max_height: height as f64,
                    },
                    window,
                    #[cfg(not(target_arch = "wasm32"))]
                    async_handle: self.async_runtime.handle().clone(),
                };

                #[allow(clippy::collapsible_if)]
                if self.widget_root.is_none() {
                    if let Some(w) = self.pending_widget.take() {
                        self.widget_root = Some(w.to_element(&build_ctx));
                    }
                }

                if let Some(root) = &self.widget_root {
                    Self::render_widget_tree(root.as_ref(), &build_ctx);
                }
            }else{
                utils::info!("Canvas context is not ready yet.");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use attribute::dimension::Dimension;
    use attribute::position::Vec2d;
    use attribute::size::Size;
    use widget::base::BuildContext;
    use widget::Element;

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
