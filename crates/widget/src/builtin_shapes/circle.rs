use crate::{StatelessWidget, context::BuildContext};
use skia_safe::{Color, Paint, Point};
pub struct Circle {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub color: u32,
}

impl StatelessWidget for Circle {
    fn draw(&self, ctx: &BuildContext) {
        let mut paint = Paint::default();
        paint.set_color(Color::from(self.color));
        ctx.canvas
            .draw_circle(Point::new(self.cx, self.cy), self.radius, &paint);
    }
}
