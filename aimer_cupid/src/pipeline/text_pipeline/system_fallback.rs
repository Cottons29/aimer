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

/// A face discovered on demand, described by where it lives on disk.
struct ResolvedFace {
    path: PathBuf,
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
    /// Answers for codepoints already looked up, including the negative ones,
    /// so an unresolvable codepoint costs one platform query in total.
    ids_by_codepoint: HashMap<char, Option<FontId>>,
}

impl FallbackStore {
    fn record_by_id(&self, font_id: FontId) -> Option<&FontRecord> {
        self.records.get(font_id.checked_sub(SYSTEM_FALLBACK_ID_BASE)? as usize)
    }

    /// Returns the id of `face`, registering it when it is seen the first time.
    fn intern(&mut self, face: ResolvedFace) -> FontId {
        let source = (face.path, face.collection_index);
        if let Some(font_id) = self.ids_by_source.get(&source) {
            return *font_id;
        }
        let font_id = SYSTEM_FALLBACK_ID_BASE + self.records.len() as FontId;
        self.records.push(FontRecord {
            id: font_id,
            bytes: None,
            collection_index: source.1,
            _path: Some(Arc::new(source.0.clone())),
            is_color: face.is_color,
        });
        self.ids_by_source.insert(source, font_id);
        font_id
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
pub(crate) fn fallback_for_codepoint(codepoint: char) -> Option<FontRecord> {
    if let Ok(store) = STORE.read()
        && let Some(cached) = store.ids_by_codepoint.get(&codepoint)
    {
        return cached.and_then(|font_id| store.record_by_id(font_id).cloned());
    }

    // Asking the platform is expensive, so faces discovered for earlier
    // codepoints get a chance first: one CJK face typically covers a whole
    // paragraph, turning every codepoint after the first into a cmap lookup.
    let known: Vec<FontRecord> = STORE.read().ok()?.records.clone();
    if let Some(record) = known
        .into_iter()
        .find(|record| record_draws_codepoint(record, codepoint))
    {
        STORE
            .write()
            .ok()?
            .ids_by_codepoint
            .insert(codepoint, Some(record.id));
        return Some(record);
    }

    // Resolution touches the file system, so it happens outside the lock. A
    // concurrent resolver may win the race; interning is idempotent, so the
    // duplicate work is wasted but never observable.
    let resolved = resolve_face_for_codepoint(codepoint);

    let mut store = STORE.write().ok()?;
    let font_id = resolved.map(|face| store.intern(face));
    store.ids_by_codepoint.insert(codepoint, font_id);
    store.record_by_id(font_id?).cloned()
}

/// Returns a face previously handed out by [`fallback_for_codepoint`].
///
/// Workers that only rasterize receive glyph keys resolved elsewhere and never
/// run the lookup themselves; this is how they obtain the matching face.
pub(crate) fn fallback_by_id(font_id: FontId) -> Option<FontRecord> {
    STORE.read().ok()?.record_by_id(font_id).cloned()
}

/// Reports whether `record` can draw `codepoint`, not merely map it.
///
/// The distinction matters: font files exist whose character map covers a
/// codepoint while the face behind it carries no glyph data at all, and
/// accepting one of those reintroduces the blank boxes this module exists to
/// remove.
fn record_draws_codepoint(record: &FontRecord, codepoint: char) -> bool {
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

/// Asks the platform which installed face draws `codepoint`.
///
/// The platform proposes candidates in preference order and each is checked
/// with [`face_drawing_codepoint`] until one can actually be drawn. Validation
/// is not optional: `CTFontCreateForString` never fails outright — it returns
/// the font it was queried with when nothing matches — and its preferred
/// answer for CJK on recent macOS builds is a private file carrying no glyph
/// data a third-party rasterizer can read.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn resolve_face_for_codepoint(codepoint: char) -> Option<ResolvedFace> {
    crate::text_pipeline::apple_fonts::font_paths_for_codepoint(codepoint)
        .into_iter()
        .find_map(|path| face_drawing_codepoint(path, codepoint))
}

/// Returns the face of `path` able to draw `codepoint`, if the file holds one.
///
/// A font file is not a face: collections (`.ttc`) hold many, and only some may
/// map the codepoint. Each face is checked for a mapping *and* for renderable
/// glyph data — a color strike, or an outline with a non-empty bounding box —
/// because a cmap entry alone is satisfied by faces that draw nothing.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn face_drawing_codepoint(path: PathBuf, codepoint: char) -> Option<ResolvedFace> {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};

    use crate::text_pipeline::font_resolver::font_ref;

    let file = std::fs::File::open(&path).ok()?;
    // SAFETY: the read-only mapping is confined to this function; the record
    // built from it re-maps the file lazily through `FontRecord::data`.
    let data = unsafe { memmap2::Mmap::map(&file).ok()? };

    let face_count = match skrifa::raw::FileRef::new(data.as_ref()).ok()? {
        skrifa::raw::FileRef::Collection(collection) => collection.len(),
        skrifa::raw::FileRef::Font(_) => 1,
    };
    (0..face_count).find_map(|collection_index| {
        let face = font_ref(data.as_ref(), collection_index)?;
        let glyph_id = face.charmap().map(codepoint)?;
        if glyph_id.to_u32() == 0 {
            return None;
        }
        let is_color = FontRecord::face_is_color(&face);
        let has_outline = face
            .glyph_metrics(Size::unscaled(), LocationRef::default())
            .bounds(glyph_id)
            .is_some();
        if !is_color && !has_outline {
            return None;
        }
        Some(ResolvedFace {
            path: path.clone(),
            collection_index,
            is_color,
        })
    })
}

/// Platforms without per-codepoint font matching keep the pre-built chain.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn resolve_face_for_codepoint(_codepoint: char) -> Option<ResolvedFace> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn resolves_a_codepoint_no_probe_group_covers() {
        let record = fallback_for_codepoint('！').expect("fullwidth punctuation must resolve");
        assert!(record.id >= SYSTEM_FALLBACK_ID_BASE);
        assert!(matches!(record.glyph_index('！'), Some(glyph_id) if glyph_id != 0));
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn assigns_one_stable_id_per_face() {
        let first = fallback_for_codepoint('你').expect("han characters must resolve");
        let again = fallback_for_codepoint('你').expect("cached lookups must resolve too");
        assert_eq!(first.id, again.id);

        let shared_face = fallback_for_codepoint('好').expect("han characters must resolve");
        assert_eq!(
            shared_face.id, first.id,
            "codepoints served by the same file must share one id"
        );
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn exposes_resolved_faces_by_id_for_rasterizing_workers() {
        let record = fallback_for_codepoint('한').expect("hangul must resolve");
        let by_id = fallback_by_id(record.id).expect("a resolved face must be addressable by id");
        assert_eq!(by_id.id, record.id);
        assert_eq!(by_id.collection_index, record.collection_index);
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn detects_color_faces_for_emoji() {
        let record = fallback_for_codepoint('😀').expect("emoji must resolve");
        assert!(record.is_color, "emoji faces carry color glyph tables");
    }

    #[cfg(any(target_os = "ios", target_os = "macos"))]
    #[test]
    fn resolved_faces_can_actually_be_rasterized() {
        for codepoint in ['你', '好', '！', '，'] {
            let record = fallback_for_codepoint(codepoint).expect("must resolve");
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
