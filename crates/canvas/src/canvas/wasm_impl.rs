use std::panic::Location;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use utils::error;
use crate::canvas::AimerCanvas;
use crate::canvas::inner::AimerCanvasInner;

#[allow(dead_code)]
impl<'a> AimerCanvasInner<'a> {

    #[inline]
    pub fn fill_rect(&mut self, pos: Vec2d, size: ResolvedSize) {
         self.canvas.fill_rect(pos.x, pos.y, size.width, size.height);
    }

    #[inline]
    pub fn clear_rect(&mut self,pos: Vec2d, size: ResolvedSize) {
        self.canvas.clear_rect(pos.x, pos.y, size.width, size.height);
    }


    #[inline]
    #[track_caller]
    pub fn translate(&mut self, pos: Vec2d) {
        #[cfg(not(debug_assertions))]
        { self.canvas.translate(pos.x, pos.y).unwrap(); }
        #[cfg(debug_assertions)]
        {
            if let Err(err) =  self.canvas.translate(pos.x, pos.y) {
                let err = err.as_string().unwrap_or_default();
                let location = Location::caller();
                let file_name = location.file();
                let line = location.line();
                let column = location.column();
                let error_str = format!("{}:{}:{}", file_name, line, column);
                error!("Translation error: {err} \nat {error_str}");
            }
        }
    }
    #[inline]
    pub fn save(&self) {
        self.canvas.save();
    }
}

