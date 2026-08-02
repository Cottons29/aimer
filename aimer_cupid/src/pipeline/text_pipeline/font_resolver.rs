#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
use std::collections::HashSet;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{LazyLock, RwLock};
use std::sync::{Arc, OnceLock};

use aimer_utils::info;
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
use fontique::{Collection, CollectionOptions, SourceKind};
use skrifa::instance::{LocationRef, Size};
use skrifa::raw::TableProvider;
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::text_layout::FontId;

#[derive(Clone)]
pub struct FontRecord {
    pub id: FontId,
    pub bytes: Option<Arc<[u8]>>,
    pub(crate) collection_index: u32,
    pub(crate) path: Option<Arc<PathBuf>>,
    /// True when the font carries color glyph data (`sbix` / `CBDT` / `COLR`)
    /// and should be rasterized via color-glyph tables.
    pub is_color: bool,
}

/// Immutable shared ownership of a font record used to seed local CPU contexts.
///
/// The record itself is never mutably exposed. A preparation context obtains a
/// cheap local copy for worker-local shaping and rasterization state.
#[derive(Clone)]
pub(crate) struct SharedFontRecord(Arc<FontRecord>);

impl SharedFontRecord {
    pub(crate) fn new(record: &FontRecord) -> Self {
        Self(Arc::new(record.clone()))
    }

    pub(crate) fn local_copy(&self) -> FontRecord {
        self.0.as_ref().clone()
    }
}

#[derive(Clone)]
pub(crate) enum FontData {
    Shared(Arc<[u8]>),
    #[cfg(not(target_arch = "wasm32"))]
    Mapped(Arc<memmap2::Mmap>),
}

/// Process-wide cache of memory-mapped font files, keyed by path.
///
/// Mapping a file is not free: it opens a file descriptor and asks the kernel
/// for a new virtual memory region, and every entry point that needs font
/// bytes — glyph coverage probing, charmap lookups, advance metrics,
/// shaping and rasterization — used to repeat that work on each call. A
/// system fallback face is consulted once per codepoint, so a page of mixed
/// scripts performed thousands of redundant mappings of the same handful of
/// files.
///
/// Entries are never evicted. The set of font files an application touches is
/// small and bounded by the faces it actually renders with, and the mappings
/// were already alive for the duration of every operation that used them; the
/// pages themselves stay owned by the page cache and are reclaimable under
/// memory pressure, so retaining the mapping costs address space rather than
/// resident memory.
#[cfg(not(target_arch = "wasm32"))]
static MAPPED_FONT_FILES: LazyLock<RwLock<HashMap<PathBuf, Arc<memmap2::Mmap>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Returns the shared read-only mapping of `path`, creating it on first use.
///
/// Returns `None` when the file cannot be opened or mapped; failures are not
/// cached, so a font that becomes readable later is picked up.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn mapped_font_file(path: &Path) -> Option<Arc<memmap2::Mmap>> {
    if let Ok(cache) = MAPPED_FONT_FILES.read()
        && let Some(mapping) = cache.get(path)
    {
        return Some(mapping.clone());
    }

    let file = std::fs::File::open(path).ok()?;
    // SAFETY: the read-only mapping owns its file-backed virtual memory region
    // and remains valid independently of the `File` handle.
    let mapping = Arc::new(unsafe { memmap2::Mmap::map(&file).ok()? });

    // A concurrent caller may have mapped the same file meanwhile. Keeping the
    // entry that is already published guarantees one mapping per file, which is
    // what makes pointer identity of the bytes meaningful.
    let mut cache = MAPPED_FONT_FILES.write().ok()?;
    Some(cache.entry(path.to_path_buf()).or_insert(mapping).clone())
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

/// Returns the requested face from a standalone font or TrueType collection.
pub(crate) fn font_ref(data: &[u8], collection_index: u32) -> Option<FontRef<'_>> {
    FontRef::from_index(data, collection_index).ok()
}

impl FontRecord {
    pub(crate) fn from_static_bytes(id: FontId, bytes: &'static [u8]) -> Option<Self> {
        font_ref(bytes, 0)?;
        Some(Self {
            id,
            bytes: Some(Arc::from(bytes)),
            collection_index: 0,
            path: None,
            is_color: false,
        })
    }

    pub fn from_bytes(id: FontId, bytes: Vec<u8>) -> Option<Self> {
        let face = font_ref(&bytes, 0)?;
        let is_color = Self::face_is_color(&face);
        Some(Self {
            id,
            bytes: Some(Arc::from(bytes)),
            collection_index: 0,
            path: None,
            is_color,
        })
    }

    pub(crate) fn from_shared_bytes(id: FontId, bytes: Arc<[u8]>) -> Option<Self> {
        let face = font_ref(bytes.as_ref(), 0)?;
        let is_color = Self::face_is_color(&face);

        Some(Self {
            id,
            bytes: Some(bytes),
            collection_index: 0,
            path: None,
            is_color,
        })
    }

    /// Returns true if this collection_index of `data` contains any color glyph
    /// table that we know how to render (`sbix`, `CBDT`/`CBLC`, or
    /// `COLR`/`CPAL`).
    #[allow(dead_code)]
    pub(crate) fn face_is_color(face: &FontRef<'_>) -> bool {
        // sbix  — AppleColorEmoji (macOS/iOS)
        // cbdt  — Noto Color Emoji (Android/Linux, older builds)
        // colr  — Windows/Linux Segoe/Twemoji v1 layered outlines
        face.sbix().is_ok() || face.cbdt().is_ok() || face.colr().is_ok()
    }

    /// Probe the font with each `probes` codepoint; accept on the first match.
    /// `accept_color` allows color fonts to be admitted to the chain even when
    /// none of the probes are present (the typical case for emoji fonts whose
    /// cmap maps emoji codepoints — which is what callers should pass here, but
    /// we keep the option to make tests easier).
    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    fn probes_match(face: &FontRef<'_>, probes: &[char]) -> bool {
        let charmap = face.charmap();
        probes
            .iter()
            .any(|&codepoint| charmap.map(codepoint).is_some())
    }

    /// Retain shared in-memory data or hand out the process-wide memory map of
    /// a file-backed font, without copying the font into the process heap.
    ///
    /// The mapping is created once per file and shared by every record and
    /// every thread referring to it, so repeated calls are a hash lookup and an
    /// atomic refcount bump rather than an `open` plus `mmap`.
    pub(crate) fn data(&self) -> Option<FontData> {
        if let Some(bytes) = self.bytes.as_ref() {
            return Some(FontData::Shared(bytes.clone()));
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            mapped_font_file(self.path.as_ref()?.as_ref()).map(FontData::Mapped)
        }
        #[cfg(target_arch = "wasm32")]
        None
    }

    #[allow(dead_code)]
    pub(crate) fn ensure_face(&self) -> Option<()> {
        let data = self.data()?;
        font_ref(data.as_ref(), self.collection_index)?;
        Some(())
    }

    /// Returns the file this face was loaded from, if it is backed by one.
    ///
    /// Fonts registered from memory have no path. A path is what identifies a
    /// face to the platform rasterizer, which is the only way to draw faces
    /// whose glyph data Cupid cannot decode.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[inline]
    pub(crate) fn path(&self) -> Option<&std::path::Path> {
        self.path.as_ref().map(|path| path.as_path())
    }

    pub(crate) fn glyph_index(&self, codepoint: char) -> Option<u16> {
        let data = self.data()?;
        let face = font_ref(data.as_ref(), self.collection_index)?;
        face.charmap().map(codepoint).map(|id| id.to_u32() as u16)
    }

    pub(crate) fn advance_width_for_glyph(&self, glyph_id: u16, font_size: f32) -> Option<f32> {
        let data = self.data()?;
        advance_width_from_face(data.as_ref(), self.collection_index, glyph_id, font_size)
    }
}

pub fn advance_width_from_face(
    bytes: &[u8],
    collection_index: u32,
    glyph_id: u16,
    font_size: f32,
) -> Option<f32> {
    let face = font_ref(bytes, collection_index)?;
    face.glyph_metrics(Size::new(font_size), LocationRef::default())
        .advance_width(GlyphId::new(glyph_id as u32))
}

/// A probe group: one script / category with the codepoints used to verify
/// that a font actually covers it.  `hint_color` marks probe groups that
/// identify color-emoji fonts so we can set `is_color` even before decoding.
///
/// Probe groups exist only for platforms without a system-provided cascade
/// list. Apple platforms resolve fonts per codepoint through Core Text — see
/// [`system_fallback`](crate::text_pipeline::system_fallback) — and therefore
/// need no hardcoded script table at all.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
#[allow(dead_code)]
struct ProbeGroup {
    label: &'static str,
    probes: &'static [char],
    hint_color: bool,
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
const fn probe_group(label: &'static str, probes: &'static [char], hint_color: bool) -> ProbeGroup {
    ProbeGroup {
        label,
        probes,
        hint_color,
    }
}

/// All script / category probe groups we want covered in the fallback chain.
/// Order is significant: earlier groups are preferred over later ones when a
/// single font file covers multiple scripts (the first probe group that matches
/// controls whether the font is added to the chain for that group).
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
static PROBE_GROUPS: &[ProbeGroup] = &[
    probe_group("emoji", &['😀', '👍'], true),
    probe_group("cjk", &['你', '漢', '한'], false),
    probe_group("hangul", &['가', '나', '다'], false),
    probe_group("arabic", &['\u{0639}', '\u{0627}'], false), /* ع ا */
    probe_group("hebrew", &['\u{05D0}', '\u{05D1}'], false), /* א ב */
    probe_group("devanagari", &['\u{0915}', '\u{0930}'], false), /* क र */
    probe_group("tamil", &['\u{0B95}', '\u{0BB5}'], false),  /* க வ */
    probe_group("thai", &['\u{0E01}', '\u{0E02}'], false),   /* ก ข */
    probe_group("armenian", &['\u{0531}', '\u{0532}'], false), /* Ա Բ */
    probe_group("georgian", &['\u{10D0}', '\u{10D1}'], false), /* ა ბ */
    probe_group("ethiopic", &['\u{1200}', '\u{1201}'], false), /* ሀ ሁ */
    probe_group("myanmar", &['\u{1000}', '\u{1001}'], false), /* က ခ */
    probe_group("khmer", &['\u{1780}', '\u{1781}'], false),  /* ក ខ */
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
    probe_group("yi", &['\u{A000}'], false),                 /* ꀀ */
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
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn font_data_matches_probes(
    data: &[u8],
    ci: u32,
    probes: &[char],
    hint_color: bool,
) -> Option<bool> {
    let face = font_ref(data, ci)?;
    if !FontRecord::probes_match(&face, probes) {
        return None;
    }

    if hint_color {
        // For emoji probe groups we require real bitmap strike tables (sbix or
        // cbdt).  Fonts that only carry COLR (e.g. LastResort.otf, most text
        // fonts with COLR decorative glyphs) are not usable as emoji fonts here
        // because our `rasterize_color_glyph` path prefers sbix/cbdt.
        if face.sbix().is_err() && face.cbdt().is_err() {
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
    let charmap = face.charmap();
    let glyph_ids: HashSet<u16> = probes
        .iter()
        .filter_map(|&codepoint| charmap.map(codepoint))
        .map(|id| id.to_u32() as u16)
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
    let has_usable_outline = probes.iter().any(|&c| {
        charmap
            .map(c)
            .and_then(|id| {
                face.glyph_metrics(Size::unscaled(), LocationRef::default())
                    .bounds(id)
            })
            .is_some()
    });
    if !has_usable_outline {
        return None;
    }

    Some(false) // non-color font confirmed usable
}

/// Apple platforms carry no pre-built fallback chain.
///
/// Core Text owns the cascade list the system itself renders with and can
/// answer, for any single codepoint, which face would draw it. Guessing that
/// list up front from a hardcoded table of script samples is both slower —
/// every entry costs a query plus a memory map at first glyph miss — and
/// incomplete, because a face chosen by a sample character is not guaranteed
/// to cover the rest of its script (a Japanese face selected for `漢` has no
/// glyph for `你`, which is how tofu boxes appear in otherwise fine text).
///
/// Faces are therefore resolved one codepoint at a time and cached process
/// wide by [`system_fallback`](crate::text_pipeline::system_fallback).
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn build_fallback_chain(_next_id: FontId) -> Vec<FontRecord> {
    Vec::new()
}

/// Build the fallback chain dynamically from Fontique's platform font sources.
///
/// For each `ProbeGroup` we walk all font faces in order and add the first
/// face that satisfies the group's probes.  A font face is never added twice
/// (deduped by a stable key).  The result is a flat `Vec<FontRecord>` ordered
/// by probe group priority.
///
/// This function uses `db.with_face_data()` to access the raw bytes of each
/// candidate face so it works uniformly for both on-disk (file-backed) and
/// in-memory (WASM-embedded or iOS binary-blob) sources.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn build_fallback_chain(next_id: FontId) -> Vec<FontRecord> {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: !cfg!(target_arch = "wasm32"),
    });
    let mut fallbacks: Vec<FontRecord> = Vec::new();
    let mut seen_ids = HashSet::new();
    let family_names: Vec<String> = collection.family_names().map(str::to_owned).collect();

    for group in PROBE_GROUPS {
        'family_loop: for family_name in &family_names {
            let Some(family) = collection.family_by_name(family_name) else {
                continue;
            };
            for font_info in family.fonts() {
                let source_id = font_info.source().id();
                let ci = font_info.index();
                if seen_ids.contains(&(source_id, ci)) {
                    continue;
                }

                let is_color = match font_info.source().kind() {
                    SourceKind::Path(path) => {
                        let file = match std::fs::File::open(path.as_ref()) {
                            Ok(file) => file,
                            Err(_) => continue,
                        };
                        let data = match unsafe { memmap2::Mmap::map(&file) } {
                            Ok(data) => data,
                            Err(_) => continue,
                        };
                        font_data_matches_probes(&data, ci, group.probes, group.hint_color)
                    }
                    SourceKind::Memory(data) => {
                        font_data_matches_probes(data.as_ref(), ci, group.probes, group.hint_color)
                    }
                };
                let Some(is_color) = is_color else {
                    continue;
                };

                let (record_bytes, record_path) = match font_info.source().kind() {
                    SourceKind::Path(path) => (None, Some(Arc::new(path.as_ref().to_path_buf()))),
                    SourceKind::Memory(data) => (Some(Arc::from(data.as_ref())), None),
                };
                let id = next_id + fallbacks.len() as FontId;
                fallbacks.push(FontRecord {
                    id,
                    bytes: record_bytes,
                    collection_index: ci,
                    path: record_path,
                    is_color,
                });
                seen_ids.insert((source_id, ci));
                break 'family_loop;
            }
        }
    }

    fallbacks
}

pub fn shared_fallback_chain() -> Vec<FontRecord> {
    static FALLBACKS: OnceLock<Vec<FontRecord>> = OnceLock::new();
    FALLBACKS.get_or_init(|| build_fallback_chain(1)).clone()
}

/// Pre-build the fallback chain and validate each fallback face with
/// Skrifa, avoiding eager whole-font parsing during warmup. Safe to call
/// from any thread; the inner `OnceLock` is also used by
/// `GlyphRasterizer::ensure_fallbacks`.
#[allow(dead_code)]
pub fn warm_fallbacks() {
    let start = aimer_utils::AnimInstant::now();
    let chain = shared_fallback_chain();
    for record in &chain {
        let _ = record.ensure_face();
    }
    info!("warm_fallbacks() took {} ms", start.elapsed().as_millis());
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use std::path::PathBuf;
    #[cfg(not(target_arch = "wasm32"))]
    use std::sync::Arc;

    use skrifa::MetadataProvider;
    
    #[cfg(not(target_arch = "wasm32"))]
    use super::{FontData, FontRecord, font_ref};

    const TEST_FONT: &[u8] = include_bytes!("../../../fonts/JetBrainsMono-Regular.ttf");

    #[test]
    fn skrifa_font_ref_rejects_invalid_data_and_preserves_face_metadata() {
        assert!(font_ref(b"not a font", 0).is_none());

        let face = font_ref(TEST_FONT, 0).expect("bundled test font should parse");
        let glyph_id = face
            .charmap()
            .map('A')
            .expect("bundled test font should map Latin glyphs");
        let advance = face
            .glyph_metrics(
                skrifa::instance::Size::new(16.0),
                skrifa::instance::LocationRef::default(),
            )
            .advance_width(glyph_id)
            .expect("mapped glyph should have an advance width");

        assert!(advance > 0.0);
        assert!(face.outline_glyphs().get(glyph_id).is_some());
        assert!(!FontRecord::face_is_color(&face));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn file_backed_font_data_is_memory_mapped_instead_of_heap_copied() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/pipeline/text_pipeline/font_resolver.rs");
        let record = FontRecord {
            id: 1,
            bytes: None,
            collection_index: 0,
            path: Some(Arc::new(path)),
            is_color: false,
        };

        assert!(matches!(record.data(), Some(FontData::Mapped(_))));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn file_backed_font_data_maps_each_file_once_per_process() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts/JetBrainsMono-Regular.ttf");
        let first = FontRecord {
            id: 1,
            bytes: None,
            collection_index: 0,
            path: Some(Arc::new(path.clone())),
            is_color: false,
        };
        let second = FontRecord {
            id: 2,
            bytes: None,
            collection_index: 0,
            path: Some(Arc::new(path)),
            is_color: false,
        };

        let a = first.data().expect("font file should map");
        let b = first.data().expect("font file should map");
        let c = second.data().expect("font file should map");

        assert_eq!(
            a.as_ref().as_ptr(),
            b.as_ref().as_ptr(),
            "repeated access to one record must reuse the same mapping"
        );
        assert_eq!(
            a.as_ref().as_ptr(),
            c.as_ref().as_ptr(),
            "records sharing a path must reuse the same mapping"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn mapping_an_unreadable_path_fails_without_poisoning_the_cache() {
        let missing = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts/does-not-exist.ttf");
        let record = FontRecord {
            id: 1,
            bytes: None,
            collection_index: 0,
            path: Some(Arc::new(missing)),
            is_color: false,
        };

        assert!(record.data().is_none());
        assert!(record.glyph_index('A').is_none());

        let readable = FontRecord {
            id: 2,
            bytes: None,
            collection_index: 0,
            path: Some(Arc::new(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fonts/JetBrainsMono-Regular.ttf"),
            )),
            is_color: false,
        };
        assert!(readable.glyph_index('A').is_some());
    }
}
