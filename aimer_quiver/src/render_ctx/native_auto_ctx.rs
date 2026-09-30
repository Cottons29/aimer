use aimer_cupid::AntiAlias;
use aimer_cupid::damage_region::DamageSet;
use aimer_cupid::frame::FramePacket;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::render_ctx::{
    NativeBackendChoice, OpenGlApi, PresentOutcome, VulkanApi, choose_native_backend,
};

enum ActiveBackend {
    Vulkan(VulkanApi),
    OpenGl(OpenGlApi),
}

/// Linux and Android presenter that tries Vulkan first, then OpenGL if initialization fails.
pub struct NativeAutoApi {
    antialiasing: AntiAlias,
    active: Option<ActiveBackend>,
}

impl Default for NativeAutoApi {
    fn default() -> Self {
        Self::new(AntiAlias::default())
    }
}

impl NativeAutoApi {
    /// Creates an uninitialized auto-selecting native renderer.
    pub fn new(antialiasing: AntiAlias) -> Self {
        Self {
            antialiasing,
            active: None,
        }
    }

    #[inline]
    pub fn is_ready(&self) -> bool {
        self.active.as_ref().is_some_and(|active| match active {
            ActiveBackend::Vulkan(api) => api.is_ready(),
            ActiveBackend::OpenGl(api) => api.is_ready(),
        })
    }

    #[inline]
    pub fn is_offloaded(&self) -> bool {
        false
    }

    /// Initializes Vulkan first and falls back to OpenGL on failure.
    pub fn initialize(&mut self, window: &'static Window, size: PhysicalSize<u32>) {
        if self.is_ready() {
            self.resize(size);
            return;
        }

        let mut vulkan = VulkanApi::new(self.antialiasing);
        let mut opengl = OpenGlApi::new(self.antialiasing);
        let mut vulkan_error = None;
        let mut opengl_error = None;
        let choice = choose_native_backend(
            || match vulkan.try_initialize(window, size) {
                Ok(()) => true,
                Err(error) => {
                    vulkan_error = Some(error.to_string());
                    false
                }
            },
            || match opengl.try_initialize(window, size) {
                Ok(()) => true,
                Err(error) => {
                    opengl_error = Some(error.to_string());
                    false
                }
            },
        );
        match choice {
            Some(NativeBackendChoice::Vulkan) => self.active = Some(ActiveBackend::Vulkan(vulkan)),
            Some(NativeBackendChoice::OpenGl) => self.active = Some(ActiveBackend::OpenGl(opengl)),
            None => panic!(
                "initialize native rendering failed: Vulkan: {}; OpenGL: {}",
                vulkan_error.as_deref().unwrap_or("no error detail"),
                opengl_error.as_deref().unwrap_or("no error detail"),
            ),
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        match &mut self.active {
            Some(ActiveBackend::Vulkan(api)) => api.resize(size),
            Some(ActiveBackend::OpenGl(api)) => api.resize(size),
            None => {}
        }
    }

    pub fn render_frame_packet(
        &mut self,
        draw_fn: impl FnOnce(&aimer_cupid::canvas::CupidCanvas, u32, u32) -> (f32, DamageSet),
    ) -> PresentOutcome {
        match &mut self.active {
            Some(ActiveBackend::Vulkan(api)) => api.render_frame_packet(draw_fn),
            Some(ActiveBackend::OpenGl(api)) => api.render_frame_packet(draw_fn),
            None => PresentOutcome::Dropped,
        }
    }

    pub fn present_packet(&mut self, packet: FramePacket) -> PresentOutcome {
        match &mut self.active {
            Some(ActiveBackend::Vulkan(api)) => api.present_packet(packet),
            Some(ActiveBackend::OpenGl(api)) => api.present_packet(packet),
            None => PresentOutcome::Dropped,
        }
    }
}
