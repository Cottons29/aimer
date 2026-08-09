#[cfg(not(target_arch = "wasm32"))]
use std::collections::HashMap;
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
    ///
    /// This is [`any_probe_is_mapped`] applied to an already-parsed face. The
    /// two must stay the same predicate: the fallback chain builder uses the
    /// standalone form as a gate in front of the parse, and a gate that
    /// rejected a face this accepts would silently drop it from the chain.
    #[cfg_attr(any(target_os = "ios", target_os = "macos"), allow(dead_code))]
    fn probes_match(face: &FontRef<'_>, probes: &[char]) -> bool {
        let charmap = face.charmap();
        any_probe_is_mapped(|codepoint| charmap.map(codepoint).map(|id| id.to_u32()), probes)
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

/// Advances of `glyph_ids`, in order, from one reading of the face.
///
/// Metrics are a property of the face at a size, not of a glyph: obtaining
/// them parses the face and its horizontal metrics tables, and the result then
/// answers for every glyph. Asking per glyph — as
/// [`advance_width_from_face`] does — repeats that reading once per glyph,
/// which is why a run of glyphs asks here instead.
///
/// An element is `None` where the face has no advance for that glyph, which is
/// what tells the caller the face cannot draw it. Returns `None` only when the
/// face itself cannot be read.
pub fn advance_widths_from_face(
    bytes: &[u8],
    collection_index: u32,
    glyph_ids: impl IntoIterator<Item = u16>,
    font_size: f32,
) -> Option<Vec<Option<f32>>> {
    let face = font_ref(bytes, collection_index)?;
    let metrics = face.glyph_metrics(Size::new(font_size), LocationRef::default());

    Some(
        glyph_ids
            .into_iter()
            .map(|glyph_id| metrics.advance_width(GlyphId::new(u32::from(glyph_id))))
            .collect(),
    )
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
fn face_matches_probes(face: &FontRef<'_>, probes: &[char], hint_color: bool) -> Option<bool> {
    if !FontRecord::probes_match(face, probes) {
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

/// Build the fallback chain dynamically from Fontique's platform font sources.
///
/// Every face the platform exposes is visited exactly once: its bytes are
/// mapped once, parsed at most once, and offered to the probe groups that are
/// still unfilled via [`first_open_match`]. Faces that match nothing are
/// recorded as seen so a second family listing the same file costs nothing.
///
/// A face is only parsed when its character map — which the enumerator has
/// already indexed, so consulting it needs no parse — reports coverage of a
/// script still missing from the chain. Most installed faces cover none, so
/// most are dismissed on the character map alone.
///
/// The previous shape — one full sweep of the font set per probe group — made
/// the cost `groups x faces`: a face rejected by the first group was re-opened,
/// re-mapped and re-parsed for each of the remaining ones, and groups with no
/// installed font (Yi, Cherokee, Mongolian on a typical desktop) always paid
/// for a complete scan. On a distribution shipping several hundred font files
/// that is tens of thousands of redundant parses, run synchronously on the
/// thread rasterizing the frame.
///
/// The result is a flat `Vec<FontRecord>` ordered by probe group priority, with
/// ids assigned consecutively from `next_id`.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn build_fallback_chain(next_id: FontId) -> Vec<FontRecord> {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: !cfg!(target_arch = "wasm32"),
    });
    let family_names: Vec<String> = collection.family_names().map(str::to_owned).collect();

    let mut slots: Vec<Option<FontRecord>> = (0..PROBE_GROUPS.len()).map(|_| None).collect();
    let mut open = vec![true; PROBE_GROUPS.len()];
    let mut open_count = PROBE_GROUPS.len();
    let mut seen_ids = HashSet::new();

    'families: for family_name in &family_names {
        let Some(family) = collection.family_by_name(family_name) else {
            continue;
        };
        for font_info in family.fonts() {
            let source_id = font_info.source().id();
            let ci = font_info.index();
            if !seen_ids.insert((source_id, ci)) {
                continue;
            }

            // The mapping used for probing is the shared one whenever the file
            // is already published, and a private one otherwise: the cache
            // never evicts, so filling it from a sweep of the entire font set
            // would pin one mapping per installed font. Only the face that
            // ends up in the chain is handed over to the cache, below.
            let probe_mapping = match font_info.source().kind() {
                SourceKind::Path(path) => match probe_font_file(path.as_ref()) {
                    Some(mapping) => Some(mapping),
                    None => continue,
                },
                SourceKind::Memory(_) => None,
            };
            let (data, record_bytes, record_path): (&[u8], _, _) = match font_info.source().kind() {
                SourceKind::Path(path) => (
                    probe_mapping.as_ref().map_or(&[][..], |mapping| &mapping[..]),
                    None,
                    Some(Arc::new(path.as_ref().to_path_buf())),
                ),
                SourceKind::Memory(bytes) => {
                    (bytes.as_ref(), Some(Arc::from(bytes.as_ref())), None)
                }
            };

            // The character map is the cheap gate in front of the parse: the
            // enumerator already located the `cmap` subtable of every face, so
            // asking it whether a script is present at all costs a bounds check
            // and a lookup, while `font_ref` walks the table directory. A face
            // covering none of the scripts still wanted — which is nearly every
            // installed face, once the common ones are placed — is therefore
            // rejected without ever being parsed.
            let Some(charmap) = font_info.charmap_index().charmap(data) else {
                continue;
            };

            let mut parsed = None;
            let Some((group, is_color)) = first_open_match(&open, |group| {
                let group = &PROBE_GROUPS[group];
                if !any_probe_is_mapped(|codepoint| charmap.map(codepoint), group.probes) {
                    return None;
                }
                let face = parsed.get_or_insert_with(|| font_ref(data, ci)).as_ref()?;
                face_matches_probes(face, group.probes, group.hint_color)
            }) else {
                continue;
            };

            slots[group] = Some(FontRecord {
                id: next_id,
                bytes: record_bytes,
                collection_index: ci,
                path: record_path.clone(),
                is_color,
            });
            if let (Some(path), Some(mapping)) = (record_path, probe_mapping) {
                retain_probed_font_file(path.as_path(), mapping);
            }
            open[group] = false;
            open_count -= 1;
            if open_count == 0 {
                break 'families;
            }
        }
    }

    slots
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, record)| FontRecord {
            id: next_id + index as FontId,
            ..record
        })
        .collect()
}

pub fn shared_fallback_chain() -> Vec<FontRecord> {
    static FALLBACKS: OnceLock<Vec<FontRecord>> = OnceLock::new();
    FALLBACKS.get_or_init(|| build_fallback_chain(1)).clone()
}

/// Pre-build the fallback chain and validate each fallback face with
/// Skrifa, avoiding eager whole-font parsing during warmup. Safe to call
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

    use skrifa::MetadataProvider;

    use super::{any_probe_is_mapped, claim_warm_up, first_open_match};
    
    #[cfg(not(target_arch = "wasm32"))]
    use super::{
        FontData, FontRecord, cached_font_file, font_ref, mapped_font_file, publish_font_file,
    };

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

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_cheap_gate_agrees_with_the_full_face_probe() {
        let face = font_ref(TEST_FONT, 0).expect("bundled test font should parse");
        let charmap = face.charmap();

        for probes in [
            &['A', 'B'][..],         // covered by the bundled monospace face
            &['你', '漢', '한'][..], // not covered by it
        ] {
            let gate = any_probe_is_mapped(
                |codepoint| charmap.map(codepoint).map(|id| id.to_u32()),
                probes,
            );

            assert_eq!(
                gate,
                FontRecord::probes_match(&face, probes),
                "the gate must admit exactly the faces the full probe accepts"
            );
        }
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
