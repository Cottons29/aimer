use skia_safe::{Canvas, Paint, Color, Rect};
use crate::{StatelessWidget, context::BuildContext};
pub struct Square {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: u32,
}

impl StatelessWidget for Square {
    fn draw(&self, ctx: &BuildContext) {
        let mut paint = Paint::default();
        paint.set_color(Color::from(self.color));
        let rect = Rect::from_xywh(self.x, self.y, self.size, self.size);
        ctx.canvas.draw_rect(rect, &paint);
    }
}
