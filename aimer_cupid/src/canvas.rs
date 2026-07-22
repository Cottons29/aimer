use std::cell::{Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::font::{FontFamily, FontStyle, FontWeight};

use crate::draw_cmd::DrawList;
use crate::svg::{SvgNodeStyleOverride, SvgScene};
use crate::text_pipeline::TextOverflowMode;
use crate::text_pipeline::glyph_rasterizer::GlyphRasterizer;
use crate::utilities::{Color, Rect, TextureId, Vec2d};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub line_height: f32,
    pub line_count: usize,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
struct TextMetricsKey {
    text: String,
    font_size_tenths: u32,
    max_width_tenths: u32,
    font_family: FontFamily,
    font_style: FontStyle,
    font_weight: u16,
}

#[derive(Clone, Debug)]
struct CachedTextMetrics {
    metrics: TextMetrics,
    line_widths: Vec<f32>,
}

#[derive(Clone)]
pub struct CupidCanvas {
    draw_list: Rc<RefCell<DrawList>>,
    rasterizer: Rc<RefCell<GlyphRasterizer>>,
    metrics_cache: Rc<RefCell<HashMap<TextMetricsKey, CachedTextMetrics>>>,
}

impl CupidCanvas {
    pub fn new() -> Self {
        Self {
            draw_list: Rc::new(RefCell::new(DrawList::new())),
            rasterizer: Rc::new(RefCell::new(GlyphRasterizer::new())),
            metrics_cache: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn begin_frame(&self) {
        self.draw_list
            .borrow_mut()
            .clear();
    }

    pub fn register_font_bytes(&self, bytes: Vec<u8>) -> Option<crate::text_layout::FontId> {
        let font_id = self
            .rasterizer
            .borrow_mut()
            .register_font_bytes(bytes)?;
        self.metrics_cache
            .borrow_mut()
            .clear();
        Some(font_id)
    }

    pub fn fill_rect(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
        border_radius: [f32; 4],
    ) {
        self.draw_list
            .borrow_mut()
            .fill_rect(
                Rect::new(x, y, width, height),
                color,
                border_radius,
                [0.0; 4],
                Color::transparent(),
            );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect_with_border(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
    ) {
        self.draw_list
            .borrow_mut()
            .fill_rect(
                Rect::new(x, y, width, height),
                color,
                border_radius,
                [border_width; 4],
                border_color,
            );
    }

    /// Draws a filled rectangle with per-corner border radii and per-side
    /// border widths. `border_radius`: [top-left, top-right, bottom-right,
    /// bottom-left] `border_width`: [top, right, bottom, left]
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect_with_per_side_border(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
    ) {
        self.draw_list
            .borrow_mut()
            .fill_rect(
                Rect::new(x, y, width, height),
                color,
                border_radius,
                border_width,
                border_color,
            );
    }

    pub fn clear_rect(&self, x: f32, y: f32, width: f32, height: f32) {
        self.draw_list
            .borrow_mut()
            .clear_rect(Rect::new(x, y, width, height));
    }

    pub fn translate(&self, x: f32, y: f32) {
        self.draw_list
            .borrow_mut()
            .translate(x, y);
    }

    pub fn scale(&self, sx: f32, sy: f32) {
        self.draw_list
            .borrow_mut()
            .scale(sx, sy);
    }

    pub fn rotate(&self, radians: f32) {
        self.draw_list
            .borrow_mut()
            .rotate(radians);
    }

    pub fn save(&self) {
        self.draw_list
            .borrow_mut()
            .save();
    }

    pub fn restore(&self) {
        self.draw_list
            .borrow_mut()
            .restore();
    }

    pub fn draw_text(
        &self,
        x: f32,
        y: f32,
        text: &str,
        font_size: f32,
        color: Color,
        font_weight: u16,
    ) {
        self.draw_text_styled(
            x,
            y,
            text,
            font_size,
            color,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            font_weight,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_styled(
        &self,
        x: f32,
        y: f32,
        text: &str,
        font_size: f32,
        color: Color,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) {
        self.draw_list
            .borrow_mut()
            .draw_text_styled(
                Vec2d::new(x, y),
                Arc::from(text),
                font_size,
                color,
                font_family,
                font_style,
                font_weight,
            );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_wrapped(
        &self,
        x: f32,
        y: f32,
        text: &str,
        font_size: f32,
        color: Color,
        max_width: f32,
        font_weight: u16,
    ) {
        self.draw_text_wrapped_styled(
            x,
            y,
            text,
            font_size,
            color,
            max_width,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            font_weight,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_wrapped_styled(
        &self,
        x: f32,
        y: f32,
        text: &str,
        font_size: f32,
        color: Color,
        max_width: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) {
        self.draw_list
            .borrow_mut()
            .draw_text_with_overflow(
                Vec2d::new(x, y),
                Arc::from(text),
                font_size,
                color,
                Some(max_width),
                None,
                TextOverflowMode::Wrap,
                font_family,
                font_style,
                font_weight,
            );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_with_overflow(
        &self,
        x: f32,
        y: f32,
        text: &str,
        font_size: f32,
        color: Color,
        bounds_width: f32,
        bounds_height: f32,
        overflow: TextOverflowMode,
        font_weight: u16,
    ) {
        self.draw_text_with_overflow_styled(
            x,
            y,
            text,
            font_size,
            color,
            bounds_width,
            bounds_height,
            overflow,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            font_weight,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_with_overflow_styled(
        &self,
        x: f32,
        y: f32,
        text: &str,
        font_size: f32,
        color: Color,
        bounds_width: f32,
        bounds_height: f32,
        overflow: TextOverflowMode,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) {
        self.draw_list
            .borrow_mut()
            .draw_text_with_overflow(
                Vec2d::new(x, y),
                Arc::from(text),
                font_size,
                color,
                Some(bounds_width),
                Some(bounds_height),
                overflow,
                font_family,
                font_style,
                font_weight,
            );
    }

    pub fn draw_image(&self, x: f32, y: f32, width: f32, height: f32, texture_id: TextureId) {
        self.draw_list
            .borrow_mut()
            .draw_image(Rect::new(x, y, width, height), texture_id);
    }

    pub fn draw_svg(
        &self,
        scene: Arc<SvgScene>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        overrides: Arc<[SvgNodeStyleOverride]>,
    ) {
        self.draw_list
            .borrow_mut()
            .draw_svg(scene, Rect::new(x, y, width, height), overrides);
    }

    /// Draw a styled text-decoration line. `(x, y)` is the band top-left,
    /// `width`/`band_height` its extent; the text engine renders the styled
    /// stroke (`style` id, `thickness`, `period`) inside the band.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_decoration(
        &self,
        x: f32,
        y: f32,
        width: f32,
        band_height: f32,
        color: Color,
        style: u32,
        thickness: f32,
        period: f32,
    ) {
        self.draw_list
            .borrow_mut()
            .draw_text_decoration(
                Rect::new(x, y, width, band_height),
                color,
                style,
                thickness,
                period,
            );
    }

    /// Measure text width using the cached text rasterizer.
    pub fn measure_text(&self, text: &str, font_size: f32) -> f32 {
        self.measure_text_styled(
            text,
            font_size,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
        )
    }

    pub fn measure_text_styled(
        &self,
        text: &str,
        font_size: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) -> f32 {
        self.rasterizer
            .borrow_mut()
            .measure_text_for_family(
                text,
                font_size,
                font_family,
                FontWeight::Value(u32::from(font_weight)),
                font_style,
            )
    }

    pub fn measure_text_metrics(&self, text: &str, font_size: f32, max_width: f32) -> TextMetrics {
        self.measure_text_metrics_styled(
            text,
            font_size,
            max_width,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
        )
    }

    pub fn measure_text_metrics_styled(
        &self,
        text: &str,
        font_size: f32,
        max_width: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) -> TextMetrics {
        let key = TextMetricsKey {
            text: text.to_string(),
            font_size_tenths: (font_size * 10.0) as u32,
            max_width_tenths: (max_width.max(0.0) * 10.0) as u32,
            font_family,
            font_style,
            font_weight,
        };
        if let Some(cached) = self
            .metrics_cache
            .borrow()
            .get(&key)
        {
            return cached.metrics;
        }

        let mut rasterizer = self.rasterizer.borrow_mut();
        let weight = FontWeight::Value(u32::from(font_weight));
        let (ascent, descent, line_gap) =
            rasterizer.line_metrics_for_family(font_size, font_family, weight, font_style);
        let line_height = ascent - descent + line_gap;
        let mut width = 0.0_f32;
        let mut current_width = 0.0_f32;
        let mut line_count = 1_usize;
        let mut line_widths = Vec::new();
        // Width position right after the most recent whitespace on the current
        // line (relative to the line start). `None` means no break opportunity
        // is available on the current line yet. This mirrors the word-wrapping
        // performed by `layout_shaped_text` so the measured line count matches
        // the rendered one (otherwise the last line would be clipped).
        let mut last_space_end: Option<f32> = None;

        for c in text.chars() {
            if c == '\n' {
                width = width.max(current_width);
                line_widths.push(current_width);
                current_width = 0.0;
                line_count += 1;
                last_space_end = None;
                continue;
            }

            let glyph_width =
                rasterizer.advance_width_for_family(c, font_size, font_family, weight, font_style);

            // Track the last whitespace position as the preferred break point.
            if c.is_whitespace() {
                last_space_end = Some(current_width + glyph_width);
            }

            if max_width > 0.0 && current_width > 0.0 && current_width + glyph_width > max_width {
                if let Some(space_end) = last_space_end {
                    // Word-wrap: the partial word after the last space moves to
                    // the next line, so the current line ends at the space.
                    let moved_width = (current_width - space_end).max(0.0);
                    width = width.max(space_end);
                    line_widths.push(space_end);
                    current_width = moved_width;
                    line_count += 1;
                    last_space_end = None;
                } else {
                    // No break opportunity — fall back to character wrapping.
                    width = width.max(current_width);
                    line_widths.push(current_width);
                    current_width = 0.0;
                    line_count += 1;
                }
            }
            current_width += glyph_width;
        }

        width = width.max(current_width);
        line_widths.push(current_width);

        // Subtract one line_gap: it only appears *between* lines, not after
        // the last one.  This matches the corrected layout_paragraph height.
        let metrics = TextMetrics {
            width,
            height: line_count as f32 * line_height - line_gap,
            ascent,
            descent,
            line_gap,
            line_height,
            line_count,
        };

        let mut cache = self
            .metrics_cache
            .borrow_mut();
        if cache.len() > 1024 {
            cache.clear();
        }
        cache.insert(
            key,
            CachedTextMetrics {
                metrics,
                line_widths,
            },
        );
        metrics
    }

    /// Measures the rendered width of each line after applying the same wrapping rules as drawing.
    #[allow(clippy::too_many_arguments)]
    pub fn measure_text_line_widths_styled(
        &self,
        text: &str,
        font_size: f32,
        max_width: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) -> Vec<f32> {
        let key = TextMetricsKey {
            text: text.to_string(),
            font_size_tenths: (font_size * 10.0) as u32,
            max_width_tenths: (max_width.max(0.0) * 10.0) as u32,
            font_family,
            font_style,
            font_weight,
        };
        self.measure_text_metrics_styled(
            text,
            font_size,
            max_width,
            font_family,
            font_style,
            font_weight,
        );
        self.metrics_cache
            .borrow()
            .get(&key)
            .map(|cached| cached.line_widths.clone())
            .unwrap_or_default()
    }

    /// Draws a filled rectangle with border and outline in a single pass (no
    /// gap).
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect_with_border_and_outline(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
        outline_width: f32,
        outline_color: Color,
    ) {
        self.draw_list
            .borrow_mut()
            .fill_rect_with_outline(
                Rect::new(x, y, width, height),
                color,
                border_radius,
                [border_width; 4],
                border_color,
                [outline_width; 4],
                outline_color,
            );
    }

    /// Draws a filled rectangle with border and outline with
    /// per-corner/per-side control.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect_with_border_and_outline_per_side(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
        outline_width: [f32; 4],
        outline_color: Color,
    ) {
        self.draw_list
            .borrow_mut()
            .fill_rect_with_outline(
                Rect::new(x, y, width, height),
                color,
                border_radius,
                border_width,
                border_color,
                outline_width,
                outline_color,
            );
    }

    /// Draws a stroked (outline-only) rectangle.
    #[allow(clippy::too_many_arguments)]
    pub fn stroke_rect(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        stroke_color: Color,
        stroke_width: f32,
        border_radius: [f32; 4],
    ) {
        self.draw_list
            .borrow_mut()
            .fill_rect(
                Rect::new(x, y, width, height),
                Color::transparent(),
                border_radius,
                [stroke_width; 4],
                stroke_color,
            );
    }

    /// Draws a stroked (outline-only) rectangle with per-corner radii and
    /// per-side widths. `border_radius`: [top-left, top-right,
    /// bottom-right, bottom-left] `stroke_width`: [top, right, bottom,
    /// left]
    #[allow(clippy::too_many_arguments)]
    pub fn stroke_rect_per_side(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        stroke_color: Color,
        stroke_width: [f32; 4],
        border_radius: [f32; 4],
    ) {
        self.draw_list
            .borrow_mut()
            .fill_rect(
                Rect::new(x, y, width, height),
                Color::transparent(),
                border_radius,
                stroke_width,
                stroke_color,
            );
    }

    /// Draws a filled rectangle with a specific color (convenience method).
    pub fn fill_color_rect(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
        border_radius: [f32; 4],
    ) {
        self.draw_list
            .borrow_mut()
            .fill_rect(
                Rect::new(x, y, width, height),
                color,
                border_radius,
                [0.0; 4],
                Color::transparent(),
            );
    }

    /// Draws a filled rectangle with per-corner border radii.
    /// `border_radius`: [top-left, top-right, bottom-right, bottom-left]
    pub fn fill_color_rect_per_corner(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
        border_radius: [f32; 4],
    ) {
        self.draw_list
            .borrow_mut()
            .fill_rect(
                Rect::new(x, y, width, height),
                color,
                border_radius,
                [0.0; 4],
                Color::transparent(),
            );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_shadow_rect(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        shadow_color: Color,
        shadow_params: [f32; 4],
        border_radius: [f32; 4],
        inset: bool,
        side_params: [f32; 3],
    ) {
        self.draw_list
            .borrow_mut()
            .draw_shadow_rect(
                Rect::new(x, y, width, height),
                shadow_color,
                shadow_params,
                border_radius,
                inset,
                side_params,
            );
    }

    pub fn set_clip(&self, x: f32, y: f32, width: f32, height: f32) {
        self.draw_list
            .borrow_mut()
            .push_clip(Rect::new(x, y, width, height));
    }

    pub fn set_clip_rounded(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        border_radius: [f32; 4],
    ) {
        self.draw_list
            .borrow_mut()
            .push_clip_rounded(Rect::new(x, y, width, height), border_radius);
    }

    pub fn clear_clip(&self) {
        self.draw_list
            .borrow_mut()
            .pop_clip();
    }

    pub fn get_transform_translation(&self) -> (f32, f32) {
        let transform = self.draw_list.borrow();
        let t = transform.current_transform();
        (t.cols[2][0], t.cols[2][1])
    }

    pub fn set_alpha(&self, alpha: f32) {
        self.draw_list
            .borrow_mut()
            .set_alpha(alpha);
    }

    /// Enables/disables synthetic italic for subsequent plain text draws.
    pub fn set_italic(&self, italic: bool) {
        self.draw_list
            .borrow_mut()
            .set_italic(italic);
    }

    pub fn restore_alpha(&self) {
        self.draw_list
            .borrow_mut()
            .restore_alpha();
    }

    pub fn load_image(&self, bytes: &[u8], width: u32, height: u32) -> TextureId {
        self.draw_list
            .borrow_mut()
            .load_image(bytes, width, height)
    }

    pub fn load_image_with_id(&self, texture_id: TextureId, bytes: &[u8], width: u32, height: u32) {
        self.draw_list
            .borrow_mut()
            .load_image_with_id(texture_id, bytes, width, height)
    }

    pub fn remove_texture(&self, texture_id: TextureId) {
        self.draw_list
            .borrow_mut()
            .remove_texture(texture_id);
    }

    pub fn set_texture_size(&self, texture_id: TextureId, width: u32, height: u32) {
        self.draw_list
            .borrow_mut()
            .set_texture_size(texture_id, width, height);
    }

    pub fn draw_list(&self) -> Ref<'_, DrawList> {
        self.draw_list.borrow()
    }

    pub fn get_image_size(&self, texture_id: TextureId) -> Option<(u32, u32)> {
        self.draw_list
            .borrow()
            .get_texture_size(texture_id)
    }
}

impl Default for CupidCanvas {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod family_metrics_tests {
    use crate::font::{FontFamily, FontStyle, FontWeight};

    use super::CupidCanvas;

    #[test]
    fn metrics_cache_isolated_by_selected_family() {
        let canvas = CupidCanvas::new();
        let sans = canvas.measure_text_metrics_styled(
            "Mi",
            20.0,
            0.0,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
        );
        let mono = canvas.measure_text_metrics_styled(
            "Mi",
            20.0,
            0.0,
            FontFamily::MONOSPACE,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
        );

        assert_ne!(sans.width, mono.width);
        assert_eq!(
            canvas
                .metrics_cache
                .borrow()
                .len(),
            2
        );
    }

    #[test]
    fn wrapped_line_widths_preserve_short_final_line() {
        let canvas = CupidCanvas::new();
        let widths = canvas.measure_text_line_widths_styled(
            "MMMM i",
            13.0,
            45.0,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
        );

        assert_eq!(widths.len(), 2);
        assert!(widths[1] < widths[0]);
    }
}
