use std::collections::HashMap;
use utils::*;
use utils::log::debug;

/// Embedded primary font (Roboto) — covers Latin and common scripts.
const PRIMARY_FONT: &[u8] = include_bytes!("../../../fonts/Roboto.ttf");

/// A rasterized glyph bitmap with its metrics.
#[derive(Clone)]
pub struct RasterizedGlyph {
    pub bitmap: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Horizontal offset from the pen position to the left edge of the bitmap.
    pub offset_x: f32,
    /// Vertical offset from the baseline to the top edge of the bitmap.
    pub offset_y: f32,
    /// Horizontal advance width.
    pub advance_width: f32,
}

/// Key for caching rasterized glyphs: (codepoint, size in tenths of a pixel).
#[derive(Hash, Eq, PartialEq, Clone, Copy)]
pub struct GlyphKey {
    pub codepoint: char,
    pub size_tenths: u32,
}

impl GlyphKey {
    pub fn new(codepoint: char, font_size: f32) -> Self {
        Self { codepoint, size_tenths: (font_size * 10.0) as u32 }
    }
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
fn load_system_font(family: &str) -> Option<Vec<u8>> {
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
        std::fs::read(path).ok()
    }
}

/// Android: try to read font files directly from /system/fonts.
#[cfg(target_os = "android")]
fn load_system_font(family: &str) -> Option<Vec<u8>> {
    // Android stores fonts in /system/fonts. Try common CJK/fallback font files.
    let candidates: &[&str] = match family {
        "Noto Sans CJK" => &[
            "/system/fonts/NotoSansCJK-Regular.ttc",
            "/system/fonts/NotoSansSC-Regular.otf",
            "/system/fonts/DroidSansFallback.ttf",
        ],
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

/// Attempt to load a fontdue::Font from system font data for the given family.
fn try_load_system_fontdue(family: &str) -> Option<fontdue::Font> {
    let start = chrono::Utc::now().timestamp_millis();
    let data = load_system_font(family)?;
    debug!("Font data len : {}, size : {}", data.len(), size_of_val(&1_u8) * data.len());
    let font = fontdue::Font::from_bytes(data, fontdue::FontSettings::default()).ok();
    let end = chrono::Utc::now().timestamp_millis();
    info!("Font::from_bytes took :  '{}' in {}ms", family, end - start);
    font
}

/// Build the list of fallback fonts: system fonts first, then embedded NotoSansSC.
fn build_fallback_chain() -> Vec<fontdue::Font> {
    let start_time = chrono::Utc::now().timestamp_millis();
    let mut fallbacks = Vec::new();

    // Platform-appropriate system font families to try as fallbacks.
    let system_families: &[&str] = if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        &[
            // "PingFang SC",
            // "Hiragino Sans",
            // "Apple SD Gothic Neo",
            // "Arial Unicode MS"
            "Noto Sans SC"
        ]
    } else if cfg!(target_os = "windows") {
        &["Microsoft YaHei", "Malgun Gothic", "Yu Gothic", "MS Gothic"]
    } else if cfg!(target_os = "android") {
        &["Noto Sans CJK", "Droid Sans Fallback"]
    } else {
        // Linux and others
        &["Noto Sans CJK SC", "Noto Sans CJK", "WenQuanYi Micro Hei", "DejaVu Sans"]
    };

    for family in system_families {
        if let Some(font) = try_load_system_fontdue(family) {
            fallbacks.push(font);
            break; // One good CJK system font is enough
        }
    }

    let end_time = chrono::Utc::now().timestamp_millis();

    debug!("System fallback font loading took AAA: {} ms", end_time - start_time);
    
    fallbacks
}

pub struct GlyphRasterizer {
    /// Primary font (Roboto) for Latin/common glyphs.
    primary: fontdue::Font,
    /// Fallback fonts for extended Unicode coverage (CJK, etc.).
    /// Loaded lazily on first encounter of a glyph not in the primary font,
    /// to avoid the massive memory cost (~800MB) of parsing large CJK fonts
    /// when only ASCII text is rendered.
    fallbacks: Option<Vec<fontdue::Font>>,
    /// Whether to attempt loading fallbacks when needed.
    enable_fallbacks: bool,
    cache: HashMap<GlyphKey, RasterizedGlyph>,
}

impl GlyphRasterizer {
    pub fn new() -> Self {
        let primary = fontdue::Font::from_bytes(PRIMARY_FONT, fontdue::FontSettings::default())
            .expect("failed to load primary font");

        Self {
            primary,
            fallbacks: None, // loaded lazily on first miss
            enable_fallbacks: true,
            cache: HashMap::new(),
        }
    }

    /// Create a lightweight rasterizer with only the primary font (no fallbacks).
    /// Suitable for text measurement where CJK rendering is not needed.
    pub fn primary_only() -> Self {
        let primary = fontdue::Font::from_bytes(PRIMARY_FONT, fontdue::FontSettings::default())
            .expect("failed to load primary font");
        Self { primary, fallbacks: None, enable_fallbacks: false, cache: HashMap::new() }
    }

    /// Ensure fallback fonts are loaded. Called lazily on first glyph miss.
    fn ensure_fallbacks(&mut self) {
        if self.fallbacks.is_none() && self.enable_fallbacks {
            info!("Loading fallback fonts (first non-primary glyph encountered)...");
            self.fallbacks = Some(build_fallback_chain());
        }
    }

    /// Rasterize a single glyph at the given size, returning cached result if available.
    pub fn rasterize(&mut self, codepoint: char, font_size: f32) -> &RasterizedGlyph {
        let key = GlyphKey::new(codepoint, font_size);

        // Check if we need to load fallbacks for this glyph.
        if !self.cache.contains_key(&key) && !self.primary.has_glyph(codepoint) {
            self.ensure_fallbacks();
        }

        let primary = &self.primary;
        let fallbacks = &self.fallbacks;
        self.cache.entry(key).or_insert_with(|| {
            let font = if primary.has_glyph(codepoint) {
                primary
            } else {
                fallbacks
                    .as_ref()
                    .and_then(|fbs| fbs.iter().find(|fb| fb.has_glyph(codepoint)))
                    .unwrap_or(primary)
            };
            let (metrics, bitmap) = font.rasterize(codepoint, font_size);
            RasterizedGlyph {
                bitmap,
                width: metrics.width as u32,
                height: metrics.height as u32,
                offset_x: metrics.xmin as f32,
                offset_y: metrics.ymin as f32,
                advance_width: metrics.advance_width,
            }
        })
    }

    /// Returns line metrics (ascent, descent, line_gap) for the given font size.
    /// Uses the primary font for consistent line spacing.
    pub fn line_metrics(&self, font_size: f32) -> (f32, f32, f32) {
        let m = self
            .primary
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
        text.chars()
            .map(|c| {
                let g = self.rasterize(c, font_size);
                g.advance_width
            })
            .sum()
    }
}
