//! On-demand system font fallback, resolved one codepoint at a time.
//!
//! A pre-built fallback chain has to guess which faces the application will
//! need before it has seen a single character. The usual way to guess is a
//! table of script samples, but a face selected by a sample only guarantees
//! coverage of that sample: a Japanese face picked for `漢` has no glyph for
//! `你`, and no sample at all is taken for punctuation such as `！`, so those
//! characters end up drawn as `.notdef` boxes.
//!
//! This module removes the guess. When no already-loaded face covers a
//! codepoint, the platform is asked which face draws *that* codepoint, the
//! answer is memory-mapped, and it becomes part of the fallback set. Nothing
//! is loaded for text the application never renders.
//!
//! # Stable font ids
//!
//! Text preparation is parallel: shaping, layout and rasterization run in
//! separate [`GlyphRasterizer`] copies created from a shared snapshot, and a
//! glyph is handed between them as a `(font id, glyph id)` pair. A font id
//! assigned locally by whichever worker happened to resolve the codepoint
//! first would therefore mean different faces in different workers.
//!
//! The store lives process-wide instead: a face is identified by its
//! `(path, collection index)` pair and keeps one id for the lifetime of the
//! process, so every worker resolving the same codepoint observes the same id.
//! Ids start at [`SYSTEM_FALLBACK_ID_BASE`], which keeps them clear of the
//! statically built chain and of fonts registered at runtime.
//!
//! [`GlyphRasterizer`]: crate::text_pipeline::glyph_rasterizer::GlyphRasterizer

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, RwLock};

use crate::text_layout::FontId;
use crate::text_pipeline::font_resolver::FontRecord;

/// First font id handed out to a dynamically resolved system face.
///
/// The statically built chain numbers its faces from `1` upwards and runtime
/// registrations continue from there, so a high base keeps the two ranges from
/// ever meeting. It stays below the reserved monospace id (`0x7fff_fffe`).
pub(crate) const SYSTEM_FALLBACK_ID_BASE: FontId = 0x4000_0000;

/// Where a resolved face's bytes come from.
#[derive(Clone)]
enum FaceSource {
    /// A face backed by an on-disk font file, memory-mapped on use.
    File(PathBuf),
    /// A face reassembled from the platform's font tables, for fonts that no
    /// readable file backs, identified by its PostScript name.
    #[cfg_attr(not(any(target_os = "ios", target_os = "macos")), allow(dead_code))]
    Memory {
        postscript_name: String,
        bytes: Arc<[u8]>,
    },
}

/// A face discovered on demand, described by where its bytes come from.
struct ResolvedFace {
    source: FaceSource,
    collection_index: u32,
    is_color: bool,
}

/// Process-wide set of faces discovered so far, plus the answers already given.
#[derive(Default)]
struct FallbackStore {
    /// Discovered faces in discovery order; ids are `index + BASE`.
    records: Vec<FontRecord>,
    /// Maps a face's on-disk identity to the id it was assigned, so a font
    /// covering many scripts is mapped once and reused.
    ids_by_source: HashMap<(PathBuf, u32), FontId>,
    /// The same for faces carried as reassembled bytes, keyed by the
    /// PostScript name that identifies them in the platform's catalogue.
    ids_by_name: HashMap<String, FontId>,
    /// Answers for codepoints already looked up, including the negative ones,
    /// so an unresolvable codepoint costs one platform query in total. Keyed by
    /// the requirement as well, because the same character resolves to
    /// different faces depending on the script around it — see
    /// [`ScriptRequirement`].
    ids_by_codepoint: HashMap<(char, u64), Option<FontId>>,
}

impl FallbackStore {
    fn record_by_id(&self, font_id: FontId) -> Option<&FontRecord> {
        self.records.get(font_id.checked_sub(SYSTEM_FALLBACK_ID_BASE)? as usize)
    }

    /// Returns the id of `face`, registering it when it is seen the first time.
    fn intern(&mut self, face: ResolvedFace) -> FontId {
        let next_id = SYSTEM_FALLBACK_ID_BASE + self.records.len() as FontId;
        match face.source {
            FaceSource::File(path) => {
                let source = (path, face.collection_index);
                if let Some(font_id) = self.ids_by_source.get(&source) {
                    return *font_id;
                }
                self.records.push(FontRecord {
                    id: next_id,
                    bytes: None,
                    collection_index: source.1,
                    path: Some(Arc::new(source.0.clone())),
                    is_color: face.is_color,
                });
                self.ids_by_source.insert(source, next_id);
            }
            FaceSource::Memory {
                postscript_name,
                bytes,
            } => {
                if let Some(font_id) = self.ids_by_name.get(&postscript_name) {
                    return *font_id;
                }
                self.records.push(FontRecord {
                    id: next_id,
                    bytes: Some(bytes),
                    collection_index: face.collection_index,
                    path: None,
                    is_color: face.is_color,
                });
                self.ids_by_name.insert(postscript_name, next_id);
            }
        }
        next_id
    }
}

static STORE: LazyLock<RwLock<FallbackStore>> =
    LazyLock::new(|| RwLock::new(FallbackStore::default()));

/// Returns a face able to draw `codepoint`, asking the platform when needed.
///
/// The lookup is cached both ways: a resolved face is reused for every other
/// codepoint it covers, and a codepoint the platform cannot serve is
/// remembered as unresolvable so it is never queried again.
///
/// Returns `None` when the platform exposes no per-codepoint font matching
/// (every target except macOS and iOS today), or when no installed face covers
/// `codepoint`.
pub(crate) fn fallback_for_codepoint(
    codepoint: char,
    requirement: ScriptRequirement,
) -> Option<FontRecord> {
    let cache_key = (codepoint, requirement.fingerprint());
    if let Ok(store) = STORE.read()
        && let Some(cached) = store.ids_by_codepoint.get(&cache_key)
    {
        return cached.and_then(|font_id| store.record_by_id(font_id).cloned());
    }

    // Asking the platform is expensive, so faces discovered for earlier
    // codepoints get a chance first: one CJK face typically covers a whole
    // paragraph, turning every codepoint after the first into a cmap lookup.
    // A known face is accepted only if it also covers the rest of the
    // codepoint's script in the text being drawn, so a narrow face adopted for
    // a neighbouring script cannot claim half of a word — see
    // [`ScriptRequirement`] — and only if this crate decodes its glyphs itself:
    // a platform-only face admitted earlier as a last resort must not shadow a
    // decodable face a fresh resolution would find.
    let probes = requirement.as_slice();
    let known: Vec<FontRecord> = STORE.read().ok()?.records.clone();
    let decodes_script = |record: &FontRecord| {
        record_decodes_codepoint(record, codepoint) && record_decodes_all(record, probes)
    };
    if let Some(record) = known.iter().find(|record| decodes_script(record)).cloned() {
        STORE
            .write()
            .ok()?
            .ids_by_codepoint
            .insert(cache_key, Some(record.id));
        return Some(record);
    }

    // Resolution touches the file system, so it happens outside the lock. A
    // concurrent resolver may win the race; interning is idempotent, so the
    // duplicate work is wasted but never observable.
    let resolved = resolve_face_for_codepoint(codepoint, probes);

    let mut store = STORE.write().ok()?;
    let font_id = resolved.map(|face| store.intern(face)).or_else(|| {
        // Nothing covers the script as a whole — a platform that offers no
        // per-character matching, or a system carrying only a partial face.
        // Drawing the character in a narrow face still beats a blank box.
        known
            .iter()
            .find(|record| record_draws_codepoint(record, codepoint))
            .map(|record| record.id)
    });
    store.ids_by_codepoint.insert(cache_key, font_id);
    store.record_by_id(font_id?).cloned()
}

/// Returns a face previously handed out by [`fallback_for_codepoint`].
///
/// Workers that only rasterize receive glyph keys resolved elsewhere and never
/// run the lookup themselves; this is how they obtain the matching face.
pub(crate) fn fallback_by_id(font_id: FontId) -> Option<FontRecord> {
    STORE.read().ok()?.record_by_id(font_id).cloned()
}

/// Sample characters that a face serving `codepoint`'s script should cover.
///
/// Asking the platform about a single character is not enough to choose a
/// face. Apple's cascade answers with the face the *device's language* prefers,
/// so on a Japanese system every Han character a Japanese face happens to carry
/// — `你`, `好`, `多`, `就` — is served by Hiragino Sans, while the characters
/// only Chinese faces carry — `吗`, `顶` — fall through to PingFang. One word
/// then arrives in two typefaces of different stroke weight, which is what a
/// reader sees as "some characters are thinner".
///
/// The probes make the choice about the *script* instead of the character: a
/// candidate face is preferred only when it draws these too, which rules out a
/// face covering just the part of the script the platform's language prefers.
/// The Han probes are deliberately simplified-only characters, so the face that
/// wins is the one Apple ships for Chinese.
///
/// An empty slice means "no script-wide expectation": the character is judged
/// on its own, exactly as before.
///
/// The rule is not the platform query's alone: a face already loaded for a
/// neighbouring script sits in the rasterizer's own chain and is consulted
/// first, so [`GlyphRasterizer`] applies the same probes before accepting one.
///
/// [`GlyphRasterizer`]: crate::text_pipeline::glyph_rasterizer::GlyphRasterizer
pub(crate) fn script_probes(codepoint: char) -> &'static [char] {
    /// Simplified-only Han characters, absent from Japanese-only faces.
    const HAN: [char; 3] = ['吗', '顶', '这'];

    match codepoint as u32 {
        // CJK Unified Ideographs, its extension A, and the compatibility block:
        // the range a Japanese and a Chinese face both claim in part.
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF => &HAN,
        // Ideographic supplements live beyond the BMP and are carried by the
        // same faces.
        0x20000..=0x3FFFF => &HAN,
        _ => &[],
    }
}

/// The characters a face must draw to be accepted for a codepoint's script.
///
/// Han is unified, so a Japanese face carries every ideograph Japanese shares
/// with Chinese and none of the rest. Asked about one character at a time, the
/// platform hands out whichever face the device's language prefers, and a word
/// arrives in two typefaces of different stroke weight — the defect that made
/// `你好吗` render `好` lighter than its neighbours on a Japanese phone.
///
/// The requirement turns the question from "who draws this character" into "who
/// draws this script", and the honest answer to *which* script depends on the
/// text: `時` inside `あの時は` belongs to a Japanese word and must keep the face
/// its kana use, while `好` inside `你好吗` must not. So the preferred form is
/// [`ScriptRequirement::from_run`], built from the characters actually present
/// in the run being drawn; [`ScriptRequirement::probes`] is the contextless
/// fallback for callers holding a single character, and asks for the simplified
/// only samples a Japanese face never carries.
///
/// The set is a fixed inline array, so passing one costs no allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScriptRequirement {
    chars: [char; Self::CAPACITY],
    len: usize,
}

impl ScriptRequirement {
    /// Most characters a requirement keeps.
    ///
    /// A run longer than this is represented by its first distinct characters:
    /// the point is to tell one face's half of a script from another's, and a
    /// dozen ideographs settle that as surely as a thousand.
    const CAPACITY: usize = 12;

    /// A requirement any face satisfies — "judge the character on its own".
    pub(crate) const EMPTY: Self = Self {
        chars: ['\0'; Self::CAPACITY],
        len: 0,
    };

    /// The static samples for `codepoint`'s script, for callers with no run.
    pub(crate) fn probes(codepoint: char) -> Self {
        Self::from_chars(script_probes(codepoint).iter().copied())
    }

    /// The distinct characters of `run` belonging to a partly covered script.
    ///
    /// Empty when `run` holds none, which leaves every other script on the
    /// plain per-character path.
    pub(crate) fn from_run(run: &str) -> Self {
        Self::from_chars(
            run.chars()
                .filter(|codepoint| !script_probes(*codepoint).is_empty()),
        )
    }

    /// Reports whether any face satisfies this requirement.
    pub(crate) fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The characters a face must draw.
    pub(crate) fn as_slice(&self) -> &[char] {
        &self.chars[..self.len]
    }

    /// A stable key for caching the answers given for this requirement.
    ///
    /// Two requirements listing the same characters in the same order share a
    /// fingerprint; a different order merely misses the cache.
    pub(crate) fn fingerprint(&self) -> u64 {
        // FNV-1a: the set is at most a dozen characters, so a hasher would cost
        // more than the hash.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for codepoint in self.as_slice() {
            hash ^= u64::from(*codepoint as u32);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash
    }

    /// Collects distinct characters, stopping at [`Self::CAPACITY`].
    fn from_chars(source: impl Iterator<Item = char>) -> Self {
        let mut requirement = Self::EMPTY;
        for codepoint in source {
            if requirement.len == Self::CAPACITY {
                break;
            }
            if requirement.as_slice().contains(&codepoint) {
                continue;
            }
            requirement.chars[requirement.len] = codepoint;
            requirement.len += 1;
        }
        requirement
    }
}

/// Reports whether `record` can draw every character of `required`.
///
/// An empty `required` is satisfied by any face, which is what keeps scripts
/// without a probe set on the plain per-character path.
#[cfg(all(test, any(target_os = "ios", target_os = "macos")))]
fn record_draws_all(record: &FontRecord, required: &[char]) -> bool {
    required
        .iter()
        .all(|codepoint| record_draws_codepoint(record, *codepoint))
}

/// Reports whether this crate can decode every character of `required` from
/// `record` without the platform's rasterizer.
fn record_decodes_all(record: &FontRecord, required: &[char]) -> bool {
    required
        .iter()
        .all(|codepoint| record_decodes_codepoint(record, *codepoint))
}

/// Reports whether `record` can draw `codepoint`, not merely map it.
///
/// The distinction matters: font files exist whose character map covers a
/// codepoint while the face behind it carries no glyph data at all, and
/// accepting one of those reintroduces the blank boxes this module exists to
/// remove.
fn record_draws_codepoint(record: &FontRecord, codepoint: char) -> bool {
    if record_decodes_codepoint(record, codepoint) {
        return true;
    }
    record
        .glyph_index(codepoint)
        .is_some_and(|glyph_id| glyph_id != 0 && platform_draws_glyph(record, glyph_id))
}

/// Reports whether this crate can rasterize `codepoint` from `record` itself.
///
/// A face passes when its glyph carries data Cupid's own rasterizers decode —
/// a color strike or an outline with a non-empty bounding box. Faces that only
/// the platform's off-screen rasterizer can draw do not pass; those are
/// tolerated as a last resort, never preferred, because the platform layer
/// renders with different antialiasing and its private faces do not match the
/// stroke weight of the faces drawn by Cupid in the same line.
fn record_decodes_codepoint(record: &FontRecord, codepoint: char) -> bool {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};

    use crate::text_pipeline::font_resolver::font_ref;

    let Some(data) = record.data() else {
        return false;
    };
    let Some(face) = font_ref(data.as_ref(), record.collection_index) else {
        return false;
    };
    let Some(glyph_id) = face.charmap().map(codepoint) else {
        return false;
    };
    if glyph_id.to_u32() == 0 {
        return false;
    }
    record.is_color
        || face
            .glyph_metrics(Size::unscaled(), LocationRef::default())
            .bounds(glyph_id)
            .is_some()
}

/// Reports whether the platform can draw a glyph this crate cannot decode.
///
/// Apple's system faces keep their glyph data in private formats — `hvgl`
/// outlines, `emjc` strikes — so "no readable outline" does not mean "no
/// glyph": the platform rasterizer draws these, and rejecting the face would
/// leave the codepoint with no face at all.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_draws_glyph(record: &FontRecord, glyph_id: u16) -> bool {
    record.path().is_some_and(|path| {
        crate::text_pipeline::core_text_raster::draws_glyph(path, record.collection_index, glyph_id)
    })
}

/// Platforms whose fonts this crate decodes itself.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn platform_draws_glyph(_record: &FontRecord, _glyph_id: u16) -> bool {
    false
}

/// Asks the platform which installed face draws `codepoint`.
///
/// The platform proposes candidates in preference order and each is checked
/// with [`face_drawing`] until one can actually be drawn. Validation
/// is not optional: `CTFontCreateForString` never fails outright — it returns
/// the font it was queried with when nothing matches — and its preferred
/// answer for CJK on recent macOS builds is a private file carrying no glyph
/// data a third-party rasterizer can read.
///
/// Candidates are examined in two passes: a face whose glyphs this crate
/// decodes itself wins outright, and only when no such face covers the request
/// may one that needs the platform's off-screen rasterizer be chosen. The
/// platform layer renders with different antialiasing and its private CJK
/// faces do not match the stroke weight of the faces Cupid draws in the same
/// line, so it is a last resort, never a preference.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn resolve_face_for_codepoint(codepoint: char, probes: &[char]) -> Option<ResolvedFace> {
    use crate::text_pipeline::apple_fonts::{
        SystemFaceSource, fallback_font_path_for_probes, font_paths_for_codepoint,
        language_fallback_sources,
    };

    let required: Vec<char> = std::iter::once(codepoint)
        .chain(probes.iter().copied())
        .collect();

    // The language-aware cascades answer first: for every UI language the
    // platform names the concrete face it pairs with the system font — stroke
    // weight included — so their answers blend with the rest of the line. The
    // plain cascade and the catalogue matches follow as backstops.
    let mut candidates: Vec<FaceSource> = Vec::new();
    let push_path = |candidates: &mut Vec<FaceSource>, path: PathBuf| {
        let seen = candidates
            .iter()
            .any(|source| matches!(source, FaceSource::File(existing) if *existing == path));
        if !seen {
            candidates.push(FaceSource::File(path));
        }
    };
    for source in language_fallback_sources(&required) {
        match source {
            SystemFaceSource::File(path) => push_path(&mut candidates, path),
            SystemFaceSource::Data {
                postscript_name,
                bytes,
            } => candidates.push(FaceSource::Memory {
                postscript_name,
                bytes: Arc::from(bytes),
            }),
        }
    }
    if !probes.is_empty() {
        // Asked about the whole script at once, the platform proposes a face
        // covering all of it rather than the one its language prefers for this
        // single character.
        if let Some(path) = fallback_font_path_for_probes(&required) {
            push_path(&mut candidates, path);
        }
    }
    for path in font_paths_for_codepoint(codepoint) {
        push_path(&mut candidates, path);
    }

    // A face covering the script wins over one covering the character alone,
    // so a script no face covers whole still gets drawn instead of falling
    // back to a blank box; within each rung, self-decodable faces win over
    // platform-only ones.
    if !probes.is_empty() {
        for accept_platform in [false, true] {
            if let Some(face) = candidates
                .iter()
                .find_map(|source| face_drawing(source, &required, accept_platform))
            {
                return Some(face);
            }
        }
    }

    for accept_platform in [false, true] {
        if let Some(face) = candidates
            .iter()
            .find_map(|source| face_drawing(source, &[codepoint], accept_platform))
        {
            return Some(face);
        }
    }
    None
}

/// Returns the face of `source` able to draw every character of `required`.
///
/// A font file is not a face: collections (`.ttc`) hold many, and only some may
/// map the characters. Each face is checked for a mapping *and* for renderable
/// glyph data — a color strike or an outline with a non-empty bounding box —
/// because a cmap entry alone is satisfied by faces that draw nothing. Only
/// with `accept_platform` may a file-backed glyph that solely the platform
/// rasterizer draws qualify; reassembled bytes have no file the platform could
/// raster from, so they qualify on decodable data alone.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn face_drawing(
    source: &FaceSource,
    required: &[char],
    accept_platform: bool,
) -> Option<ResolvedFace> {
    match source {
        FaceSource::File(path) => {
            let file = std::fs::File::open(path).ok()?;
            // SAFETY: the read-only mapping is confined to this function; the
            // record built from it re-maps the file lazily through
            // `FontRecord::data`.
            let data = unsafe { memmap2::Mmap::map(&file).ok()? };
            let (collection_index, is_color) =
                face_in_data_drawing(data.as_ref(), required, |collection_index, glyph_id| {
                    accept_platform
                        && crate::text_pipeline::core_text_raster::draws_glyph(
                            path,
                            collection_index,
                            glyph_id,
                        )
                })?;
            Some(ResolvedFace {
                source: source.clone(),
                collection_index,
                is_color,
            })
        }
        FaceSource::Memory { bytes, .. } => {
            let (collection_index, is_color) =
                face_in_data_drawing(bytes.as_ref(), required, |_, _| false)?;
            Some(ResolvedFace {
                source: source.clone(),
                collection_index,
                is_color,
            })
        }
    }
}

/// Returns the first face of `data` whose glyphs cover all of `required`.
///
/// A glyph qualifies through a color strike, an outline with a non-empty
/// bounding box, or — for glyph data this crate cannot decode — through
/// `platform_draws`, which the caller scopes to what its candidate allows.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn face_in_data_drawing(
    data: &[u8],
    required: &[char],
    platform_draws: impl Fn(u32, u16) -> bool,
) -> Option<(u32, bool)> {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};

    use crate::text_pipeline::font_resolver::font_ref;

    let face_count = match skrifa::raw::FileRef::new(data).ok()? {
        skrifa::raw::FileRef::Collection(collection) => collection.len(),
        skrifa::raw::FileRef::Font(_) => 1,
    };
    (0..face_count).find_map(|collection_index| {
        let face = font_ref(data, collection_index)?;
        let is_color = FontRecord::face_is_color(&face);
        let draws = |codepoint: &char| {
            let Some(glyph_id) = face.charmap().map(*codepoint) else {
                return false;
            };
            if glyph_id.to_u32() == 0 {
                return false;
            }
            let has_outline = face
                .glyph_metrics(Size::unscaled(), LocationRef::default())
                .bounds(glyph_id)
                .is_some();
            is_color
                || has_outline
                || platform_draws(collection_index, glyph_id.to_u32() as u16)
        };
        if !required.iter().all(draws) {
            return None;
        }
        Some((collection_index, is_color))
    })
}

/// Platforms without per-codepoint font matching keep the pre-built chain.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn resolve_face_for_codepoint(_codepoint: char, _probes: &[char]) -> Option<ResolvedFace> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn resolves_a_codepoint_no_probe_group_covers() {
        let record = fallback_for_codepoint('！', ScriptRequirement::probes('！'))
            .expect("fullwidth punctuation must resolve");
        assert!(record.id >= SYSTEM_FALLBACK_ID_BASE);
        assert!(matches!(record.glyph_index('！'), Some(glyph_id) if glyph_id != 0));
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn assigns_one_stable_id_per_face() {
        let requirement = ScriptRequirement::probes('你');
        let first = fallback_for_codepoint('你', requirement).expect("han characters must resolve");
        let again =
            fallback_for_codepoint('你', requirement).expect("cached lookups must resolve too");
        assert_eq!(first.id, again.id);

        let shared_face =
            fallback_for_codepoint('好', requirement).expect("han characters must resolve");
        assert_eq!(
            shared_face.id, first.id,
            "codepoints served by the same file must share one id"
        );
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn exposes_resolved_faces_by_id_for_rasterizing_workers() {
        let record = fallback_for_codepoint('한', ScriptRequirement::probes('한'))
            .expect("hangul must resolve");
        let by_id = fallback_by_id(record.id).expect("a resolved face must be addressable by id");
        assert_eq!(by_id.id, record.id);
        assert_eq!(by_id.collection_index, record.collection_index);
    }

    // A kanji standing among kana belongs to a Japanese word, so the only
    // coverage its face owes is that word's own ideographs — asking for
    // simplified-only samples would push it onto a Chinese face and leave the
    // kana beside it in a lighter one.
    #[test]
    fn a_japanese_run_requires_only_the_ideographs_it_contains() {
        assert_eq!(
            ScriptRequirement::from_run("あの時は").as_slice(),
            &['時'],
            "a japanese run may not demand chinese-only coverage"
        );
    }

    #[test]
    fn a_chinese_run_requires_every_ideograph_it_contains() {
        assert_eq!(
            ScriptRequirement::from_run("你好吗").as_slice(),
            &['你', '好', '吗'],
            "a face drawing half of a chinese word may not be accepted for it"
        );
    }

    #[test]
    fn runs_without_ideographs_require_nothing() {
        for run in ["Hello, world!", "あいうえお", "한글", "😀"] {
            assert!(
                ScriptRequirement::from_run(run).is_empty(),
                "{run:?} holds no script a face may cover in part"
            );
        }
    }

    #[test]
    fn a_requirement_keeps_distinct_characters_within_its_capacity() {
        let repeated = ScriptRequirement::from_run("時時時好");
        assert_eq!(repeated.as_slice(), &['時', '好']);

        let long: String = ('\u{4e00}'..='\u{4e20}').collect();
        assert_eq!(
            ScriptRequirement::from_run(&long).as_slice().len(),
            ScriptRequirement::CAPACITY,
            "a long run must stay within the inline array"
        );
    }

    #[test]
    fn requirements_are_fingerprinted_by_their_characters() {
        assert_eq!(
            ScriptRequirement::from_run("あの時は").fingerprint(),
            ScriptRequirement::from_run("時").fingerprint(),
            "the same demanded characters must share one cache key"
        );
        assert_ne!(
            ScriptRequirement::from_run("時").fingerprint(),
            ScriptRequirement::from_run("你好吗").fingerprint(),
            "different demands may not share cached answers"
        );
        assert_ne!(
            ScriptRequirement::EMPTY.fingerprint(),
            ScriptRequirement::from_run("時").fingerprint()
        );
    }

    #[test]
    fn only_han_asks_for_script_wide_coverage() {
        for codepoint in ['你', '吗', '漢', '\u{3400}', '\u{20000}'] {
            assert!(
                !script_probes(codepoint).is_empty(),
                "{codepoint:?} shares its block with faces covering only part of it"
            );
        }
        for codepoint in ['A', 'あ', 'ア', '한', '！', '😀', 'ü'] {
            assert!(
                script_probes(codepoint).is_empty(),
                "{codepoint:?} needs no script-wide probe"
            );
        }
    }

    // The defect this rule exists for: Apple's cascade answers per character
    // and per *device language*, so on a Japanese system the Han characters a
    // Japanese face happens to carry are served by it and the simplified-only
    // ones fall through to a Chinese face — one word, two typefaces, two
    // stroke weights.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn one_word_of_han_is_served_by_one_face() {
        let faces: Vec<_> = "你好吗？顶多就"
            .chars()
            .filter(|codepoint| !script_probes(*codepoint).is_empty())
            .map(|codepoint| {
                let record = fallback_for_codepoint(codepoint, ScriptRequirement::probes(codepoint))
                    .expect("han must resolve");
                (codepoint, record.id, record.collection_index)
            })
            .collect();

        let first = faces.first().expect("the sample carries han characters").1;
        assert!(
            faces.iter().all(|(_, id, _)| *id == first),
            "han characters split across faces: {faces:?}"
        );
    }

    // The device's language decides which of these Core Text offers first, so
    // the rule cannot be asserted through the cascade on a developer machine.
    // It can be asserted where it acts: a face carrying only the Japanese half
    // of Han must be rejected for `你` even though it draws `你` perfectly.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn a_face_covering_only_half_of_han_is_not_chosen_for_it() {
        use crate::text_pipeline::apple_fonts::font_paths_for_codepoint;

        let required: Vec<char> = std::iter::once('你')
            .chain(script_probes('你').iter().copied())
            .collect();
        let partial: Vec<_> = font_paths_for_codepoint('你')
            .into_iter()
            .filter(|path| {
                let source = FaceSource::File(path.clone());
                face_drawing(&source, &['你'], true).is_some()
                    && face_drawing(&source, &required, true).is_none()
            })
            .collect();

        assert!(
            !partial.is_empty(),
            "apple systems always ship a japanese face carrying 你 but not 吗"
        );

        let chosen =
            resolve_face_for_codepoint('你', script_probes('你')).expect("han must resolve");
        let FaceSource::File(chosen_path) = &chosen.source else {
            return; // A reassembled face is never one of the partial files.
        };
        assert!(
            !partial.contains(chosen_path),
            "chose a face covering only part of han: {chosen_path:?}"
        );
    }

    // The screenshot defect this fix exists for: a Japanese line holding a few
    // simplified-only characters had its whole Han run pushed onto a private
    // face only the platform's off-screen rasterizer could draw, so every
    // kanji came back heavier than the kana beside it. Apple pairs the UI font
    // of every CJK language with faces Cupid decodes itself (Hiragino Sans GB
    // for simplified Han on a Japanese system), so whenever such a face covers
    // the requirement it must win over a platform-only face.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn han_resolves_to_a_face_cupid_decodes_itself() {
        for run in ["你好吗", "あの時は你好吗"] {
            let requirement = ScriptRequirement::from_run(run);
            for codepoint in run.chars().filter(|cp| !script_probes(*cp).is_empty()) {
                let record = fallback_for_codepoint(codepoint, requirement)
                    .expect("han must resolve");
                assert!(
                    record_decodes_codepoint(&record, codepoint),
                    "{codepoint:?} of {run:?} resolved to a face only the platform \
                     rasterizer draws: {:?}",
                    record.path
                );
            }
        }
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn the_han_face_draws_the_probes_it_was_chosen_for() {
        let record =
            fallback_for_codepoint('你', ScriptRequirement::probes('你')).expect("han must resolve");

        assert!(record_draws_all(&record, script_probes('你')));
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn detects_color_faces_for_emoji() {
        let record = fallback_for_codepoint('😀', ScriptRequirement::EMPTY)
            .expect("emoji must resolve");
        assert!(record.is_color, "emoji faces carry color glyph tables");
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn resolved_faces_can_actually_be_rasterized() {
        for codepoint in ['你', '好', '！', '，'] {
            let record = fallback_for_codepoint(codepoint, ScriptRequirement::probes(codepoint))
                .expect("must resolve");
            let glyph_id = record
                .glyph_index(codepoint)
                .expect("a resolved face maps the codepoint");
            assert!(
                record_draws_codepoint(&record, codepoint),
                "{codepoint:?} resolved to a face without drawable glyph {glyph_id}"
            );
        }
    }

    #[test]
    fn unknown_ids_resolve_to_nothing() {
        assert!(fallback_by_id(0).is_none());
        assert!(fallback_by_id(SYSTEM_FALLBACK_ID_BASE - 1).is_none());
        assert!(fallback_by_id(FontId::MAX).is_none());
    }
}
