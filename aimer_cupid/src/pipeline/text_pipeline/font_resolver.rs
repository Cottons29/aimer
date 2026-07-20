use std::collections::HashSet;
use std::path::PathBuf;
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
use std::sync::LazyLock;
use std::sync::{Arc, OnceLock};

use aimer_utils::info;
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
use fontdb::Database as FontDatabase;

use crate::text_layout::FontId;

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
static FONT_DB: LazyLock<FontDatabase> = LazyLock::new(|| {
    let mut db = FontDatabase::new();
    // `load_system_fonts` scans every installed font, which is expensive on
    // startup.  Apple platforms use CoreText per-script fallback resolution
    // instead; other desktop platforms keep the fontdb system scan for now.
    // On WASM there is no filesystem, so we leave the database empty (only the
    // embedded primary font / Roboto is available on that platform).
    #[cfg(not(target_arch = "wasm32"))]
    db.load_system_fonts();

    db
});

#[derive(Clone)]
pub struct FontRecord {
    pub id: FontId,
    pub bytes: Option<Arc<[u8]>>,
    pub font: Option<Arc<fontdue::Font>>,
    pub(crate) byte_len: Option<u64>,
    pub(crate) collection_index: u32,
    pub(crate) _path: Option<Arc<PathBuf>>,
    /// True when the font carries color glyph data (`sbix` / `CBDT` / `COLR`)
    /// and should be rasterized via color-glyph tables instead of `fontdue`.
    pub is_color: bool,
}

pub(crate) enum FontData {
    Shared(Arc<[u8]>),
    #[cfg(not(target_arch = "wasm32"))]
    Mapped(memmap2::Mmap),
}

impl AsRef<[u8]> for FontData {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Shared(bytes) => bytes,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Mapped(bytes) => bytes,
        }
    }
}

impl FontRecord {
    const FONTDUE_MAX_BYTES: u64 = 8 * 1024 * 1024;

    pub(crate) fn from_static_bytes(id: FontId, bytes: &'static [u8]) -> Option<Self> {
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()?;
        Some(Self {
            id,
            bytes: Some(Arc::from(bytes)),
            font: Some(Arc::new(font)),
            byte_len: Some(bytes.len() as u64),
            collection_index: 0,
            _path: None,
            is_color: false,
        })
    }

    pub fn from_bytes(id: FontId, bytes: Vec<u8>) -> Option<Self> {
        let face = ttf_parser::Face::parse(&bytes, 0).ok()?;
        let is_color = Self::face_is_color(&face);
        let font = if !is_color && bytes.len() as u64 <= Self::FONTDUE_MAX_BYTES {
            fontdue::Font::from_bytes(bytes.as_slice(), fontdue::FontSettings::default())
                .ok()
                .map(Arc::new)
        } else {
            None
        };

        let byte_len = bytes.len() as u64;
        Some(Self {
            id,
            bytes: Some(Arc::from(bytes)),
            font,
            byte_len: Some(byte_len),
            collection_index: 0,
            _path: None,
            is_color,
        })
    }

    pub(crate) fn from_shared_bytes(id: FontId, bytes: Arc<[u8]>) -> Option<Self> {
        let face = ttf_parser::Face::parse(bytes.as_ref(), 0).ok()?;
        let is_color = Self::face_is_color(&face);
        let font = if !is_color && bytes.len() as u64 <= Self::FONTDUE_MAX_BYTES {
            fontdue::Font::from_bytes(bytes.as_ref(), fontdue::FontSettings::default())
                .ok()
                .map(Arc::new)
        } else {
            None
        };
        let byte_len = bytes.len() as u64;

        Some(Self {
            id,
            bytes: Some(bytes),
            font,
            byte_len: Some(byte_len),
            collection_index: 0,
            _path: None,
            is_color,
        })
    }

    pub(crate) fn should_use_fontdue(&self) -> bool {
        #[cfg(any(target_os = "ios", target_os = "macos"))]
        if self._path.is_some() {
            return false;
        }

        !self.is_color
            && self
                .byte_len
                .unwrap_or(Self::FONTDUE_MAX_BYTES + 1)
                <= Self::FONTDUE_MAX_BYTES
    }

    /// Returns true if this collection_index of `data` contains any color glyph
    /// table that we know how to render (`sbix`, `CBDT`/`CBLC`, or
    /// `COLR`/`CPAL`).
    #[allow(dead_code)]
    fn face_is_color(face: &ttf_parser::Face<'_>) -> bool {
        let tables = face.tables();
        // sbix  — AppleColorEmoji (macOS/iOS)
        // cbdt  — Noto Color Emoji (Android/Linux, older builds)
        // colr  — Windows/Linux Segoe/Twemoji v1 layered outlines
        tables
            .sbix
            .is_some()
            || tables
                .cbdt
                .is_some()
            || tables
                .colr
                .is_some()
    }

    /// Probe the font with each `probes` codepoint; accept on the first match.
    /// `accept_color` allows color fonts to be admitted to the chain even when
    /// none of the probes are present (the typical case for emoji fonts whose
    /// cmap maps emoji codepoints — which is what callers should pass here, but
    /// we keep the option to make tests easier).
    fn probes_match(face: &ttf_parser::Face<'_>, probes: &[char]) -> bool {
        probes
            .iter()
            .any(|&c| {
                face.glyph_index(c)
                    .is_some()
            })
    }

    /// Retain shared in-memory data or memory-map a file-backed font without
    /// copying the entire font into the process heap.
    pub(crate) fn data(&self) -> Option<FontData> {
        if let Some(bytes) = self.bytes.as_ref() {
            return Some(FontData::Shared(bytes.clone()));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = self
                ._path
                .as_ref()?;
            let file = std::fs::File::open(path.as_ref()).ok()?;
            // SAFETY: the read-only mapping owns its file-backed virtual memory
            // region and remains valid independently of the `File` handle.
            unsafe {
                memmap2::Mmap::map(&file)
                    .ok()
                    .map(FontData::Mapped)
            }
        }
        #[cfg(target_arch = "wasm32")]
        None
    }

    #[allow(dead_code)]
    pub(crate) fn ensure_face(&self) -> Option<()> {
        if let Some(bytes) = self.bytes.as_ref() {
            ttf_parser::Face::parse(bytes.as_ref(), self.collection_index).ok()?;
            return Some(());
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = self
                ._path
                .as_ref()?;
            let file = std::fs::File::open(path.as_ref()).ok()?;
            let map = unsafe { memmap2::Mmap::map(&file).ok()? };
            ttf_parser::Face::parse(&map, self.collection_index).ok()?;
            Some(())
        }
        #[cfg(target_arch = "wasm32")]
        None
    }

    pub(crate) fn ensure_font(&mut self) -> Option<Arc<fontdue::Font>> {
        if !self.should_use_fontdue() {
            return None;
        }

        if self.font.is_none() {
            let settings = fontdue::FontSettings {
                collection_index: self.collection_index,
                ..fontdue::FontSettings::default()
            };
            let font = if let Some(bytes) = self.bytes.as_ref() {
                fontdue::Font::from_bytes(bytes.as_ref(), settings).ok()?
            } else {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let path = self
                        ._path
                        .as_ref()?;
                    let data = std::fs::read(path.as_ref()).ok()?;
                    fontdue::Font::from_bytes(data, settings).ok()?
                }
                #[cfg(target_arch = "wasm32")]
                return None;
            };
            self.font = Some(Arc::new(font));
        }

        self.font.clone()
    }

    pub(crate) fn glyph_index(&self, codepoint: char) -> Option<u16> {
        if let Some(font) = self.font.as_ref() {
            return font
                .has_glyph(codepoint)
                .then(|| font.lookup_glyph_index(codepoint));
        }

        if let Some(bytes) = self.bytes.as_ref() {
            let face = ttf_parser::Face::parse(bytes.as_ref(), self.collection_index).ok()?;
            return face
                .glyph_index(codepoint)
                .map(|id| id.0);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = self
                ._path
                .as_ref()?;
            let file = std::fs::File::open(path.as_ref()).ok()?;
            let map = unsafe { memmap2::Mmap::map(&file).ok()? };
            let face = ttf_parser::Face::parse(&map, self.collection_index).ok()?;
            face.glyph_index(codepoint)
                .map(|id| id.0)
        }
        #[cfg(target_arch = "wasm32")]
        None
    }

    pub(crate) fn advance_width_for_glyph(&self, glyph_id: u16, font_size: f32) -> Option<f32> {
        if let Some(font) = self.font.as_ref() {
            return Some(
                font.metrics_indexed(glyph_id, font_size)
                    .advance_width,
            );
        }

        if let Some(bytes) = self.bytes.as_ref() {
            return advance_width_from_face(
                bytes.as_ref(),
                self.collection_index,
                glyph_id,
                font_size,
            );
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let path = self
                ._path
                .as_ref()?;
            let file = std::fs::File::open(path.as_ref()).ok()?;
            let map = unsafe { memmap2::Mmap::map(&file).ok()? };
            advance_width_from_face(&map, self.collection_index, glyph_id, font_size)
        }
        #[cfg(target_arch = "wasm32")]
        None
    }
}

pub fn advance_width_from_face(
    bytes: &[u8],
    collection_index: u32,
    glyph_id: u16,
    font_size: f32,
) -> Option<f32> {
    let face = ttf_parser::Face::parse(bytes, collection_index).ok()?;
    let units_per_em = f32::from(face.units_per_em());
    let advance = f32::from(face.glyph_hor_advance(ttf_parser::GlyphId(glyph_id))?);
    Some(advance * font_size / units_per_em)
}

/// A probe group: one script / category with the codepoints used to verify
/// that a font actually covers it.  `hint_color` marks probe groups that
/// identify color-emoji fonts so we can set `is_color` even before decoding.
#[allow(dead_code)]
struct ProbeGroup {
    label: &'static str,
    probes: &'static [char],
    hint_color: bool,
}

const fn probe_group(label: &'static str, probes: &'static [char], hint_color: bool) -> ProbeGroup {
    ProbeGroup { label, probes, hint_color }
}

/// All script / category probe groups we want covered in the fallback chain.
/// Order is significant: earlier groups are preferred over later ones when a
/// single font file covers multiple scripts (the first probe group that matches
/// controls whether the font is added to the chain for that group).
static PROBE_GROUPS: &[ProbeGroup] = &[
    probe_group("emoji", &['😀', '👍'], true),
    probe_group("cjk", &['你', '漢', '한'], false),
    probe_group("hangul", &['가', '나', '다'], false),
    probe_group("arabic", &['\u{0639}', '\u{0627}'], false), // ع ا
    probe_group("hebrew", &['\u{05D0}', '\u{05D1}'], false), // א ב
    probe_group("devanagari", &['\u{0915}', '\u{0930}'], false), // क र
    probe_group("tamil", &['\u{0B95}', '\u{0BB5}'], false),  // க வ
    probe_group("thai", &['\u{0E01}', '\u{0E02}'], false),   // ก ข
    probe_group("armenian", &['\u{0531}', '\u{0532}'], false), // Ա Բ
    probe_group("georgian", &['\u{10D0}', '\u{10D1}'], false), // ა ბ
    probe_group("ethiopic", &['\u{1200}', '\u{1201}'], false), // ሀ ሁ
    probe_group("myanmar", &['\u{1000}', '\u{1001}'], false), // က ခ
    probe_group("khmer", &['\u{1780}', '\u{1781}'], false),  // ក ខ
    probe_group("tibetan", &['\u{0F00}'], false),            // ༀ
    probe_group("sinhala", &['\u{0D9A}'], false),            // ක
    probe_group("telugu", &['\u{0C15}'], false),             // క
    probe_group("kannada", &['\u{0C95}'], false),            // ಕ
    probe_group("malayalam", &['\u{0D15}'], false),          // ക
    probe_group("gujarati", &['\u{0A95}'], false),           // ક
    probe_group("gurmukhi", &['\u{0A15}'], false),           // ਕ
    probe_group("bengali", &['\u{0995}'], false),            // ক
    probe_group("oriya", &['\u{0B15}'], false),              // କ
    probe_group("lao", &['\u{0E81}'], false),                // ກ
    probe_group("mongolian", &['\u{1820}'], false),          // ᠠ
    probe_group("cherokee", &['\u{13A0}'], false),           // Ꭰ
    probe_group("yi", &['\u{A000}'], false),                 // ꀀ
];

/// Check whether font data (passed as a byte slice) satisfies `probes`.
/// Returns `Some(is_color)` on success, or `None` if the font doesn't match.
///
/// For color/emoji probe groups (`hint_color=true`) we additionally require
/// that the font has an `sbix` or `cbdt` table (real bitmap strikes) — a
/// COLR-only table is not enough, since placeholder/fallback fonts like
/// LastResort.otf also carry COLR but contain no usable emoji bitmaps.
///
/// For regular (non-color) probe groups we additionally verify that at least
/// one probe glyph has a non-empty bounding box, which filters out fonts that
/// declare a cmap entry for a codepoint but store the glyph as a composite with
/// no direct outline (e.g., some older pan-Unicode fonts for certain CJK
/// ranges).
fn font_data_matches_probes(
    data: &[u8],
    ci: u32,
    probes: &[char],
    hint_color: bool,
) -> Option<bool> {
    let face = ttf_parser::Face::parse(data, ci).ok()?;
    if !FontRecord::probes_match(&face, probes) {
        return None;
    }

    if hint_color {
        // For emoji probe groups we require real bitmap strike tables (sbix or
        // cbdt).  Fonts that only carry COLR (e.g. LastResort.otf, most text
        // fonts with COLR decorative glyphs) are not usable as emoji fonts here
        // because our `rasterize_color_glyph` path prefers sbix/cbdt.
        let tables = face.tables();
        if tables
            .sbix
            .is_none()
            && tables
                .cbdt
                .is_none()
        {
            return None;
        }
        return Some(true); // confirmed bitmap-color emoji font
    }

    // For non-color probe groups: require that the probe glyphs map to at least
    // two *distinct* non-zero glyph IDs.  Pan-Unicode placeholder fonts like
    // LastResort.otf map every codepoint to the same single "missing character"
    // box (always glyph ID 4 in that font), so they pass a naïve bounding-box
    // check but do not contain real script outlines.  If all probes resolve to
    // the same glyph, we know the font is a placeholder and reject it.
    let glyph_ids: HashSet<u16> = probes
        .iter()
        .filter_map(|&c| face.glyph_index(c))
        .map(|id| id.0)
        .filter(|&id| id != 0) // 0 == .notdef, not meaningful
        .collect();

    // Need at least 2 distinct non-zero glyph IDs among the probes.  Single-probe
    // groups (like tibetan, sinhala, etc.) get a pass on the distinctness check —
    // we just verify the glyph has a bounding box instead.
    if probes.len() >= 2 && glyph_ids.len() < 2 {
        // All probes mapped to the same glyph — very likely a placeholder font.
        return None;
    }

    // Additionally verify at least one probe glyph has a non-empty bounding box
    // so we know the font can actually produce visible outlines for it.
    let has_usable_outline = probes
        .iter()
        .any(|&c| {
            face.glyph_index(c)
                .and_then(|id| face.glyph_bounding_box(id))
                .is_some()
        });
    if !has_usable_outline {
        return None;
    }

    Some(false) // non-color font confirmed usable
}

/// Build the fallback chain dynamically by scanning every font known to
/// `FONT_DB` (which is pre-populated with the system font sources appropriate
/// for each platform).
///
/// For each `ProbeGroup` we walk all font faces in order and add the first
/// face that satisfies the group's probes.  A font face is never added twice
/// (deduped by a stable key).  The result is a flat `Vec<FontRecord>` ordered
/// by probe group priority.
///
/// This function uses `db.with_face_data()` to access the raw bytes of each
/// candidate face so it works uniformly for both on-disk (file-backed) and
/// in-memory (WASM-embedded or iOS binary-blob) sources.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn core_text_fallback_path_for_probes(probes: &[char]) -> Option<PathBuf> {
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::CFRange;

    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontCreateWithName(
            name: core_foundation_sys::string::CFStringRef,
            size: f64,
            matrix: *const std::ffi::c_void,
        ) -> *const std::ffi::c_void;

        fn CTFontCreateForString(
            current_font: *const std::ffi::c_void,
            string: core_foundation_sys::string::CFStringRef,
            range: CFRange,
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

    let base_name = CFString::new(".AppleSystemUIFont");
    let sample: String = probes
        .iter()
        .collect();
    let sample_len = sample
        .encode_utf16()
        .count() as isize;
    if sample_len == 0 {
        return None;
    }
    let sample = CFString::new(&sample);

    unsafe {
        let base_font =
            CTFontCreateWithName(base_name.as_concrete_TypeRef() as _, 12.0, std::ptr::null());
        if base_font.is_null() {
            return None;
        }

        let fallback_font = CTFontCreateForString(
            base_font,
            sample.as_concrete_TypeRef() as _,
            CFRange { location: 0, length: sample_len },
        );
        CFRelease(base_font);

        if fallback_font.is_null() {
            return None;
        }

        let url_ref = CTFontCopyAttribute(fallback_font, kCTFontURLAttribute);
        CFRelease(fallback_font);

        if url_ref.is_null() {
            return None;
        }

        let mut path_buf = [0u8; 1024];
        let ok = CFURLGetFileSystemRepresentation(
            url_ref,
            true,
            path_buf.as_mut_ptr(),
            path_buf.len() as isize,
        );
        CFRelease(url_ref);
        if !ok {
            return None;
        }

        let path_len = path_buf
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(0);
        let path = std::str::from_utf8(&path_buf[..path_len]).ok()?;
        Some(PathBuf::from(path))
    }
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn build_fallback_chain(next_id: FontId) -> Vec<FontRecord> {
    let mut fallbacks: Vec<FontRecord> = Vec::new();
    let mut seen: HashSet<(PathBuf, u32)> = HashSet::new();

    for group in PROBE_GROUPS {
        let Some(path) = core_text_fallback_path_for_probes(group.probes) else {
            continue;
        };
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(_) => continue,
        };
        // SAFETY: this read-only mapping is used only while `file` and `data`
        // are alive in the current probe iteration.
        let data = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(data) => data,
            Err(_) => continue,
        };

        let face_count = ttf_parser::fonts_in_collection(data.as_ref())
            .unwrap_or(1)
            .max(1);
        for ci in 0..face_count {
            if seen.contains(&(path.clone(), ci)) {
                continue;
            }

            let Some(is_color) =
                font_data_matches_probes(data.as_ref(), ci, group.probes, group.hint_color)
            else {
                continue;
            };

            let id = next_id + fallbacks.len() as FontId;
            let byte_len = std::fs::metadata(&path)
                .ok()
                .map(|m| m.len())
                .or(Some(data.len() as u64));
            fallbacks.push(FontRecord {
                id,
                bytes: None,
                font: None,
                byte_len,
                collection_index: ci,
                _path: Some(Arc::new(path.clone())),
                is_color,
            });
            seen.insert((path.clone(), ci));
            break;
        }
    }

    fallbacks
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn build_fallback_chain(next_id: FontId) -> Vec<FontRecord> {
    let db = &*FONT_DB;
    let mut fallbacks: Vec<FontRecord> = Vec::new();
    // Dedup key: for file sources use (path, ci), for binary sources use (id).
    let mut seen_ids: HashSet<fontdb::ID> = HashSet::new();

    for group in PROBE_GROUPS {
        // Walk all faces registered in fontdb for this probe group.
        'face_loop: for face_info in db.faces() {
            let face_id = face_info.id;
            let ci = face_info.index;

            if seen_ids.contains(&face_id) {
                continue;
            }

            // Use `with_face_data` so we never manually open files — fontdb
            // already handles file mapping, binary sources, etc.  This makes
            // the code safe on WASM (no `std::fs`) and on iOS (sandboxed FS).
            let result = db.with_face_data(face_id, |data, _ci| {
                font_data_matches_probes(data, ci, group.probes, group.hint_color)
            });

            let Some(Some(is_color)) = result else {
                continue;
            };

            // Build a FontRecord.  Prefer storing the path (avoids keeping all
            // font bytes in RAM) but fall back to in-memory bytes for sources
            // that don't have a backing file (e.g. WASM embedded binary blobs).
            let (record_bytes, record_path, byte_len) = match &face_info.source {
                fontdb::Source::File(p) => (
                    None,
                    Some(Arc::new(p.clone())),
                    std::fs::metadata(p)
                        .ok()
                        .map(|m| m.len()),
                ),
                fontdb::Source::SharedFile(p, _) => {
                    let path = p
                        .as_path()
                        .to_path_buf();
                    let byte_len = std::fs::metadata(&path)
                        .ok()
                        .map(|m| m.len());
                    (None, Some(Arc::new(path)), byte_len)
                }
                fontdb::Source::Binary(arc) => {
                    // Keep the bytes in memory so the record is self-contained.
                    let bytes: Arc<[u8]> = Arc::from(
                        arc.as_ref()
                            .as_ref(),
                    );
                    let byte_len = Some(bytes.len() as u64);
                    (Some(bytes), None, byte_len)
                }
            };

            // let display_name = record_path
            //     .as_ref()
            //     .and_then(|p| p.file_name())
            //     .map(|n| n.to_string_lossy().into_owned())
            //     .unwrap_or_else(|| format!("<binary id={:?}>", face_id));

            let id = next_id + fallbacks.len() as FontId;
            let record = FontRecord {
                id,
                bytes: record_bytes,
                font: None,
                byte_len,
                collection_index: ci,
                _path: record_path,
                is_color,
            };

            seen_ids.insert(face_id);
            // info!("Dynamic fallback [{}]: {} (ci={}, color={})", group.label,
            // display_name, ci, is_color);
            fallbacks.push(record);

            // One font per probe group is enough — stop scanning for this group.
            break 'face_loop;
        }
    }

    fallbacks
}

pub fn shared_fallback_chain() -> Vec<FontRecord> {
    static FALLBACKS: OnceLock<Vec<FontRecord>> = OnceLock::new();
    FALLBACKS
        .get_or_init(|| build_fallback_chain(1))
        .clone()
}

/// Pre-build the fallback chain and validate each fallback face with
/// `ttf-parser`, avoiding eager whole-font parsing during warmup. Safe to call
/// from any thread; the inner `OnceLock` is also used by
/// `GlyphRasterizer::ensure_fallbacks`.
#[allow(dead_code)]
pub fn warm_fallbacks() {
    let start = aimer_utils::AnimInstant::now();
    let chain = shared_fallback_chain();
    for record in &chain {
        let _ = record.ensure_face();
    }
    info!(
        "warm_fallbacks() took {} ms",
        start
            .elapsed()
            .as_millis()
    );
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::{FontData, FontRecord};

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn file_backed_font_data_is_memory_mapped_instead_of_heap_copied() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/pipeline/text_pipeline/font_resolver.rs");
        let record = FontRecord {
            id: 1,
            bytes: None,
            font: None,
            byte_len: std::fs::metadata(&path)
                .ok()
                .map(|metadata| metadata.len()),
            collection_index: 0,
            _path: Some(Arc::new(path)),
            is_color: false,
        };

        assert!(matches!(record.data(), Some(FontData::Mapped(_))));
    }
}
