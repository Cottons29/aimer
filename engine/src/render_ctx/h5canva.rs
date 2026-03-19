#[cfg(target_arch = "wasm32")]
pub mod render_ctx {
    use wasm_bindgen::JsCast;
    use web_sys::CanvasRenderingContext2d;
    use winit::dpi::PhysicalSize;
    use winit::platform::web::WindowExtWebSys;
    use winit::window::Window;

    pub struct H5CanvasApi {
        pub canvas_ctx: Option<CanvasRenderingContext2d>,
    }

    impl Default for H5CanvasApi {
        fn default() -> Self {
            Self { canvas_ctx: None }
        }
    }

    impl H5CanvasApi {
        pub fn initialize(&mut self, window: &Window, _size: PhysicalSize<u32>) {
            let canvas = window.canvas().unwrap();
            let web_window = web_sys::window().unwrap();
            let document = web_window.document().unwrap();
            let body = document.body().unwrap();
            utils::info!("Creating canvas...");
            body.append_child(&canvas).unwrap();
            utils::info!("Canvas created.");

            canvas.set_attribute("id", "aimer_app").unwrap();

            utils::info!("Getting canvas context...");
            let ctx = canvas
                .get_context("2d")
                .unwrap()
                .unwrap()
                .dyn_into::<CanvasRenderingContext2d>()
                .unwrap();
            utils::info!("Canvas context obtained.");
            self.canvas_ctx = Some(ctx);
        }

        pub fn resize(&mut self, window: &Window, size: PhysicalSize<u32>) {
            if let Some(canvas) = window.canvas() {
                canvas.set_width(size.width);
                canvas.set_height(size.height);
            }
        }

        /// Clear the canvas and call `draw_fn` with the 2d context and dimensions.
        pub fn render_frame(
            &mut self,
            window: &Window,
            draw_fn: impl FnOnce(&CanvasRenderingContext2d, u32, u32),
        ) {
            if let Some(ctx) = &self.canvas_ctx {
                let width = window.inner_size().width;
                let height = window.inner_size().height;
                ctx.clear_rect(0.0, 0.0, width as f64, height as f64);
                draw_fn(ctx, width, height);
            } else {
                utils::info!("Canvas context is not ready yet.");
            }
        }
    }
}