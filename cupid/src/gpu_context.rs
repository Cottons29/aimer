use utils::{debug, error, info};
use wgpu::{Device, Instance, Limits, Queue, Surface, SurfaceConfiguration, TextureFormat};
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct GpuContext<'w> {
    pub device: Device,
    pub queue: Queue,
    pub surface: Surface<'w>,
    pub config: SurfaceConfiguration,
    pub format: TextureFormat,
    pub is_srgb: bool,
}

impl<'w> GpuContext<'w> {
    pub fn initialize(window: &'w Window, size: PhysicalSize<u32>) -> Self {
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
            #[cfg(target_arch = "wasm32")]
            {
                wgpu::Backends::BROWSER_WEBGPU
            }
        };

        debug!("gpu backends: {:?}", backends);

        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::default(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });

        let surface = instance
            .create_surface(window)
            .expect("failed to create surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .or_else(|_| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: true,
            }))
        })
        .map_err(|e| {
            error!("Failed to find a suitable adapter: {}", e);
        })
        .unwrap();

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
        #[cfg(not(target_os = "android"))]
        let limit = Limits::default();

        let (device, queue) = match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("cupid gpu renderer device"),
            required_features: wgpu::Features::default(),
            required_limits: limit,
            ..Default::default()
        })) {
            Ok((device, queue)) => (device, queue),
            Err(e) => {
                error!("Failed to create device: {}", e);
                std::process::exit(1);
            }
        };

        let caps = surface.get_capabilities(&adapter);

        debug!("Surface format: {:?}", caps.formats);

        let selected_format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let is_srgb = selected_format.is_srgb();

        let max_dim = device.limits().max_texture_dimension_2d;

        debug!("Gpu Context : Initialized with max texture dimension: {} and is_srgb: {}", max_dim, is_srgb);
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: selected_format,
            width: size.width.max(1).min(max_dim),
            height: size.height.max(1).min(max_dim),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        Self { device, queue, surface, config, format: selected_format, is_srgb }
    }

    pub fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width > 0 && size.height > 0 {
            let max_dim = self.device.limits().max_texture_dimension_2d;
            self.config.width = size.width.min(max_dim);
            self.config.height = size.height.min(max_dim);
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub fn width(&self) -> u32 {
        self.config.width
    }

    pub fn height(&self) -> u32 {
        self.config.height
    }

    pub fn begin_frame(&self) -> wgpu::CurrentSurfaceTexture {
        self.surface.get_current_texture()
    }

    pub fn end_frame(&self, frame: wgpu::SurfaceTexture) {
        frame.present();
    }
}
