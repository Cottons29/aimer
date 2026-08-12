//! Keys and job inputs for the shaping and layout caches.
//!
//! Both caches are consulted several times per span on every frame, so a key
//! must be cheap to build: the text rides in it as the `Arc<str>` the draw
//! request already carries — a refcount bump, not a string copy — and every
//! `f32` parameter is stored as its rounded integer representation so the key
//! can implement `Hash + Eq`.

use std::sync::Arc;

use hashbrown::HashMap;

use crate::font::{FontFamily, FontStyle, TextLanguage};
use crate::text_pipeline::text_layout::ShapedText;

/// Key used to memoize the output of `layout_text` across frames.
#[derive(Debug, Hash, Eq, PartialEq, Clone)]
pub(super) struct LayoutCacheKey {
    text: Arc<str>,
    /// `font_size` × 100, rounded, stored as u32 to make it hashable.
    font_size_u32: u32,
    /// `bounds_width` × 100, rounded, stored as u32.
    bounds_width_u32: u32,
    font_family: FontFamily,
    font_style: FontStyle,
    font_weight: u16,
    /// The language the run is written in, which selects the face its
    /// ideographs are drawn from and therefore their advances.
    language: Option<TextLanguage>,
}

impl LayoutCacheKey {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        text: impl Into<Arc<str>>,
        font_size: f32,
        bounds_width: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
        language: Option<TextLanguage>,
    ) -> Self {
        Self {
            text: text.into(),
            font_size_u32: (font_size * 100.0).round() as u32,
            bounds_width_u32: (bounds_width * 100.0).round() as u32,
            font_family,
            font_style,
            font_weight,
            language,
        }
    }
}

/// Width-independent sibling of [`LayoutCacheKey`]: shaped glyph ids and
/// advances depend on the text and the font parameters, never on the
/// wrapping width.
#[derive(Debug, Hash, Eq, PartialEq, Clone)]
pub(super) struct ShapingCacheKey {
    text: Arc<str>,
    /// `font_size` × 100, rounded, stored as u32 to make it hashable.
    font_size_u32: u32,
    font_family: FontFamily,
    font_style: FontStyle,
    font_weight: u16,
    /// Part of the identity for the same reason it is part of
    /// [`LayoutCacheKey`]: `你好` shaped for a Chinese field may not be handed
    /// to a Japanese one, whose face gives it different glyphs.
    language: Option<TextLanguage>,
}

impl ShapingCacheKey {
    pub(super) fn new(
        text: impl Into<Arc<str>>,
        font_size: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
        language: Option<TextLanguage>,
    ) -> Self {
        Self {
            text: text.into(),
            font_size_u32: (font_size * 100.0).round() as u32,
            font_family,
            font_style,
            font_weight,
            language,
        }
    }
}

/// Slack subtracted from a wrapping width before a layout is declared
/// width-independent.
///
/// [`ShapedText::max_line_width`] and the positioning pen accumulate the same
/// advances in slightly different grouping, so the two sums can disagree by a
/// few float ulps. Half a pixel dwarfs that error by orders of magnitude
/// while flipping essentially no real layout the other way: text within half
/// a pixel of its column's edge simply keeps its width-keyed cache entry.
const WIDTH_INDEPENDENCE_SLACK: f32 = 0.5;

/// The cache identity of one span's layout at one requested wrapping width.
///
/// A wrapped layout only depends on the width when the text wraps at it:
/// a span whose widest line fits inside the requested width positions
/// identically at every width it fits in, so its layout is cached under the
/// canonical unbounded key (`width == 0`) and survives a window resize. The
/// canonical form is only knowable once the span's shaping is cached; keys
/// built before that carry the requested width, so a read consults
/// [`primary`](Self::primary) first and falls back to
/// [`fallback`](Self::fallback) — the width-keyed entry an earlier frame may
/// have minted before the shaping existed.
pub(super) struct SpanLayoutKeys {
    /// The width-independent sibling key; also what a preparation batch
    /// shapes under.
    pub(super) shaping_key: ShapingCacheKey,
    /// The key this frame prepares and draws under.
    pub(super) primary: LayoutCacheKey,
    /// The width-keyed entry an earlier frame may hold for the same glyphs,
    /// present only when `primary` was canonicalized away from the requested
    /// width.
    pub(super) fallback: Option<LayoutCacheKey>,
    /// The wrapping width `primary` stands for — `0.0` when canonicalized.
    pub(super) layout_width: f32,
}

/// Builds the [`SpanLayoutKeys`] of one span at `requested_width`.
///
/// The canonicalization reads `shaping_cache` and nothing else, so callers on
/// both sides of a preparation pass agree with each other as long as the
/// cache only grows between them.
#[allow(clippy::too_many_arguments)]
pub(super) fn span_layout_keys(
    shaping_cache: &HashMap<ShapingCacheKey, ShapedText>,
    text: &Arc<str>,
    font_size: f32,
    font_family: FontFamily,
    font_style: FontStyle,
    font_weight: u16,
    language: Option<TextLanguage>,
    requested_width: f32,
) -> SpanLayoutKeys {
    let shaping_key = ShapingCacheKey::new(
        text.clone(),
        font_size,
        font_family,
        font_style,
        font_weight,
        language,
    );

    let fits_unwrapped = requested_width > 0.0
        && shaping_cache
            .get(&shaping_key)
            .is_some_and(|shaped| {
                shaped.max_line_width + WIDTH_INDEPENDENCE_SLACK <= requested_width
            });
    let (layout_width, fallback) = if fits_unwrapped {
        let width_keyed = LayoutCacheKey::new(
            text.clone(),
            font_size,
            requested_width,
            font_family,
            font_style,
            font_weight,
            language,
        );
        (0.0, Some(width_keyed))
    } else {
        (requested_width, None)
    };

    SpanLayoutKeys {
        primary: LayoutCacheKey::new(
            text.clone(),
            font_size,
            layout_width,
            font_family,
            font_style,
            font_weight,
            language,
        ),
        shaping_key,
        fallback,
        layout_width,
    }
}

/// Everything a worker needs to shape one span.
#[derive(Clone)]
pub(super) struct ShapingInput {
    pub(super) text: Arc<str>,
    pub(super) font_size: f32,
    pub(super) font_family: FontFamily,
    pub(super) font_style: FontStyle,
    pub(super) font_weight: u16,
    /// The language the span is written in, carried to the worker so it
    /// chooses the same faces the drawing pass will.
    pub(super) language: Option<TextLanguage>,
}

/// Everything a worker needs to position one shaped span.
#[derive(Clone)]
pub(super) struct LayoutInput {
    pub(super) shaping_key: ShapingCacheKey,
    pub(super) layout_width: f32,
}

#[cfg(test)]
mod tests {
    use super::{LayoutCacheKey, ShapingCacheKey};
    use crate::font::{FontFamily, FontStyle, FontWeight};

    #[test]
    fn text_cache_keys_isolate_font_families_and_variants() {
        let sans_layout = LayoutCacheKey::new(
            "same",
            16.0,
            100.0,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
            None,
        );
        let mono_layout = LayoutCacheKey::new(
            "same",
            16.0,
            100.0,
            FontFamily::MONOSPACE,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
            None,
        );
        assert_ne!(sans_layout, mono_layout);

        let normal_shape = ShapingCacheKey::new(
            "same",
            16.0,
            FontFamily::MONOSPACE,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
            None,
        );
        let italic_shape = ShapingCacheKey::new(
            "same",
            16.0,
            FontFamily::MONOSPACE,
            FontStyle::Italic,
            FontWeight::Normal.numeric(),
            None,
        );
        assert_ne!(normal_shape, italic_shape);
    }
}
