#[cfg(target_os = "android")]
pub mod render_ctx {
    use khronos_egl as egl;
    use skia_safe::gpu::{
        self as gpu_android, DirectContext as AndroidDirectContext, SurfaceOrigin as AndroidSurfaceOrigin,
        backend_render_targets as android_backend_render_targets, gl as skia_gl,
    };
    use skia_safe::ColorType as AndroidColorType;
    use winit::dpi::PhysicalSize;
    use winit::raw_window_handle::HasWindowHandle;
    use winit::window::Window;

    pub struct OpenGLES2Api {
        pub egl_display: Option<egl::Display>,
        pub egl_surface: Option<egl::Surface>,
        pub egl_context: Option<egl::Context>,
        pub skia_gl_context: Option<gpu_android::DirectContext>,
    }

    impl Default for OpenGLES2Api {
        fn default() -> Self {
            Self { egl_display: None, egl_surface: None, egl_context: None, skia_gl_context: None }
        }
    }

    impl OpenGLES2Api {
        pub fn initialize(&mut self, window: &Window, _size: PhysicalSize<u32>) {
            if self.egl_display.is_some() {
                return;
            }

            let egl_lib = unsafe { egl::DynamicInstance::<egl::EGL1_4>::load_required() }
                .expect("failed to load EGL");

            let display = unsafe { egl_lib.get_display(egl::DEFAULT_DISPLAY) }
                .expect("failed to get EGL display");

            egl_lib
                .initialize(display)
                .expect("failed to initialize EGL");

            let config_attribs = [
                egl::RED_SIZE, 8,
                egl::GREEN_SIZE, 8,
                egl::BLUE_SIZE, 8,
                egl::ALPHA_SIZE, 8,
                egl::DEPTH_SIZE, 0,
                egl::STENCIL_SIZE, 8,
                egl::RENDERABLE_TYPE, egl::OPENGL_ES2_BIT,
                egl::SURFACE_TYPE, egl::WINDOW_BIT,
                egl::NONE,
            ];

            let config = egl_lib
                .choose_first_config(display, &config_attribs)
                .expect("failed to choose EGL config")
                .expect("no matching EGL config");

            let context_attribs = [egl::CONTEXT_CLIENT_VERSION, 2, egl::NONE];
            let egl_context = egl_lib
                .create_context(display, config, None, &context_attribs)
                .expect("failed to create EGL context");

            let native_window = match window.window_handle().unwrap().as_raw() {
                raw_window_handle::RawWindowHandle::AndroidNdk(handle) => handle.a_native_window.as_ptr(),
                _ => panic!("Expected AndroidNdk window handle"),
            };

            let surface_attribs = [egl::NONE];
            let egl_surface =
                unsafe { egl_lib.create_window_surface(display, config, native_window, Some(&surface_attribs)) }
                    .expect("failed to create EGL window surface");

            egl_lib
                .make_current(display, Some(egl_surface), Some(egl_surface), Some(egl_context))
                .expect("failed to make EGL context current");

            let interface = skia_gl::Interface::new_native().expect("failed to create Skia GL interface");
            let skia_context =
                gpu_android::direct_contexts::make_gl(interface, None).expect("failed to create Skia GL DirectContext");

            self.egl_display = Some(display);
            self.egl_surface = Some(egl_surface);
            self.egl_context = Some(egl_context);
            self.skia_gl_context = Some(skia_context);
        }

        pub fn resize(&mut self, _size: PhysicalSize<u32>) {
            // Android EGL surfaces are tied to the native window; no explicit resize needed.
        }

        /// Create a Skia surface from the GL framebuffer, call `draw_fn` with the canvas and dimensions,
        /// then flush and swap buffers.
        pub fn render_frame(&mut self, window: &Window, draw_fn: impl FnOnce(&skia_safe::Canvas, u32, u32)) {
            let (skia_context, egl_surface, egl_display) = match (
                &mut self.skia_gl_context,
                &self.egl_surface,
                &self.egl_display,
            ) {
                (Some(sc), Some(es), Some(ed)) => (sc, es, ed),
                _ => return,
            };

            let size = window.inner_size();
            let width = size.width;
            let height = size.height;

            let fb_info = skia_gl::FramebufferInfo {
                fboid: 0,
                format: skia_gl::Format::RGBA8.into(),
                ..Default::default()
            };

            let backend_render_target =
                android_backend_render_targets::make_gl((width as i32, height as i32), Some(0), 8, fb_info);

            let mut surface = gpu_android::surfaces::wrap_backend_render_target(
                skia_context,
                &backend_render_target,
                AndroidSurfaceOrigin::BottomLeft,
                AndroidColorType::RGBA8888,
                None,
                None,
            );

            if let Some(ref mut surface) = surface {
                surface.canvas().clear(skia_safe::Color::WHITE);
                draw_fn(surface.canvas(), width, height);
            }

            skia_context.flush_and_submit();
            drop(surface);

            let egl_lib = unsafe { egl::DynamicInstance::<egl::EGL1_4>::load_required() }
                .expect("failed to load EGL");
            egl_lib
                .swap_buffers(*egl_display, *egl_surface)
                .expect("failed to swap EGL buffers");
        }
    }
}