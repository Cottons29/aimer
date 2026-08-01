//! Dynamic system font discovery on Apple platforms.
//!
//! macOS and iOS ship their font catalogue behind Core Text rather than a
//! directory that can be enumerated cheaply. Walking every installed face —
//! which is what generic font databases do — costs hundreds of milliseconds at
//! startup and keeps large CJK and emoji faces resident in memory, so Cupid
//! instead asks Core Text three narrow questions:
//!
//! * [`system_font_path`] — "where does the family called *X* live?"
//! * [`fallback_font_path_for_probes`] — "which face would you use to draw
//!   these codepoints?"
//! * [`font_paths_for_codepoint`] — "which faces *could* draw this character?"
//!
//! All answer with file system paths. The caller memory-maps those files, which
//! keeps the bytes out of the heap and lets the kernel evict them under
//! pressure.
//!
//! Core Text's preferred answer is not always usable by a third-party
//! rasterizer: on recent macOS builds the system UI cascade resolves CJK to
//! `PingFangUI.ttc`, a private file whose faces expose neither `glyf` outlines
//! nor bitmap strikes, so rendering it produces blank glyphs. This is why
//! [`font_paths_for_codepoint`] returns a *list* — the caller validates the
//! candidates in order and keeps the first one it can actually draw.
//!
//! The bindings come from [`objc2_core_text`] and [`objc2_core_foundation`].
//! Every Core Text object is returned as a [`CFRetained`] handle, so ownership
//! follows the Core Foundation *Create Rule* automatically and no manual
//! `CFRelease` is needed.
//!
//! [`CFRetained`]: objc2_core_foundation::CFRetained

use std::path::PathBuf;

use objc2_core_foundation::{CFCharacterSet, CFDictionary, CFRange, CFString, CFURL};
use objc2_core_text::{
    CTFont, CTFontDescriptor, kCTFontCharacterSetAttribute, kCTFontURLAttribute,
};

/// Point size used for the transient `CTFont` instances created here.
///
/// Font *selection* is size independent — only metrics scale with the point
/// size — so any positive value works. A concrete value still has to be passed
/// because `0.0` means "use the descriptor's size" to Core Text.
const LOOKUP_FONT_SIZE: f64 = 12.0;

/// The system UI font, used as the root of Core Text's cascade list.
///
/// Resolving a fallback always starts from a *current* font; starting from the
/// system UI font yields the same chain the platform itself would use.
const SYSTEM_UI_FONT_NAME: &str = ".AppleSystemUIFont";

/// Upper bound on the candidates reported for a single codepoint.
///
/// Widely covered characters — Latin punctuation, common Han ideographs — are
/// carried by dozens of installed faces. The first usable one wins, so the tail
/// of that list is never reached in practice and only costs memory.
const MAX_CANDIDATE_FACES: usize = 16;

/// Returns the file the given font was loaded from, if it is backed by one.
///
/// Fonts registered from memory (for example a `CGFont` built from a data
/// provider) have no `kCTFontURLAttribute`, and fonts served from the shared
/// on-demand font asset store may resolve to a non-`file://` URL. Both cases
/// return `None` rather than a path that cannot be opened.
fn font_file_path(font: &CTFont) -> Option<PathBuf> {
    // SAFETY: `kCTFontURLAttribute` is a Core Text constant string and `font`
    // is a valid, retained font reference.
    let attribute = unsafe { font.attribute(kCTFontURLAttribute) }?;
    let url = attribute.downcast::<CFURL>().ok()?;
    url.to_file_path()
}

/// Resolves a font family name to the file backing its default face.
///
/// The lookup goes through `CTFontCreateWithName`, which performs the same
/// best-match resolution as the rest of the system: an exact family match wins,
/// otherwise Core Text substitutes the closest available face. Because a match
/// is always produced, the returned path is only `None` when the resolved face
/// has no file backing it.
///
/// # Examples
///
/// ```ignore
/// let path = system_font_path("Helvetica").expect("Helvetica is always installed");
/// assert!(path.exists());
/// ```
pub(crate) fn system_font_path(family: &str) -> Option<PathBuf> {
    if family.is_empty() {
        return None;
    }
    let name = CFString::from_str(family);
    // SAFETY: a null matrix requests the identity transform, which is what the
    // documentation prescribes for "no transform".
    let font = unsafe { CTFont::with_name(&name, LOOKUP_FONT_SIZE, std::ptr::null()) };
    font_file_path(&font)
}

/// Resolves the file of the face Core Text would fall back to for `probes`.
///
/// The probes are joined into a single string and handed to
/// `CTFontCreateForString` alongside the system UI font. Core Text walks its
/// cascade list and returns the first face able to draw the string — the same
/// decision it makes when rendering text the current font cannot cover.
///
/// Returns `None` when `probes` is empty or when the chosen face is not backed
/// by a file. Note that Core Text never fails outright: if nothing covers the
/// probes it hands back the current font, so callers must still verify that the
/// returned face actually contains glyphs for the codepoints they care about.
pub(crate) fn fallback_font_path_for_probes(probes: &[char]) -> Option<PathBuf> {
    let sample: String = probes.iter().collect();
    // `CFRange` counts UTF-16 code units, not `char`s, so astral-plane probes
    // such as emoji contribute two units each.
    let length = sample.encode_utf16().count() as isize;
    if length == 0 {
        return None;
    }

    let base_name = CFString::from_str(SYSTEM_UI_FONT_NAME);
    let sample = CFString::from_str(&sample);
    // SAFETY: a null matrix requests the identity transform.
    let base_font = unsafe { CTFont::with_name(&base_name, LOOKUP_FONT_SIZE, std::ptr::null()) };
    // SAFETY: the range spans exactly the UTF-16 content of `sample`.
    let fallback = unsafe { base_font.for_string(&sample, CFRange { location: 0, length }) };

    font_file_path(&fallback)
}

/// Returns the files of every face that can draw `codepoint`, best first.
///
/// This is the query that drives on-demand fallback: instead of guessing which
/// faces might be needed from a table of script samples, the system is asked
/// about the one character that could not be drawn.
///
/// The order encodes decreasing confidence:
///
/// 1. the face `CTFontCreateForString` picks — what the platform itself would
///    render the character with;
/// 2. every face matching a descriptor built from a character set holding just
///    `codepoint`, which is the full set of installed faces covering it.
///
/// Paths are deduplicated and the list is capped at [`MAX_CANDIDATE_FACES`],
/// because the caller pays a memory map plus a font parse per candidate it has
/// to reject.
pub(crate) fn font_paths_for_codepoint(codepoint: char) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut push = |path: PathBuf| {
        if paths.len() < MAX_CANDIDATE_FACES && !paths.contains(&path) {
            paths.push(path);
        }
    };

    if let Some(preferred) = fallback_font_path_for_probes(&[codepoint]) {
        push(preferred);
    }
    for path in matching_font_paths_for_codepoint(codepoint) {
        push(path);
    }

    paths
}

/// Returns every installed face whose character set contains `codepoint`.
///
/// Unlike `CTFontCreateForString` this ignores the cascade list and asks the
/// font catalogue directly, so it also surfaces faces the system would never
/// pick on its own. The order Core Text returns is preserved.
fn matching_font_paths_for_codepoint(codepoint: char) -> Vec<PathBuf> {
    let sample = CFString::from_str(&codepoint.to_string());
    // SAFETY: both arguments are valid; `None` requests the default allocator.
    let Some(character_set) =
        (unsafe { CFCharacterSet::with_characters_in_string(None, Some(&sample)) })
    else {
        return Vec::new();
    };

    // SAFETY: `kCTFontCharacterSetAttribute` is a Core Text constant string.
    let attribute_key = unsafe { kCTFontCharacterSetAttribute };
    let attributes = CFDictionary::from_slices(&[attribute_key], &[&*character_set]);
    // SAFETY: the dictionary holds a documented attribute key paired with the
    // `CFCharacterSet` value that key expects.
    let descriptor = unsafe { CTFontDescriptor::with_attributes(attributes.as_opaque()) };
    // SAFETY: no mandatory attributes are requested, so no generic type has to
    // line up.
    let Some(matches) = (unsafe { descriptor.matching_font_descriptors(None) }) else {
        return Vec::new();
    };

    // SAFETY: `CTFontDescriptorCreateMatchingFontDescriptors` is documented to
    // return an array of font descriptors.
    let matches = unsafe { matches.cast_unchecked::<CTFontDescriptor>() };

    matches
        .iter()
        .filter_map(|candidate| {
            // SAFETY: `kCTFontURLAttribute` is a Core Text constant string.
            let url = unsafe { candidate.attribute(kCTFontURLAttribute) }?;
            url.downcast::<CFURL>().ok()?.to_file_path()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_paths_for_codepoint_lists_the_preferred_face_first() {
        let preferred =
            fallback_font_path_for_probes(&['你']).expect("the system always proposes a face");
        let paths = font_paths_for_codepoint('你');
        assert_eq!(paths.first(), Some(&preferred));
    }

    #[test]
    fn font_paths_for_codepoint_offers_alternatives_to_the_preferred_face() {
        // Han ideographs are carried by many installed faces, so the platform's
        // first pick must not be the only option — rejecting it has to leave
        // something to fall back on.
        let paths = font_paths_for_codepoint('你');
        assert!(
            paths.len() > 1,
            "expected alternatives beyond the preferred face, got {paths:?}"
        );
        assert!(paths.iter().all(|path| path.exists()));
    }

    #[test]
    fn font_paths_for_codepoint_deduplicates_and_stays_bounded() {
        let paths = font_paths_for_codepoint('！');
        assert!(paths.len() <= MAX_CANDIDATE_FACES);
        let unique: std::collections::HashSet<_> = paths.iter().collect();
        assert_eq!(unique.len(), paths.len(), "duplicate candidates: {paths:?}");
    }

    #[test]
    fn font_paths_for_codepoint_finds_a_color_emoji_face() {
        let paths = font_paths_for_codepoint('\u{1F600}');
        let first = paths.first().expect("emoji must resolve to a font file");
        let name = first
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        assert!(name.contains("emoji"), "expected an emoji face, got {first:?}");
    }

    #[test]
    fn system_font_path_resolves_an_existing_file() {
        let path = system_font_path("Helvetica").expect("Helvetica must resolve on Apple platforms");
        assert!(path.is_absolute(), "expected an absolute path, got {path:?}");
        assert!(path.exists(), "resolved font file does not exist: {path:?}");
    }

    #[test]
    fn system_font_path_rejects_an_empty_family() {
        assert!(system_font_path("").is_none());
    }

    #[test]
    fn system_font_path_falls_back_for_an_unknown_family() {
        // Core Text substitutes a best match instead of failing, so an unknown
        // family still yields a usable file rather than a dangling path.
        let path = system_font_path("aimer-does-not-exist-font-family")
            .expect("Core Text substitutes a default face");
        assert!(path.exists(), "substituted font file does not exist: {path:?}");
    }

    #[test]
    fn fallback_font_path_for_probes_finds_a_color_emoji_face() {
        let path = fallback_font_path_for_probes(&['\u{1F600}', '\u{1F601}'])
            .expect("emoji probes must resolve to a font file");
        assert!(path.exists(), "resolved font file does not exist: {path:?}");
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        assert!(
            name.contains("emoji"),
            "expected an emoji face for emoji probes, got {path:?}"
        );
    }

    #[test]
    fn fallback_font_path_for_probes_handles_a_cjk_script() {
        let path = fallback_font_path_for_probes(&['漢', '字'])
            .expect("CJK probes must resolve to a font file");
        assert!(path.exists(), "resolved font file does not exist: {path:?}");
    }

    #[test]
    fn fallback_font_path_for_probes_rejects_empty_probes() {
        assert!(fallback_font_path_for_probes(&[]).is_none());
    }
}
