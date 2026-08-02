//! Glyph rasterization performed by the system on Apple platforms.
//!
//! Cupid rasterizes glyphs itself: outlines come from `glyf` or `CFF `, color
//! artwork from `sbix`, `CBDT` or `COLR`. Apple's own system fonts do not
//! always use those formats. Two of them are load bearing on iOS:
//!
//! * `PingFangUI.ttc` — the only face on the system covering Simplified
//!   Chinese — stores its outlines in a private `hvgl` table and ships neither
//!   `glyf` nor `CFF `;
//! * `AppleColorEmoji-160px.ttc` stores its `sbix` strikes as `emjc`, Apple's
//!   compressed emoji format, where the macOS build of the same font uses
//!   plain `png `.
//!
//! Every table needed to *shape* text is present in both — `cmap`, `hmtx`,
//! `GSUB`, `morx` — only the pixels are unreachable, so shaping, measurement
//! and layout stay with Cupid and just the final rasterization is handed to
//! Core Text, which is the only code that can read these formats.
//!
//! # Glyph identity
//!
//! A [`RasterizedGlyph`] is addressed by glyph id, and glyph ids are per face:
//! asking Core Text for the wrong face of a collection would draw a different
//! character. The face is therefore selected by PostScript name — read from
//! the very face Cupid shaped with — and the collection index is used only
//! when the file exposes no name to match on.
//!
//! # Cost
//!
//! Building a `CTFont` parses the file, so fonts are cached per
//! `(file, face, size)`. The cache is thread local, which keeps it lock free
//! and matches how text preparation runs: each worker owns its rasterizer.
//! Everything downstream — the atlas, the bitmap cache, the metrics store —
//! is unaware that the pixels came from the platform.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use objc2_core_foundation::{CFRetained, CFString, CFURL, CGPoint, CGRect};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGGlyph, CGImageAlphaInfo,
};
use objc2_core_text::{
    CTFont, CTFontDescriptor, CTFontManagerCreateFontDescriptorsFromURL, CTFontOrientation,
    kCTFontNameAttribute,
};
use skrifa::MetadataProvider;
use skrifa::string::StringId;

use crate::text_pipeline::font_resolver::{font_ref, mapped_font_file};
use crate::text_pipeline::glyph_rasterizer::RasterizedGlyph;

/// Point size used when only a yes/no answer about a glyph is needed.
///
/// Bounding rectangles scale linearly with the point size, so any positive
/// value decides emptiness identically.
const PROBE_FONT_SIZE: f32 = 32.0;

/// Largest bitmap accepted for a single glyph, per axis.
///
/// This matches the atlas bound: a larger glyph could not be stored anyway,
/// and refusing it here keeps a malformed or hostile font from asking for a
/// multi-gigabyte allocation.
const MAX_GLYPH_EXTENT: u32 = 2048;

/// One pixel of slack around the reported bounding box.
///
/// Antialiased coverage reaches marginally beyond the design bounds, and
/// bitmap strikes may be authored slightly larger than the outline box they
/// are attached to. A single pixel is enough to keep edges from being clipped
/// and shifts the glyph's offsets consistently, so positioning is unaffected.
const BOUNDS_PADDING: f32 = 1.0;

/// Identity of a platform font: the file, the face inside it, and the point
/// size in tenths — the same quantization [`GlyphKey`] uses, so a cache entry
/// corresponds to exactly one rasterization size.
///
/// [`GlyphKey`]: crate::text_pipeline::glyph_rasterizer::GlyphKey
type FontKey = (PathBuf, u32, u32);

/// Fonts built so far, including the files the platform refused (`None`).
type FontCache = HashMap<FontKey, Option<CFRetained<CTFont>>>;

thread_local! {
    /// Fonts already built by this thread, keyed by file, face and size.
    static FONT_CACHE: RefCell<FontCache> = RefCell::new(HashMap::new());
}

/// Reports whether the platform can draw `glyph_id` of the given face.
///
/// This is the question [`system_fallback`] has to answer before adopting a
/// face whose glyph data Cupid cannot read: a `cmap` entry alone proves
/// nothing, and neither does the absence of `glyf`. A glyph the platform
/// renders as blank — a space, or an unimplemented slot — reports `false`.
///
/// [`system_fallback`]: crate::text_pipeline::system_fallback
pub(crate) fn draws_glyph(path: &Path, collection_index: u32, glyph_id: u16) -> bool {
    with_font(path, collection_index, PROBE_FONT_SIZE, |font| {
        let bounds = glyph_bounds(font, glyph_id)?;
        (bounds.size.width > 0.0 && bounds.size.height > 0.0).then_some(())
    })
    .is_some()
}

/// Draws `glyph_id` of the given face at `font_size` and returns its pixels.
///
/// The bitmap is 8-bit coverage for outline faces and non-premultiplied RGBA8
/// for color faces, which is exactly what [`GlyphAtlas`] and its color sibling
/// expect. `advance_width` is passed in rather than queried: shaping, layout
/// and measurement all derive advances from `hmtx`, and a glyph must not
/// advance differently just because the platform drew it.
///
/// Returns `None` when the face cannot be built, when the glyph is blank, or
/// when its bitmap would exceed [`MAX_GLYPH_EXTENT`].
///
/// [`GlyphAtlas`]: crate::text_pipeline::glyph_atlas::GlyphAtlas
pub(crate) fn rasterize_glyph(
    path: &Path,
    collection_index: u32,
    glyph_id: u16,
    font_size: f32,
    is_color: bool,
    advance_width: f32,
) -> Option<RasterizedGlyph> {
    with_font(path, collection_index, font_size, |font| {
        let bounds = glyph_bounds(font, glyph_id)?;
        let (left, bottom, width, height) = pixel_box(bounds)?;

        let bitmap = if is_color {
            draw_color(font, glyph_id, left, bottom, width, height)?
        } else {
            draw_coverage(font, glyph_id, left, bottom, width, height)?
        };

        Some(RasterizedGlyph {
            bitmap,
            width,
            height,
            offset_x: left,
            offset_y: bottom,
            advance_width,
            is_color,
        })
    })
}

/// Returns the design bounding rectangle of `glyph_id`, in pixels.
fn glyph_bounds(font: &CTFont, glyph_id: u16) -> Option<CGRect> {
    let glyph: CGGlyph = glyph_id;
    let glyphs = std::ptr::NonNull::from(&glyph);
    // SAFETY: `glyphs` points at one `CGGlyph` and the count says so; passing
    // null for `bounding_rects` requests only the overall rectangle, which is
    // the return value.
    let bounds = unsafe {
        font.bounding_rects_for_glyphs(
            CTFontOrientation::Default,
            glyphs,
            std::ptr::null_mut(),
            1,
        )
    };
    (bounds.size.width.is_finite()
        && bounds.size.height.is_finite()
        && bounds.size.width > 0.0
        && bounds.size.height > 0.0)
        .then_some(bounds)
}

/// Converts a design bounding rectangle into a whole-pixel bitmap box.
///
/// Returns the left and bottom edges relative to the pen — the origin
/// [`RasterizedGlyph`] uses — together with the bitmap dimensions.
fn pixel_box(bounds: CGRect) -> Option<(f32, f32, u32, u32)> {
    let left = (bounds.origin.x as f32 - BOUNDS_PADDING).floor();
    let bottom = (bounds.origin.y as f32 - BOUNDS_PADDING).floor();
    let right = (bounds.origin.x + bounds.size.width) as f32 + BOUNDS_PADDING;
    let top = (bounds.origin.y + bounds.size.height) as f32 + BOUNDS_PADDING;

    let width = (right.ceil() - left) as i64;
    let height = (top.ceil() - bottom) as i64;
    if width <= 0 || height <= 0 || width > MAX_GLYPH_EXTENT as i64 || height > MAX_GLYPH_EXTENT as i64
    {
        return None;
    }

    Some((left, bottom, width as u32, height as u32))
}

/// Renders `glyph_id` into an 8-bit coverage mask.
fn draw_coverage(
    font: &CTFont,
    glyph_id: u16,
    left: f32,
    bottom: f32,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let mut bitmap = vec![0u8; (width * height) as usize];
    // SAFETY: the buffer holds `width * height` bytes, which is what the
    // 8-bit, one-component-per-pixel layout described here addresses. The
    // context is dropped before `bitmap` is moved out.
    let context = unsafe {
        CGBitmapContextCreate(
            bitmap.as_mut_ptr().cast(),
            width as usize,
            height as usize,
            8,
            width as usize,
            None,
            CGImageAlphaInfo::Only.0,
        )
    }?;

    prepare(&context);
    // An alpha-only context keeps coverage, not color, so the fill has to be
    // fully opaque for the glyph to leave anything behind.
    CGContext::set_gray_fill_color(Some(&context), 1.0, 1.0);
    draw(&context, font, glyph_id, left, bottom);
    drop(context);

    Some(bitmap)
}

/// Renders `glyph_id` into non-premultiplied RGBA8 color artwork.
fn draw_color(
    font: &CTFont,
    glyph_id: u16,
    left: f32,
    bottom: f32,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    let mut bitmap = vec![0u8; (width * height * 4) as usize];
    let color_space = CGColorSpace::new_device_rgb()?;
    // SAFETY: the buffer holds `width * height * 4` bytes, matching the 8 bits
    // per component, four components per pixel layout described here. The
    // context is dropped before `bitmap` is moved out.
    let context = unsafe {
        CGBitmapContextCreate(
            bitmap.as_mut_ptr().cast(),
            width as usize,
            height as usize,
            8,
            (width * 4) as usize,
            Some(&color_space),
            CGImageAlphaInfo::PremultipliedLast.0,
        )
    }?;

    prepare(&context);
    draw(&context, font, glyph_id, left, bottom);
    drop(context);

    un_premultiply(&mut bitmap);
    Some(bitmap)
}

/// Applies the rendering settings shared by both bitmap formats.
///
/// Font smoothing is Core Graphics' subpixel antialiasing, which would encode
/// a specific pixel geometry into the atlas; Cupid blends glyph coverage
/// itself, so plain grayscale antialiasing is what it needs.
fn prepare(context: &CGContext) {
    CGContext::set_should_antialias(Some(context), true);
    CGContext::set_allows_font_smoothing(Some(context), false);
    CGContext::set_should_smooth_fonts(Some(context), false);
    CGContext::set_allows_font_subpixel_positioning(Some(context), false);
    CGContext::set_should_subpixel_position_fonts(Some(context), false);
    CGContext::set_allows_font_subpixel_quantization(Some(context), false);
    CGContext::set_should_subpixel_quantize_fonts(Some(context), false);
}

/// Draws the glyph so that its bounding box lands on the bitmap's origin.
///
/// Core Graphics places glyphs relative to the text origin, so shifting the
/// pen by the negated bounding box moves the box onto the bitmap. The context
/// keeps its default bottom-left orientation: bitmap memory starts at the top
/// row regardless, which is the row order the atlas uploads.
fn draw(context: &CGContext, font: &CTFont, glyph_id: u16, left: f32, bottom: f32) {
    let glyph: CGGlyph = glyph_id;
    let position = CGPoint {
        x: -left as f64,
        y: -bottom as f64,
    };
    // SAFETY: both pointers address exactly one element, which is the count
    // passed alongside them.
    unsafe {
        font.draw_glyphs(
            std::ptr::NonNull::from(&glyph),
            std::ptr::NonNull::from(&position),
            1,
            context,
        );
    }
}

/// Converts premultiplied RGBA8 in place into the straight alpha the color
/// atlas stores.
///
/// Core Graphics bitmap contexts only composite premultiplied, while the atlas
/// is documented to hold non-premultiplied bytes; dividing the color channels
/// back out keeps a glyph's edges from darkening when the shader multiplies by
/// alpha again.
fn un_premultiply(bitmap: &mut [u8]) {
    for pixel in bitmap.chunks_exact_mut(4) {
        let alpha = pixel[3];
        if alpha == 0 || alpha == u8::MAX {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((*channel as u16 * 255 + alpha as u16 / 2) / alpha as u16).min(255) as u8;
        }
    }
}

/// Runs `f` with the platform font for `(path, collection_index, font_size)`.
///
/// The font is built at most once per thread per size; a file the platform
/// refuses is remembered as unusable so it is not reopened on every glyph.
fn with_font<T>(
    path: &Path,
    collection_index: u32,
    font_size: f32,
    f: impl FnOnce(&CTFont) -> Option<T>,
) -> Option<T> {
    if font_size <= 0.0 {
        return None;
    }
    let key = (path.to_path_buf(), collection_index, (font_size * 10.0) as u32);
    FONT_CACHE.with_borrow_mut(|cache| {
        let font = cache
            .entry(key)
            .or_insert_with(|| create_font(path, collection_index, font_size));
        f(font.as_deref()?)
    })
}

/// Builds the platform font for one face of a font file.
fn create_font(
    path: &Path,
    collection_index: u32,
    font_size: f32,
) -> Option<CFRetained<CTFont>> {
    let descriptor = face_descriptor(path, collection_index)?;
    // SAFETY: a null matrix requests the identity transform.
    Some(unsafe { CTFont::with_font_descriptor(&descriptor, font_size as f64, std::ptr::null()) })
}

/// Returns the descriptor of the face Cupid shaped with.
///
/// Descriptors are matched by PostScript name, because that is the identity
/// glyph ids belong to. The collection index serves as the fallback for files
/// whose name table cannot be read — the descriptor array follows the order of
/// the faces in the file.
fn face_descriptor(path: &Path, collection_index: u32) -> Option<CFRetained<CTFontDescriptor>> {
    let url = CFURL::from_file_path(path)?;
    // SAFETY: `url` is a valid file URL.
    let descriptors = unsafe { CTFontManagerCreateFontDescriptorsFromURL(&url) }?;
    // SAFETY: the function is documented to return an array of font
    // descriptors.
    let descriptors = unsafe { descriptors.cast_unchecked::<CTFontDescriptor>() };

    if let Some(expected) = postscript_name(path, collection_index)
        && let Some(matched) = descriptors
            .iter()
            .find(|descriptor| descriptor_name(descriptor).as_deref() == Some(expected.as_str()))
    {
        return Some(matched);
    }

    descriptors.get(collection_index as usize)
}

/// Returns the PostScript name a descriptor advertises.
fn descriptor_name(descriptor: &CTFontDescriptor) -> Option<String> {
    // SAFETY: `kCTFontNameAttribute` is a Core Text constant string.
    let attribute = unsafe { descriptor.attribute(kCTFontNameAttribute) }?;
    Some(attribute.downcast::<CFString>().ok()?.to_string())
}

/// Returns the PostScript name recorded in the font file itself.
fn postscript_name(path: &Path, collection_index: u32) -> Option<String> {
    let data = mapped_font_file(path)?;
    let face = font_ref(data.as_ref(), collection_index)?;
    face.localized_strings(StringId::POSTSCRIPT_NAME)
        .english_or_first()
        .map(|name| name.chars().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The face carrying Simplified Chinese on Apple platforms, whose outlines
    /// live in a private table.
    fn simplified_chinese_face() -> Option<(PathBuf, u32, u16)> {
        crate::text_pipeline::apple_fonts::font_paths_for_codepoint('吗')
            .into_iter()
            .find_map(|path| {
                let data = mapped_font_file(&path)?;
                let count = match skrifa::raw::FileRef::new(data.as_ref()).ok()? {
                    skrifa::raw::FileRef::Collection(collection) => collection.len(),
                    skrifa::raw::FileRef::Font(_) => 1,
                };
                (0..count).find_map(|index| {
                    let face = font_ref(data.as_ref(), index)?;
                    let glyph_id = face.charmap().map('吗')?.to_u32() as u16;
                    (glyph_id != 0).then_some((path.clone(), index, glyph_id))
                })
            })
    }

    #[test]
    fn draws_a_glyph_whose_outlines_only_the_platform_can_read() {
        let (path, index, glyph_id) =
            simplified_chinese_face().expect("Apple platforms always carry a Chinese face");

        assert!(
            draws_glyph(&path, index, glyph_id),
            "{path:?}#{index} glyph {glyph_id} reported as undrawable"
        );

        let glyph = rasterize_glyph(&path, index, glyph_id, 32.0, false, 32.0)
            .expect("the platform must draw a face it reports as drawable");
        assert!(!glyph.is_color);
        assert_eq!(glyph.bitmap.len(), (glyph.width * glyph.height) as usize);
        assert!(
            glyph.bitmap.iter().any(|coverage| *coverage > 0),
            "the coverage mask is blank"
        );
        // An ideograph is roughly square and fills its em box.
        assert!(glyph.width >= 16 && glyph.height >= 16, "{}x{}", glyph.width, glyph.height);
    }

    #[test]
    fn draws_color_emoji_as_straight_alpha_rgba() {
        let path = crate::text_pipeline::apple_fonts::font_paths_for_codepoint('😀')
            .into_iter()
            .next()
            .expect("emoji resolve to a font file");
        let data = mapped_font_file(&path).expect("the emoji file is readable");
        let face = font_ref(data.as_ref(), 0).expect("the emoji file holds a face");
        let glyph_id = face.charmap().map('😀').expect("the face maps the emoji").to_u32() as u16;

        let glyph = rasterize_glyph(&path, 0, glyph_id, 32.0, true, 32.0)
            .expect("the platform must draw color emoji");

        assert!(glyph.is_color);
        assert_eq!(glyph.bitmap.len(), (glyph.width * glyph.height * 4) as usize);
        assert!(
            glyph.bitmap.chunks_exact(4).any(|pixel| pixel[3] > 0),
            "the emoji artwork is fully transparent"
        );
        // The artwork is colorful: at least one visible pixel must have
        // channels that differ, which a coverage mask expanded to RGBA could
        // never produce.
        assert!(
            glyph
                .bitmap
                .chunks_exact(4)
                .any(|pixel| pixel[3] > 0 && (pixel[0] != pixel[1] || pixel[1] != pixel[2])),
            "the emoji artwork is monochrome"
        );
    }

    #[test]
    fn scales_with_the_requested_point_size() {
        let (path, index, glyph_id) =
            simplified_chinese_face().expect("Apple platforms always carry a Chinese face");

        let small = rasterize_glyph(&path, index, glyph_id, 16.0, false, 16.0).expect("drawn");
        let large = rasterize_glyph(&path, index, glyph_id, 64.0, false, 64.0).expect("drawn");

        assert!(
            large.width > small.width && large.height > small.height,
            "{}x{} is not larger than {}x{}",
            large.width,
            large.height,
            small.width,
            small.height
        );
    }

    #[test]
    fn keeps_the_advance_width_it_was_given() {
        let (path, index, glyph_id) =
            simplified_chinese_face().expect("Apple platforms always carry a Chinese face");

        let glyph = rasterize_glyph(&path, index, glyph_id, 24.0, false, 17.5).expect("drawn");

        assert_eq!(glyph.advance_width, 17.5);
    }

    #[test]
    fn rejects_a_file_that_holds_no_font() {
        let path = std::env::temp_dir().join("aimer-core-text-raster-not-a-font");
        std::fs::write(&path, b"not a font").expect("the temporary file is writable");

        assert!(!draws_glyph(&path, 0, 1));
        assert!(rasterize_glyph(&path, 0, 1, 16.0, false, 8.0).is_none());

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn rejects_a_non_positive_point_size() {
        let (path, index, glyph_id) =
            simplified_chinese_face().expect("Apple platforms always carry a Chinese face");

        assert!(rasterize_glyph(&path, index, glyph_id, 0.0, false, 8.0).is_none());
    }

    #[test]
    fn un_premultiply_restores_straight_alpha() {
        // Half-transparent pure red, premultiplied: the color channel comes
        // back to full intensity.
        let mut bitmap = vec![128, 0, 0, 128, 255, 255, 255, 255, 0, 0, 0, 0];
        un_premultiply(&mut bitmap);

        assert_eq!(bitmap[0], 255);
        assert_eq!(bitmap[3], 128);
        // Opaque and fully transparent pixels are left untouched.
        assert_eq!(&bitmap[4..8], &[255, 255, 255, 255]);
        assert_eq!(&bitmap[8..], &[0, 0, 0, 0]);
    }
}
