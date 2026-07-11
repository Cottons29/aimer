use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_color::prelude::Color;
pub use aimer_cupid::text_pipeline::TextOverflowMode;
pub use aimer_cupid::canvas::TextMetrics;
mod native_impl;

pub trait CanvasRendering: Clone {
    fn begin_frame(&self);
    fn fill_rect(&self, pos: Vec2d, size: ResolvedSize);
    fn fill_rect_with_border(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
    );
    /// Draws a filled rectangle with per-corner border radii and per-side border widths.
    /// `border_radius`: [top-left, top-right, bottom-right, bottom-left]
    /// `border_width`: [top, right, bottom, left]
    fn fill_rect_with_per_side_border(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
    );
    fn clear_rect(&self, pos: Vec2d, size: ResolvedSize);
    fn translate(&self, pos: Vec2d);
    fn scale(&self, sx: f32, sy: f32);
    fn rotate(&self, radians: f32);
    fn save(&self);
    fn restore(&self);
    fn draw_text(&self, text: &str, pos: Vec2d, font_size: f32, color: Color, font_weight: u16);
    #[allow(clippy::too_many_arguments)]
    fn draw_text_wrapped(&self, text: &str, pos: Vec2d, font_size: f32, color: Color, max_width: f32, font_weight: u16);
    #[allow(clippy::too_many_arguments)]
    fn draw_text_with_overflow(
        &self,
        text: &str,
        pos: Vec2d,
        font_size: f32,
        color: Color,
        bounds_width: f32,
        bounds_height: f32,
        overflow: TextOverflowMode,
        font_weight: u16,
    );
    /// Draw a styled text-decoration line (underline/overline/line-through).
    /// `pos`/`size` describe the band; `style` is `TextDecorationStyle::id`.
    #[allow(clippy::too_many_arguments)]
    fn draw_text_decoration(&self, pos: Vec2d, size: ResolvedSize, color: Color, style: u32, thickness: f32, period: f32);
    fn draw_image(&self, image_id: u32, pos: Vec2d, size: ResolvedSize);
    fn get_image_size(&self, image_id: u32) -> Option<(u32, u32)>;
    fn set_clip(&self, pos: Vec2d, size: ResolvedSize);
    fn set_clip_rounded(&self, pos: Vec2d, size: ResolvedSize, border_radius: [f32; 4]);
    fn clear_clip(&self);
    fn measure_text(&self, text: &str, font_size: f32) -> f32;
    fn measure_text_metrics(&self, text: &str, font_size: f32, max_width: f32) -> TextMetrics;
    fn stroke_rect(&self, pos: Vec2d, size: ResolvedSize, stroke_color: Color, stroke_width: f32, border_radius: [f32; 4]);
    /// Draws a stroked rectangle with per-corner radii and per-side widths.
    /// `border_radius`: [top-left, top-right, bottom-right, bottom-left]
    /// `stroke_width`: [top, right, bottom, left]
    fn stroke_rect_per_side(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        stroke_color: Color,
        stroke_width: [f32; 4],
        border_radius: [f32; 4],
    );
    #[allow(clippy::too_many_arguments)]
    /// Draws a filled rectangle with border and outline in a single pass.
    fn fill_rect_with_border_and_outline(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
        outline_width: f32,
        outline_color: Color,
    );
    #[allow(clippy::too_many_arguments)]
    /// Draws a filled rectangle with border and outline with per-corner/per-side control.
    fn fill_rect_with_border_and_outline_per_side(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
        outline_width: [f32; 4],
        outline_color: Color,
    );

    fn fill_color_rect(&self, pos: Vec2d, size: ResolvedSize, color: Color, border_radius: [f32; 4]);
    /// Draws a filled rectangle with per-corner border radii.
    /// `border_radius`: [top-left, top-right, bottom-right, bottom-left]
    fn fill_color_rect_per_corner(&self, pos: Vec2d, size: ResolvedSize, color: Color, border_radius: [f32; 4]);
    #[allow(clippy::too_many_arguments)]
    fn draw_shadow_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        shadow_color: Color,
        shadow_params: [f32; 4],
        border_radius: [f32; 4],
        inset: bool,
        side_params: [f32; 3],
    );
    fn set_alpha(&self, alpha: f32);
    fn restore_alpha(&self);
    /// Enables/disables synthetic italic for subsequent plain text draws.
    /// Default is a no-op for backends that don't support it.
    fn set_italic(&self, _italic: bool) {}
    fn load_image(&self, bytes: &[u8], width: u32, height: u32) -> u32;
    fn load_image_with_id(&self, image_id: u32, bytes: &[u8], width: u32, height: u32);
    fn set_texture_size(&self, image_id: u32, width: u32, height: u32);
    fn get_transform_translation(&self) -> (f32, f32) {
        (0.0, 0.0)
    }
}

use aimer_cupid::canvas::CupidCanvas as Canvas;

pub type InnerCanvas = Canvas;

#[allow(dead_code)]
// #[derive(Clone)]
#[derive(Clone)]
pub struct AimerCanvas<'a> {
    inner: &'a Canvas,
}

impl<'a> AimerCanvas<'a> {
    #[allow(dead_code)]
    #[inline]
    /// Provides low level control to AimerCanvas
    ///
    /// # Safety
    /// This function is marked as `unsafe` because it directly returns a reference
    /// to an internal `Canvas` object. The caller need to write platform-specific code
    /// for making platform-specific operations.
    ///
    /// # Returns
    /// * `&'a Canvas` - A reference to the internal `Canvas` object.
    ///
    /// # Example
    /// ```rust ignore
    /// let canvas = my_object.get_canvas();
    /// // Ensure no mutable operations on `my_object` while using `canvas`.
    /// ```
    ///
    unsafe fn get_canvas(&'a self) -> &'a Canvas {
        self.inner
    }

    #[allow(dead_code)]
    #[inline]
    pub fn new(canvas: &'a Canvas) -> Self {
        Self { inner: canvas }
    }

    pub fn get_inner_canvas(&self) -> &Canvas {
        self.inner
    }
}

impl<'a> AimerCanvas<'a> {
    /// Prepares the canvas for a new frame, clearing any previous draw commands.
    #[allow(dead_code)]
    #[inline]
    pub fn begin_frame(&self) {
        CanvasRendering::begin_frame(self.inner);
    }

    /// Fills a rectangular area on the canvas with the specified position and size.
    #[allow(dead_code)]
    #[inline]
    pub fn fill_rect(&self, pos: Vec2d, size: ResolvedSize) {
        CanvasRendering::fill_rect(self.inner, pos, size);
    }

    /// Fills a rectangular area with explicit color, border radius, border width, and border color.
    #[allow(dead_code)]
    #[inline]
    pub fn fill_rect_with_border(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
    ) {
        CanvasRendering::fill_rect_with_border(self.inner, pos, size, color, border_radius, border_width, border_color);
    }

    /// Fills a rectangular area with per-corner border radii and per-side border widths.
    /// `border_radius`: [top-left, top-right, bottom-right, bottom-left]
    /// `border_width`: [top, right, bottom, left]
    #[allow(dead_code)]
    #[inline]
    pub fn fill_rect_with_per_side_border(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
    ) {
        CanvasRendering::fill_rect_with_per_side_border(
            self.inner,
            pos,
            size,
            color,
            border_radius,
            border_width,
            border_color,
        );
    }

    /// Clears a rectangular area on the canvas at the specified position and with the specified size.
    #[allow(dead_code)]
    #[inline]
    pub fn clear_rect(&self, pos: Vec2d, size: ResolvedSize) {
        CanvasRendering::clear_rect(self.inner, pos, size);
    }

    /// Translates the canvas origin by the given vector.
    #[allow(dead_code)]
    #[inline]
    pub fn translate(&self, pos: Vec2d) {
        CanvasRendering::translate(self.inner, pos);
    }

    /// Scales the canvas by the given factors.
    #[allow(dead_code)]
    #[inline]
    pub fn scale(&self, sx: f32, sy: f32) {
        CanvasRendering::scale(self.inner, sx, sy);
    }

    /// Rotates the canvas by the given angle in radians.
    #[allow(dead_code)]
    #[inline]
    pub fn rotate(&self, radians: f32) {
        CanvasRendering::rotate(self.inner, radians);
    }

    /// Saves the entire state of the canvas by pushing the current state onto a stack.
    #[allow(dead_code)]
    #[inline]
    pub fn save(&self) {
        CanvasRendering::save(self.inner);
    }

    /// Restores the most recently saved canvas state from the stack.
    #[allow(dead_code)]
    #[inline]
    pub fn restore(&self) {
        CanvasRendering::restore(self.inner);
    }

    /// Draws text at the specified position with the given font size and color.
    #[allow(dead_code)]
    #[inline]
    pub fn draw_text(&self, text: &str, pos: Vec2d, font_size: f32, color: Color, font_weight: u16) {
        CanvasRendering::draw_text(self.inner, text, pos, font_size, color, font_weight);
    }

    /// Draws wrapped text constrained to `max_width`.
    #[allow(dead_code)]
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_wrapped(&self, text: &str, pos: Vec2d, font_size: f32, color: Color, max_width: f32, font_weight: u16) {
        CanvasRendering::draw_text_wrapped(self.inner, text, pos, font_size, color, max_width, font_weight);
    }

    /// Draws text with explicit bounds and overflow behavior.
    #[allow(dead_code)]
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_with_overflow(
        &self,
        text: &str,
        pos: Vec2d,
        font_size: f32,
        color: Color,
        bounds_width: f32,
        bounds_height: f32,
        overflow: TextOverflowMode,
        font_weight: u16,
    ) {
        CanvasRendering::draw_text_with_overflow(self.inner, text, pos, font_size, color, bounds_width, bounds_height, overflow, font_weight);
    }

    /// Draws a styled text-decoration line (underline/overline/line-through).
    #[allow(dead_code)]
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_decoration(&self, pos: Vec2d, size: ResolvedSize, color: Color, style: u32, thickness: f32, period: f32) {
        CanvasRendering::draw_text_decoration(self.inner, pos, size, color, style, thickness, period);
    }

    /// Draws an image identified by `image_id` at the specified position and size.
    #[allow(dead_code)]
    #[inline]
    pub fn draw_image(&self, image_id: u32, pos: Vec2d, size: ResolvedSize) {
        CanvasRendering::draw_image(self.inner, image_id, pos, size);
    }

    #[allow(dead_code)]
    #[inline]
    pub fn get_image_size(&self, image_id: u32) -> Option<(u32, u32)> {
        CanvasRendering::get_image_size(self.inner, image_id)
    }

    #[allow(dead_code)]
    #[inline]
    pub fn load_image(&self, bytes:  &[u8], width: u32, height: u32) -> u32 {
        CanvasRendering::load_image(self.inner, bytes,  width, height)
    }

    /// Loads an image from the specified path with a predefined image ID.
    #[allow(dead_code)]
    #[inline]
    pub fn load_image_with_id(&self, image_id: u32, bytes: &[u8], width: u32, height: u32) {
        CanvasRendering::load_image_with_id(self.inner, image_id, bytes, width, height)
    }

    /// Sets the intrinsic size of a texture. This is useful for preserving metadata across frames.
    pub fn set_texture_size(&self, image_id: u32, width: u32, height: u32) {
        CanvasRendering::set_texture_size(self.inner, image_id, width, height);
    }

    /// Sets a clipping rectangle. Drawing outside this rect will be clipped.
    #[allow(dead_code)]
    #[inline]
    pub fn set_clip(&self, pos: Vec2d, size: ResolvedSize) {
        CanvasRendering::set_clip(self.inner, pos, size);
    }

    /// Sets a rounded clipping rectangle. Drawing outside this rounded rect will be clipped.
    #[allow(dead_code)]
    #[inline]
    pub fn set_clip_rounded(&self, pos: Vec2d, size: ResolvedSize, border_radius: [f32; 4]) {
        CanvasRendering::set_clip_rounded(self.inner, pos, size, border_radius);
    }

    /// Clears the current clipping rectangle.
    #[allow(dead_code)]
    #[inline]
    pub fn clear_clip(&self) {
        CanvasRendering::clear_clip(self.inner);
    }

    /// Measures the approximate width of text at the given font size.
    #[allow(dead_code)]
    #[inline]
    pub fn measure_text(&self, text: &str, font_size: f32) -> f32 {
        CanvasRendering::measure_text(self.inner, text, font_size)
    }

    /// Measures paragraph text metrics at the given font size and optional max width.
    #[allow(dead_code)]
    #[inline]
    pub fn measure_text_metrics(&self, text: &str, font_size: f32, max_width: f32) -> TextMetrics {
        CanvasRendering::measure_text_metrics(self.inner, text, font_size, max_width)
    }

    /// Draws a stroked (outline-only) rectangle.
    #[allow(dead_code)]
    #[inline]
    pub fn stroke_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        stroke_color: Color,
        stroke_width: f32,
        border_radius: [f32; 4],
    ) {
        CanvasRendering::stroke_rect(self.inner, pos, size, stroke_color, stroke_width, border_radius);
    }

    /// Draws a stroked rectangle with per-corner radii and per-side widths.
    /// `border_radius`: [top-left, top-right, bottom-right, bottom-left]
    /// `stroke_width`: [top, right, bottom, left]
    #[allow(dead_code)]
    #[inline]
    pub fn stroke_rect_per_side(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        stroke_color: Color,
        stroke_width: [f32; 4],
        border_radius: [f32; 4],
    ) {
        CanvasRendering::stroke_rect_per_side(self.inner, pos, size, stroke_color, stroke_width, border_radius);
    }

    /// Sets the global alpha (opacity) for subsequent draw commands.
    #[allow(dead_code)]
    #[inline]
    pub fn set_alpha(&self, alpha: f32) {
        CanvasRendering::set_alpha(self.inner, alpha);
    }

    /// Restores the alpha to the previous value.
    #[allow(dead_code)]
    #[inline]
    pub fn restore_alpha(&self) {
        CanvasRendering::restore_alpha(self.inner);
    }

    /// Enables/disables synthetic italic for subsequent plain text draws.
    #[allow(dead_code)]
    #[inline]
    pub fn set_italic(&self, italic: bool) {
        CanvasRendering::set_italic(self.inner, italic);
    }

    /// Returns the current transform's translation (tx, ty) in physical pixels.
    #[allow(dead_code)]
    #[inline]
    pub fn get_transform_translation(&self) -> (f32, f32) {
        CanvasRendering::get_transform_translation(self.inner)
    }

    /// Draws a filled rectangle with border and outline in a single pass (no gap).
    #[allow(dead_code, clippy::too_many_arguments)]
    #[inline]
    pub fn fill_rect_with_border_and_outline(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
        outline_width: f32,
        outline_color: Color,
    ) {
        CanvasRendering::fill_rect_with_border_and_outline(
            self.inner,
            pos,
            size,
            color,
            border_radius,
            border_width,
            border_color,
            outline_width,
            outline_color,
        );
    }

    /// Draws a filled rectangle with border and outline with per-corner/per-side control.
    #[allow(dead_code, clippy::too_many_arguments)]
    #[inline]
    pub fn fill_rect_with_border_and_outline_per_side(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
        outline_width: [f32; 4],
        outline_color: Color,
    ) {
        CanvasRendering::fill_rect_with_border_and_outline_per_side(
            self.inner,
            pos,
            size,
            color,
            border_radius,
            border_width,
            border_color,
            outline_width,
            outline_color,
        );
    }

    /// Draws a shadow rectangle using GPU-accelerated SDF shadow rendering.
    #[allow(dead_code, clippy::too_many_arguments)]
    #[inline]
    pub fn draw_shadow_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        shadow_color: Color,
        shadow_params: [f32; 4],
        border_radius: [f32; 4],
        inset: bool,
        side_params: [f32; 3],
    ) {
        CanvasRendering::draw_shadow_rect(self.inner, pos, size, shadow_color, shadow_params, border_radius, inset, side_params);
    }

    /// Draws a filled rectangle with a specific color.
    #[allow(dead_code)]
    #[inline]
    pub fn fill_color_rect(&self, pos: Vec2d, size: ResolvedSize, color: Color, border_radius: [f32; 4]) {
        CanvasRendering::fill_color_rect(self.inner, pos, size, color, border_radius);
    }

    /// Draws a filled rectangle with per-corner border radii.
    /// `border_radius`: [top-left, top-right, bottom-right, bottom-left]
    #[allow(dead_code)]
    #[inline]
    pub fn fill_color_rect_per_corner(&self, pos: Vec2d, size: ResolvedSize, color: Color, border_radius: [f32; 4]) {
        CanvasRendering::fill_color_rect_per_corner(self.inner, pos, size, color, border_radius);
    }
}
