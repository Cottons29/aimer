#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{LazyLock, RwLock};
use std::sync::Arc;
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
use std::sync::OnceLock;

use aimer_utils::info;

use crate::text_layout::FontId;

#[derive(Clone)]
pub struct FontRecord {
    pub id: FontId,
    pub bytes: Option<Arc<[u8]>>,
    pub(crate) collection_index: u32,
    pub(crate) path: Option<Arc<PathBuf>>,
    /// True when the font carries color glyph data (`sbix` / `CBDT` / `COLR` /
    /// `SVG `) or a private color strike such as Apple's `emjc`. Private
    /// color data is marked here only to route it to the platform compatibility
    /// renderer; the portable Aimer reader never decodes it.
    pub is_color: bool,
}

/// A script/category lane in the platform-independent fallback chain.
///
/// Each lane is discovered independently. This keeps a first emoji miss from
/// enumerating every installed CJK, Arabic, and Indic face, while preserving a
/// stable id range for faces found in a later lane. Apple platforms do not use
/// these probe lanes for selection — Core Text resolves a face per codepoint —
/// but the classification is still useful to keep the rasterizer's loading
/// state explicit and testable.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) enum FallbackScript {
    Emoji,
    Cjk,
    Hangul,
    Arabic,
    Hebrew,
    Devanagari,
    Tamil,
    Thai,
    Armenian,
    Georgian,
    Ethiopic,
    Myanmar,
    Khmer,
    Tibetan,
    Sinhala,
    Telugu,
    Kannada,
    Malayalam,
    Gujarati,
    Gurmukhi,
    Bengali,
    Oriya,
    Lao,
    Mongolian,
    Cherokee,
    Yi,
}

impl FallbackScript {
    pub(crate) const COUNT: usize = 26;
    pub(crate) const ALL: [Self; Self::COUNT] = [
        Self::Emoji,
        Self::Cjk,
        Self::Hangul,
        Self::Arabic,
        Self::Hebrew,
        Self::Devanagari,
        Self::Tamil,
        Self::Thai,
        Self::Armenian,
        Self::Georgian,
        Self::Ethiopic,
        Self::Myanmar,
        Self::Khmer,
        Self::Tibetan,
        Self::Sinhala,
        Self::Telugu,
        Self::Kannada,
        Self::Malayalam,
        Self::Gujarati,
        Self::Gurmukhi,
        Self::Bengali,
        Self::Oriya,
        Self::Lao,
        Self::Mongolian,
        Self::Cherokee,
        Self::Yi,
    ];

    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    const fn index(self) -> usize {
        match self {
            Self::Emoji => 0,
            Self::Cjk => 1,
            Self::Hangul => 2,
            Self::Arabic => 3,
            Self::Hebrew => 4,
            Self::Devanagari => 5,
            Self::Tamil => 6,
            Self::Thai => 7,
            Self::Armenian => 8,
            Self::Georgian => 9,
            Self::Ethiopic => 10,
            Self::Myanmar => 11,
            Self::Khmer => 12,
            Self::Tibetan => 13,
            Self::Sinhala => 14,
            Self::Telugu => 15,
            Self::Kannada => 16,
            Self::Malayalam => 17,
            Self::Gujarati => 18,
            Self::Gurmukhi => 19,
            Self::Bengali => 20,
            Self::Oriya => 21,
            Self::Lao => 22,
            Self::Mongolian => 23,
            Self::Cherokee => 24,
            Self::Yi => 25,
        }
    }

    /// Stable base id for the one face selected for this lane.
    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    pub(crate) const fn id_base(self) -> FontId {
        FALLBACK_CHAIN_ID_BASE + self.index() as FontId * FALLBACK_CHAIN_ID_STRIDE
    }

    fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

/// Reserved ids for faces found by the platform-independent probe lanes.
///
/// Each lane currently contributes at most one face, but reserving a stride
/// leaves room for a future deterministic per-lane shortlist without making
/// runtime registration ids depend on which scripts happened to be rendered
/// first.
pub(crate) const FALLBACK_CHAIN_ID_BASE: FontId = 0x1000_0000;
const FALLBACK_CHAIN_ID_STRIDE: FontId = 0x1000;

/// Classifies a codepoint before a fallback lookup.
pub(crate) fn fallback_script_for_codepoint(codepoint: char) -> Option<FallbackScript> {
    match codepoint as u32 {
        0x1F000..=0x1FAFF | 0x2600..=0x27BF => Some(FallbackScript::Emoji),
        0x3000..=0x303F
        | 0x3040..=0x30FF
        | 0x31F0..=0x31FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0xFF00..=0xFFEF
        | 0x20000..=0x3FFFF => Some(FallbackScript::Cjk),
        0x1100..=0x11FF
        | 0x3130..=0x318F
        | 0xA960..=0xA97F
        | 0xAC00..=0xD7FF => Some(FallbackScript::Hangul),
        0x0590..=0x05FF => Some(FallbackScript::Hebrew),
        0x0600..=0x06FF
        | 0x0750..=0x077F
        | 0x08A0..=0x08FF
        | 0xFB50..=0xFDFF
        | 0xFE70..=0xFEFF => Some(FallbackScript::Arabic),
        0x0900..=0x097F => Some(FallbackScript::Devanagari),
        0x0B80..=0x0BFF => Some(FallbackScript::Tamil),
        0x0E00..=0x0E7F => Some(FallbackScript::Thai),
        0x0530..=0x058F => Some(FallbackScript::Armenian),
        0x10A0..=0x10FF => Some(FallbackScript::Georgian),
        0x1200..=0x137F => Some(FallbackScript::Ethiopic),
        0x1000..=0x109F => Some(FallbackScript::Myanmar),
        0x1780..=0x17FF => Some(FallbackScript::Khmer),
        0x0F00..=0x0FFF => Some(FallbackScript::Tibetan),
        0x0D80..=0x0DFF => Some(FallbackScript::Sinhala),
        0x0C00..=0x0C7F => Some(FallbackScript::Telugu),
        0x0C80..=0x0CFF => Some(FallbackScript::Kannada),
        0x0D00..=0x0D7F => Some(FallbackScript::Malayalam),
        0x0A80..=0x0AFF => Some(FallbackScript::Gujarati),
        0x0A00..=0x0A7F => Some(FallbackScript::Gurmukhi),
        0x0980..=0x09FF => Some(FallbackScript::Bengali),
        0x0B00..=0x0B7F => Some(FallbackScript::Oriya),
        0x0E80..=0x0EFF => Some(FallbackScript::Lao),
        0x1800..=0x18AF => Some(FallbackScript::Mongolian),
        0x13A0..=0x13FF => Some(FallbackScript::Cherokee),
        0xA000..=0xA4CF => Some(FallbackScript::Yi),
        _ => None,
    }
}

/// Maps a statically assigned chain id back to the lane that owns it.
pub(crate) fn fallback_script_for_font_id(font_id: FontId) -> Option<FallbackScript> {
    let offset = font_id.checked_sub(FALLBACK_CHAIN_ID_BASE)?;
    if offset >= FallbackScript::COUNT as FontId * FALLBACK_CHAIN_ID_STRIDE {
        return None;
    }
    FallbackScript::from_index((offset / FALLBACK_CHAIN_ID_STRIDE) as usize)
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

/// Returns the mapping of `path` if one has already been published, without
/// creating it.
///
/// This is the half of the cache a caller wants when a mapping is worth
/// *reusing* but not worth *retaining*: probing a candidate font answers a
/// yes/no question and is usually a no, so publishing every file it touches
/// would grow a cache that never evicts by the whole installed font set.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn cached_font_file(path: &Path) -> Option<Arc<memmap2::Mmap>> {
    MAPPED_FONT_FILES.read().ok()?.get(path).cloned()
}

/// Publishes `mapping` as the process-wide mapping of `path` and returns the
/// mapping callers will observe from now on.
///
/// A concurrent caller may have published the same file meanwhile. Keeping the
/// entry that is already there guarantees one mapping per file, which is what
/// makes pointer identity of the bytes meaningful — so the returned handle is
/// the published one, which is not necessarily the argument.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn publish_font_file(path: &Path, mapping: Arc<memmap2::Mmap>) -> Arc<memmap2::Mmap> {
    let Ok(mut cache) = MAPPED_FONT_FILES.write() else {
        return mapping;
    };
    cache.entry(path.to_path_buf()).or_insert(mapping).clone()
}

/// Maps `path` read-only without consulting or filling the shared cache.
///
/// Returns `None` when the file cannot be opened or mapped.
fn map_font_file(path: &Path) -> Option<Arc<memmap2::Mmap>> {
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: the read-only mapping owns its file-backed virtual memory region
    // and remains valid independently of the `File` handle.
    Some(Arc::new(unsafe { memmap2::Mmap::map(&file).ok()? }))
}

/// Returns the shared read-only mapping of `path`, creating it on first use.
///
/// Returns `None` when the file cannot be opened or mapped; failures are not
/// cached, so a font that becomes readable later is picked up.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn mapped_font_file(path: &Path) -> Option<Arc<memmap2::Mmap>> {
    if let Some(mapping) = cached_font_file(path) {
        return Some(mapping);
    }

    Some(publish_font_file(path, map_font_file(path)?))
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

/// The OpenType `wght` value of a face that asks for no emphasis.
///
/// This is both the weight a face is read as when it declares none and the
/// weight at which a request is passed through untouched, so the path every
/// unemphasized line takes does no weight work at all.
pub(crate) const REGULAR_WEIGHT: u16 = 400;

fn face_metadata(data: FontData, collection_index: u32) -> Option<bool> {
    let face = crate::text_pipeline::aimer_font::SfntFace::from_font_data(
        data,
        collection_index,
    )
    .ok()?;
    face.metrics().ok()?;
    face.table(*b"cmap")?;
    Some(face.has_color_tables() || face.has_apple_private_color_tables())
}

impl FontRecord {
    pub(crate) fn from_static_bytes(id: FontId, bytes: &'static [u8]) -> Option<Self> {
        let bytes: Arc<[u8]> = Arc::from(bytes);
        let is_color = face_metadata(FontData::Shared(bytes.clone()), 0)?;
        Some(Self {
            id,
            bytes: Some(bytes),
            collection_index: 0,
            path: None,
            is_color,
        })
    }

    pub fn from_bytes(id: FontId, bytes: Vec<u8>) -> Option<Self> {
        let bytes: Arc<[u8]> = Arc::from(bytes);
        let is_color = face_metadata(FontData::Shared(bytes.clone()), 0)?;
        Some(Self {
            id,
            bytes: Some(bytes),
            collection_index: 0,
            path: None,
            is_color,
        })
    }

    pub(crate) fn from_shared_bytes(id: FontId, bytes: Arc<[u8]>) -> Option<Self> {
        let is_color = face_metadata(FontData::Shared(bytes.clone()), 0)?;

        Some(Self {
            id,
            bytes: Some(bytes),
            collection_index: 0,
            path: None,
            is_color,
        })
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
        crate::text_pipeline::aimer_font::SfntFace::from_font_data(
            data,
            self.collection_index,
        )
        .ok()?;
        Some(())
    }

    /// Returns the file this face was loaded from, if it is backed by one.
    ///
    /// Fonts registered from memory have no path. A path is what identifies a
    /// face to the platform rasterizer, which is the only way to draw faces
    /// whose glyph data Cupid cannot decode.
    #[cfg(all(
        any(target_os = "ios", target_os = "macos"),
        feature = "apple-core-text"
    ))]
    #[inline]
    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_ref().map(|path| path.as_path())
    }

    /// Returns whether this face has an outline table understood by the
    /// Aimer-owned reader.
    pub(crate) fn has_standard_outline(&self) -> bool {
        let Some(data) = self.data() else {
            return false;
        };
        crate::text_pipeline::aimer_font::SfntFace::from_font_data(
            data,
            self.collection_index,
        )
        .ok()
        .is_some_and(|face| face.has_standard_outline())
    }

    /// Returns a face's design weight from its `OS/2` table.
    pub(crate) fn design_weight(&self) -> Option<u16> {
        let data = self.data()?;
        crate::text_pipeline::aimer_font::SfntFace::from_bytes(
            data.as_ref(),
            self.collection_index,
        )
        .ok()?
        .design_weight()
    }

    /// Returns a glyph's scaled horizontal advance from the active reader.
    pub(crate) fn advance_width_for_glyph(&self, glyph_id: u16, font_size: f32) -> Option<f32> {
        let data = self.data()?;
        advance_width_from_face(data.as_ref(), self.collection_index, glyph_id, font_size)
    }

    /// Returns scaled ascent, descent, and line gap from the Aimer reader.
    pub(crate) fn line_metrics(&self, font_size: f32) -> Option<(f32, f32, f32)> {
        if !font_size.is_finite() || font_size <= 0.0 {
            return None;
        }
        let data = self.data()?;
        let metrics = crate::text_pipeline::aimer_font::metrics_from_font_data(
            data,
            self.collection_index,
        )
        .ok()?;
        let scale = font_size / f32::from(metrics.units_per_em);
        Some((
            f32::from(metrics.ascender) * scale,
            f32::from(metrics.descender) * scale,
            f32::from(metrics.line_gap) * scale,
        ))
    }

    pub(crate) fn glyph_index(&self, codepoint: char) -> Option<u16> {
        let data = self.data()?;
        crate::text_pipeline::aimer_font::SfntFace::from_font_data(
            data,
            self.collection_index,
        )
        .ok()?
        .glyph_index(codepoint as u32)
        .ok()?
    }
}

pub fn advance_width_from_face(
    bytes: &[u8],
    collection_index: u32,
    glyph_id: u16,
    font_size: f32,
) -> Option<f32> {
    if !font_size.is_finite() || font_size <= 0.0 {
        return None;
    }
    let face = crate::text_pipeline::aimer_font::SfntFace::from_bytes(bytes, collection_index).ok()?;
    let metrics = face.metrics().ok()?;
    let advance = face.glyph_advance_with_metrics(glyph_id, metrics).ok()??;
    let scale = font_size / f32::from(metrics.units_per_em);
    Some(f32::from(advance) * scale)
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
    // Keep Hangul out of the Han lane. A pan-CJK face may carry both, but a
    // Korean run must be able to select the Hangul lane independently.
    probe_group("cjk", &['你', '漢'], false),
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
#[cfg(all(
    not(any(target_os = "ios", target_os = "macos"))
))]
fn face_matches_probes(
    face: &crate::text_pipeline::aimer_font::SfntFace<'_>,
    probes: &[char],
    hint_color: bool,
) -> Option<bool> {
    if hint_color {
        return face.has_color_tables().then_some(true);
    }

    let glyph_ids: HashSet<u16> = probes
        .iter()
        .filter_map(|&codepoint| face.glyph_index(codepoint as u32).ok().flatten())
        .filter(|&id| id != 0)
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
    let has_usable_outline = probes.iter().any(|&codepoint| {
        let Some(glyph_id) = face.glyph_index(codepoint as u32).ok().flatten() else {
            return false;
        };
        face.outline(glyph_id)
            .ok()
            .flatten()
            .is_some()
            || face.cff_outline(glyph_id).ok().flatten().is_some()
    });
    if !has_usable_outline {
        return None;
    }

    Some(false) // non-color font confirmed usable
}

/// Returns a mapping of `path` suitable for *probing* a candidate face: the
/// shared one when the pipeline has already published it, a private one
/// otherwise.
///
/// Reusing the published mapping is what makes a warm cache free — a font the
/// application already renders with is not opened again — while a candidate
/// that is only being examined stays out of a cache that never evicts.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn probe_font_file(path: &Path) -> Option<Arc<memmap2::Mmap>> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(mapping) = cached_font_file(path) {
        return Some(mapping);
    }

    map_font_file(path)
}

/// Hands the mapping a probe created over to the shared cache, so the face's
/// first render — and every metric query after it — finds the file already
/// mapped instead of paying for another `open` plus `mmap`.
///
/// Only worth calling for a face that has actually been accepted into the
/// chain; see [`probe_font_file`].
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn retain_probed_font_file(path: &Path, mapping: Arc<memmap2::Mmap>) {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = publish_font_file(path, mapping);
    #[cfg(target_arch = "wasm32")]
    let _ = (path, mapping);
}

/// Returns whether `lookup` — a font's character map — maps any of `probes`.
///
/// This is the cheap half of face selection. Answering it needs nothing but a
/// `cmap` subtable, which the platform font enumerator has already located for
/// every installed face, whereas the full probe in `face_matches_probes`
/// needs the face parsed: a table directory walk plus, for the groups it does
/// not reject outright, glyph metrics and color table lookups. Since the full
/// probe starts by requiring exactly this condition, using it as a gate rejects
/// no face the chain would have accepted while sparing the parse for the
/// overwhelming majority of faces, which cover none of the scripts still
/// wanted.
///
/// `lookup` is called at most once per probe and stops at the first hit.
#[cfg_attr(any(target_os = "ios", target_os = "macos"), allow(dead_code))]
fn any_probe_is_mapped(mut lookup: impl FnMut(char) -> Option<u32>, probes: &[char]) -> bool {
    probes
        .iter()
        .any(|&codepoint| lookup(codepoint).is_some())
}

/// Claims the highest-priority group slot that is still open and whose probes
/// the offered face satisfies, returning the slot index and the color flag the
/// probe reported.
///
/// `open[i]` tells whether group `i` still needs a face; `probe` is only ever
/// called for open slots, at most once each, and never again once a slot has
/// been claimed. That is what keeps chain construction linear in the number of
/// faces: a single face is parsed once and offered to every group that still
/// wants one, instead of every group re-scanning every face.
///
/// The scan runs in slot order, so a face able to serve several scripts is
/// spent on the most important one — matching the priority encoded in the
/// group table.
#[cfg_attr(any(target_os = "ios", target_os = "macos"), allow(dead_code))]
fn first_open_match(
    open: &[bool],
    mut probe: impl FnMut(usize) -> Option<bool>,
) -> Option<(usize, bool)> {
    open.iter()
        .enumerate()
        .filter(|&(_, is_open)| *is_open)
        .find_map(|(group, _)| probe(group).map(|is_color| (group, is_color)))
}

/// Builds only the fallback face for `script`.
///
/// The platform collection is still walked until this lane has a usable face,
/// but no face is parsed for unrelated scripts and no unrelated font record is
/// retained. The caller gives each lane its own id range, so the result is
/// independent of the order in which scripts first appear in the UI.
#[cfg(all(
    not(any(target_os = "ios", target_os = "macos"))
))]
fn build_fallback_chain_for_script(script: FallbackScript) -> Vec<FontRecord> {
    build_fallback_chain_for_scripts(std::slice::from_ref(&script), script.id_base())
}

#[cfg(all(
    not(any(target_os = "ios", target_os = "macos"))
))]
fn build_fallback_chain_for_scripts(
    scripts: &[FallbackScript],
    next_id: FontId,
) -> Vec<FontRecord> {
    let groups = scripts
        .iter()
        .map(|script| script.index())
        .collect::<Vec<_>>();

    let mut slots: Vec<Option<FontRecord>> = (0..groups.len()).map(|_| None).collect();
    let mut open = vec![true; groups.len()];
    let mut open_count = groups.len();
    if open_count == 0 {
        return Vec::new();
    }

    for path in system_font_paths() {
        let Some(mapping) = probe_font_file(&path) else {
            continue;
        };
        let data = &mapping[..];
        for collection_index in 0..64 {
            let Ok(face) = crate::text_pipeline::aimer_font::SfntFace::from_bytes(
                data,
                collection_index,
            ) else {
                break;
            };

            let Some((group, is_color)) = first_open_match(&open, |group| {
                let group = &PROBE_GROUPS[groups[group]];
                if !any_probe_is_mapped(
                    |codepoint| face.glyph_index(codepoint as u32).ok().flatten().map(u32::from),
                    group.probes,
                ) {
                    return None;
                }
                face_matches_probes(&face, group.probes, group.hint_color)
            }) else {
                continue;
            };

            let record_path = Arc::new(path.clone());
            slots[group] = Some(FontRecord {
                id: next_id + group as FontId,
                bytes: None,
                collection_index,
                path: Some(record_path.clone()),
                is_color,
            });
            retain_probed_font_file(record_path.as_path(), mapping.clone());
            open[group] = false;
            open_count -= 1;
            if open_count == 0 {
                return slots.into_iter().flatten().collect();
            }
        }
    }

    slots
        .into_iter()
        .flatten()
        .collect()
}

#[cfg(all(
    not(any(target_os = "ios", target_os = "macos"))
))]
fn system_font_paths() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
    ];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".fonts"));
        roots.push(home.join(".local/share/fonts"));
    }

    let mut pending = roots;
    let mut paths = Vec::new();
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
                continue;
            };
            if matches!(extension.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc" | "otc") {
                paths.push(path);
            }
        }
    }
    paths.sort_unstable();
    paths.dedup();
    paths
}

pub fn shared_fallback_chain() -> Vec<FontRecord> {
    FallbackScript::ALL
        .iter()
        .copied()
        .flat_map(shared_fallback_chain_for_script)
        .collect()
}

/// Returns the fallback faces for one script, building that lane at most once
/// for the process.
pub(crate) fn shared_fallback_chain_for_script(script: FallbackScript) -> Vec<FontRecord> {
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        let _ = script;
        Vec::new()
    }

    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    {
        static FALLBACKS: OnceLock<
            [OnceLock<Vec<FontRecord>>; FallbackScript::COUNT],
        > = OnceLock::new();
        let fallbacks = FALLBACKS.get_or_init(|| std::array::from_fn(|_| OnceLock::new()));
        fallbacks[script.index()]
            .get_or_init(|| build_fallback_chain_for_script(script))
            .clone()
    }
}

/// Pre-build the fallback chain and validate each fallback face with the
/// checked Aimer reader, avoiding eager whole-font parsing during warmup. Safe to call
/// from any thread; the inner `OnceLock` is also used by
/// `GlyphRasterizer::ensure_fallbacks`.
#[cfg_attr(
    any(target_os = "ios", target_os = "macos", target_arch = "wasm32"),
    allow(dead_code)
)]
pub fn warm_fallbacks() {
    let start = aimer_utils::AnimInstant::now();
    let chain = shared_fallback_chain();
    for record in &chain {
        let _ = record.ensure_face();
    }
    info!("warm_fallbacks() took {} ms", start.elapsed().as_millis());
}

/// Reports whether the caller is the one that has to run the warm-up.
///
/// The warm-up is process-wide work guarded by an `OnceLock`, so a second
/// runner would not repeat it — it would *block* on the first one. Claiming
/// the flag before spawning keeps that blocked thread from existing at all,
/// and the swap makes the claim atomic, so racing callers cannot both see an
/// unclaimed flag.
#[cfg_attr(
    any(target_os = "ios", target_os = "macos", target_arch = "wasm32"),
    allow(dead_code)
)]
fn claim_warm_up(started: &AtomicBool) -> bool {
    !started.swap(true, Ordering::Relaxed)
}

/// Starts building the platform fallback chain on a background thread, at most
/// once per process.
///
/// The chain is otherwise built by the first glyph that no loaded face covers,
/// on whichever thread happens to be rasterizing — so the frame that first
/// shows CJK, emoji or any other uncovered script pays for a sweep of the
/// installed font set. Doing it ahead of time off the UI thread means that
/// frame finds the chain already built.
///
/// This is a no-op where no chain is built: Apple platforms resolve fallbacks
/// per codepoint through the system instead (see
/// [`system_fallback`](crate::text_pipeline::system_fallback)), and wasm has no
/// threads to spare the UI one.
#[inline]
pub fn warm_fallbacks_in_background() {
    #[cfg(not(any(target_os = "ios", target_os = "macos", target_arch = "wasm32")))]
    {
        static STARTED: AtomicBool = AtomicBool::new(false);
        if !claim_warm_up(&STARTED) {
            return;
        }

        // A failure to spawn is not worth failing startup over: the chain is
        // still built lazily on first miss, exactly as it was before.
        if std::thread::Builder::new()
            .name("cupid-font-warmup".into())
            .spawn(warm_fallbacks)
            .is_err()
        {
            info!("warm_fallbacks() could not be started off the UI thread");
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[cfg(not(target_arch = "wasm32"))]
    use std::sync::Arc;

    use super::{
        FallbackScript, any_probe_is_mapped, claim_warm_up, fallback_script_for_codepoint,
        first_open_match,
    };

    #[cfg(not(target_arch = "wasm32"))]
    use super::{
        FontData, FontRecord, cached_font_file, mapped_font_file, publish_font_file,
    };

    const TEST_FONT: &[u8] = include_bytes!("../../../fonts/JetBrainsMono-Regular.ttf");

    #[test]
    fn fallback_lane_classifier_keeps_scripts_distinct() {
        assert_eq!(
            fallback_script_for_codepoint('😀'),
            Some(FallbackScript::Emoji)
        );
        assert_eq!(
            fallback_script_for_codepoint('你'),
            Some(FallbackScript::Cjk)
        );
        assert_eq!(
            fallback_script_for_codepoint('한'),
            Some(FallbackScript::Hangul)
        );
        assert_eq!(
            fallback_script_for_codepoint('ع'),
            Some(FallbackScript::Arabic)
        );
        assert_eq!(fallback_script_for_codepoint('A'), None);
    }

    #[cfg(not(any(target_os = "ios", target_os = "macos")))]
    #[test]
    fn fallback_lane_ids_are_stable_and_disjoint() {
        for (index, script) in FallbackScript::ALL.iter().copied().enumerate() {
            assert_eq!(script.id_base(), super::FALLBACK_CHAIN_ID_BASE + index as u32 * 0x1000);
            assert_eq!(
                super::fallback_script_for_font_id(script.id_base()),
                Some(script)
            );
            assert_eq!(
                super::fallback_script_for_font_id(script.id_base() + 1),
                Some(script)
            );
        }
    }

    #[test]
    fn aimer_face_metadata_supports_a_registered_standard_font() {
        let record = FontRecord::from_bytes(7, TEST_FONT.to_vec())
            .expect("the checked-in face should pass the Aimer validator");

        assert!(record.glyph_index('A').is_some_and(|glyph_id| glyph_id != 0));
        assert_eq!(record.design_weight(), Some(400));
        assert!(record.has_standard_outline());
        assert!(!record.is_color);
        assert!(record
            .advance_width_for_glyph(record.glyph_index('A').unwrap(), 16.0)
            .is_some_and(|advance| advance > 0.0));
        assert!(record
            .line_metrics(16.0)
            .is_some_and(|(ascent, descent, _)| ascent > 0.0 && descent < 0.0));
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
    fn a_path_is_only_cached_once_a_mapping_has_been_published() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");

        assert!(
            cached_font_file(&path).is_none(),
            "a file nothing has mapped yet must not be reported as cached"
        );

        let mapped = mapped_font_file(&path).expect("readable file should map");

        assert_eq!(
            cached_font_file(&path)
                .expect("a mapping handed out by `mapped_font_file` is published")
                .as_ptr(),
            mapped.as_ptr(),
            "the lookup must return the very mapping that was published"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn publishing_a_second_mapping_keeps_the_one_already_shared() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs");
        let shared = mapped_font_file(&path).expect("readable file should map");

        let file = std::fs::File::open(&path).expect("readable file should open");
        // SAFETY: read-only mapping of a file that stays valid for the test.
        let private = Arc::new(unsafe { memmap2::Mmap::map(&file) }.expect("file should map"));
        assert_ne!(
            private.as_ptr(),
            shared.as_ptr(),
            "a private mapping is a distinct region"
        );

        let published = publish_font_file(&path, private);

        assert_eq!(
            published.as_ptr(),
            shared.as_ptr(),
            "publishing must hand back the mapping already in use, so pointer identity holds"
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

    #[test]
    fn a_face_claims_the_highest_priority_group_it_still_fits() {
        let open = [false, true, true];

        let claimed = first_open_match(&open, |group| (group >= 1).then_some(group == 2));

        assert_eq!(claimed, Some((1, false)));
    }

    #[test]
    fn a_face_matching_nothing_open_claims_no_group() {
        let open = [false, true, true];

        let claimed = first_open_match(&open, |group| (group == 0).then_some(false));

        assert!(claimed.is_none());
    }

    #[test]
    fn the_cheap_gate_stops_at_the_first_mapped_probe() {
        let mut looked_up = Vec::new();

        let mapped = any_probe_is_mapped(
            |codepoint| {
                looked_up.push(codepoint);
                (codepoint == '漢').then_some(42)
            },
            &['你', '漢', '한'],
        );

        assert!(mapped);
        assert_eq!(
            looked_up,
            vec!['你', '漢'],
            "the gate must answer as soon as one probe is mapped"
        );
    }

    #[test]
    fn the_cheap_gate_rejects_a_character_map_covering_no_probe() {
        assert!(!any_probe_is_mapped(|_| None, &['你', '漢', '한']));
        assert!(
            !any_probe_is_mapped(|_| Some(7), &[]),
            "a group without probes cannot be satisfied"
        );
    }

    #[test]
    fn only_the_first_claim_starts_the_warm_up() {
        let started = AtomicBool::new(false);

        assert!(
            claim_warm_up(&started),
            "the first caller is the one that must do the work"
        );
        assert!(!claim_warm_up(&started));
        assert!(!claim_warm_up(&started));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn concurrent_claims_start_the_warm_up_exactly_once() {
        let started = AtomicBool::new(false);
        let claims = std::sync::atomic::AtomicUsize::new(0);

        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    if claim_warm_up(&started) {
                        claims.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });

        assert_eq!(
            claims.load(Ordering::Relaxed),
            1,
            "racing callers must not each spawn a chain build"
        );
    }

    #[test]
    fn each_open_group_is_probed_at_most_once_and_filled_ones_never_are() {
        let open = [true, false, true, true];
        let mut probed = Vec::new();

        let claimed = first_open_match(&open, |group| {
            probed.push(group);
            (group == 2).then_some(true)
        });

        assert_eq!(claimed, Some((2, true)));
        assert_eq!(
            probed,
            vec![0, 2],
            "probing must stop at the claimed group and skip already filled ones"
        );
    }
}
