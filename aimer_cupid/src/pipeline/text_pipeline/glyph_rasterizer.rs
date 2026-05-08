use super::text_layout::FontId;
use aimer_utils::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// Embedded primary font (Roboto) — covers Latin and common scripts.
const PRIMARY_FONT: &[u8] = include_bytes!("../../../fonts/Roboto.ttf");

/// A rasterized glyph bitmap with its metrics.
///
/// `bitmap` layout depends on `is_color`:
///   * `is_color == false` — `width * height` bytes, single-channel coverage
///     (8-bit alpha), as produced by `fontdue`.
///   * `is_color == true`  — `width * height * 4` bytes, RGBA8 (non-premultiplied),
///     as produced from `sbix` PNG strikes (AppleColorEmoji, etc.).
///
/// The text pipeline routes color glyphs to a separate RGBA8 atlas + shader.
#[derive(Clone)]
pub struct RasterizedGlyph {
    pub bitmap: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Horizontal offset from the pen position to the left edge of the bitmap.
    pub offset_x: f32,
    /// Vertical offset from the baseline to the bottom edge of the bitmap (y-up,
    /// matches `fontdue::Metrics::ymin`).
    pub offset_y: f32,
    /// Horizontal advance width.
    pub advance_width: f32,
    /// Whether the bitmap is RGBA8 color data (true) or single-channel alpha (false).
    pub is_color: bool,
}

/// Key for caching rasterized-shaped glyphs.
#[derive(Hash, Eq, PartialEq, Clone, Copy)]
pub struct GlyphKey {
    pub font_id: FontId,
    pub glyph_id: u16,
    pub size_tenths: u32,
    pub subpixel_x: u8,
    pub subpixel_y: u8,
}

impl GlyphKey {
    pub fn new(font_id: FontId, glyph_id: u16, font_size: f32) -> Self {
        Self { font_id, glyph_id, size_tenths: (font_size * 10.0) as u32, subpixel_x: 0, subpixel_y: 0 }
    }
}

#[derive(Clone)]
pub struct FontRecord {
    pub id: FontId,
    pub bytes: Option<Arc<[u8]>>,
    pub font: Option<Arc<fontdue::Font>>,
    collection_index: u32,
    path: Option<Arc<PathBuf>>,
    /// True when the font carries color glyph data (`sbix` / `CBDT` / `COLR`)
    /// and should be rasterized via `Face::glyph_raster_image` instead of `fontdue`.
    /// In that case `font` stays `None` for the lifetime of the record.
    pub is_color: bool,
}

impl FontRecord {
    fn from_static_bytes(id: FontId, bytes: &'static [u8]) -> Option<Self> {
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()?;
        Some(Self { id, bytes: Some(Arc::from(bytes)), font: Some(Arc::new(font)), collection_index: 0, path: None, is_color: false })
    }

    /// Returns true if this collection_index of `data` contains any color glyph
    /// table that we know how to render (currently only `sbix`).
    fn face_is_color(face: &ttf_parser::Face<'_>) -> bool {
        // `Face::tables().sbix` is `Option<sbix::Table>`. AppleColorEmoji is the
        // canonical user. We don't yet decode CBDT/COLR but they're easy to add later.
        face.tables().sbix.is_some()
    }

    /// Probe the font with each `probes` codepoint; accept on the first match.
    /// `accept_color` allows color fonts to be admitted to the chain even when
    /// none of the probes are present (the typical case for emoji fonts whose
    /// cmap maps emoji codepoints — which is what callers should pass here, but
    /// we keep the option to make tests easier).
    fn probes_match(face: &ttf_parser::Face<'_>, probes: &[char]) -> bool {
        probes.iter().any(|&c| face.glyph_index(c).is_some())
    }

    #[allow(dead_code)]
    fn from_bytes_with_probes(id: FontId, bytes: Vec<u8>, probes: &[char], hint_color: bool) -> Option<Self> {
        for collection_index in 0..16 {
            let Ok(face) = ttf_parser::Face::parse(&bytes, collection_index) else {
                if collection_index == 0 {
                    return None;
                }
                break;
            };

            if !Self::probes_match(&face, probes) {
                continue;
            }

            let is_color = hint_color || Self::face_is_color(&face);
            return Some(Self { id, bytes: Some(Arc::from(bytes)), font: None, collection_index, path: None, is_color });
        }

        None
    }

    fn from_path_with_probes(id: FontId, path: PathBuf, probes: &[char], hint_color: bool) -> Option<Self> {
        let file = std::fs::File::open(&path).ok()?;
        let map = unsafe { memmap2::Mmap::map(&file).ok()? };

        for collection_index in 0..16 {
            let Ok(face) = ttf_parser::Face::parse(&map, collection_index) else {
                if collection_index == 0 {
                    return None;
                }
                break;
            };

            if !Self::probes_match(&face, probes) {
                continue;
            }

            let is_color = hint_color || Self::face_is_color(&face);
            return Some(Self { id, bytes: None, font: None, collection_index, path: Some(Arc::new(path)), is_color });
        }

        None
    }

    /// Borrow the underlying font data either from the in-memory `bytes` or by
    /// reading the on-disk path. Returns `None` if neither source is available.
    fn read_data(&self) -> Option<Vec<u8>> {
        if let Some(bytes) = self.bytes.as_ref() {
            return Some(bytes.as_ref().to_vec());
        }
        let path = self.path.as_ref()?;
        std::fs::read(path.as_ref()).ok()
    }

    fn ensure_face(&self) -> Option<()> {
        if let Some(bytes) = self.bytes.as_ref() {
            ttf_parser::Face::parse(bytes.as_ref(), self.collection_index).ok()?;
            return Some(());
        }

        let path = self.path.as_ref()?;
        let file = std::fs::File::open(path.as_ref()).ok()?;
        let map = unsafe { memmap2::Mmap::map(&file).ok()? };
        ttf_parser::Face::parse(&map, self.collection_index).ok()?;
        Some(())
    }

    fn ensure_font(&mut self) -> Option<Arc<fontdue::Font>> {
        if self.font.is_none() {
            let settings = fontdue::FontSettings { collection_index: self.collection_index, ..fontdue::FontSettings::default() };
            let font = if let Some(bytes) = self.bytes.as_ref() {
                fontdue::Font::from_bytes(bytes.as_ref(), settings).ok()?
            } else {
                let path = self.path.as_ref()?;
                let data = std::fs::read(path.as_ref()).ok()?;
                fontdue::Font::from_bytes(data, settings).ok()?
            };
            self.font = Some(Arc::new(font));
        }

        self.font.clone()
    }

    fn glyph_index(&self, codepoint: char) -> Option<u16> {
        if let Some(font) = self.font.as_ref() {
            return font
                .has_glyph(codepoint)
                .then(|| font.lookup_glyph_index(codepoint));
        }

        if let Some(bytes) = self.bytes.as_ref() {
            let face = ttf_parser::Face::parse(bytes.as_ref(), self.collection_index).ok()?;
            return face.glyph_index(codepoint).map(|id| id.0);
        }

        let path = self.path.as_ref()?;
        let file = std::fs::File::open(path.as_ref()).ok()?;
        let map = unsafe { memmap2::Mmap::map(&file).ok()? };
        let face = ttf_parser::Face::parse(&map, self.collection_index).ok()?;
        face.glyph_index(codepoint).map(|id| id.0)
    }

    fn advance_width_for_glyph(&self, glyph_id: u16, font_size: f32) -> Option<f32> {
        if let Some(font) = self.font.as_ref() {
            return Some(font.metrics_indexed(glyph_id, font_size).advance_width);
        }

        if let Some(bytes) = self.bytes.as_ref() {
            return advance_width_from_face(bytes.as_ref(), self.collection_index, glyph_id, font_size);
        }

        let path = self.path.as_ref()?;
        let file = std::fs::File::open(path.as_ref()).ok()?;
        let map = unsafe { memmap2::Mmap::map(&file).ok()? };
        advance_width_from_face(&map, self.collection_index, glyph_id, font_size)
    }
}

fn advance_width_from_face(bytes: &[u8], collection_index: u32, glyph_id: u16, font_size: f32) -> Option<f32> {
    let face = ttf_parser::Face::parse(bytes, collection_index).ok()?;
    let units_per_em = f32::from(face.units_per_em());
    let advance = f32::from(face.glyph_hor_advance(ttf_parser::GlyphId(glyph_id))?);
    Some(advance * font_size / units_per_em)
}

#[derive(Default)]
struct GlyphOutline {
    contours: Vec<Vec<(f32, f32)>>,
    current: Vec<(f32, f32)>,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl GlyphOutline {
    fn new(scale: f32, offset_x: f32, offset_y: f32) -> Self {
        Self { scale, offset_x, offset_y, ..Self::default() }
    }

    fn push_point(&mut self, x: f32, y: f32) {
        self.current
            .push((x * self.scale - self.offset_x, y * self.scale - self.offset_y));
    }

    fn finish_contour(&mut self) {
        if self.current.len() >= 2 {
            self.contours.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
    }
}

impl ttf_parser::OutlineBuilder for GlyphOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish_contour();
        self.push_point(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push_point(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let Some(&(x0, y0)) = self.current.last() else { return };
        let x1 = x1 * self.scale - self.offset_x;
        let y1 = y1 * self.scale - self.offset_y;
        let x2 = x * self.scale - self.offset_x;
        let y2 = y * self.scale - self.offset_y;
        for step in 1..=12 {
            let t = step as f32 / 12.0;
            let mt = 1.0 - t;
            self.current
                .push((mt * mt * x0 + 2.0 * mt * t * x1 + t * t * x2, mt * mt * y0 + 2.0 * mt * t * y1 + t * t * y2));
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let Some(&(x0, y0)) = self.current.last() else { return };
        let x1 = x1 * self.scale - self.offset_x;
        let y1 = y1 * self.scale - self.offset_y;
        let x2 = x2 * self.scale - self.offset_x;
        let y2 = y2 * self.scale - self.offset_y;
        let x3 = x * self.scale - self.offset_x;
        let y3 = y * self.scale - self.offset_y;
        for step in 1..=16 {
            let t = step as f32 / 16.0;
            let mt = 1.0 - t;
            self.current.push((
                mt * mt * mt * x0 + 3.0 * mt * mt * t * x1 + 3.0 * mt * t * t * x2 + t * t * t * x3,
                mt * mt * mt * y0 + 3.0 * mt * mt * t * y1 + 3.0 * mt * t * t * y2 + t * t * t * y3,
            ));
        }
    }

    fn close(&mut self) {
        self.finish_contour();
    }
}

fn point_inside(contours: &[Vec<(f32, f32)>], x: f32, y: f32) -> bool {
    let mut inside = false;
    for contour in contours {
        let mut prev = *contour.last().expect("contour is non-empty");
        for &curr in contour {
            if (curr.1 > y) != (prev.1 > y) && x < (prev.0 - curr.0) * (y - curr.1) / (prev.1 - curr.1) + curr.0 {
                inside = !inside;
            }
            prev = curr;
        }
    }
    inside
}

fn rasterize_outline_glyph(record: &FontRecord, glyph_id: u16, font_size: f32) -> Option<RasterizedGlyph> {
    let data = record.read_data()?;
    let face = ttf_parser::Face::parse(&data, record.collection_index).ok()?;
    let glyph = ttf_parser::GlyphId(glyph_id);
    let bbox = face.glyph_bounding_box(glyph)?;
    let units_per_em = f32::from(face.units_per_em());
    let scale = font_size / units_per_em;
    let offset_x = f32::from(bbox.x_min) * scale;
    let offset_y = f32::from(bbox.y_min) * scale;
    let width = (f32::from(bbox.x_max - bbox.x_min) * scale).ceil().max(1.0) as u32;
    let height = (f32::from(bbox.y_max - bbox.y_min) * scale).ceil().max(1.0) as u32;

    let mut outline = GlyphOutline::new(scale, offset_x, offset_y);
    face.outline_glyph(glyph, &mut outline)?;
    outline.finish_contour();

    let mut bitmap = vec![0u8; (width * height) as usize];
    const SAMPLES: u32 = 4;
    let sample_count = (SAMPLES * SAMPLES) as f32;
    for y in 0..height {
        for x in 0..width {
            let mut covered = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let py = height as f32 - (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32);
                    if point_inside(&outline.contours, px, py) {
                        covered += 1;
                    }
                }
            }
            bitmap[(y * width + x) as usize] = ((covered as f32 / sample_count) * 255.0).round() as u8;
        }
    }

    Some(RasterizedGlyph {
        bitmap,
        width,
        height,
        offset_x,
        offset_y,
        advance_width: advance_width_from_face(&data, record.collection_index, glyph_id, font_size)?,
        is_color: false,
    })
}

fn primary_font_record() -> FontRecord {
    static PRIMARY_FONT_RECORD: OnceLock<FontRecord> = OnceLock::new();
    PRIMARY_FONT_RECORD
        .get_or_init(|| FontRecord::from_static_bytes(0, PRIMARY_FONT).expect("failed to load primary font"))
        .clone()
}

// ---------------------------------------------------------------------------
// Platform-specific system font loading
// ---------------------------------------------------------------------------

/// Try to load system font bytes for a given family name.
/// Returns `None` if the font cannot be found or loading fails.
///
/// On Linux/Windows we use font-kit which enumerates system fonts.
#[cfg(not(any(target_arch = "wasm32", target_os = "ios", target_os = "macos", target_os = "android")))]
fn load_system_font(family: &str) -> Option<Vec<u8>> {
    use font_kit::family_name::FamilyName;
    use font_kit::properties::Properties;
    use font_kit::source::SystemSource;

    let source = SystemSource::new();
    let handle = source
        .select_best_match(&[FamilyName::Title(family.to_string()), FamilyName::SansSerif], &Properties::new())
        .ok()?;
    let font = handle.load().ok()?;
    let data = font.copy_font_data()?;
    Some(data.to_vec())
}

/// macOS / iOS: use Core Text FFI to load a system font by family name.
/// This avoids font-kit's SystemSource::new() which enumerates ALL system
/// fonts, causing high RAM usage and slow startup.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn load_system_font_path(family: &str) -> Option<PathBuf> {
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;

    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontCreateWithName(
            name: core_foundation_sys::string::CFStringRef,
            size: f64,
            matrix: *const std::ffi::c_void,
        ) -> *const std::ffi::c_void;

        fn CTFontCopyAttribute(
            font: *const std::ffi::c_void,
            attribute: core_foundation_sys::string::CFStringRef,
        ) -> *const std::ffi::c_void;

        static kCTFontURLAttribute: core_foundation_sys::string::CFStringRef;

        fn CFRelease(cf: *const std::ffi::c_void);

        fn CFURLGetFileSystemRepresentation(
            url: *const std::ffi::c_void,
            resolve_against_base: bool,
            buffer: *mut u8,
            max_buf_len: isize,
        ) -> bool;
    }

    let cf_name = CFString::new(family);
    unsafe {
        let ct_font = CTFontCreateWithName(cf_name.as_concrete_TypeRef() as _, 12.0, std::ptr::null());
        if ct_font.is_null() {
            return None;
        }

        let url_ref = CTFontCopyAttribute(ct_font, kCTFontURLAttribute);
        CFRelease(ct_font);

        if url_ref.is_null() {
            return None;
        }

        let mut path_buf = [0u8; 1024];
        let ok = CFURLGetFileSystemRepresentation(url_ref, true, path_buf.as_mut_ptr(), 1024);
        CFRelease(url_ref);

        if !ok {
            return None;
        }

        let path_len = path_buf.iter().position(|&b| b == 0).unwrap_or(0);

        let path = std::str::from_utf8(&path_buf[..path_len]).ok()?;
        Some(PathBuf::from(path))
    }
}

/// Android: try to read font files directly from /system/fonts.
#[cfg(target_os = "android")]
fn load_system_font(family: &str) -> Option<Vec<u8>> {
    // Android stores fonts in /system/fonts. Try common CJK/fallback font files.
    let candidates: &[&str] = match family {
        "Noto Sans CJK" => {
            &["/system/fonts/NotoSansCJK-Regular.ttc", "/system/fonts/NotoSansSC-Regular.otf", "/system/fonts/DroidSansFallback.ttf"]
        }
        "Droid Sans Fallback" => &["/system/fonts/DroidSansFallback.ttf"],
        _ => &[],
    };
    for path in candidates {
        if let Ok(data) = std::fs::read(path) {
            return Some(data);
        }
    }
    None
}

/// WASM: no system font access.
#[cfg(target_arch = "wasm32")]
fn load_system_font(_family: &str) -> Option<Vec<u8>> {
    None
}

/// Spec for one fallback family the rasterizer should try to load.
///
/// `probes` are the codepoints we use to verify the family actually covers
/// content we care about (CJK / Hangul / emoji). `hint_color` lets the caller
/// declare a family as a color font *before* we have the parsed face — useful
/// for "AppleColorEmoji" whose probe is itself an emoji codepoint.
#[derive(Clone, Copy)]
struct FallbackSpec {
    family: &'static str,
    probes: &'static [char],
    hint_color: bool,
}

const fn spec(family: &'static str, probes: &'static [char], hint_color: bool) -> FallbackSpec {
    FallbackSpec { family, probes, hint_color }
}

/// Attempt to load a font record from system font data for the given family.
fn try_load_system_font_record(id: FontId, spec: FallbackSpec) -> Option<FontRecord> {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    let font = {
        let path = load_system_font_path(spec.family)?;
        FontRecord::from_path_with_probes(id, path, spec.probes, spec.hint_color)
    };

    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    let font = {
        let data = load_system_font(spec.family)?;
        debug!("Font data len : {}, size : {}", data.len(), size_of_val(&1_u8) * data.len());
        FontRecord::from_bytes_with_probes(id, data, spec.probes, spec.hint_color)
    };
    font
}

/// Build the list of fallback fonts.
///
/// Note: we no longer stop at the first CJK match — emoji fonts cover disjoint
/// Unicode blocks from CJK fonts, so the chain may legitimately contain both
/// (and a Hangul / Latin extender). We dedupe by absolute file path.
fn build_fallback_chain(next_id: FontId) -> Vec<FontRecord> {
    const CJK_PROBES: &[char] = &['你'];
    const HANGUL_PROBES: &[char] = &['가'];
    const EMOJI_PROBES: &[char] = &['😀', '👍'];

    // Platform-appropriate fallback specs. Order is preference order: the first
    // family that supports a given codepoint wins in `font_and_glyph_for_codepoint`.
    let system_specs: &[FallbackSpec] = if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        &[
            spec("AppleColorEmoji", EMOJI_PROBES, true),
            spec("HiraginoSansGB-W3", CJK_PROBES, false),
            spec("HiraginoSans-W3", CJK_PROBES, false),
            spec("PingFangSC-Regular", CJK_PROBES, false),
            spec("AppleSDGothicNeo-Regular", HANGUL_PROBES, false),
            spec("ArialUnicodeMS", CJK_PROBES, false),
            spec("NotoSansSC-Regular", CJK_PROBES, false),
        ]
    } else if cfg!(target_os = "windows") {
        &[
            spec("Segoe UI Emoji", EMOJI_PROBES, true),
            spec("Microsoft YaHei", CJK_PROBES, false),
            spec("Malgun Gothic", HANGUL_PROBES, false),
            spec("Yu Gothic", CJK_PROBES, false),
            spec("MS Gothic", CJK_PROBES, false),
        ]
    } else if cfg!(target_os = "android") {
        &[
            spec("Noto Color Emoji", EMOJI_PROBES, true),
            spec("Noto Sans CJK", CJK_PROBES, false),
            spec("Droid Sans Fallback", CJK_PROBES, false),
        ]
    } else {
        // Linux and others
        &[
            spec("Noto Color Emoji", EMOJI_PROBES, true),
            spec("Noto Sans CJK SC", CJK_PROBES, false),
            spec("Noto Sans CJK", CJK_PROBES, false),
            spec("WenQuanYi Micro Hei", CJK_PROBES, false),
        ]
    };

    let mut fallbacks: Vec<FontRecord> = Vec::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();

    for fb_spec in system_specs {
        let id = next_id + fallbacks.len() as FontId;
        let Some(font) = try_load_system_font_record(id, *fb_spec) else { continue };

        let Some(path) = font.path.as_deref() else { continue };

        if !seen_paths.insert(path.clone()) {
            debug!("Skipping duplicate fallback for '{}' at {:?}", fb_spec.family, path);
            continue;
        }

        fallbacks.push(font);
        // No early `break;` — keep collecting CJK + emoji so both can render.
    }

    fallbacks
}

fn shared_fallback_chain() -> Vec<FontRecord> {
    static FALLBACKS: OnceLock<Vec<FontRecord>> = OnceLock::new();
    FALLBACKS.get_or_init(|| build_fallback_chain(1)).clone()
}

/// Pre-build the fallback chain and validate each fallback face with
/// `ttf-parser`, avoiding the ~100-400 ms `fontdue::Font::from_bytes` cost for
/// PingFang / Hiragino during warmup. Safe to call from any thread; the inner
/// `OnceLock` is also used by `GlyphRasterizer::ensure_fallbacks`.
///
/// `fontdue` is now constructed lazily only when a grayscale fallback glyph is
/// actually rasterized; color emoji continue to use the `ttf-parser` sbix path.
pub fn warm_fallbacks() {
    let start = chrono::Utc::now().timestamp_millis();
    let chain = shared_fallback_chain();
    for record in &chain {
        let _ = record.ensure_face();
    }
    let end = chrono::Utc::now().timestamp_millis();
    info!("warm_fallbacks() took {} ms", end - start);
}

// ---------------------------------------------------------------------------
// Color glyph rasterization (sbix PNG strikes)
// ---------------------------------------------------------------------------

/// Rasterize a color glyph from an `sbix` strike.
///
/// We parse the face from `record`'s in-memory bytes or its mmapped path, ask
/// `ttf-parser` for the largest available raster image, decode the PNG, and
/// downsample to the requested `font_size` resolution. The returned bitmap is
/// non-premultiplied RGBA8 (`width * height * 4` bytes); the dedicated color
/// shader handles alpha multiplication and clipping.
fn rasterize_color_glyph(record: &FontRecord, glyph_id: u16, font_size: f32) -> Option<RasterizedGlyph> {
    let data = record.read_data()?;
    let face = ttf_parser::Face::parse(&data, record.collection_index).ok()?;

    // Pass `u16::MAX` to request the largest available strike. We always
    // downsample (rather than picking the closest size) so the color atlas
    // doesn't accumulate near-duplicate entries for one emoji at slightly
    // different point sizes.
    let raster = face.glyph_raster_image(ttf_parser::GlyphId(glyph_id), u16::MAX)?;
    if !matches!(raster.format, ttf_parser::RasterImageFormat::PNG) {
        // We only handle PNG strikes today; JPEG / TIFF / dupe redirects are
        // ignored. ttf-parser already follows `dupe` links transparently.
        return None;
    }

    let decoded = image::load_from_memory_with_format(raster.data, image::ImageFormat::Png).ok()?;
    let rgba = decoded.to_rgba8();
    let strike_w = rgba.width();
    let strike_h = rgba.height();
    if strike_w == 0 || strike_h == 0 {
        return None;
    }

    // Convert "pixels at strike" to "pixels at requested font size".
    let strike_ppem = raster.pixels_per_em.max(1) as f32;
    let scale = font_size / strike_ppem;

    let render_w = ((strike_w as f32) * scale).round().max(1.0) as u32;
    let render_h = ((strike_h as f32) * scale).round().max(1.0) as u32;

    let resampled = if render_w == strike_w && render_h == strike_h {
        rgba
    } else {
        // Triangle (linear) is the right choice for emoji bitmaps: cheap,
        // visually clean, and avoids the ringing of Lanczos on alpha-bearing
        // sources.
        image::imageops::resize(&rgba, render_w, render_h, image::imageops::FilterType::Triangle)
    };

    // Advance from the same `hmtx` table the outline path uses; in font units
    // → scaled to font_size pixels.
    let units_per_em = f32::from(face.units_per_em());
    let advance_units = f32::from(face.glyph_hor_advance(ttf_parser::GlyphId(glyph_id))?);
    let advance_width = advance_units * font_size / units_per_em;

    // sbix `x`/`y` are pixel offsets at the strike's `pixels_per_em`:
    //   x: from cursor to bitmap's left edge   (positive → bitmap shifted right)
    //   y: from baseline to bitmap's bottom    (positive → bitmap above baseline, y-up)
    // Both map directly to our `RasterizedGlyph::offset_x` / `offset_y`
    // conventions (the same y-up convention as `fontdue::Metrics::ymin`).
    let offset_x = f32::from(raster.x) * scale;
    let offset_y = f32::from(raster.y) * scale;

    Some(RasterizedGlyph {
        bitmap: resampled.into_raw(),
        width: render_w,
        height: render_h,
        offset_x,
        offset_y,
        advance_width,
        is_color: true,
    })
}

pub struct GlyphRasterizer {
    /// Primary font (Roboto) for Latin/common glyphs.
    primary: FontRecord,
    /// Fallback fonts for extended Unicode coverage (CJK, etc.).
    /// Loaded lazily on first encounter of a glyph not in the primary font,
    /// to avoid the massive memory cost (~800MB) of parsing large CJK fonts
    /// when only ASCII text is rendered.
    fallbacks: Option<Vec<FontRecord>>,
    /// Whether to attempt loading fallbacks when needed.
    enable_fallbacks: bool,
    cache: HashMap<GlyphKey, RasterizedGlyph>,
    advance_cache: HashMap<GlyphKey, f32>,
    unsupported_codepoints: HashSet<char>,
}

impl GlyphRasterizer {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let primary = primary_font_record();

        Self {
            primary,
            fallbacks: None, // loaded lazily on first miss
            enable_fallbacks: true,
            cache: HashMap::new(),
            advance_cache: HashMap::new(),
            unsupported_codepoints: HashSet::new(),
        }
    }

    /// Create a lightweight rasterizer with only the primary font (no fallbacks).
    /// Suitable for text measurement where CJK rendering is not needed.
    pub fn primary_only() -> Self {
        let primary = primary_font_record();
        Self {
            primary,
            fallbacks: None,
            enable_fallbacks: false,
            cache: HashMap::new(),
            advance_cache: HashMap::new(),
            unsupported_codepoints: HashSet::new(),
        }
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

    pub fn glyph_key_for_codepoint(&mut self, codepoint: char, font_size: f32) -> GlyphKey {
        let primary = self.primary.font.as_ref().expect("primary font is loaded");
        if !primary.has_glyph(codepoint) && !self.unsupported_codepoints.contains(&codepoint) {
            self.ensure_fallbacks();
        }

        let (font_id, glyph_id, supported) = self.font_and_glyph_for_codepoint(codepoint);
        if !supported {
            self.unsupported_codepoints.insert(codepoint);
        }
        GlyphKey::new(font_id, glyph_id, font_size)
    }

    pub fn font_id_for_codepoint(&mut self, codepoint: char) -> FontId {
        let primary = self.primary.font.as_ref().expect("primary font is loaded");
        if !primary.has_glyph(codepoint) && !self.unsupported_codepoints.contains(&codepoint) {
            self.ensure_fallbacks();
        }

        let (font_id, _, supported) = self.font_and_glyph_for_codepoint(codepoint);
        if !supported {
            self.unsupported_codepoints.insert(codepoint);
        }
        font_id
    }

    fn font_and_glyph_for_codepoint(&self, codepoint: char) -> (FontId, u16, bool) {
        let primary = self.primary.font.as_ref().expect("primary font is loaded");
        if primary.has_glyph(codepoint) {
            (self.primary.id, primary.lookup_glyph_index(codepoint), true)
        } else {
            let fallback = self.fallbacks.as_ref().and_then(|fbs| {
                fbs.iter()
                    .find_map(|fb| fb.glyph_index(codepoint).map(|glyph_id| (fb.id, glyph_id)))
            });
            if let Some(font) = fallback { (font.0, font.1, true) } else { (self.primary.id, primary.lookup_glyph_index(codepoint), false) }
        }
    }

    fn select_font_for_key(&mut self, key: GlyphKey) -> &mut FontRecord {
        if key.font_id == self.primary.id {
            &mut self.primary
        } else {
            self.fallbacks
                .as_mut()
                .and_then(|fbs| fbs.iter_mut().find(|fb| fb.id == key.font_id))
                .unwrap_or(&mut self.primary)
        }
    }

    /// Rasterize a single glyph at the given size, returning cached result if available.
    pub fn rasterize(&mut self, codepoint: char, font_size: f32) -> &RasterizedGlyph {
        let key = self.glyph_key_for_codepoint(codepoint, font_size);

        self.rasterize_key(key, font_size)
    }

    pub fn rasterize_key(&mut self, key: GlyphKey, font_size: f32) -> &RasterizedGlyph {
        // Check if we need to load fallbacks for this glyph.
        if !self.cache.contains_key(&key) && key.font_id != self.primary.id {
            self.ensure_fallbacks();
        }
        if !self.cache.contains_key(&key) {
            let is_color = self.select_font_for_key(key).is_color;

            let glyph = if is_color {
                let record_snapshot = self.select_font_for_key(key).clone();
                rasterize_color_glyph(&record_snapshot, key.glyph_id, font_size).unwrap_or_else(|| RasterizedGlyph {
                    bitmap: Vec::new(),
                    width: 0,
                    height: 0,
                    offset_x: 0.0,
                    offset_y: 0.0,
                    advance_width: font_size * 0.5,
                    is_color: true,
                })
            } else if key.font_id == self.primary.id {
                let font = self
                    .select_font_for_key(key)
                    .ensure_font()
                    .expect("selected font should load");
                let (metrics, bitmap) = font.rasterize_indexed(key.glyph_id, font_size);
                RasterizedGlyph {
                    bitmap,
                    width: metrics.width as u32,
                    height: metrics.height as u32,
                    offset_x: metrics.xmin as f32,
                    offset_y: metrics.ymin as f32,
                    advance_width: metrics.advance_width,
                    is_color: false,
                }
            } else {
                let record_snapshot = self.select_font_for_key(key).clone();
                rasterize_outline_glyph(&record_snapshot, key.glyph_id, font_size).unwrap_or_else(|| RasterizedGlyph {
                    bitmap: Vec::new(),
                    width: 0,
                    height: 0,
                    offset_x: 0.0,
                    offset_y: 0.0,
                    advance_width: record_snapshot
                        .advance_width_for_glyph(key.glyph_id, font_size)
                        .unwrap_or(0.0),
                    is_color: false,
                })
            };

            self.advance_cache.insert(key, glyph.advance_width);
            self.cache.insert(key, glyph);
        }

        self.cache.get(&key).expect("glyph was just inserted")
    }

    pub fn glyph_metrics_for_key(&mut self, key: GlyphKey, font_size: f32) -> RasterizedGlyph {
        self.rasterize_key(key, font_size).clone()
    }

    pub fn preload_text(&mut self, text: &str, font_size: f32) -> Vec<(GlyphKey, RasterizedGlyph)> {
        let mut glyphs = Vec::new();
        for c in text.chars() {
            if c.is_control() {
                continue;
            }

            let key = self.glyph_key_for_codepoint(c, font_size);
            let glyph = self.rasterize_key(key, font_size).clone();
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

    /// Returns line metrics (ascent, descent, line_gap) for the given font size.
    /// Uses the primary font for consistent line spacing.
    pub fn line_metrics(&self, font_size: f32) -> (f32, f32, f32) {
        let m = self
            .primary
            .font
            .as_ref()
            .expect("primary font is loaded")
            .horizontal_line_metrics(font_size)
            .unwrap_or(fontdue::LineMetrics {
                ascent: font_size * 0.8,
                descent: font_size * -0.2,
                line_gap: 0.0,
                new_line_size: font_size,
            });
        (m.ascent, m.descent, m.line_gap)
    }

    /// Convenience: measure the advance width of a string.
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> f32 {
        text.chars().map(|c| self.advance_width(c, font_size)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn primary_font_is_shared_between_rasterizers() {
        let first = GlyphRasterizer::new();
        let second = GlyphRasterizer::primary_only();

        assert!(Arc::ptr_eq(
            first.primary.font.as_ref().expect("primary font missing"),
            second.primary.font.as_ref().expect("primary font missing")
        ));
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

    #[test]
    fn cjk_lookup_does_not_eagerly_construct_fallback_font() {
        let mut rasterizer = GlyphRasterizer::new();

        let key = rasterizer.glyph_key_for_codepoint('你', 16.0);

        let fallbacks = rasterizer
            .fallbacks
            .as_ref()
            .expect("fallbacks should be discovered");
        let fallback = fallbacks
            .iter()
            .find(|font| font.id == key.font_id)
            .expect("selected fallback missing");
        assert!(fallback.font.is_none(), "fallback font should stay unloaded until glyph metrics/bitmap are demanded");
    }

    #[test]
    fn cjk_glyphs_use_renderable_fallback_font() {
        let mut rasterizer = GlyphRasterizer::new();

        for c in "你哈皮".chars() {
            let key = rasterizer.glyph_key_for_codepoint(c, 16.0);
            assert_ne!(key.font_id, rasterizer.primary_font_id(), "{c} should use a fallback font");

            let glyph = rasterizer.glyph_metrics_for_key(key, 16.0);
            assert!(glyph.width > 0, "{c} fallback glyph should have bitmap width");
            assert!(glyph.height > 0, "{c} fallback glyph should have bitmap height");
            assert!(!glyph.bitmap.is_empty(), "{c} fallback glyph should have bitmap data");
            assert!(!glyph.is_color, "{c} should be a monochrome glyph");
        }
    }

    /// macOS ships AppleColorEmoji at /System/Library/Fonts/AppleColorEmoji.ttc.
    /// On a system without that font (or in CI containers), the chain just won't
    /// contain it; the test stays informative either way by asserting *if* the
    /// font was loaded, the record is correctly tagged as color.
    #[cfg(target_os = "macos")]
    #[test]
    fn fallback_chain_keeps_both_emoji_and_cjk() {
        let chain = shared_fallback_chain();

        let has_emoji = chain.iter().any(|fb| fb.is_color);
        let has_cjk = chain.iter().any(|fb| !fb.is_color);

        // We don't hard-fail when the system lacks AppleColorEmoji — just log.
        if !has_emoji {
            eprintln!("[note] no color font loaded — AppleColorEmoji missing from this macOS build");
        }
        // CJK *should* be present on any modern macOS install.
        assert!(has_cjk, "no CJK fallback was loaded — chain: {} entries", chain.len());

        // Crucially, when both are present the chain must hold both — the old
        // `break;` would have stopped at the first hit. This is the regression
        // guard for fix C.
        if has_emoji && has_cjk {
            assert!(chain.len() >= 2, "chain truncated: emoji + CJK should coexist");
        }
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
        assert!(glyph.width > 0 && glyph.height > 0, "'😀' bitmap dimensions must be non-zero");
        // RGBA8 → 4 bytes per pixel. The bitmap may be empty if the sbix
        // strike was missing/unsupported (we'd hit the placeholder branch in
        // rasterize_key), so guard the size check on the bitmap being present.
        if !glyph.bitmap.is_empty() {
            assert_eq!(glyph.bitmap.len(), (glyph.width * glyph.height * 4) as usize, "'😀' bitmap must be RGBA8 (4 bytes per pixel)");
        }
    }
}
