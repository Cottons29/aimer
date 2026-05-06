use std::collections::HashMap;

use super::glyph_rasterizer::GlyphKey;

/// Region within the atlas texture for a single glyph.
#[derive(Clone, Copy, Debug)]
pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl AtlasRegion {
    /// Returns UV coordinates as (u_min, v_min, u_max, v_max) given atlas dimensions.
    pub fn uvs(&self, atlas_w: u32, atlas_h: u32) -> [f32; 4] {
        let aw = atlas_w as f32;
        let ah = atlas_h as f32;
        [self.x as f32 / aw, self.y as f32 / ah, (self.x + self.width) as f32 / aw, (self.y + self.height) as f32 / ah]
    }
}

/// Simple shelf/row packer for glyph atlas allocation.
struct ShelfPacker {
    width: u32,
    height: u32,
    /// Current x cursor on the active shelf.
    cursor_x: u32,
    /// Y origin of the active shelf.
    shelf_y: u32,
    /// Height of the active shelf (tallest glyph in the row).
    shelf_height: u32,
}

impl ShelfPacker {
    fn new(width: u32, height: u32) -> Self {
        Self { width, height, cursor_x: 0, shelf_y: 0, shelf_height: 0 }
    }

    /// Try to allocate a region of `w × h`. Returns `None` if the atlas is full.
    fn allocate(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w == 0 || h == 0 {
            return Some((0, 0));
        }

        // Pad by 1 pixel to avoid sampling neighbours.
        let pw = w + 1;
        let ph = h + 1;

        if self.cursor_x + pw > self.width {
            // Move to next shelf.
            self.shelf_y += self.shelf_height;
            self.cursor_x = 0;
            self.shelf_height = 0;
        }

        if self.shelf_y + ph > self.height {
            return None; // Atlas full.
        }

        let x = self.cursor_x;
        let y = self.shelf_y;
        self.cursor_x += pw;
        if ph > self.shelf_height {
            self.shelf_height = ph;
        }
        Some((x, y))
    }

    fn reset(&mut self) {
        self.cursor_x = 0;
        self.shelf_y = 0;
        self.shelf_height = 0;
    }
}

/// Tracks the rectangular region of the atlas that has been modified.
#[derive(Clone, Copy, Debug)]
struct DirtyRegion {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl DirtyRegion {
    /// Create a dirty region covering a single glyph placement.
    fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// Expand this region to also cover another rectangle.
    fn union(&mut self, x: u32, y: u32, w: u32, h: u32) {
        let x_min = self.x.min(x);
        let y_min = self.y.min(y);
        let x_max = (self.x + self.width).max(x + w);
        let y_max = (self.y + self.height).max(y + h);
        self.x = x_min;
        self.y = y_min;
        self.width = x_max - x_min;
        self.height = y_max - y_min;
    }
}

pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    packer: ShelfPacker,
    cache: HashMap<GlyphKey, AtlasRegion>,
    /// CPU-side pixel data for re-upload on rebuild.
    pixels: Vec<u8>,
    /// Tracks the bounding rectangle of modified pixels since last upload.
    dirty_region: Option<DirtyRegion>,
    /// Incremented each time the texture is recreated (grow).
    generation: u64,
}

impl GlyphAtlas {
    const INITIAL_SIZE: u32 = 512;

    pub fn new(device: &wgpu::Device) -> Self {
        let width = Self::INITIAL_SIZE;
        let height = Self::INITIAL_SIZE;
        let (texture, view) = Self::create_texture(device, width, height);
        Self {
            texture,
            view,
            width,
            height,
            packer: ShelfPacker::new(width, height),
            cache: HashMap::new(),
            pixels: vec![0u8; (width * height) as usize],
            dirty_region: None,
            generation: 0,
        }
    }

    fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    /// Look up a cached glyph region without inserting.
    pub fn get(&self, key: &GlyphKey) -> Option<AtlasRegion> {
        self.cache.get(key).copied()
    }

    /// Returns the current atlas generation (incremented on texture recreate).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Look up or insert a glyph into the atlas. Returns the atlas region.
    /// `bitmap` must be `width * height` bytes (grayscale alpha).
    pub fn get_or_insert(&mut self, device: &wgpu::Device, key: GlyphKey, glyph_w: u32, glyph_h: u32, bitmap: &[u8]) -> AtlasRegion {
        if let Some(region) = self.cache.get(&key) {
            return *region;
        }

        // Try to allocate.
        let pos = self.packer.allocate(glyph_w, glyph_h);
        let (x, y) = match pos {
            Some(p) => p,
            None => {
                // Atlas full — grow and rebuild.
                self.grow(device);
                self.packer
                    .allocate(glyph_w, glyph_h)
                    .expect("glyph too large for atlas even after grow")
            }
        };

        // Copy bitmap into CPU pixel buffer.
        for row in 0..glyph_h {
            let dst_start = ((y + row) * self.width + x) as usize;
            let src_start = (row * glyph_w) as usize;
            self.pixels[dst_start..dst_start + glyph_w as usize].copy_from_slice(&bitmap[src_start..src_start + glyph_w as usize]);
        }

        match &mut self.dirty_region {
            Some(dr) => dr.union(x, y, glyph_w, glyph_h),
            None => self.dirty_region = Some(DirtyRegion::new(x, y, glyph_w, glyph_h)),
        }

        let region = AtlasRegion { x, y, width: glyph_w, height: glyph_h };
        self.cache.insert(key, region);
        region
    }

    /// Upload only the dirty region of pixels to the GPU texture.
    pub fn upload(&mut self, queue: &wgpu::Queue) {
        let dr = match self.dirty_region.take() {
            Some(dr) => dr,
            None => return,
        };

        // Build a contiguous buffer containing only the rows within the dirty region.
        let row_bytes = dr.width as usize;
        let mut region_buf = Vec::with_capacity(row_bytes * dr.height as usize);
        for row in 0..dr.height {
            let start = ((dr.y + row) * self.width + dr.x) as usize;
            region_buf.extend_from_slice(&self.pixels[start..start + row_bytes]);
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: dr.x, y: dr.y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &region_buf,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(dr.width), rows_per_image: Some(dr.height) },
            wgpu::Extent3d { width: dr.width, height: dr.height, depth_or_array_layers: 1 },
        );
    }

    /// Double the atlas size and re-pack all cached glyphs.
    fn grow(&mut self, device: &wgpu::Device) {
        let new_w = self.width * 2;
        let new_h = self.height * 2;
        let (texture, view) = Self::create_texture(device, new_w, new_h);

        // Allocate new pixel buffer and reset packer — we don't re-pack here,
        // we just expand. Old data is preserved by re-uploading.
        let mut new_pixels = vec![0u8; (new_w * new_h) as usize];
        // Copy old rows into new buffer.
        for row in 0..self.height {
            let src_start = (row * self.width) as usize;
            let dst_start = (row * new_w) as usize;
            new_pixels[dst_start..dst_start + self.width as usize]
                .copy_from_slice(&self.pixels[src_start..src_start + self.width as usize]);
        }

        self.texture = texture;
        self.view = view;
        self.pixels = new_pixels;
        self.packer = ShelfPacker::new(new_w, new_h);

        // Re-insert existing regions into the new packer (they keep their positions).
        // Since we only expanded, old positions are still valid. Just advance the packer
        // past the old content by replaying allocations.
        // Simpler: reset packer and re-allocate all cached entries in order.
        let old_cache: Vec<(GlyphKey, AtlasRegion)> = self.cache.drain().collect();
        for (key, region) in &old_cache {
            let _ = self.packer.allocate(region.width, region.height);
            self.cache.insert(*key, *region);
        }

        self.width = new_w;
        self.height = new_h;
        self.dirty_region = Some(DirtyRegion::new(0, 0, new_w, new_h));
        self.generation += 1;
    }
}

// ---------------------------------------------------------------------------
// Color glyph atlas (RGBA8, for sbix PNG strikes)
// ---------------------------------------------------------------------------

/// Sibling to [`GlyphAtlas`] that stores RGBA8 color glyphs (Apple Color
/// Emoji, etc.). The shape and behavior are intentionally near-identical: a
/// shelf packer, lazy re-upload of a dirty rectangle, and 2× growth on
/// overflow. Only the per-pixel size and texture format differ.
pub struct ColorGlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    packer: ShelfPacker,
    cache: HashMap<GlyphKey, AtlasRegion>,
    /// CPU-side pixel data for re-upload on grow. RGBA8 → 4 bytes per pixel.
    pixels: Vec<u8>,
    dirty_region: Option<DirtyRegion>,
    generation: u64,
}

impl ColorGlyphAtlas {
    const INITIAL_SIZE: u32 = 512;
    const BYTES_PER_PIXEL: u32 = 4;

    pub fn new(device: &wgpu::Device) -> Self {
        let width = Self::INITIAL_SIZE;
        let height = Self::INITIAL_SIZE;
        let (texture, view) = Self::create_texture(device, width, height);
        Self {
            texture,
            view,
            width,
            height,
            packer: ShelfPacker::new(width, height),
            cache: HashMap::new(),
            pixels: vec![0u8; (width * height * Self::BYTES_PER_PIXEL) as usize],
            dirty_region: None,
            generation: 0,
        }
    }

    fn create_texture(device: &wgpu::Device, width: u32, height: u32) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("color glyph atlas"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        (texture, view)
    }

    pub fn get(&self, key: &GlyphKey) -> Option<AtlasRegion> {
        self.cache.get(key).copied()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// `bitmap` must be `width * height * 4` bytes (non-premultiplied RGBA8).
    pub fn get_or_insert(
        &mut self,
        device: &wgpu::Device,
        key: GlyphKey,
        glyph_w: u32,
        glyph_h: u32,
        bitmap: &[u8],
    ) -> AtlasRegion {
        if let Some(region) = self.cache.get(&key) {
            return *region;
        }

        let pos = self.packer.allocate(glyph_w, glyph_h);
        let (x, y) = match pos {
            Some(p) => p,
            None => {
                self.grow(device);
                self.packer
                    .allocate(glyph_w, glyph_h)
                    .expect("color glyph too large for atlas even after grow")
            }
        };

        // Copy RGBA8 rows from `bitmap` into the CPU pixel buffer.
        let bpp = Self::BYTES_PER_PIXEL as usize;
        let row_bytes_glyph = (glyph_w as usize) * bpp;
        let row_bytes_atlas = (self.width as usize) * bpp;
        for row in 0..glyph_h {
            let dst_start = (y as usize + row as usize) * row_bytes_atlas + (x as usize) * bpp;
            let src_start = (row as usize) * row_bytes_glyph;
            self.pixels[dst_start..dst_start + row_bytes_glyph]
                .copy_from_slice(&bitmap[src_start..src_start + row_bytes_glyph]);
        }

        match &mut self.dirty_region {
            Some(dr) => dr.union(x, y, glyph_w, glyph_h),
            None => self.dirty_region = Some(DirtyRegion::new(x, y, glyph_w, glyph_h)),
        }

        let region = AtlasRegion { x, y, width: glyph_w, height: glyph_h };
        self.cache.insert(key, region);
        region
    }

    pub fn upload(&mut self, queue: &wgpu::Queue) {
        let dr = match self.dirty_region.take() {
            Some(dr) => dr,
            None => return,
        };

        let bpp = Self::BYTES_PER_PIXEL as usize;
        let row_bytes_atlas = (self.width as usize) * bpp;
        let row_bytes_region = (dr.width as usize) * bpp;
        let mut region_buf = Vec::with_capacity(row_bytes_region * dr.height as usize);
        for row in 0..dr.height {
            let start = (dr.y as usize + row as usize) * row_bytes_atlas + (dr.x as usize) * bpp;
            region_buf.extend_from_slice(&self.pixels[start..start + row_bytes_region]);
        }

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: dr.x, y: dr.y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &region_buf,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(dr.width * Self::BYTES_PER_PIXEL),
                rows_per_image: Some(dr.height),
            },
            wgpu::Extent3d { width: dr.width, height: dr.height, depth_or_array_layers: 1 },
        );
    }

    fn grow(&mut self, device: &wgpu::Device) {
        let new_w = self.width * 2;
        let new_h = self.height * 2;
        let (texture, view) = Self::create_texture(device, new_w, new_h);

        let bpp = Self::BYTES_PER_PIXEL as usize;
        let mut new_pixels = vec![0u8; (new_w * new_h) as usize * bpp];
        let old_row = (self.width as usize) * bpp;
        let new_row = (new_w as usize) * bpp;
        for row in 0..self.height as usize {
            let src_start = row * old_row;
            let dst_start = row * new_row;
            new_pixels[dst_start..dst_start + old_row]
                .copy_from_slice(&self.pixels[src_start..src_start + old_row]);
        }

        self.texture = texture;
        self.view = view;
        self.pixels = new_pixels;
        self.packer = ShelfPacker::new(new_w, new_h);

        let old_cache: Vec<(GlyphKey, AtlasRegion)> = self.cache.drain().collect();
        for (key, region) in &old_cache {
            let _ = self.packer.allocate(region.width, region.height);
            self.cache.insert(*key, *region);
        }

        self.width = new_w;
        self.height = new_h;
        self.dirty_region = Some(DirtyRegion::new(0, 0, new_w, new_h));
        self.generation += 1;
    }
}
