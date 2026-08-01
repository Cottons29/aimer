use std::hash::{Hash, Hasher};
#[allow(unused)]
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use aimer_utils::time_cost;
use hashbrown::{HashMap, HashSet};
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};
use swash::FontRef;
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;

use super::text_layout::FontId;
use crate::font::{FontFamily, FontRegistry, FontStyle, FontWeight, bundled_monospace_bytes};
use crate::text_pipeline::font_resolver::{
    FontData, FontRecord, SharedFontRecord, advance_width_from_face, font_ref,
    shared_fallback_chain,
};
use crate::text_pipeline::glyph_metrics::{self, GlyphMetrics};
use crate::text_pipeline::glyph_outline::rasterize_outline_glyph;
use crate::text_pipeline::system_fallback::{
    SYSTEM_FALLBACK_ID_BASE, fallback_by_id, fallback_for_codepoint,
};

/// Embedded primary font (Roboto) — covers Latin and common scripts.
const PRIMARY_FONT: &[u8] = include_bytes!("../../../fonts/GoogleSans-Regular.ttf");
const MONOSPACE_FONT_ID: FontId = 0x7fff_fffe;
// const JAPANESE_FONT: &[u8] =
// include_bytes!("../../../fonts/NotoSansJP-VariableFont_wght.ttf");
/// A rasterized glyph bitmap with its metrics.
///
/// `bitmap` layout depends on `is_color`:
///   * `is_color == false` — `width * height` bytes, single-channel coverage
///     (8-bit alpha), as produced by Swash.
///   * `is_color == true`  — `width * height * 4` bytes, RGBA8
///     (non-premultiplied), as produced from `sbix` PNG strikes
///     (AppleColorEmoji, etc.).
///
/// The text pipeline routes color glyphs to a separate RGBA8 atlas + shader.
#[derive(Clone)]
pub struct RasterizedGlyph {
    pub bitmap: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Horizontal offset from the pen position to the left edge of the bitmap.
    pub offset_x: f32,
    /// Vertical offset from the baseline to the bottom edge of the bitmap
    /// (y-up, matches the font's scaled glyph bounding-box minimum y.
    pub offset_y: f32,
    /// Horizontal advance width.
    pub advance_width: f32,
    /// Whether the bitmap is RGBA8 color data (true) or single-channel alpha
    /// (false).
    pub is_color: bool,
}

/// Key for caching rasterized-shaped glyphs.
#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct GlyphKey {
    pub font_id: FontId,
    pub glyph_id: u16,
    pub size_tenths: u32,
    pub subpixel_x: u8,
    pub subpixel_y: u8,
}

impl Hash for GlyphKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let font_and_size = u64::from(self.font_id) | (u64::from(self.size_tenths) << 32);
        let glyph_and_subpixel = u64::from(self.glyph_id)
            | (u64::from(self.subpixel_x) << 16)
            | (u64::from(self.subpixel_y) << 24);
        let compact = font_and_size
            ^ glyph_and_subpixel
                .wrapping_mul(0x9e37_79b1_85eb_ca87)
                .rotate_left(17);
        state.write_u64(compact);
    }
}

pub struct ShapedRunGlyph {
    pub glyph_key: GlyphKey,
    pub advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,
    pub cluster: usize,
}

impl GlyphKey {
    pub fn new(font_id: FontId, glyph_id: u16, font_size: f32) -> Self {
        Self {
            font_id,
            glyph_id,
            size_tenths: (font_size * 10.0) as u32,
            subpixel_x: 0,
            subpixel_y: 0,
        }
    }
}

fn rasterize_swash_glyph(
    record: &FontRecord,
    glyph_id: u16,
    font_size: f32,
) -> Option<RasterizedGlyph> {
    let data = time_cost!("   |-MapSwashFontData", || record.data())?;
    let data = data.as_ref();
    let font = FontRef::from_index(data, record.collection_index as usize)?;
    let mut context = ScaleContext::new();
    let mut scaler = context.builder(font).size(font_size).hint(true).build();
    let image = Render::new(&[Source::Outline])
        .format(Format::Alpha)
        .render(&mut scaler, glyph_id)?;
    let advance_width =
        advance_width_from_face(data, record.collection_index, glyph_id, font_size)?;

    Some(RasterizedGlyph {
        bitmap: image.data,
        width: image.placement.width,
        height: image.placement.height,
        offset_x: image.placement.left as f32,
        offset_y: (image.placement.top - image.placement.height as i32) as f32,
        advance_width,
        is_color: false,
    })
}

fn primary_font_record() -> FontRecord {
    static PRIMARY_FONT_RECORD: OnceLock<FontRecord> = OnceLock::new();
    PRIMARY_FONT_RECORD
        .get_or_init(|| {
            FontRecord::from_static_bytes(0, PRIMARY_FONT).expect("failed to load primary font")
        })
        .clone()
}

fn monospace_font_record() -> FontRecord {
    static MONOSPACE_FONT_RECORD: OnceLock<FontRecord> = OnceLock::new();
    MONOSPACE_FONT_RECORD
        .get_or_init(|| {
            FontRecord::from_static_bytes(MONOSPACE_FONT_ID, bundled_monospace_bytes())
                .expect("failed to load bundled monospace font")
        })
        .clone()
}

// ---------------------------------------------------------------------------
// Platform-specific system font loading
// ---------------------------------------------------------------------------

/// Try to load system font bytes for a given family name.
/// Returns `None` if the font cannot be found or loading fails.
///
/// On Linux and Windows, Fontique resolves and loads the requested family.
#[cfg(not(any(
    target_arch = "wasm32",
    target_os = "ios",
    target_os = "macos",
    target_os = "android"
)))]
fn load_system_font(family: &str) -> Option<Vec<u8>> {
    let mut collection = fontique::Collection::new(fontique::CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    let family = collection.family_by_name(family)?;
    let font = family.fonts().next()?;
    Some(font.load(None)?.data().to_vec())
}

/// macOS / iOS: resolve a system font by family name through Core Text.
///
/// This avoids enumerating every installed face — the approach taken by
/// generic font databases — which causes high RAM usage and slow startup on
/// Apple platforms. See [`apple_fonts`] for the underlying lookup.
///
/// [`apple_fonts`]: crate::text_pipeline::apple_fonts
#[allow(dead_code)]
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub(crate) fn load_system_font_path(family: &str) -> Option<PathBuf> {

    crate::text_pipeline::apple_fonts::system_font_path(family)
}

// ---------------------------------------------------------------------------
// Color glyph rasterization (embedded bitmaps and layered outlines)
// ---------------------------------------------------------------------------
/// Rasterizes a color glyph through Swash, which uses Skrifa for font scaling.
///
/// Embedded color bitmaps are preferred to preserve emoji artwork. Layered
/// color outlines are used when no bitmap representation is available.
fn rasterize_color_glyph(
    record: &FontRecord,
    glyph_id: u16,
    font_size: f32,
) -> Option<RasterizedGlyph> {
    let data = record.data()?;
    let font = FontRef::from_index(data.as_ref(), record.collection_index as usize)?;
    let mut context = ScaleContext::new();
    let mut scaler = context.builder(font).size(font_size).hint(true).build();
    let image = Render::new(&[
        Source::ColorBitmap(StrikeWith::BestFit),
        Source::ColorOutline(0),
    ])
    .render(&mut scaler, glyph_id)?;
    if image.content != Content::Color {
        return None;
    }
    let advance_width =
        advance_width_from_face(data.as_ref(), record.collection_index, glyph_id, font_size)?;

    Some(RasterizedGlyph {
        bitmap: image.data,
        width: image.placement.width,
        height: image.placement.height,
        offset_x: image.placement.left as f32,
        offset_y: (image.placement.top - image.placement.height as i32) as f32,
        advance_width,
        is_color: true,
    })
}

#[inline]
pub fn point_inside(contours: &[Vec<(f32, f32)>], x: f32, y: f32) -> bool {
    let mut inside = false;
    for contour in contours {
        let mut prev = *contour.last().expect("contour is non-empty");
        for &curr in contour {
            if (curr.1 > y) != (prev.1 > y)
                && x < (prev.0 - curr.0) * (y - curr.1) / (prev.1 - curr.1) + curr.0
            {
                inside = !inside;
            }
            prev = curr;
        }
    }
    inside
}

/// Immutable font ownership copied into worker-local preparation contexts.
///
/// Font bytes and parsed font objects remain reference counted, while every
/// context created from this snapshot receives independent shaping and bitmap
/// caches. Fallback discovery remains lazy when the source rasterizer has not
/// loaded its fallback chain yet.
#[derive(Clone)]
pub(super) struct FontSnapshot {
    primary: SharedFontRecord,
    family_faces: Arc<[SharedFamilyFontRecord]>,
    fallbacks: Option<Arc<[SharedFontRecord]>>,
    enable_fallbacks: bool,
}

#[derive(Clone)]
struct SharedFamilyFontRecord {
    family: FontFamily,
    weight: u16,
    style: FontStyle,
    record: SharedFontRecord,
}

/// Mutable CPU-only text preparation state owned by one worker job.
///
/// This context contains no atlas, GPU object, canvas state, or renderer cache.
/// Its rasterizer and scratch buffers are never shared between workers.
pub(super) struct GlyphPreparationContext {
    rasterizer: GlyphRasterizer,
}

impl GlyphPreparationContext {
    pub(super) fn new(snapshot: FontSnapshot) -> Self {
        Self {
            rasterizer: GlyphRasterizer::from_font_snapshot(snapshot),
        }
    }

    pub(super) fn rasterizer_mut(&mut self) -> &mut GlyphRasterizer {
        &mut self.rasterizer
    }

    pub(super) fn prepare_glyph(&mut self, key: GlyphKey, font_size: f32) -> RasterizedGlyph {
        self.rasterizer.rasterize_bitmap_key(key, font_size).clone()
    }
}

pub struct GlyphRasterizer {
    /// Primary font (Roboto) for Latin/common glyphs.
    primary: FontRecord,
    family_faces: Vec<FamilyFontRecord>,
    /// Fallback fonts for extended Unicode coverage (CJK, etc.).
    /// Loaded lazily on first encounter of a glyph not in the primary font,
    /// to avoid the massive memory cost (~800MB) of parsing large CJK fonts
    /// when only ASCII text is rendered.
    fallbacks: Option<Vec<FontRecord>>,
    /// Whether to attempt loading fallbacks when needed.
    enable_fallbacks: bool,
    cache: HashMap<GlyphKey, RasterizedGlyph>,
    retained_bitmap_bytes: usize,
    advance_cache: HashMap<GlyphKey, f32>,
    glyph_index_cache: HashMap<(FontId, char), Option<u16>>,
    unsupported_codepoints: HashSet<char>,
    /// Cached font bytes per font_id to avoid re-reading from disk or
    /// re-cloning Arc<[u8]> on every `shape_cluster` call.
    font_bytes_cache: HashMap<FontId, FontData>,
    /// Cached HarfRust shaping metadata per font id.
    shaper_data_cache: HashMap<FontId, harfrust::ShaperData>,
    /// Reusable `UnicodeBuffer` for HarfRust — reset between calls instead
    /// of allocating a new buffer per cluster.
    shape_buffer: Option<harfrust::UnicodeBuffer>,
    #[cfg(test)]
    shape_call_count: usize,
    #[cfg(test)]
    rasterize_call_count: usize,
}

#[derive(Clone)]
struct FamilyFontRecord {
    family: FontFamily,
    weight: u16,
    style: FontStyle,
    record: FontRecord,
}

fn registered_family_faces() -> Vec<FamilyFontRecord> {
    let mut faces = vec![FamilyFontRecord {
        family: FontFamily::MONOSPACE,
        weight: FontWeight::Normal.numeric(),
        style: FontStyle::Normal,
        record: monospace_font_record(),
    }];
    faces.extend(FontRegistry::faces().into_iter().filter_map(|face| {
        Some(FamilyFontRecord {
            family: face.family,
            weight: face.weight,
            style: face.style,
            record: FontRecord::from_shared_bytes(face.face_id, face.bytes)?,
        })
    }));
    faces
}

fn family_style_distance(requested: FontStyle, candidate: FontStyle) -> u8 {
    if requested == candidate {
        0
    } else if candidate == FontStyle::Normal {
        1
    } else if matches!(requested, FontStyle::Oblique | FontStyle::ObliqueDeg(_))
        && matches!(candidate, FontStyle::Oblique | FontStyle::ObliqueDeg(_))
    {
        2
    } else {
        3
    }
}

impl GlyphRasterizer {
    const BITMAP_CACHE_CAPACITY_BYTES: usize = 8 * 1024 * 1024;
    const GLYPH_INDEX_CACHE_CAPACITY: usize = 16 * 1024;

    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let primary = primary_font_record();

        Self {
            primary,
            family_faces: registered_family_faces(),
            fallbacks: None, // loaded lazily on first miss
            enable_fallbacks: true,
            cache: HashMap::default(),
            retained_bitmap_bytes: 0,
            advance_cache: HashMap::default(),
            glyph_index_cache: HashMap::default(),
            unsupported_codepoints: HashSet::default(),
            font_bytes_cache: HashMap::default(),
            shaper_data_cache: HashMap::default(),
            shape_buffer: Some(harfrust::UnicodeBuffer::new()),
            #[cfg(test)]
            shape_call_count: 0,
            #[cfg(test)]
            rasterize_call_count: 0,
        }
    }

    /// Create a lightweight rasterizer with only the primary font (no
    /// fallbacks). Suitable for text measurement where CJK rendering is not
    /// needed.
    pub fn primary_only() -> Self {
        let primary = primary_font_record();
        Self {
            primary,
            family_faces: registered_family_faces(),
            fallbacks: None,
            enable_fallbacks: false,
            cache: HashMap::default(),
            retained_bitmap_bytes: 0,
            advance_cache: HashMap::default(),
            glyph_index_cache: HashMap::default(),
            unsupported_codepoints: HashSet::default(),
            font_bytes_cache: HashMap::default(),
            shaper_data_cache: HashMap::default(),
            shape_buffer: Some(harfrust::UnicodeBuffer::new()),
            #[cfg(test)]
            shape_call_count: 0,
            #[cfg(test)]
            rasterize_call_count: 0,
        }
    }

    /// Captures immutable font ownership for worker-local CPU preparation.
    pub(super) fn font_snapshot(&self) -> FontSnapshot {
        FontSnapshot {
            primary: SharedFontRecord::new(&self.primary),
            family_faces: Arc::from(
                self.family_faces
                    .iter()
                    .map(|face| SharedFamilyFontRecord {
                        family: face.family,
                        weight: face.weight,
                        style: face.style,
                        record: SharedFontRecord::new(&face.record),
                    })
                    .collect::<Vec<_>>(),
            ),
            fallbacks: self.fallbacks.as_ref().map(|fallbacks| {
                Arc::from(
                    fallbacks
                        .iter()
                        .map(SharedFontRecord::new)
                        .collect::<Vec<_>>(),
                )
            }),
            enable_fallbacks: self.enable_fallbacks,
        }
    }

    fn from_font_snapshot(snapshot: FontSnapshot) -> Self {
        Self {
            primary: snapshot.primary.local_copy(),
            family_faces: snapshot
                .family_faces
                .iter()
                .map(|face| FamilyFontRecord {
                    family: face.family,
                    weight: face.weight,
                    style: face.style,
                    record: face.record.local_copy(),
                })
                .collect(),
            fallbacks: snapshot
                .fallbacks
                .map(|fallbacks| fallbacks.iter().map(SharedFontRecord::local_copy).collect()),
            enable_fallbacks: snapshot.enable_fallbacks,
            cache: HashMap::default(),
            retained_bitmap_bytes: 0,
            advance_cache: HashMap::default(),
            glyph_index_cache: HashMap::default(),
            unsupported_codepoints: HashSet::default(),
            font_bytes_cache: HashMap::default(),
            shaper_data_cache: HashMap::default(),
            shape_buffer: Some(harfrust::UnicodeBuffer::new()),
            #[cfg(test)]
            shape_call_count: 0,
            #[cfg(test)]
            rasterize_call_count: 0,
        }
    }

    #[cfg(test)]
    pub fn reset_shape_call_count(&mut self) {
        self.shape_call_count = 0;
    }

    #[cfg(test)]
    pub fn shape_call_count(&self) -> usize {
        self.shape_call_count
    }

    #[cfg(test)]
    pub fn reset_rasterize_call_count(&mut self) {
        self.rasterize_call_count = 0;
    }

    #[cfg(test)]
    pub fn rasterize_call_count(&self) -> usize {
        self.rasterize_call_count
    }

    /// Ensure fallback fonts are loaded. Called lazily on first glyph miss.
    fn ensure_fallbacks(&mut self) {
        if self.fallbacks.is_some() || !self.enable_fallbacks {
            return;
        }
        self.fallbacks = Some(shared_fallback_chain());
    }

    pub fn primary_font_id(&self) -> FontId {
        self.primary.id
    }

    pub fn font_id_for_family(
        &mut self,
        family: FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> FontId {
        self.family_record(family, weight, style)
            .map_or(self.primary.id, |record| record.id)
    }

    pub fn glyph_key_for_family_codepoint(
        &mut self,
        codepoint: char,
        font_size: f32,
        family: FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> GlyphKey {
        if let Some(font_id) = self
            .family_record(family, weight, style)
            .map(|record| record.id)
            && let Some(glyph_id) = self.glyph_index_for_font(font_id, codepoint)
        {
            return GlyphKey::new(font_id, glyph_id, font_size);
        }

        self.glyph_key_for_codepoint(codepoint, font_size)
    }

    pub fn advance_width_for_family(
        &mut self,
        codepoint: char,
        font_size: f32,
        family: FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> f32 {
        let key = self.glyph_key_for_family_codepoint(codepoint, font_size, family, weight, style);
        if let Some(width) = self.advance_cache.get(&key) {
            return *width;
        }
        let width = self
            .select_font_for_key(key)
            .advance_width_for_glyph(key.glyph_id, font_size)
            .unwrap_or(0.0);
        self.advance_cache.insert(key, width);
        width
    }

    pub fn measure_text_for_family(
        &mut self,
        text: &str,
        font_size: f32,
        family: FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> f32 {
        text.chars()
            .map(|codepoint| {
                self.advance_width_for_family(codepoint, font_size, family, weight, style)
            })
            .sum()
    }

    fn family_record(
        &self,
        family: FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<&FontRecord> {
        if family == FontFamily::SANS_SERIF {
            return Some(&self.primary);
        }
        let numeric_weight = weight.numeric();
        self.family_faces
            .iter()
            .filter(|face| face.family == family)
            .min_by_key(|face| {
                (
                    family_style_distance(style, face.style),
                    face.weight.abs_diff(numeric_weight),
                    face.weight,
                    face.record.id,
                )
            })
            .map(|face| &face.record)
    }

    pub fn register_font_bytes(&mut self, bytes: Vec<u8>) -> Option<FontId> {
        let font_id = self.next_fallback_font_id();
        let record = FontRecord::from_bytes(font_id, bytes)?;
        self.ensure_fallbacks();
        self.fallbacks.get_or_insert_with(Vec::new).push(record);
        self.unsupported_codepoints.clear();
        self.cache.clear();
        self.advance_cache.clear();
        self.glyph_index_cache.clear();
        glyph_metrics::forget_font(font_id);
        self.font_bytes_cache.remove(&font_id);
        self.shaper_data_cache.remove(&font_id);
        Some(font_id)
    }

    /// Next id available for a font registered at runtime.
    ///
    /// Ids at or above [`SYSTEM_FALLBACK_ID_BASE`] belong to faces discovered
    /// on demand and are owned by the shared store, so they are skipped here:
    /// continuing from one of them would hand out an id the store may later
    /// assign to a different face.
    fn next_fallback_font_id(&self) -> FontId {
        self.fallbacks
            .as_ref()
            .into_iter()
            .flatten()
            .map(|record| record.id)
            .filter(|id| *id < SYSTEM_FALLBACK_ID_BASE)
            .chain(std::iter::once(self.primary.id))
            .max()
            .unwrap_or(self.primary.id)
            .saturating_add(1)
    }

    pub fn glyph_key_for_codepoint(&mut self, codepoint: char, font_size: f32) -> GlyphKey {
        if self
            .glyph_index_for_font(self.primary.id, codepoint)
            .is_none()
            && !self.unsupported_codepoints.contains(&codepoint)
        {
            self.ensure_fallbacks();
        }

        let (font_id, glyph_id, supported) = self.font_and_glyph_for_codepoint(codepoint);
        if !supported {
            self.unsupported_codepoints.insert(codepoint);
        }
        GlyphKey::new(font_id, glyph_id, font_size)
    }

    pub fn font_id_for_codepoint(&mut self, codepoint: char) -> FontId {
        if self
            .glyph_index_for_font(self.primary.id, codepoint)
            .is_none()
            && !self.unsupported_codepoints.contains(&codepoint)
        {
            self.ensure_fallbacks();
        }

        let (font_id, _, supported) = self.font_and_glyph_for_codepoint(codepoint);
        if !supported {
            self.unsupported_codepoints.insert(codepoint);
        }
        font_id
    }

    fn font_and_glyph_for_codepoint(&mut self, codepoint: char) -> (FontId, u16, bool) {
        if let Some(glyph_id) = self.glyph_index_for_font(self.primary.id, codepoint) {
            (self.primary.id, glyph_id, true)
        } else {
            let fallback_count = self.fallbacks.as_ref().map_or(0, Vec::len);
            for index in 0..fallback_count {
                let Some(font_id) = self
                    .fallbacks
                    .as_ref()
                    .and_then(|fallbacks| fallbacks.get(index))
                    .map(|record| record.id)
                else {
                    break;
                };
                if let Some(glyph_id) = self.glyph_index_for_font(font_id, codepoint) {
                    return (font_id, glyph_id, true);
                }
            }
            match self.resolve_system_fallback(codepoint) {
                Some((font_id, glyph_id)) => (font_id, glyph_id, true),
                None => (self.primary.id, 0, false),
            }
        }
    }

    /// Asks the platform for a face covering `codepoint` and adopts it.
    ///
    /// This runs only after every loaded face has been tried, which keeps the
    /// cost off the common path: the answer is cached process wide, so a
    /// codepoint is queried once per process no matter how many rasterizers
    /// encounter it.
    fn resolve_system_fallback(&mut self, codepoint: char) -> Option<(FontId, u16)> {
        if !self.enable_fallbacks || self.unsupported_codepoints.contains(&codepoint) {
            return None;
        }
        let record = fallback_for_codepoint(codepoint)?;
        let font_id = record.id;
        self.adopt_fallback(record);
        let glyph_id = self.glyph_index_for_font(font_id, codepoint)?;
        (glyph_id != 0).then_some((font_id, glyph_id))
    }

    /// Adds a face to this rasterizer's chain unless its id is already there.
    fn adopt_fallback(&mut self, record: FontRecord) {
        let fallbacks = self.fallbacks.get_or_insert_with(Vec::new);
        if !fallbacks.iter().any(|fallback| fallback.id == record.id) {
            fallbacks.push(record);
        }
    }

    /// Adopts the on-demand face named by `font_id`, if this is a foreign id.
    ///
    /// Shaping, layout and rasterization run in separate rasterizers, so a key
    /// can name a face this instance never resolved itself. The shared store
    /// keeps ids stable process wide, which makes recovering the face a plain
    /// lookup.
    fn ensure_system_fallback_loaded(&mut self, font_id: FontId) {
        if font_id < SYSTEM_FALLBACK_ID_BASE || self.font_record_by_id(font_id).is_some() {
            return;
        }
        if let Some(record) = fallback_by_id(font_id) {
            self.adopt_fallback(record);
        }
    }

    fn glyph_index_for_font(&mut self, font_id: FontId, codepoint: char) -> Option<u16> {
        let cache_key = (font_id, codepoint);
        if let Some(glyph_id) = self.glyph_index_cache.get(&cache_key) {
            return *glyph_id;
        }

        let glyph_id = self
            .font_record_by_id(font_id)
            .and_then(|record| record.glyph_index(codepoint));
        if self.glyph_index_cache.len() >= Self::GLYPH_INDEX_CACHE_CAPACITY {
            self.glyph_index_cache.clear();
        }
        self.glyph_index_cache.insert(cache_key, glyph_id);
        glyph_id
    }

    fn font_record_by_id(&self, font_id: FontId) -> Option<&FontRecord> {
        if font_id == self.primary.id {
            return Some(&self.primary);
        }
        if let Some(face) = self
            .family_faces
            .iter()
            .find(|face| face.record.id == font_id)
        {
            return Some(&face.record);
        }
        self.fallbacks
            .as_ref()?
            .iter()
            .find(|record| record.id == font_id)
    }

    #[cfg(test)]
    fn glyph_index_cache_len(&self) -> usize {
        self.glyph_index_cache.len()
    }

    fn select_font_for_key(&mut self, key: GlyphKey) -> &mut FontRecord {
        self.ensure_system_fallback_loaded(key.font_id);

        if key.font_id == self.primary.id {
            &mut self.primary
        } else if let Some(index) = self
            .family_faces
            .iter()
            .position(|face| face.record.id == key.font_id)
        {
            &mut self.family_faces[index].record
        } else {
            self.fallbacks
                .as_mut()
                .and_then(|fbs| fbs.iter_mut().find(|fb| fb.id == key.font_id))
                .unwrap_or(&mut self.primary)
        }
    }

    /// Rasterize a single glyph at the given size, returning cached result if
    /// available.
    pub fn rasterize(&mut self, codepoint: char, font_size: f32) -> &RasterizedGlyph {
        let key = self.glyph_key_for_codepoint(codepoint, font_size);

        self.rasterize_key(key, font_size)
    }

    pub fn rasterize_key(&mut self, key: GlyphKey, font_size: f32) -> &RasterizedGlyph {
        let is_cached = self.cache.contains_key(&key);
        // Check if we need to load fallbacks for this glyph.
        if !is_cached && key.font_id != self.primary.id {
            // debug!("----------------------------------------------------------------------------");
            time_cost!("FallbackFont", || self.ensure_fallbacks())
        }
        if !is_cached {
            // #[cfg(debug_assertions)]
            // debug!("----------------------------------------------------------------------------");
            let is_color = time_cost!("SelectingFontColor", || self
                .select_font_for_key(key)
                .is_color);

            let glyph = time_cost!("   |-RasterizingLogic", {
                if is_color {
                    let record_snapshot = time_cost!("       |-RecordSnapshot", || self
                        .select_font_for_key(key)
                        .clone());
                    time_cost!("       |-RasterizeColorGlyph", || {
                        rasterize_color_glyph(&record_snapshot, key.glyph_id, font_size)
                            .unwrap_or_else(|| RasterizedGlyph {
                                bitmap: Vec::new(),
                                width: 0,
                                height: 0,
                                offset_x: 0.0,
                                offset_y: 0.0,
                                advance_width: font_size * 0.5,
                                is_color: true,
                            })
                    })
                } else {
                    let record = time_cost!("   |-SelectFontForRasterize", || {
                        self.select_font_for_key(key)
                    });
                    let fallback_advance = time_cost!("   |-FallbackAdvance", || {
                        record
                            .advance_width_for_glyph(key.glyph_id, font_size)
                            .unwrap_or(0.0)
                    });
                    let record_snapshot = time_cost!("   |-RecordSnapshot", || record.clone());
                    time_cost!("   |-RasterizeSwashGlyph", || {
                        rasterize_swash_glyph(&record_snapshot, key.glyph_id, font_size)
                            .or_else(|| {
                                rasterize_outline_glyph(&record_snapshot, key.glyph_id, font_size)
                            })
                            .unwrap_or_else(|| RasterizedGlyph {
                                bitmap: Vec::new(),
                                width: 0,
                                height: 0,
                                offset_x: 0.0,
                                offset_y: 0.0,
                                advance_width: fallback_advance,
                                is_color: false,
                            })
                    })
                }
            });

            #[cfg(test)]
            {
                self.rasterize_call_count += 1;
            }
            self.advance_cache.insert(key, glyph.advance_width);
            glyph_metrics::store(key, &glyph);
            self.insert_cached_glyph(key, glyph);
        }

        self.cache.get(&key).expect("glyph was just inserted")
    }

    pub fn rasterize_bitmap_key(&mut self, key: GlyphKey, font_size: f32) -> &RasterizedGlyph {
        if self
            .cache
            .get(&key)
            .is_some_and(|glyph| glyph.width > 0 && glyph.height > 0 && glyph.bitmap.is_empty())
        {
            self.cache.remove(&key);
        }
        self.rasterize_key(key, font_size)
    }

    pub fn release_bitmap(&mut self, key: GlyphKey) {
        if let Some(glyph) = self.cache.get_mut(&key) {
            self.retained_bitmap_bytes = self
                .retained_bitmap_bytes
                .saturating_sub(glyph.bitmap.capacity());
            glyph.bitmap.clear();
            glyph.bitmap.shrink_to_fit();
        }
    }

    fn insert_cached_glyph(&mut self, key: GlyphKey, glyph: RasterizedGlyph) {
        if let Some(previous) = self.cache.remove(&key) {
            self.retained_bitmap_bytes = self
                .retained_bitmap_bytes
                .saturating_sub(previous.bitmap.capacity());
        }

        let incoming_bytes = glyph.bitmap.capacity();
        self.make_bitmap_capacity_for(incoming_bytes);
        self.retained_bitmap_bytes = self.retained_bitmap_bytes.saturating_add(incoming_bytes);
        self.cache.insert(key, glyph);
    }

    fn make_bitmap_capacity_for(&mut self, incoming_bytes: usize) {
        if self.retained_bitmap_bytes.saturating_add(incoming_bytes)
            <= Self::BITMAP_CACHE_CAPACITY_BYTES
        {
            return;
        }

        for glyph in self.cache.values_mut() {
            self.retained_bitmap_bytes = self
                .retained_bitmap_bytes
                .saturating_sub(glyph.bitmap.capacity());
            glyph.bitmap.clear();
            glyph.bitmap.shrink_to_fit();
            if self.retained_bitmap_bytes.saturating_add(incoming_bytes)
                <= Self::BITMAP_CACHE_CAPACITY_BYTES
            {
                break;
            }
        }
    }

    pub fn bitmap_cache_bytes(&self) -> usize {
        self.retained_bitmap_bytes
    }

    pub fn cached_glyph_count(&self) -> usize {
        self.cache.len()
    }

    pub(super) fn needs_prepared_glyph(&self, key: GlyphKey, needs_bitmap: bool) -> bool {
        self.cache
            .get(&key)
            .is_none_or(|glyph| needs_bitmap && glyph.bitmap.is_empty())
    }

    pub(super) fn commit_prepared_glyph(&mut self, key: GlyphKey, glyph: RasterizedGlyph) {
        self.advance_cache.insert(key, glyph.advance_width);
        glyph_metrics::store(key, &glyph);
        self.insert_cached_glyph(key, glyph);
    }

    pub(super) fn cached_glyph_descriptor(&self, key: GlyphKey) -> Option<(bool, u32, u32)> {
        self.cache
            .get(&key)
            .map(|glyph| (glyph.is_color, glyph.width, glyph.height))
    }

    #[cfg(test)]
    fn cached_glyph(&self, key: GlyphKey) -> Option<&RasterizedGlyph> {
        self.cache.get(&key)
    }

    pub fn glyph_metrics_for_key(&mut self, key: GlyphKey, font_size: f32) -> RasterizedGlyph {
        self.rasterize_key(key, font_size).clone()
    }

    /// Returns the pixel box of `key` — bitmap size, pen and baseline offsets
    /// and advance — rasterizing only when the glyph has never been measured.
    ///
    /// Positioning is the only consumer that needs those numbers without the
    /// coverage bitmap. Because the metrics depend solely on the glyph key,
    /// they are shared process-wide, so a layout pass running on a freshly
    /// created worker context — which is what every frame of a window resize
    /// does — reuses them instead of re-rasterizing the whole page.
    pub(super) fn metrics_for_key(&mut self, key: GlyphKey, font_size: f32) -> GlyphMetrics {
        if let Some(glyph) = self.cache.get(&key) {
            return GlyphMetrics::from(glyph);
        }
        if let Some(metrics) = glyph_metrics::cached(key) {
            return metrics;
        }

        GlyphMetrics::from(self.rasterize_key(key, font_size))
    }

    pub fn preload_text(&mut self, text: &str, font_size: f32) -> Vec<(GlyphKey, RasterizedGlyph)> {
        let mut glyphs = Vec::new();
        for c in text.chars() {
            if c.is_control() {
                continue;
            }

            let key = self.glyph_key_for_codepoint(c, font_size);
            let glyph = self.rasterize_bitmap_key(key, font_size).clone();
            glyphs.push((key, glyph));
        }
        glyphs
    }

    pub fn advance_width(&mut self, codepoint: char, font_size: f32) -> f32 {
        let key = self.glyph_key_for_codepoint(codepoint, font_size);
        if let Some(width) = self.advance_cache.get(&key) {
            return *width;
        }

        if key.font_id != self.primary.id {
            self.ensure_fallbacks();
        }

        let width = self
            .select_font_for_key(key)
            .advance_width_for_glyph(key.glyph_id, font_size)
            .unwrap_or(0.0);
        self.advance_cache.insert(key, width);
        width
    }

    pub fn advance_width_for_key(&mut self, key: GlyphKey, font_size: f32) -> f32 {
        if let Some(width) = self.advance_cache.get(&key) {
            return *width;
        }

        if key.font_id != self.primary.id {
            self.ensure_fallbacks();
        }

        let width = self
            .select_font_for_key(key)
            .advance_width_for_glyph(key.glyph_id, font_size)
            .unwrap_or(0.0);
        self.advance_cache.insert(key, width);
        width
    }

    /// Returns line metrics (ascent, descent, line_gap) for the given font
    /// size. Uses the primary font for consistent line spacing.
    pub fn line_metrics(&self, font_size: f32) -> (f32, f32, f32) {
        self.line_metrics_for_family(
            font_size,
            FontFamily::SANS_SERIF,
            FontWeight::Normal,
            FontStyle::Normal,
        )
    }

    pub fn line_metrics_for_family(
        &self,
        font_size: f32,
        family: FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> (f32, f32, f32) {
        let record = self
            .family_record(family, weight, style)
            .unwrap_or(&self.primary);
        let Some(data) = record.bytes.as_ref() else {
            return (font_size * 0.8, font_size * -0.2, 0.0);
        };
        let Some(face) = font_ref(data.as_ref(), record.collection_index) else {
            return (font_size * 0.8, font_size * -0.2, 0.0);
        };
        let metrics = face.metrics(Size::new(font_size), LocationRef::default());
        (metrics.ascent, metrics.descent, metrics.leading)
    }

    /// Convenience: measure the advance width of a string.
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> f32 {
        text.chars().map(|c| self.advance_width(c, font_size)).sum()
    }

    /// Shape a single grapheme cluster using the correct font (primary or
    /// fallback).
    ///
    /// Uses HarfRust to shape the entire cluster as a unit, so that
    /// complex-script sequences (e.g. Khmer base + COENG + subscript
    /// consonant) produce the correct ligature glyph IDs and advances
    /// rather than being split into separate unrelated glyphs.
    ///
    /// Returns a list of `(GlyphKey, advance, x_offset, y_offset)` tuples.
    /// If shaping fails or the cluster is empty, returns an empty vec.
    pub fn shape_cluster(
        &mut self,
        cluster: &str,
        font_size: f32,
    ) -> Vec<(GlyphKey, f32, f32, f32)> {
        self.shape_cluster_for_family(
            cluster,
            font_size,
            FontFamily::SANS_SERIF,
            FontWeight::Normal,
            FontStyle::Normal,
        )
    }

    pub fn shape_cluster_for_family(
        &mut self,
        cluster: &str,
        font_size: f32,
        family: FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> Vec<(GlyphKey, f32, f32, f32)> {
        self.shape_run_for_family(cluster, font_size, family, weight, style)
            .into_iter()
            .map(|glyph| {
                (
                    glyph.glyph_key,
                    glyph.advance,
                    glyph.x_offset,
                    glyph.y_offset,
                )
            })
            .collect()
    }

    pub fn font_id_for_family_cluster(
        &mut self,
        cluster: &str,
        font_size: f32,
        family: FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> Option<FontId> {
        let base_char = cluster.chars().find(|codepoint| !codepoint.is_control())?;
        Some(
            self.glyph_key_for_family_codepoint(base_char, font_size, family, weight, style)
                .font_id,
        )
    }

    pub fn shape_run_for_family(
        &mut self,
        text: &str,
        font_size: f32,
        family: FontFamily,
        weight: FontWeight,
        style: FontStyle,
    ) -> Vec<ShapedRunGlyph> {
        let Some(font_id) = self.font_id_for_family_cluster(text, font_size, family, weight, style)
        else {
            return Vec::new();
        };
        self.shape_run_with_font_id(text, font_size, font_id)
    }

    pub fn shape_run_with_font_id(
        &mut self,
        text: &str,
        font_size: f32,
        font_id: FontId,
    ) -> Vec<ShapedRunGlyph> {
        if text.is_empty() {
            return Vec::new();
        }
        self.ensure_system_fallback_loaded(font_id);

        // Retrieve cached font bytes for this font_id, populating the cache on
        // first access.  This avoids a file read (or Arc<[u8]> clone followed by
        // a heap copy) on every call.
        if !self.font_bytes_cache.contains_key(&font_id) {
            let bytes = self
                .family_faces
                .iter()
                .find(|face| face.record.id == font_id)
                .and_then(|face| face.record.data())
                .or_else(|| {
                    if font_id == self.primary.id {
                        return self.primary.data();
                    }
                    self.fallbacks
                        .as_ref()
                        .and_then(|fbs| fbs.iter().find(|fb| fb.id == font_id))
                        .and_then(FontRecord::data)
                });
            if let Some(b) = bytes {
                self.font_bytes_cache.insert(font_id, b);
            }
        }

        let font_data = match self.font_bytes_cache.get(&font_id) {
            Some(data) => data,
            None => return Vec::new(),
        };

        // Shape the cluster with HarfRust.
        let collection_index = self
            .family_faces
            .iter()
            .find(|face| face.record.id == font_id)
            .map(|face| face.record.collection_index)
            .unwrap_or_else(|| {
                if font_id == self.primary.id {
                    return self.primary.collection_index;
                }
                self.fallbacks
                    .as_ref()
                    .and_then(|fbs| fbs.iter().find(|fb| fb.id == font_id))
                    .map(|fb| fb.collection_index)
                    .unwrap_or(0)
            });

        let face = match harfrust::FontRef::from_index(font_data.as_ref(), collection_index) {
            Ok(face) => face,
            Err(_) => return Vec::new(),
        };
        self.shaper_data_cache
            .entry(font_id)
            .or_insert_with(|| harfrust::ShaperData::new(&face));
        let shaper = self
            .shaper_data_cache
            .get(&font_id)
            .expect("shaper data was just inserted")
            .shaper(&face)
            .build();

        let upem = shaper.units_per_em() as f32;
        let scale = if upem > 0.0 { font_size / upem } else { 1.0 };

        // Re-use the pre-allocated UnicodeBuffer by taking it out, resetting it,
        // filling it with the cluster text, shaping, then putting it back.
        let mut buffer = self.shape_buffer.take().unwrap_or_default();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        #[cfg(test)]
        {
            self.shape_call_count += 1;
        }
        let output = shaper.shape(buffer, harfrust::ShapeOptions::default());

        let result = output
            .glyph_infos()
            .iter()
            .zip(output.glyph_positions())
            .map(|(info, pos)| {
                let glyph_id = info.glyph_id as u16;
                ShapedRunGlyph {
                    glyph_key: GlyphKey::new(font_id, glyph_id, font_size),
                    advance: pos.x_advance as f32 * scale,
                    x_offset: pos.x_offset as f32 * scale,
                    y_offset: pos.y_offset as f32 * scale,
                    cluster: info.cluster as usize,
                }
            })
            .collect();

        // Return the buffer (now a GlyphBuffer) back to a UnicodeBuffer for reuse.
        self.shape_buffer = Some(output.clear());

        result
    }
}

#[cfg(test)]
mod tests {
    use std::hash::{Hash, Hasher};
    use std::sync::Arc;

    use hashbrown::{HashMap, HashSet};

    use super::*;
    use crate::font::{FontFamily, FontRegistration, FontRegistry, FontStyle, FontWeight};
    use crate::text_pipeline::text_layout::{layout_shaped_text, shape_text_styled};

    fn assert_send_sync<T: Send + Sync>() {}

    #[derive(Default)]
    struct HashWriteCounter {
        writes: usize,
        value: u64,
    }

    impl Hasher for HashWriteCounter {
        fn finish(&self) -> u64 {
            0
        }

        fn write(&mut self, _bytes: &[u8]) {
            self.writes += 1;
        }

        fn write_u64(&mut self, value: u64) {
            self.writes += 1;
            self.value = value;
        }
    }

    fn compact_glyph_key_hash(key: GlyphKey) -> u64 {
        let mut hasher = HashWriteCounter::default();
        key.hash(&mut hasher);
        assert_eq!(hasher.writes, 1);
        hasher.value
    }

    #[test]
    fn glyph_key_hashes_as_one_compact_value() {
        let key = GlyphKey {
            font_id: 7,
            glyph_id: 42,
            size_tenths: 160,
            subpixel_x: 1,
            subpixel_y: 2,
        };
        let expected = compact_glyph_key_hash(key);
        for distinct_key in [
            GlyphKey { font_id: 8, ..key },
            GlyphKey {
                glyph_id: 43,
                ..key
            },
            GlyphKey {
                size_tenths: 170,
                ..key
            },
            GlyphKey {
                subpixel_x: 2,
                ..key
            },
            GlyphKey {
                subpixel_y: 3,
                ..key
            },
        ] {
            assert_ne!(compact_glyph_key_hash(distinct_key), expected);
        }
    }

    #[test]
    fn rasterizer_uses_fast_hashers_for_internal_caches() {
        fn assert_fast_map<K, V>(_: &HashMap<K, V>) {}
        fn assert_fast_set<K>(_: &HashSet<K>) {}

        let rasterizer = GlyphRasterizer::primary_only();
        assert_fast_map(&rasterizer.cache);
        assert_fast_map(&rasterizer.advance_cache);
        assert_fast_set(&rasterizer.unsupported_codepoints);
        assert_fast_map(&rasterizer.font_bytes_cache);
        assert_fast_map(&rasterizer.shaper_data_cache);
    }

    #[test]
    fn codepoints_outside_the_static_probe_groups_rasterize_to_visible_glyphs() {
        let mut rasterizer = GlyphRasterizer::new();
        for codepoint in ['你', '好', '！', '，', 'ü', '₫'] {
            let key = rasterizer.glyph_key_for_codepoint(codepoint, 20.0);
            assert_ne!(
                key.glyph_id, 0,
                "{codepoint:?} resolved to .notdef instead of a real glyph"
            );
            let glyph = rasterizer.rasterize_key(key, 20.0);
            assert!(
                !glyph.bitmap.is_empty(),
                "{codepoint:?} rasterized to an empty bitmap"
            );
        }
    }

    #[test]
    fn bitmap_cache_bytes_are_tracked_across_replace_and_release() {
        let mut rasterizer = GlyphRasterizer::primary_only();
        let key = GlyphKey::new(rasterizer.primary_font_id(), 1, 16.0);
        let glyph = |bytes| RasterizedGlyph {
            bitmap: vec![255; bytes],
            width: bytes as u32,
            height: 1,
            offset_x: 0.0,
            offset_y: 0.0,
            advance_width: 12.0,
            is_color: false,
        };

        rasterizer.commit_prepared_glyph(key, glyph(1024));
        assert_eq!(rasterizer.retained_bitmap_bytes, 1024);
        assert_eq!(rasterizer.bitmap_cache_bytes(), 1024);

        rasterizer.commit_prepared_glyph(key, glyph(256));
        assert_eq!(rasterizer.retained_bitmap_bytes, 256);
        assert_eq!(rasterizer.bitmap_cache_bytes(), 256);

        rasterizer.release_bitmap(key);
        assert_eq!(rasterizer.retained_bitmap_bytes, 0);
        assert_eq!(rasterizer.bitmap_cache_bytes(), 0);
    }

    #[test]
    fn positioning_already_measured_glyphs_does_not_rasterize_again() {
        // Resizing a window re-lays out every visible string at a new wrapping
        // width on every frame, and each layout job runs in a freshly created
        // worker context whose bitmap cache starts empty. Positioning only
        // needs the glyph's bitmap box, which depends solely on the glyph key,
        // so a glyph measured once must never be rasterized again.
        let mut renderer = GlyphRasterizer::new();
        let text = "Resize 你好 ជំរាបសួរ mixed العربية text";
        let shaped = shape_text_styled(
            &mut renderer,
            text,
            18.0,
            FontFamily::SANS_SERIF,
            FontWeight::Normal,
            FontStyle::Normal,
        );
        let expected = layout_shaped_text(&mut renderer, &shaped, 0.0, 0.0, 200.0);
        assert!(!expected.is_empty());

        let mut worker = GlyphPreparationContext::new(renderer.font_snapshot());
        worker.rasterizer_mut().reset_rasterize_call_count();
        let actual = layout_shaped_text(worker.rasterizer_mut(), &shaped, 0.0, 0.0, 200.0);

        assert_eq!(
            worker.rasterizer_mut().rasterize_call_count(),
            0,
            "a resize frame must reuse glyph metrics instead of rasterizing again"
        );
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(&expected) {
            assert_eq!(actual.glyph_key, expected.glyph_key);
            assert_eq!(actual.x, expected.x);
            assert_eq!(actual.y, expected.y);
            assert_eq!(actual.width, expected.width);
            assert_eq!(actual.height, expected.height);
        }
    }

    #[test]
    fn font_snapshot_is_naturally_send_and_sync() {
        assert_send_sync::<FontSnapshot>();
    }

    #[test]
    fn worker_context_matches_shaping_and_layout_output() {
        let mut renderer = GlyphRasterizer::new();
        let mut worker = GlyphPreparationContext::new(renderer.font_snapshot());
        let text = "office text wraps";

        let expected_shaped = shape_text_styled(
            &mut renderer,
            text,
            18.0,
            FontFamily::SANS_SERIF,
            FontWeight::Normal,
            FontStyle::Normal,
        );
        let actual_shaped = shape_text_styled(
            worker.rasterizer_mut(),
            text,
            18.0,
            FontFamily::SANS_SERIF,
            FontWeight::Normal,
            FontStyle::Normal,
        );

        assert_eq!(actual_shaped.font_size, expected_shaped.font_size);
        assert_eq!(actual_shaped.line_height, expected_shaped.line_height);
        assert_eq!(actual_shaped.clusters.len(), expected_shaped.clusters.len());
        for (actual, expected) in actual_shaped.clusters.iter().zip(&expected_shaped.clusters) {
            assert_eq!(actual.text, expected.text);
            assert_eq!(actual.base_codepoint, expected.base_codepoint);
            assert_eq!(actual.width, expected.width);
            assert_eq!(actual.glyphs, expected.glyphs);
        }

        let expected_layout = layout_shaped_text(&mut renderer, &expected_shaped, 0.0, 0.0, 80.0);
        let actual_layout =
            layout_shaped_text(worker.rasterizer_mut(), &actual_shaped, 0.0, 0.0, 80.0);
        assert_eq!(actual_layout.len(), expected_layout.len());
        for (actual, expected) in actual_layout.iter().zip(&expected_layout) {
            assert!(actual.glyph_key == expected.glyph_key);
            assert_eq!((actual.x, actual.y), (expected.x, expected.y));
            assert_eq!(
                (actual.width, actual.height),
                (expected.width, expected.height)
            );
            assert_eq!(actual.font_size, expected.font_size);
        }
    }

    #[test]
    fn worker_context_preserves_fallback_resolution() {
        let mut renderer = GlyphRasterizer::new();
        let mut worker = GlyphPreparationContext::new(renderer.font_snapshot());

        for codepoint in ['A', '界', '😀'] {
            let expected = renderer.glyph_key_for_codepoint(codepoint, 20.0);
            let actual = worker
                .rasterizer_mut()
                .glyph_key_for_codepoint(codepoint, 20.0);
            assert!(actual == expected);
        }
    }

    #[test]
    fn worker_context_returns_owned_alpha_glyph_with_matching_bitmap() {
        let mut renderer = GlyphRasterizer::new();
        let mut worker = GlyphPreparationContext::new(renderer.font_snapshot());
        let key = renderer.glyph_key_for_codepoint('A', 20.0);
        let expected = renderer.rasterize_key(key, 20.0).clone();
        let actual = worker.prepare_glyph(key, 20.0);

        assert!(!actual.is_color);
        assert_eq!(
            (actual.width, actual.height),
            (expected.width, expected.height)
        );
        assert_eq!(actual.bitmap, expected.bitmap);
    }

    #[test]
    fn worker_context_returns_owned_color_glyph_with_matching_dimensions() {
        let mut renderer = GlyphRasterizer::new();
        let mut worker = GlyphPreparationContext::new(renderer.font_snapshot());
        let key = renderer.glyph_key_for_codepoint('😀', 32.0);
        let expected = renderer.rasterize_key(key, 32.0).clone();
        let actual = worker.prepare_glyph(key, 32.0);

        assert_eq!(actual.is_color, expected.is_color);
        assert_eq!(
            (actual.width, actual.height),
            (expected.width, expected.height)
        );
        assert_eq!(actual.bitmap.len(), expected.bitmap.len());
        if expected.is_color {
            assert_eq!(actual.bitmap, expected.bitmap);
        }
    }

    #[test]
    fn worker_context_remains_usable_after_malformed_font_is_rejected() {
        let mut renderer = GlyphRasterizer::new();
        assert!(renderer.register_font_bytes(vec![0, 1, 2, 3]).is_none());

        let mut worker = GlyphPreparationContext::new(renderer.font_snapshot());
        let key = worker.rasterizer_mut().glyph_key_for_codepoint('A', 16.0);
        let glyph = worker.prepare_glyph(key, 16.0);
        assert!(glyph.width > 0);
        assert!(glyph.height > 0);
        assert!(!glyph.bitmap.is_empty());
    }

    #[test]
    fn warm_renderer_cache_suppresses_worker_jobs_until_bitmap_is_needed() {
        let mut renderer = GlyphRasterizer::new();
        let key = renderer.glyph_key_for_codepoint('A', 16.0);
        let glyph = renderer.rasterize_key(key, 16.0).clone();

        assert!(!renderer.needs_prepared_glyph(key, true));
        renderer.commit_prepared_glyph(key, glyph);
        renderer.release_bitmap(key);

        assert!(!renderer.needs_prepared_glyph(key, false));
        assert!(renderer.needs_prepared_glyph(key, true));
    }

    #[test]
    fn repeated_glyph_resolution_memoizes_font_cmap_lookup() {
        let mut rasterizer = GlyphRasterizer::primary_only();

        let first = rasterizer.glyph_key_for_codepoint('A', 16.0);
        let second = rasterizer.glyph_key_for_codepoint('A', 24.0);
        assert_eq!(
            (first.font_id, first.glyph_id),
            (second.font_id, second.glyph_id)
        );
        assert_eq!(rasterizer.glyph_index_cache_len(), 1);

        let first_miss = rasterizer.glyph_key_for_codepoint('\u{10ffff}', 16.0);
        let second_miss = rasterizer.glyph_key_for_codepoint('\u{10ffff}', 24.0);
        assert_eq!(
            (first_miss.font_id, first_miss.glyph_id),
            (second_miss.font_id, second_miss.glyph_id)
        );
        assert_eq!(rasterizer.glyph_index_cache_len(), 2);

        assert!(
            rasterizer
                .register_font_bytes(PRIMARY_FONT.to_vec())
                .is_some()
        );
        assert_eq!(rasterizer.glyph_index_cache_len(), 0);
    }

    #[test]
    fn registered_families_resolve_consistently_across_rasterizers() {
        let family = FontRegistry::register(FontRegistration {
            family: "cupid-family-resolution-test",
            bytes: PRIMARY_FONT,
            weight: FontWeight::Normal,
            style: FontStyle::Normal,
        })
        .unwrap();
        FontRegistry::register(FontRegistration {
            family: "cupid-family-resolution-test",
            bytes: PRIMARY_FONT,
            weight: FontWeight::Bold,
            style: FontStyle::Normal,
        })
        .unwrap();
        FontRegistry::register(FontRegistration {
            family: "cupid-family-resolution-test",
            bytes: PRIMARY_FONT,
            weight: FontWeight::Normal,
            style: FontStyle::Italic,
        })
        .unwrap();

        let mut first = GlyphRasterizer::new();
        let mut second = GlyphRasterizer::new();
        let normal = first.font_id_for_family(family, FontWeight::Normal, FontStyle::Normal);
        let bold = first.font_id_for_family(family, FontWeight::Bold, FontStyle::Normal);
        let nearest_bold =
            first.font_id_for_family(family, FontWeight::Value(600), FontStyle::Normal);
        let italic = first.font_id_for_family(family, FontWeight::Bold, FontStyle::Italic);

        assert_ne!(normal, bold);
        assert_eq!(nearest_bold, bold);
        assert_ne!(italic, bold);
        assert_eq!(
            second.font_id_for_family(family, FontWeight::Bold, FontStyle::Normal),
            bold
        );

        let key = first.glyph_key_for_family_codepoint(
            'A',
            16.0,
            family,
            FontWeight::Normal,
            FontStyle::Normal,
        );
        assert_eq!(key.font_id, normal);
    }

    #[test]
    fn generic_families_are_distinct_and_renderable() {
        let mut rasterizer = GlyphRasterizer::new();
        let sans = rasterizer.font_id_for_family(
            FontFamily::SANS_SERIF,
            FontWeight::Normal,
            FontStyle::Normal,
        );
        let monospace = rasterizer.font_id_for_family(
            FontFamily::MONOSPACE,
            FontWeight::Normal,
            FontStyle::Normal,
        );

        assert_eq!(sans, rasterizer.primary_font_id());
        assert_ne!(monospace, sans);
        let key = rasterizer.glyph_key_for_family_codepoint(
            'M',
            16.0,
            FontFamily::MONOSPACE,
            FontWeight::Normal,
            FontStyle::Normal,
        );
        assert_eq!(key.font_id, monospace);
        assert!(rasterizer.rasterize_key(key, 16.0).width > 0);
    }

    #[test]
    fn family_measurement_and_shaping_use_the_same_face() {
        let mut rasterizer = GlyphRasterizer::new();
        let mono_i = rasterizer.advance_width_for_family(
            'i',
            20.0,
            FontFamily::MONOSPACE,
            FontWeight::Normal,
            FontStyle::Normal,
        );
        let mono_m = rasterizer.advance_width_for_family(
            'M',
            20.0,
            FontFamily::MONOSPACE,
            FontWeight::Normal,
            FontStyle::Normal,
        );
        assert!((mono_i - mono_m).abs() < 0.001);

        let measured = rasterizer.measure_text_for_family(
            "Mi",
            20.0,
            FontFamily::MONOSPACE,
            FontWeight::Normal,
            FontStyle::Normal,
        );
        let shaped_width: f32 = rasterizer
            .shape_cluster_for_family(
                "Mi",
                20.0,
                FontFamily::MONOSPACE,
                FontWeight::Normal,
                FontStyle::Normal,
            )
            .iter()
            .map(|(_, advance, _, _)| advance)
            .sum();
        assert!((measured - shaped_width).abs() < 0.001);
    }

    #[test]
    fn selected_family_missing_glyph_uses_existing_unicode_fallback_chain() {
        let mut rasterizer = GlyphRasterizer::primary_only();
        let fallback_id = rasterizer
            .register_font_bytes(
                include_bytes!("../../../fonts/NotoSansJP-VariableFont_wght.ttf").to_vec(),
            )
            .expect("the bundled CJK fallback should register");

        let key = rasterizer.glyph_key_for_family_codepoint(
            '你',
            16.0,
            FontFamily::MONOSPACE,
            FontWeight::Normal,
            FontStyle::Normal,
        );

        assert_eq!(key.font_id, fallback_id);
        assert!(rasterizer.rasterize_key(key, 16.0).width > 0);
    }

    #[test]
    fn primary_font_bytes_are_shared_between_rasterizers() {
        let first = GlyphRasterizer::new();
        let second = GlyphRasterizer::primary_only();

        assert!(Arc::ptr_eq(
            first.primary.bytes.as_ref().expect("primary bytes missing"),
            second
                .primary
                .bytes
                .as_ref()
                .expect("primary bytes missing")
        ));
    }

    #[test]
    fn register_font_bytes_adds_in_memory_fallback() {
        let mut rasterizer = GlyphRasterizer::primary_only();
        let bytes = PRIMARY_FONT.to_vec();

        let font_id = rasterizer
            .register_font_bytes(bytes)
            .expect("embedded font bytes should register");

        assert_ne!(font_id, rasterizer.primary_font_id());
        let fallbacks = rasterizer
            .fallbacks
            .as_ref()
            .expect("registered fallback missing");
        let registered = fallbacks
            .iter()
            .find(|record| record.id == font_id)
            .expect("registered font record missing");
        assert!(registered.bytes.is_some());
        assert!(registered.glyph_index('A').is_some());
    }

    #[test]
    fn latin_lookup_does_not_load_fallbacks() {
        let mut rasterizer = GlyphRasterizer::new();

        for c in "Hello from Cupid!".chars() {
            rasterizer.glyph_key_for_codepoint(c, 32.0);
        }

        assert!(rasterizer.fallbacks.is_none());
        assert!(rasterizer.unsupported_codepoints.is_empty());
    }

    #[test]
    fn preload_text_is_idempotent_for_cached_glyphs() {
        let mut rasterizer = GlyphRasterizer::new();

        rasterizer.preload_text("Hello", 16.0);
        let cache_len = rasterizer.cache.len();
        let advance_cache_len = rasterizer.advance_cache.len();

        rasterizer.preload_text("Hello", 16.0);

        assert_eq!(rasterizer.cache.len(), cache_len);
        assert_eq!(rasterizer.advance_cache.len(), advance_cache_len);
        assert!(rasterizer.fallbacks.is_none());
    }

    /// macOS ships AppleColorEmoji at
    /// /System/Library/Fonts/AppleColorEmoji.ttc. On a system without that
    /// font (or in CI containers), the chain just won't contain it; the
    /// test stays informative either way by asserting *if* the
    /// font was loaded, the record is correctly tagged as color.
    // #[test]
    #[allow(dead_code)]
    fn khmer_glyphs_use_renderable_fallback_font() {
        let mut rasterizer = GlyphRasterizer::new();

        // ក ខ គ are basic Khmer consonants that must be present in any Khmer font.
        for c in "កខគ".chars() {
            let key = rasterizer.glyph_key_for_codepoint(c, 16.0);
            assert_ne!(
                key.font_id,
                rasterizer.primary_font_id(),
                "U+{:04X} {} should use a Khmer fallback font, not the primary (Roboto)",
                c as u32,
                c
            );

            let glyph = rasterizer.glyph_metrics_for_key(key, 16.0);
            assert!(
                glyph.width > 0,
                "U+{:04X} {} Khmer glyph should have bitmap width > 0",
                c as u32,
                c
            );
            assert!(
                glyph.height > 0,
                "U+{:04X} {} Khmer glyph should have bitmap height > 0",
                c as u32,
                c
            );
            assert!(
                !glyph.bitmap.is_empty(),
                "U+{:04X} {} Khmer glyph bitmap must not be empty",
                c as u32,
                c
            );
            assert!(
                !glyph.is_color,
                "U+{:04X} {} Khmer glyph should be monochrome",
                c as u32, c
            );
        }
    }

    /// Verify that `shape_cluster` handles Khmer subscript clusters (base +
    /// COENG + subscript) as a single shaped unit, producing exactly one
    /// visible glyph (the ligature) rather than three separate
    /// mispositioned glyphs for each codepoint.
    // #[test]
    #[allow(dead_code)]
    fn khmer_coeng_cluster_shapes_as_ligature() {
        let mut rasterizer = GlyphRasterizer::new();

        // "ក្ត" = ក (U+1780) + ្ (U+17D2 COENG) + ត (U+178F)
        // With proper shaping this should produce 1 ligature glyph, not 3 separate
        // glyphs.
        let cluster = "ក្ត";
        let shaped = rasterizer.shape_cluster(cluster, 16.0);

        // A Khmer font is required for this test.
        if shaped.is_empty() {
            eprintln!("[note] No Khmer fallback font found — skipping coeng cluster test");
            return;
        }

        // The shaped output should have fewer glyphs than codepoints (3).
        // In practice HarfRust + Khmer Sangam MN produces 2 glyphs for this cluster:
        // one for the base consonant with full advance, one zero-advance mark
        // (subscript).
        assert!(
            shaped.len() < cluster.chars().count(),
            "Khmer COENG cluster should produce fewer glyphs than codepoints; got {} shaped glyphs for {} codepoints",
            shaped.len(),
            cluster.chars().count()
        );

        // Each shaped glyph must use the Khmer fallback font (not Roboto primary).
        for (key, _, _, _) in &shaped {
            assert_ne!(
                key.font_id,
                rasterizer.primary_font_id(),
                "Khmer cluster glyph must use a fallback font, not primary (Roboto)"
            );
        }

        // Every shaped glyph must rasterize to a non-empty bitmap.
        for (key, _, _, _) in shaped {
            let glyph = rasterizer.rasterize_key(key, 16.0);
            assert!(
                glyph.width > 0 && glyph.height > 0 && !glyph.bitmap.is_empty(),
                "Shaped Khmer glyph (id={}) must have a renderable bitmap",
                key.glyph_id
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn fallback_chain_keeps_both_emoji_and_cjk() {
        let mut rasterizer = GlyphRasterizer::new();

        // Emoji and CJK are served by different files, so resolving both must
        // grow the chain rather than replace its single entry: the color face
        // may not answer for Han, and the Han face has no emoji strikes.
        let emoji = rasterizer.glyph_key_for_codepoint('😀', 32.0);
        let han = rasterizer.glyph_key_for_codepoint('漢', 32.0);

        assert_ne!(emoji.font_id, rasterizer.primary_font_id());
        assert_ne!(han.font_id, rasterizer.primary_font_id());
        assert_ne!(
            emoji.font_id, han.font_id,
            "emoji and CJK must resolve to different faces"
        );

        let chain = rasterizer.fallbacks.as_ref().expect("chain was populated");
        assert!(chain.iter().any(|fallback| fallback.is_color));
        assert!(chain.iter().any(|fallback| !fallback.is_color));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn emoji_glyph_rasterizes_as_color() {
        let mut rasterizer = GlyphRasterizer::new();

        let key = rasterizer.glyph_key_for_codepoint('😀', 32.0);
        if key.font_id == rasterizer.primary_font_id() {
            // No emoji fallback available — Roboto can't render '😀'. Skip.
            eprintln!("[note] '😀' resolved to primary; AppleColorEmoji not on this macOS install");
            return;
        }

        let glyph = rasterizer.glyph_metrics_for_key(key, 32.0);
        assert!(glyph.is_color, "'😀' should be tagged as a color glyph");
        assert!(
            glyph.width > 0 && glyph.height > 0,
            "'😀' bitmap dimensions must be non-zero"
        );
        // RGBA8 → 4 bytes per pixel. The bitmap may be empty if the sbix
        // strike was missing/unsupported (we'd hit the placeholder branch in
        // rasterize_key), so guard the size check on the bitmap being present.
        if !glyph.bitmap.is_empty() {
            assert_eq!(
                glyph.bitmap.len(),
                (glyph.width * glyph.height * 4) as usize,
                "'😀' bitmap must be RGBA8 (4 bytes per pixel)"
            );
        }
    }

    #[test]
    fn uploaded_glyph_bitmap_can_be_released_without_losing_metrics() {
        let mut rasterizer = GlyphRasterizer::primary_only();
        let key = rasterizer.glyph_key_for_codepoint('A', 24.0);
        let expected = rasterizer.rasterize_key(key, 24.0).clone();
        assert!(!expected.bitmap.is_empty());

        rasterizer.release_bitmap(key);

        let cached = rasterizer.cached_glyph(key).unwrap();
        assert!(cached.bitmap.is_empty());
        assert_eq!(cached.width, expected.width);
        assert_eq!(cached.height, expected.height);
        assert_eq!(cached.advance_width, expected.advance_width);
    }

    #[test]
    fn bitmap_cache_pruning_keeps_metrics_and_releases_pixel_capacity() {
        let mut rasterizer = GlyphRasterizer::primary_only();
        let key = GlyphKey::new(rasterizer.primary_font_id(), 1, 16.0);
        rasterizer.commit_prepared_glyph(
            key,
            RasterizedGlyph {
                bitmap: vec![255; 1024],
                width: 32,
                height: 32,
                offset_x: 1.0,
                offset_y: 2.0,
                advance_width: 12.0,
                is_color: false,
            },
        );

        rasterizer.make_bitmap_capacity_for(GlyphRasterizer::BITMAP_CACHE_CAPACITY_BYTES);

        let cached = rasterizer.cached_glyph(key).unwrap();
        assert!(cached.bitmap.is_empty());
        assert_eq!((cached.width, cached.height), (32, 32));
        assert_eq!(cached.advance_width, 12.0);
    }
}
