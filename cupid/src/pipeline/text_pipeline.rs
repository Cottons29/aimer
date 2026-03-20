use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Color as GlyphonColor, Family, FontSystem, Metrics,
    Resolution, Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

pub struct TextPipeline {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub atlas: TextAtlas,
    pub text_renderer: TextRenderer,
    pub viewport: Viewport,
    #[allow(dead_code)]
    cache: Cache,
}

pub struct TextDrawRequest {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub font_size: f32,
    pub color: [f32; 4],
    pub bounds_width: f32,
    pub bounds_height: f32,
}

impl TextPipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let text_renderer = TextRenderer::new(
            &mut atlas,
            device,
            wgpu::MultisampleState::default(),
            None,
        );
        let viewport = Viewport::new(device, &cache);

        Self {
            font_system,
            swash_cache,
            atlas,
            text_renderer,
            viewport,
            cache,
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        requests: &[TextDrawRequest],
    ) {
        self.viewport.update(queue, Resolution { width, height });

        let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(requests.len());

        for req in requests {
            let mut buffer = GlyphonBuffer::new(
                &mut self.font_system,
                Metrics::new(req.font_size, req.font_size * 1.2),
            );
            buffer.set_size(
                &mut self.font_system,
                Some(req.bounds_width),
                Some(req.bounds_height),
            );
            buffer.set_text(
                &mut self.font_system,
                &req.text,
                &Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
                None,
            );
            buffer.shape_until_scroll(&mut self.font_system, false);
            buffers.push(buffer);
        }

        let text_areas: Vec<TextArea<'_>> = requests
            .iter()
            .zip(buffers.iter())
            .map(|(req, buf)| {
                let c = req.color;
                TextArea {
                    buffer: buf,
                    left: req.x,
                    top: req.y,
                    scale: 1.0,
                    bounds: TextBounds {
                        left: req.x as i32,
                        top: req.y as i32,
                        right: (req.x + req.bounds_width) as i32,
                        bottom: (req.y + req.bounds_height) as i32,
                    },
                    default_color: GlyphonColor::rgba(
                        (c[0] * 255.0) as u8,
                        (c[1] * 255.0) as u8,
                        (c[2] * 255.0) as u8,
                        (c[3] * 255.0) as u8,
                    ),
                    custom_glyphs: &[],
                }
            })
            .collect();

        self.text_renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .expect("failed to prepare text");
    }

    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        self.text_renderer
            .render(&self.atlas, &self.viewport, pass)
            .expect("failed to render text");
    }
}
