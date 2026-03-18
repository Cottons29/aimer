#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod render_ctx {
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_core_foundation::CGSize;
    use objc2_metal::{MTLCommandBuffer, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice};
    use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
    use skia_safe::gpu::{self, DirectContext, SurfaceOrigin, backend_render_targets, mtl};
    use skia_safe::ColorType;
    use winit::dpi::PhysicalSize;
    use winit::raw_window_handle::HasWindowHandle;
    use winit::window::Window;

    pub struct MetalApi {
        pub metal_layer: Option<Retained<CAMetalLayer>>,
        pub command_queue: Option<Retained<ProtocolObject<dyn MTLCommandQueue>>>,
        pub skia_context: Option<DirectContext>,
    }

    impl Default for MetalApi {
        fn default() -> Self {
            Self { metal_layer: None, command_queue: None, skia_context: None }
        }
    }

    impl MetalApi {
        pub fn initialize(&mut self, window: &Window, size: PhysicalSize<u32>) {
            if self.metal_layer.is_some() {
                return;
            }

            let device = MTLCreateSystemDefaultDevice().expect("no Metal device found");

            let metal_layer = {
                let layer = CAMetalLayer::new();
                layer.setDevice(Some(&device));
                layer.setPixelFormat(objc2_metal::MTLPixelFormat::BGRA8Unorm);
                layer.setPresentsWithTransaction(false);
                layer.setFramebufferOnly(false);
                layer.setDrawableSize(CGSize::new(size.width as f64, size.height as f64));

                let view_ptr = match window.window_handle().unwrap().as_raw() {
                    #[cfg(target_os = "macos")]
                    raw_window_handle::RawWindowHandle::AppKit(appkit) => {
                        appkit.ns_view.as_ptr() as *mut objc2_app_kit::NSView
                    }
                    #[cfg(target_os = "ios")]
                    raw_window_handle::RawWindowHandle::UiKit(uikit) => {
                        uikit.ui_view.as_ptr() as *mut objc2_ui_kit::UIView
                    }
                    _ => panic!("Unsupported window handle type"),
                };
                let view = unsafe { view_ptr.as_ref().unwrap() };

                #[cfg(target_os = "macos")]
                {
                    view.setWantsLayer(true);
                    view.setLayer(Some(&layer.clone().into_super()));
                }

                #[cfg(target_os = "ios")]
                {
                    layer.setFrame(view.layer().frame());
                    view.layer().addSublayer(&layer);
                }

                layer
            };

            let command_queue = device
                .newCommandQueue()
                .expect("unable to get command queue");

            let backend = unsafe {
                mtl::BackendContext::new(
                    Retained::as_ptr(&device) as mtl::Handle,
                    Retained::as_ptr(&command_queue) as mtl::Handle,
                )
            };

            let skia_context = gpu::direct_contexts::make_metal(&backend, None).unwrap();

            self.metal_layer = Some(metal_layer);
            self.command_queue = Some(command_queue);
            self.skia_context = Some(skia_context);
        }

        pub fn resize(&mut self, size: PhysicalSize<u32>) {
            if let Some(metal_layer) = &self.metal_layer {
                metal_layer.setDrawableSize(CGSize::new(size.width as f64, size.height as f64));
            }
        }

        /// Acquire a drawable, create a Skia surface, and call `draw_fn` with the canvas and dimensions.
        /// After `draw_fn` returns, flushes Skia and presents the drawable.
        pub fn render_frame(&mut self, draw_fn: impl FnOnce(&skia_safe::Canvas, u32, u32)) {
            let (metal_layer, command_queue, skia_context) = match (
                &self.metal_layer,
                &self.command_queue,
                &mut self.skia_context,
            ) {
                (Some(ml), Some(cq), Some(sc)) => (ml, cq, sc),
                _ => return,
            };

            let drawable = match metal_layer.nextDrawable() {
                Some(d) => d,
                None => return,
            };

            let (width, height) = {
                let size = metal_layer.drawableSize();
                (size.width as u32, size.height as u32)
            };

            let texture_info =
                unsafe { mtl::TextureInfo::new(Retained::as_ptr(&drawable.texture()) as mtl::Handle) };

            let backend_render_target =
                backend_render_targets::make_mtl((width as i32, height as i32), &texture_info);

            let mut surface = gpu::surfaces::wrap_backend_render_target(
                skia_context,
                &backend_render_target,
                SurfaceOrigin::TopLeft,
                ColorType::BGRA8888,
                None,
                None,
            );

            if let Some(ref mut surface) = surface {
                surface.canvas().clear(skia_safe::Color::WHITE);
                draw_fn(surface.canvas(), width, height);
            }

            skia_context.flush_and_submit();
            drop(surface);

            let cmd_buffer = command_queue
                .commandBuffer()
                .expect("unable to get command buffer");

            let drawable_ref: Retained<ProtocolObject<dyn objc2_metal::MTLDrawable>> =
                (&drawable).into();
            cmd_buffer.presentDrawable(&drawable_ref);
            cmd_buffer.commit();
        }
    }
}
