
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use color::prelude::Color;

#[cfg(target_arch = "wasm32")]
mod wasm_impl;
#[cfg(not(target_arch = "wasm32"))]
mod native_impl;

pub trait CanvasRendering : Clone {
    fn begin_frame(&self);
    fn fill_rect(&self, pos: Vec2d, size: ResolvedSize);
    fn fill_rect_with_border(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: f32,
        border_width: f32,
        border_color: Color,
    );
    fn clear_rect(&self, pos: Vec2d, size: ResolvedSize);
    fn translate(&self, pos: Vec2d);
    fn scale(&self, sx: f32, sy: f32);
    fn rotate(&self, radians: f32);
    fn save(&self);
    fn restore(&self);
    fn draw_text(&self, text: &str, pos: Vec2d, font_size: f32, color: Color);
    fn draw_image(&self, image_id: u32, pos: Vec2d, size: ResolvedSize);
    fn set_clip(&self, pos: Vec2d, size: ResolvedSize);
    fn clear_clip(&self);
    fn measure_text(&self, text: &str, font_size: f32) -> f32;
    fn stroke_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        stroke_color: Color,
        stroke_width: f32,
        border_radius: f32,
    );
    fn fill_color_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: f32,
    );
    fn set_alpha(&self, alpha: f32);
    fn restore_alpha(&self);
    fn get_transform_translation(&self) -> (f64, f64) { (0.0, 0.0) }
}


#[cfg(target_arch = "wasm32")]
use web_sys::CanvasRenderingContext2d as Canvas;
#[cfg(not(target_arch = "wasm32"))]
use cupid::canvas::CupidCanvas as Canvas;

pub type InnerCanvas = Canvas;


#[allow(dead_code)]
// #[derive(Clone)]
#[derive(Clone)]
pub struct AimerCanvas<'a> {
    inner: &'a Canvas
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
    /// ```rust
    /// let canvas = my_object.get_canvas();
    /// // Ensure no mutable operations on `my_object` while using `canvas`.
    /// ```
    ///
    unsafe fn get_canvas(&'a self) -> &'a Canvas  {
        self.inner
    }

    #[allow(dead_code)]
    #[inline]
    pub fn new(canvas: &'a Canvas) -> Self {
        Self {
            inner: canvas
        }
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
        border_radius: f32,
        border_width: f32,
        border_color: Color,
    ) {
        CanvasRendering::fill_rect_with_border(
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
    pub fn draw_text(&self, text: &str, pos: Vec2d, font_size: f32, color: Color) {
        CanvasRendering::draw_text(self.inner, text, pos, font_size, color);
    }

    /// Draws an image identified by `image_id` at the specified position and size.
    #[allow(dead_code)]
    #[inline]
    pub fn draw_image(&self, image_id: u32, pos: Vec2d, size: ResolvedSize) {
        CanvasRendering::draw_image(self.inner, image_id, pos, size);
    }

    /// Sets a clipping rectangle. Drawing outside this rect will be clipped.
    #[allow(dead_code)]
    #[inline]
    pub fn set_clip(&self, pos: Vec2d, size: ResolvedSize) {
        CanvasRendering::set_clip(self.inner, pos, size);
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

    /// Draws a stroked (outline-only) rectangle.
    #[allow(dead_code)]
    #[inline]
    pub fn stroke_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        stroke_color: Color,
        stroke_width: f32,
        border_radius: f32,
    ) {
        CanvasRendering::stroke_rect(
            self.inner,
            pos,
            size,
            stroke_color,
            stroke_width,
            border_radius,
        );
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

    /// Returns the current transform's translation (tx, ty) in physical pixels.
    #[allow(dead_code)]
    #[inline]
    pub fn get_transform_translation(&self) -> (f64, f64) {
        CanvasRendering::get_transform_translation(self.inner)
    }

    /// Draws a filled rectangle with a specific color.
    #[allow(dead_code)]
    #[inline]
    pub fn fill_color_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: f32,
    ) {
        CanvasRendering::fill_color_rect(
            self.inner,
            pos,
            size,
            color,
            border_radius,
        );
    }
}
