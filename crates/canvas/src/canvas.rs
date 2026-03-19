use crate::canvas::inner::{AimerCanvasInner, Canvas};
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;

#[cfg(target_arch = "wasm32")]
mod wasm_impl;
#[cfg(not(target_arch = "wasm32"))]
mod native_impl;
mod inner;



#[allow(dead_code)]
pub struct AimerCanvas<'a> {    
    inner: AimerCanvasInner<'a>
}



impl<'a> AimerCanvas<'a> {
    #[allow(dead_code)]
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
    /// # Notes
    /// The return type is `skia_safe::Canvas` on non-wasm32 targets, and `web_sys::CanvasRenderingContext2d` on wasm32 targets.
    ///
    unsafe fn get_canvas(&'a self) -> &'a Canvas  {
        self.inner.canvas()
    }

    #[allow(dead_code)]
    pub fn new(canvas: &'a Canvas) -> Self {
        Self {
            inner: AimerCanvasInner {canvas }
        }
    }
}

impl<'a> AimerCanvas<'a> {
    ///
    /// Fills a rectangular area on the canvas with the specified position and size.
    ///
    /// # Parameters
    /// - `pos`: A `Vec2d` representing the top-left corner of the rectangle.
    ///   It specifies the position on the canvas where the rectangle will be drawn.
    /// - `size`: A `ResolvedSize` object defining the width and height of the rectangle.
    ///
    /// # Example
    /// ```rust skip-test
    /// use attribute::{Vec2d, ResolvedSize};
    ///
    ///
    /// let mut canvas = Canvas::new(...);
    /// let position = Vec2d::new(10.0, 20.0);
    /// let size = ResolvedSize::new(100.0, 50.0);
    /// canvas.fill_rect(position, size);
    /// ```
    ///
    /// This function forwards the call to an internal implementation to draw the rectangle.
    ///
    /// # Notes
    /// - Ensure that the `Vec2d` and `ResolvedSize` types are properly defined in your codebase
    ///   before using this function.
    /// - This method modifies the internal state of the canvas object (`self`).
    ///
    #[allow(dead_code)]
    pub fn fill_rect(&mut self, pos: Vec2d, size: ResolvedSize) {
        self.inner.fill_rect(pos, size);
    }

    ///
    /// Clears a rectangular area on the canvas at the specified position and with the specified size.
    ///
    /// # Parameters
    /// - `pos`: A `Vec2d` struct representing the top-left corner of the rectangle to be cleared.
    /// - `size`: A `ResolvedSize` struct representing the dimensions (width and height) of the rectangle to be cleared.
    ///
    /// # Behavior
    /// This function delegates the operation of clearing the rectangular area to the `clear_rect`
    /// method of the `inner` object, erasing any existing content within the specified rectangle.
    ///
    /// # Examples
    /// ```rust skip-test
    /// use attribute::{Vec2d, ResolvedSize};
    ///
    /// let mut canvas = Canvas::new(...);
    /// let position = Vec2d::new(10.0, 20.0);
    /// let size = ResolvedSize::new(100.0, 50.0);
    /// canvas.clear_rect(position, size);
    /// ```
    ///
    /// This will clear a rectangle starting at position `(10.0, 20.0)` with a width of `100.0`
    /// and a height of `50.0` on the canvas.
    ///
    #[allow(dead_code)]
    pub fn clear_rect(&mut self,pos: Vec2d, size: ResolvedSize) {
        self.inner.clear_rect(pos, size);
    }

    ///
    /// Translates the position of the object by the given vector.
    ///
    /// This method updates the position of the object by applying the specified
    /// 2D translation vector (`pos`). It modifies the object's internal position
    /// by delegating the translation operation to the `translate` method of the
    /// `inner` object.
    ///
    /// # Arguments
    ///
    /// * `pos` - A `Vec2d` structure representing the translation vector. This
    ///           specifies the amount by which the object's position should be
    ///           shifted in both the X and Y axes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// our_canvas.translate(Vec2d{x: 10.0, y: 5.0});
    /// ```
    ///
    /// After this call, the object's position will have been shifted by (10.0, 5.0).
    ///
    /// # Note
    /// The method assumes that the `inner` object has a `translate` method that
    /// takes a `Vec2d` as its argument.
    ///
    #[allow(dead_code)]
    pub fn translate(&mut self, pos: Vec2d) {
        self.inner.translate(pos);
    }
}
