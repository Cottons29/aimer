use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
#[cfg(not(target_arch = "wasm32"))]
pub type Canvas = skia_safe::Canvas;
#[cfg(target_arch = "wasm32")]
pub type Canvas = web_sys::CanvasRenderingContext2d;

#[allow(dead_code)]
pub struct AimerCanvas<'a> {    
    #[cfg(not(target_arch = "wasm32"))]
    canvas: &'a mut skia_safe::Canvas,
    #[cfg(target_arch = "wasm32")]
    canvas: &'a mut web_sys::CanvasRenderingContext2d,
}



impl<'a> AimerCanvas<'a> {
    /// 
    /// Provides access to the internal `Canvas` reference tied to the current object instance.
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
        self.canvas
    }
}

#[allow(dead_code)]
impl<'a> AimerCanvas<'a> {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(canvas: &'a mut skia_safe::Canvas) -> Self {
        Self { canvas }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new(canvas: &'a mut web_sys::CanvasRenderingContext2d) -> Self {
        Self { canvas }
    }

    pub fn fill_rect(&mut self, pos: Vec2d, size: ResolvedSize) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut paint = skia_safe::Paint::default();
            paint.set_style(skia_safe::paint::Style::Fill);
            let rect = skia_safe::Rect::from_xywh(pos.x, pos.y, size.width, size.height);
            self.canvas.draw_rect(rect, &paint);
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.canvas.fill_rect(pos.x, pos.y, size.width, size.height);
        }
    }

    pub fn clear_rect(&mut self,pos: Vec2d, size: ResolvedSize) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut paint = skia_safe::Paint::default();
            paint.set_blend_mode(skia_safe::BlendMode::Clear);
            let rect = skia_safe::Rect::from_xywh(pos.x, pos.y, size.width, size.height);
            self.canvas.draw_rect(rect, &paint);
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.canvas.clear_rect(pos.x, pos.y, size.width, size.height);
        }
    }

    // pub fn translate(&mut self, pos: Vec2d) {
    //     #[cfg(not(target_arch = "wasm32"))]
    //     {
    //         self.canvas.translate(pos.x, pos.y);
    //     }
    //     #[cfg(target_arch = "wasm32")]
    //     {
    //         match self.canvas.translate(pos.x, pos.y) {
    //             Ok(_) => {}
    //             Err(err) => {
    //                 log::error!("Failed to translate canvas: {:?}", err);
    //             }
    //         }
    //     }
    // }

    // pub fn clip_rect()
}


