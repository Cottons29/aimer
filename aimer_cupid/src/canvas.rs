use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use crate::draw_cmd::DrawList;
use crate::font::{FontFamily, FontStyle, FontWeight, TextLanguage};
use crate::lru_map::LruMap;
use crate::svg::{SvgNodeStyleOverride, SvgScene};
use crate::text_pipeline::TextOverflowMode;
use crate::text_pipeline::glyph_rasterizer::GlyphRasterizer;
use crate::text_pipeline::text_layout::line_break_opportunities;
use crate::text_pipeline::TextShadowRequest;
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
    /// Measured widths depend on the face the run resolves to, which for
    /// ideographs the language decides — see
    /// [`CupidCanvas::set_text_language`].
    language: Option<TextLanguage>,
}

#[derive(Clone, Debug)]
struct CachedTextMetrics {
    metrics: TextMetrics,
    line_widths: Vec<f32>,
}

/// How many measured strings the canvas remembers.
///
/// A scrolled list of distinct rows measures a viewport's worth of new strings
/// per frame, so the cache is sized to hold many screens of them and evicts the
/// coldest quarter when it fills. It must never be emptied outright: that would
/// drop the strings the current frame is built from and turn every newly visible
/// row into a permanent miss.
const METRICS_CACHE_CAPACITY: usize = 4096;

#[derive(Clone)]
pub struct CupidCanvas {
    draw_list: Rc<RefCell<DrawList>>,
    rasterizer: Rc<RefCell<GlyphRasterizer>>,
    metrics_cache: Rc<RefCell<LruMap<TextMetricsKey, CachedTextMetrics>>>,
    /// The language subsequent text is written in — see
    /// [`CupidCanvas::set_text_language`].
    ///
    /// Drawing records the state into the draw list, but measuring answers
    /// immediately and never reaches the renderer, so the canvas keeps the
    /// current value here as well: a field that paints its text in a Chinese
    /// face must not place its caret with a Japanese face's advances.
    text_language: Rc<Cell<Option<TextLanguage>>>,
}

impl CupidCanvas {
    pub fn new() -> Self {
        Self {
            draw_list: Rc::new(RefCell::new(DrawList::new())),
            rasterizer: Rc::new(RefCell::new(GlyphRasterizer::new())),
            metrics_cache: Rc::new(RefCell::new(LruMap::new(METRICS_CACHE_CAPACITY))),
            text_language: Rc::new(Cell::new(None)),
        }
    }

    pub fn begin_frame(&self) {
        self.draw_list.borrow_mut().clear();
    }

    /// Moves the frame recorded so far out of the canvas.
    ///
    /// The returned list owns its command buffer, so it can be handed to
    /// another thread for encoding while the canvas keeps serving the next
    /// frame. Ownership must come back through [`recycle_draw_list`] before the
    /// next [`begin_frame`], otherwise the allocation is dropped and the
    /// texture-size table the list carries is lost.
    ///
    /// [`recycle_draw_list`]: CupidCanvas::recycle_draw_list
    /// [`begin_frame`]: CupidCanvas::begin_frame
    #[inline]
    pub fn take_draw_list(&self) -> DrawList {
        std::mem::take(&mut *self.draw_list.borrow_mut())
    }

    /// Gives a list taken by [`take_draw_list`] back to the canvas so its
    /// buffers are reused instead of reallocated every frame.
    ///
    /// [`take_draw_list`]: CupidCanvas::take_draw_list
    #[inline]
    pub fn recycle_draw_list(&self, draw_list: DrawList) {
        *self.draw_list.borrow_mut() = draw_list;
    }

    pub fn register_font_bytes(&self, bytes: Vec<u8>) -> Option<crate::text_layout::FontId> {
        let font_id = self.rasterizer.borrow_mut().register_font_bytes(bytes)?;
        self.metrics_cache.borrow_mut().clear();
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
        self.draw_list.borrow_mut().fill_rect(
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
        self.draw_list.borrow_mut().fill_rect(
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
        self.draw_list.borrow_mut().fill_rect(
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
        self.draw_list.borrow_mut().translate(x, y);
    }

    pub fn scale(&self, sx: f32, sy: f32) {
        self.draw_list.borrow_mut().scale(sx, sy);
    }

    pub fn rotate(&self, radians: f32) {
        self.draw_list.borrow_mut().rotate(radians);
    }

    pub fn save(&self) {
        self.draw_list.borrow_mut().save();
    }

    pub fn restore(&self) {
        self.draw_list.borrow_mut().restore();
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
        self.draw_list.borrow_mut().draw_text_styled(
            Vec2d::new(x, y),
            Arc::from(text),
            font_size,
            color,
            font_family,
            font_style,
            font_weight,
        );
    }

    /// Records a shadow-only styled text request for the glyph pipeline.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_shadow_styled(
        &self,
        x: f32,
        y: f32,
        text: &str,
        font_size: f32,
        color: Color,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
        shadow: TextShadowRequest,
    ) {
        self.draw_list.borrow_mut().draw_text_shadow_styled(
            Vec2d::new(x, y),
            Arc::from(text),
            font_size,
            color,
            font_family,
            font_style,
            font_weight,
            shadow,
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
        self.draw_list.borrow_mut().draw_text_with_overflow(
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
        self.draw_text_aligned_with_overflow_styled(
            x,
            y,
            text,
            font_size,
            color,
            bounds_width,
            bounds_height,
            overflow,
            crate::text_pipeline::text_layout::TextHorizontalAlign::Left,
            font_family,
            font_style,
            font_weight,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_aligned_with_overflow_styled(
        &self,
        x: f32,
        y: f32,
        text: &str,
        font_size: f32,
        color: Color,
        bounds_width: f32,
        bounds_height: f32,
        overflow: TextOverflowMode,
        horizontal_align: crate::text_pipeline::text_layout::TextHorizontalAlign,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) {
        self.draw_list.borrow_mut().draw_text_aligned_with_overflow(
            Vec2d::new(x, y),
            Arc::from(text),
            font_size,
            color,
            Some(bounds_width),
            Some(bounds_height),
            overflow,
            horizontal_align,
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
        self.draw_list.borrow_mut().draw_text_decoration(
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
        self.rasterizer.borrow_mut().measure_text_for_family(
            text,
            font_size,
            font_family,
            FontWeight::Value(u32::from(font_weight)),
            font_style,
            self.text_language(),
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
        let language = self.text_language();
        let key = TextMetricsKey {
            text: text.to_string(),
            font_size_tenths: (font_size * 10.0) as u32,
            max_width_tenths: (max_width.max(0.0) * 10.0) as u32,
            font_family,
            font_style,
            font_weight,
            language,
        };
        if let Some(cached) = self.metrics_cache.borrow_mut().get(&key) {
            return cached.metrics;
        }

        let mut rasterizer = self.rasterizer.borrow_mut();
        // Measuring character by character would let an ideograph pick a face the
        // shaping pass rejects, so the run is announced first here too.
        rasterizer.begin_script_run(text, language);
        let weight = FontWeight::Value(u32::from(font_weight));
        let (ascent, descent, line_gap) =
            rasterizer.line_metrics_for_family(font_size, font_family, weight, font_style);
        let line_height = ascent - descent + line_gap;
        let mut width = 0.0_f32;
        let mut current_width = 0.0_f32;
        let mut line_count = 1_usize;
        let mut line_widths = Vec::new();
        // Width of the current line up to its last UAX #14 break opportunity.
        // `None` means no break opportunity is available on the current line
        // yet. This mirrors the word-wrapping performed by `layout_shaped_text`
        // so the measured line count matches the rendered one (otherwise the
        // last line would be clipped).
        let mut last_break_end: Option<f32> = None;
        let can_break_before = line_break_opportunities(text);

        for (offset, c) in text.char_indices() {
            if c == '\n' {
                width = width.max(current_width);
                line_widths.push(current_width);
                current_width = 0.0;
                line_count += 1;
                last_break_end = None;
                continue;
            }

            let glyph_width =
                rasterizer.advance_width_for_family(c, font_size, font_family, weight, font_style);

            // Track where this line may be broken, measured before the
            // character is added so a trailing space stays on its own line.
            if can_break_before[offset] {
                last_break_end = Some(current_width);
            }

            if max_width > 0.0 && current_width > 0.0 && current_width + glyph_width > max_width {
                if let Some(break_end) = last_break_end {
                    // Word-wrap: the text after the break opportunity moves to
                    // the next line, so the current line ends at the break.
                    let moved_width = (current_width - break_end).max(0.0);
                    width = width.max(break_end);
                    line_widths.push(break_end);
                    current_width = moved_width;
                    line_count += 1;
                    last_break_end = None;
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
        rasterizer.end_script_run();

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

        self.metrics_cache.borrow_mut().insert(
            key,
            CachedTextMetrics {
                metrics,
                line_widths,
            },
        );
        metrics
    }

    /// Measures the rendered width of each line after applying the same
    /// wrapping rules as drawing.
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
            language: self.text_language(),
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
            .borrow_mut()
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
        self.draw_list.borrow_mut().fill_rect_with_outline(
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
        self.draw_list.borrow_mut().fill_rect_with_outline(
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
        self.draw_list.borrow_mut().fill_rect(
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
        self.draw_list.borrow_mut().fill_rect(
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
        self.draw_list.borrow_mut().fill_rect(
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
        self.draw_list.borrow_mut().fill_rect(
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
        self.draw_list.borrow_mut().draw_shadow_rect(
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
        self.draw_list.borrow_mut().pop_clip();
    }

    pub fn get_transform_translation(&self) -> (f32, f32) {
        let transform = self.draw_list.borrow();
        let t = transform.current_transform();
        (t.cols[2][0], t.cols[2][1])
    }

    pub fn set_alpha(&self, alpha: f32) {
        self.draw_list.borrow_mut().set_alpha(alpha);
    }

    /// Enables/disables synthetic italic for subsequent plain text draws.
    pub fn set_italic(&self, italic: bool) {
        self.draw_list.borrow_mut().set_italic(italic);
    }

    /// Declares the language subsequent text is written in.
    ///
    /// Han is unified: `你好` is covered by a Japanese face as readily as by a
    /// Chinese one, so a run of ideographs alone cannot say which face it
    /// wants, and it keeps whichever the platform's cascade prefers until a
    /// character only one language writes joins it — at which point the whole
    /// word changes typeface. A producer that knows the language says so once
    /// here, and every text drawn *and measured* afterwards is resolved in it.
    ///
    /// The setting is canvas state, like [`set_italic`](Self::set_italic):
    /// pass `None` to restore the default, where a run is judged on its own
    /// characters.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use aimer_cupid::canvas::CupidCanvas;
    /// # use aimer_cupid::font::TextLanguage;
    /// # let canvas = CupidCanvas::new();
    /// canvas.set_text_language(Some(TextLanguage::Chinese));
    /// canvas.draw_text(0.0, 0.0, "你好", 16.0, Default::default(), 400);
    /// canvas.set_text_language(None);
    /// ```
    pub fn set_text_language(&self, language: Option<TextLanguage>) {
        self.text_language.set(language);
        self.draw_list.borrow_mut().set_text_language(language);
    }

    /// The language declared by [`set_text_language`](Self::set_text_language).
    #[inline]
    pub fn text_language(&self) -> Option<TextLanguage> {
        self.text_language.get()
    }

    pub fn restore_alpha(&self) {
        self.draw_list.borrow_mut().restore_alpha();
    }

    pub fn load_image(&self, bytes: &[u8], width: u32, height: u32) -> TextureId {
        self.draw_list.borrow_mut().load_image(bytes, width, height)
    }

    pub fn load_image_with_id(&self, texture_id: TextureId, bytes: &[u8], width: u32, height: u32) {
        self.draw_list
            .borrow_mut()
            .load_image_with_id(texture_id, bytes, width, height)
    }

    pub fn remove_texture(&self, texture_id: TextureId) {
        self.draw_list.borrow_mut().remove_texture(texture_id);
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
        self.draw_list.borrow().get_texture_size(texture_id)
    }
}

impl Default for CupidCanvas {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod family_metrics_tests {
    use super::CupidCanvas;
    use crate::font::{FontFamily, FontStyle, FontWeight};

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
        assert_eq!(canvas.metrics_cache.borrow().len(), 2);
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

    #[test]
    fn measured_cjk_lines_fill_the_available_width() {
        // Chinese carries no spaces, so measuring must break between
        // ideographs.  Rewinding to the last Latin space instead would report
        // a half-empty line — and, being one line too many, an inflated
        // height for the widget that owns the text.
        let canvas = CupidCanvas::new();
        let font_size = 16.0;
        let max_width = 240.0;

        let widths = canvas.measure_text_line_widths_styled(
            "「Hello, World!」（世界你好！）之類字串的電腦程式在大多数通用编程语言中",
            font_size,
            max_width,
            FontFamily::SANS_SERIF,
            FontStyle::Normal,
            FontWeight::Normal.numeric(),
        );

        assert!(widths.len() > 1, "the sample must wrap at {max_width}px");
        for (index, width) in widths.iter().enumerate().take(widths.len() - 1) {
            assert!(
                *width > max_width - font_size * 1.5,
                "line {index} stopped at {width}px of {max_width}px"
            );
        }
    }
}
