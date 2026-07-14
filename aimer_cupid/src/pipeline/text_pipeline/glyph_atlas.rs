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
        [
            self.x as f32 / aw,
            self.y as f32 / ah,
            (self.x + self.width) as f32 / aw,
            (self.y + self.height) as f32 / ah,
        ]
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

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.cursor_x = 0;
        self.shelf_y = 0;
        self.shelf_height = 0;
    }

    /// Position the packer so the next allocation starts a brand-new, empty shelf
    /// whose top edge is at `y`. Used after the atlas grows: existing glyphs keep
    /// their positions in the (now larger) atlas, and the packer resumes in the
    /// free space directly below them so newly inserted glyphs can never collide
    /// with the preserved content.
    fn start_fresh_shelf_at(&mut self, y: u32) {
        self.cursor_x = 0;
        self.shelf_y = y;
        self.shelf_height = 0;
    }
}

/// A glyph that has been packed into the atlas this frame but whose pixels have
/// not yet been written to the GPU texture. Holds only the bytes of the single
/// glyph (dropped right after [`GlyphAtlas::upload`]), so the atlas never keeps
/// a full-size CPU mirror of the texture in RAM.
struct PendingGlyph {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    /// Glyph bitmap rows, tightly packed (`width * height` for R8, ×4 for RGBA8).
    data: Vec<u8>,
}

pub struct GlyphAtlas {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    packer: ShelfPacker,
    cache: HashMap<GlyphKey, AtlasRegion>,
    /// Glyphs packed but not yet uploaded to the GPU texture. Each entry owns
    /// only its own bitmap, which is dropped after [`upload`](Self::upload), so
    /// no full-size CPU copy of the atlas is retained.
    pending: Vec<PendingGlyph>,
    /// Incremented each time the texture is recreated (grow).
    generation: u64,
}

impl GlyphAtlas {
    const INITIAL_SIZE: u32 = 512;
    /// Hard cap on atlas dimensions. Instead of doubling without bound (which
    /// could reach 4096² = 16 MB of GPU memory), once the atlas reaches this
    /// size a full overflow evicts every cached glyph and repacks from scratch
    /// rather than growing further.
    const MAX_SIZE: u32 = 2048;

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
            pending: Vec::new(),
            generation: 0,
        }
    }

    fn create_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            // COPY_SRC lets `grow` preserve existing glyphs with a GPU
            // texture-to-texture copy instead of re-uploading from a CPU mirror.
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
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
    pub fn get_or_insert(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: GlyphKey,
        glyph_w: u32,
        glyph_h: u32,
        bitmap: &[u8],
    ) -> AtlasRegion {
        if let Some(region) = self.cache.get(&key) {
            return *region;
        }

        // Try to allocate.
        let pos = self.packer.allocate(glyph_w, glyph_h);
        let (x, y) = match pos {
            Some(p) => p,
            None => {
                // Atlas full — grow (or evict at the size cap) and retry.
                self.grow(device, queue);
                self.packer
                    .allocate(glyph_w, glyph_h)
                    .expect("glyph too large for atlas even after grow")
            }
        };

        // Stage the glyph bitmap for the next `upload`. We keep only this glyph's
        // bytes (dropped after upload) rather than a full-size CPU mirror.
        self.pending.push(PendingGlyph {
            x,
            y,
            width: glyph_w,
            height: glyph_h,
            data: bitmap.to_vec(),
        });

        let region = AtlasRegion { x, y, width: glyph_w, height: glyph_h };
        self.cache.insert(key, region);
        region
    }

    /// Write every glyph staged since the last upload to the GPU texture, then
    /// drop the staged bytes. Each glyph is written directly at its packed
    /// position, so no full-size CPU buffer is materialized.
    pub fn upload(&mut self, queue: &wgpu::Queue) {
        if self.pending.is_empty() {
            return;
        }
        for glyph in self.pending.drain(..) {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: glyph.x, y: glyph.y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &glyph.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(glyph.width),
                    rows_per_image: Some(glyph.height),
                },
                wgpu::Extent3d {
                    width: glyph.width,
                    height: glyph.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Grow the atlas to fit more glyphs. Below [`MAX_SIZE`](Self::MAX_SIZE) the
    /// atlas doubles and the existing texture content is preserved with a GPU
    /// texture-to-texture copy (no CPU mirror needed). At the cap we instead
    /// evict everything and repack from scratch, so memory never grows past
    /// `MAX_SIZE²`.
    fn grow(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        // At the size cap: evict all cached glyphs and repack into the existing
        // texture instead of allocating a larger one. Stale pixels left in the
        // texture are simply never referenced again (the cache is cleared, so
        // every glyph is re-inserted and re-uploaded on demand).
        if self.width >= Self::MAX_SIZE {
            self.cache.clear();
            self.packer = ShelfPacker::new(self.width, self.height);
            return;
        }

        let old_w = self.width;
        let old_h = self.height;
        let new_w = self.width * 2;
        let new_h = self.height * 2;
        let (texture, view) = Self::create_texture(device, new_w, new_h);

        // Preserve every already-uploaded glyph by copying the old texture into
        // the top-left of the new one on the GPU. Existing glyphs keep their
        // exact (x, y) positions, so their cached `AtlasRegion`s — and any UVs
        // captured for them earlier this frame — stay valid once re-resolved
        // against the final dimensions. Glyphs staged this frame but not yet
        // uploaded remain in `self.pending` with their (still valid) positions
        // and are written to the new texture by the next `upload`.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("glyph atlas grow"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d { width: old_w, height: old_h, depth_or_array_layers: 1 },
        );
        queue.submit(Some(encoder.finish()));

        self.texture = texture;
        self.view = view;

        // Resume packing on a fresh shelf directly below the preserved content.
        //
        // We deliberately do NOT reset the packer and replay the old allocations:
        // the atlas width has doubled, so the shelf packer would wrap rows
        // differently than the preserved layout and could hand out positions that
        // overlap existing glyphs. New glyphs would then be written on top of old
        // ones, producing the overlapping/garbled text seen after resizing the
        // window down and back up (which reflows text and inserts many glyphs at
        // once, triggering a grow). Starting the next shelf at the old height keeps
        // all cached positions valid while guaranteeing new glyphs land in free
        // space.
        self.packer = ShelfPacker::new(new_w, new_h);
        self.packer.start_fresh_shelf_at(old_h);

        self.width = new_w;
        self.height = new_h;
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
    /// Glyphs packed but not yet uploaded. Each entry owns only its own RGBA8
    /// bytes (dropped after [`upload`](Self::upload)); no full-size CPU mirror.
    pending: Vec<PendingGlyph>,
    generation: u64,
}

impl ColorGlyphAtlas {
    const INITIAL_SIZE: u32 = 512;
    const BYTES_PER_PIXEL: u32 = 4;
    /// Hard cap on atlas dimensions (see [`GlyphAtlas::MAX_SIZE`]). Caps the
    /// RGBA8 color atlas at `MAX_SIZE² * 4` bytes of GPU memory.
    const MAX_SIZE: u32 = 2048;

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
            pending: Vec::new(),
            generation: 0,
        }
    }

    fn create_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("color glyph atlas"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            // COPY_SRC enables GPU texture-to-texture preservation on grow.
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
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
        queue: &wgpu::Queue,
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
                self.grow(device, queue);
                self.packer
                    .allocate(glyph_w, glyph_h)
                    .expect("color glyph too large for atlas even after grow")
            }
        };

        // Stage this glyph's RGBA8 bytes for the next `upload`; no full-size mirror.
        self.pending.push(PendingGlyph {
            x,
            y,
            width: glyph_w,
            height: glyph_h,
            data: bitmap.to_vec(),
        });

        let region = AtlasRegion { x, y, width: glyph_w, height: glyph_h };
        self.cache.insert(key, region);
        region
    }

    pub fn upload(&mut self, queue: &wgpu::Queue) {
        if self.pending.is_empty() {
            return;
        }
        for glyph in self.pending.drain(..) {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: glyph.x, y: glyph.y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &glyph.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(glyph.width * Self::BYTES_PER_PIXEL),
                    rows_per_image: Some(glyph.height),
                },
                wgpu::Extent3d {
                    width: glyph.width,
                    height: glyph.height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    fn grow(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        // At the size cap: evict and repack into the existing texture rather than
        // allocating a larger one (see `GlyphAtlas::grow`).
        if self.width >= Self::MAX_SIZE {
            self.cache.clear();
            self.packer = ShelfPacker::new(self.width, self.height);
            return;
        }

        let old_w = self.width;
        let old_h = self.height;
        let new_w = self.width * 2;
        let new_h = self.height * 2;
        let (texture, view) = Self::create_texture(device, new_w, new_h);

        // Preserve already-uploaded glyphs with a GPU texture-to-texture copy
        // (no CPU mirror). Glyphs staged this frame stay in `self.pending` with
        // their still-valid positions and are written by the next `upload`.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("color glyph atlas grow"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d { width: old_w, height: old_h, depth_or_array_layers: 1 },
        );
        queue.submit(Some(encoder.finish()));

        self.texture = texture;
        self.view = view;

        // Existing glyphs keep their positions in the enlarged atlas; resume packing
        // on a fresh shelf below them. Replaying the old allocations would be wrong
        // because the atlas width doubled, so the packer would wrap differently and
        // could place new glyphs over existing ones (overlapping/garbled text after
        // a resize-triggered reflow). See `GlyphAtlas::grow` for the full rationale.
        self.packer = ShelfPacker::new(new_w, new_h);
        self.packer.start_fresh_shelf_at(old_h);

        self.width = new_w;
        self.height = new_h;
        self.generation += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: &AtlasRegion, b: &AtlasRegion) -> bool {
        let ax2 = a.x + a.width;
        let ay2 = a.y + a.height;
        let bx2 = b.x + b.width;
        let by2 = b.y + b.height;
        a.x < bx2 && b.x < ax2 && a.y < by2 && b.y < ay2
    }

    /// A representative spread of glyph sizes that produces several shelves on a
    /// 512-wide atlas, mirroring what happens when many glyphs are rasterized.
    fn sample_glyphs() -> Vec<(u32, u32)> {
        let mut v = Vec::new();
        for i in 0..120u32 {
            let w = 8 + (i * 7) % 40;
            let h = 10 + (i * 5) % 30;
            v.push((w, h));
        }
        v
    }

    /// Pack a representative set of glyphs into a `size`×`size` packer and return
    /// the resulting regions (in allocation order, which is what `grow()`
    /// preserves) plus the live packer.
    fn pack_initial(size: u32) -> (ShelfPacker, Vec<AtlasRegion>) {
        let mut packer = ShelfPacker::new(size, size);
        let mut regions: Vec<AtlasRegion> = Vec::new();
        for &(w, h) in &sample_glyphs() {
            if let Some((x, y)) = packer.allocate(w, h) {
                regions.push(AtlasRegion { x, y, width: w, height: h });
            }
        }
        (packer, regions)
    }

    /// The previous `grow()` strategy: reset the packer to the *doubled* size and
    /// replay the old allocations. Because the width changed, the packer wraps
    /// rows differently than the preserved layout, so this is unsafe.
    fn old_grow_next_allocation(
        regions: &[AtlasRegion],
        new_w: u32,
        new_h: u32,
        next: (u32, u32),
    ) -> (u32, u32) {
        let mut packer = ShelfPacker::new(new_w, new_h);
        let mut sorted = regions.to_vec();
        sorted.sort_by_key(|r| (r.y, r.x));
        for r in &sorted {
            let _ = packer.allocate(r.width, r.height);
        }
        packer.allocate(next.0, next.1).unwrap()
    }

    /// The new `grow()` strategy: keep the packer at the doubled size but resume on
    /// a fresh shelf directly below the preserved content (`start_fresh_shelf_at`).
    fn new_grow_next_allocation(
        old_h: u32,
        new_w: u32,
        new_h: u32,
        next: (u32, u32),
    ) -> (u32, u32) {
        let mut packer = ShelfPacker::new(new_w, new_h);
        packer.start_fresh_shelf_at(old_h);
        packer.allocate(next.0, next.1).unwrap()
    }

    #[test]
    fn replaying_allocations_after_a_width_changing_grow_overlaps_existing_glyphs() {
        // Regression guard: prove the old approach is genuinely broken so the
        // positive test below is not vacuous. Replaying allocations into a
        // doubled-width packer hands out a position that collides with preserved
        // glyphs.
        let (_packer, regions) = pack_initial(512);
        let next = (40, 30);
        let pos = old_grow_next_allocation(&regions, 1024, 1024, next);
        let new_region = AtlasRegion { x: pos.0, y: pos.1, width: next.0, height: next.1 };
        let overlap = regions.iter().any(|r| overlaps(r, &new_region));
        assert!(
            overlap,
            "expected replay-after-grow to overlap existing glyphs; got {:?}",
            new_region
        );
    }

    #[test]
    fn fresh_shelf_after_grow_never_overlaps_existing_glyphs() {
        // The fix: after growing, all preserved glyphs live within y < old_height,
        // and the packer resumes at y == old_height, so any newly allocated glyph
        // is strictly below the preserved content and cannot overlap it.
        let (_packer, regions) = pack_initial(512);
        let old_h = 512;
        for &next in &[(40u32, 30u32), (1u32, 1u32), (300u32, 200u32)] {
            let pos = new_grow_next_allocation(old_h, 1024, 1024, next);
            let new_region = AtlasRegion { x: pos.0, y: pos.1, width: next.0, height: next.1 };
            assert!(
                new_region.y >= old_h,
                "new glyph must start below preserved content: {:?}",
                new_region
            );
            for r in &regions {
                assert!(
                    !overlaps(r, &new_region),
                    "new glyph overlapped a preserved glyph: {:?} vs {:?}",
                    r,
                    new_region
                );
            }
        }
    }
}
