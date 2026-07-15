use aimer_utils::{debug, error, info};
use wgpu::{
    Device, Instance, Limits, Queue, Surface, SurfaceColorSpace, SurfaceConfiguration,
    SurfaceTexture, TextureFormat,
};
use winit::dpi::PhysicalSize;
use winit::window::Window;

fn surface_size(size: PhysicalSize<u32>, max_dimension: u32) -> PhysicalSize<u32> {
    if size.width <= max_dimension && size.height <= max_dimension {
        return size;
    }

    if size.width >= size.height {
        PhysicalSize::new(
            max_dimension,
            ((size.height as u64 * max_dimension as u64) / size.width as u64).max(1) as u32,
        )
    } else {
        PhysicalSize::new(
            ((size.width as u64 * max_dimension as u64) / size.height as u64).max(1) as u32,
            max_dimension,
        )
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn wasm_gpu_backends() -> [wgpu::Backends; 2] {
    [wgpu::Backends::BROWSER_WEBGPU, wgpu::Backends::GL]
}

async fn create_gpu<'w>(
    window: &'w Window,
    size: PhysicalSize<u32>,
    backends: wgpu::Backends,
) -> Result<(Device, Queue, Surface<'w>, wgpu::Adapter), String> {
    #[cfg(not(target_os = "android"))]
    let _ = size;
    debug!("GPU backends: {:?}", backends);

    let instance = Instance::new(wgpu::InstanceDescriptor {
        backends,
        flags: wgpu::InstanceFlags::default(),
        backend_options: wgpu::BackendOptions::default(),
        memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
        display: None,
    });

    debug!("GPU instance: {:?}", instance);

    let surface = instance
        .create_surface(window)
        .map_err(|err| format!("failed to create surface: {err}"))?;

    debug!("Surface: {:?}", surface);

    let adapter = match instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: true,
        })
        .await
    {
        Ok(adapter) => adapter,
        Err(first_err) => instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: true,
                apply_limit_buckets: true,
            })
            .await
            .map_err(|second_err| {
                format!(
                    "failed to find a suitable adapter: {first_err}; fallback adapter: {second_err}"
                )
            })?,
    };

    info!("Creating the gpu device");

    #[cfg(target_os = "android")]
    let resolution = Limits {
        max_texture_dimension_1d: size.width,
        max_texture_dimension_2d: size.height,
        max_texture_dimension_3d: 256,
        ..Limits::default()
    };

    #[cfg(target_os = "android")]
    let limit = Limits::downlevel_webgl2_defaults().using_resolution(resolution);
    #[cfg(target_os = "ios")]
    let limit = adapter.limits();
    #[cfg(not(any(target_os = "android", target_os = "ios", target_arch = "wasm32")))]
    let limit = Limits::default();
    #[cfg(target_arch = "wasm32")]
    let limit = Limits::downlevel_webgl2_defaults();

    let mut features = wgpu::Features::default();
    if adapter
        .features()
        .contains(wgpu::Features::PIPELINE_CACHE)
    {
        features |= wgpu::Features::PIPELINE_CACHE;
    }

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("cupid gpu renderer device"),
            required_features: features,
            required_limits: limit,
            ..Default::default()
        })
        .await
        .map_err(|err| format!("failed to create device: {err}"))?;

    Ok((device, queue, surface, adapter))
}

pub struct GpuContext<'w> {
    pub device: Device,
    pub queue: Queue,
    pub surface: Surface<'w>,
    pub config: SurfaceConfiguration,
    pub format: TextureFormat,
    pub is_srgb: bool,
    viewport_size: PhysicalSize<u32>,
}

impl<'w> GpuContext<'w> {
    /// Synchronous initializer for non-wasm targets.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn initialize(window: &'w Window, size: PhysicalSize<u32>) -> Self {
        pollster::block_on(Self::initialize_async(window, size))
    }

    /// Async initializer usable on all targets (required on wasm where blocking
    /// is not allowed).
    pub async fn initialize_async(window: &'w Window, size: PhysicalSize<u32>) -> Self {
        #[cfg(target_arch = "wasm32")]
        let (device, queue, surface, adapter) = {
            let mut failures = Vec::new();
            let mut initialized = None;
            for backend in wasm_gpu_backends() {
                match create_gpu(window, size, backend).await {
                    Ok(gpu) => {
                        initialized = Some(gpu);
                        break;
                    }
                    Err(err) => {
                        error!("GPU initialization with {:?} failed: {}", backend, err);
                        failures.push(format!("{backend:?}: {err}"));
                    }
                }
            }

            initialized.unwrap_or_else(|| {
                panic!("Failed to initialize WebGPU or WebGL: {}", failures.join("; "))
            })
        };

        #[cfg(not(target_arch = "wasm32"))]
        let backends = {
            #[cfg(target_os = "android")]
            {
                wgpu::Backends::GL
            }
            #[cfg(any(target_os = "ios", target_os = "macos"))]
            {
                wgpu::Backends::METAL
            }
            #[cfg(target_os = "windows")]
            {
                wgpu::Backends::D3D11
            }
            #[cfg(target_os = "linux")]
            {
                wgpu::Backends::VULKAN
            }
        };

        #[cfg(not(target_arch = "wasm32"))]
        let (device, queue, surface, adapter) = create_gpu(window, size, backends)
            .await
            .unwrap_or_else(|err| {
                error!("Failed to initialize GPU: {}", err);
                std::process::exit(1);
            });

        let caps = surface.get_capabilities(&adapter);

        debug!("Surface format: {:?}", caps.formats);

        let selected_format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let is_srgb = selected_format.is_srgb();

        let max_dim = device
            .limits()
            .max_texture_dimension_2d;

        // Use `Fifo` (v-sync) so presentation is paced by the display/compositor
        // itself: `surface.present()` blocks until the next refresh slot, keeping
        // frames in phase with the panel. This replaces the old
        // `Mailbox`/`Immediate` render-ahead modes that were only honored when the
        // surface owned the scanout (fullscreen); in windowed mode the compositor
        // re-synchronized them anyway, and the software frame limiter that capped
        // them raced the compositor's v-sync, producing windowed judder. Letting
        // v-sync be the single timing source removes that beat pattern entirely.
        // `Fifo` is always available and still presents at the full panel refresh
        // rate (e.g. 120 Hz) on high-refresh displays.
        let present_mode = wgpu::PresentMode::Fifo;

        debug!(
            "Gpu Context : Initialized with max texture dimension: {} and is_srgb: {} present_mode: {:?}",
            max_dim, is_srgb, present_mode
        );
        let backing_size = surface_size(size, max_dim);
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: selected_format,
            width: backing_size.width.max(1),
            height: backing_size.height.max(1),
            present_mode,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        Self {
            device,
            queue,
            surface,
            config,
            format: selected_format,
            is_srgb,
            viewport_size: size,
        }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            let max_dim = self
                .device
                .limits()
                .max_texture_dimension_2d;
            let backing_size = surface_size(size, max_dim);
            self.config.width = backing_size.width;
            self.config.height = backing_size.height;
            self.viewport_size = size;
            self.surface
                .configure(&self.device, &self.config);
        }
    }

    pub fn width(&self) -> u32 {
        self.viewport_size.width
    }

    pub fn height(&self) -> u32 {
        self.viewport_size.height
    }

    pub fn begin_frame(&self) -> wgpu::CurrentSurfaceTexture {
        self.surface.get_current_texture()
    }

    pub fn end_frame(&self, frame: SurfaceTexture) {
        // wgpu 30: presentation moved from `SurfaceTexture::present()` to
        // `Queue::present()`.
        self.queue.present(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasm_gpu_backends_try_webgpu_before_webgl() {
        assert_eq!(wasm_gpu_backends(), [wgpu::Backends::BROWSER_WEBGPU, wgpu::Backends::GL]);
    }

    #[test]
    fn surface_size_preserves_aspect_ratio_when_width_exceeds_limit() {
        assert_eq!(
            surface_size(PhysicalSize::new(3072, 1728), 2048),
            PhysicalSize::new(2048, 1152)
        );
    }

    #[test]
    fn surface_size_preserves_aspect_ratio_when_height_exceeds_limit() {
        assert_eq!(
            surface_size(PhysicalSize::new(1728, 3072), 2048),
            PhysicalSize::new(1152, 2048)
        );
    }

    #[test]
    fn surface_size_keeps_dimensions_within_limit() {
        assert_eq!(
            surface_size(PhysicalSize::new(2048, 1024), 2048),
            PhysicalSize::new(2048, 1024)
        );
    }
}
