use crate::canvas::AimerCanvas;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use crate::canvas::inner::AimerCanvasInner;

#[allow(dead_code)]
impl<'a> AimerCanvasInner<'a> {

    #[inline]
    pub fn fill_rect(&mut self, pos: Vec2d, size: ResolvedSize) {

        // self.canvas.fill_rect(pos.x, pox.y, size)
    }

    #[inline]
    pub fn clear_rect(&mut self, pos: Vec2d, size: ResolvedSize) {
        // let mut paint = skia_safe::Paint::default();
        // paint.set_blend_mode(skia_safe::BlendMode::Clear);
        // let rect = skia_safe::Rect::from_xywh(pos.x, pos.y, size.width, size.height);
        // self.canvas.draw_rect(rect, &paint);
    }

    #[inline]
    pub fn translate(&mut self, pos: Vec2d) {
        // self.canvas.translate((pos.x, pos.y));
    }

    #[inline]
    pub fn save(&self) {
        // self.canvas.save();
    }

}
