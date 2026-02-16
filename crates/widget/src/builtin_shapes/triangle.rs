use skia_safe::{Canvas, Paint, Color, Rect, Point};
use crate::{StatelessWidget, components::context::BuildContext};


pub struct Triangle {
    pub p1: (f32, f32),
    pub p2: (f32, f32),
    pub p3: (f32, f32),
    pub color: u32,
}

impl StatelessWidget for Triangle {
    fn draw(&self, ctx: &BuildContext) {
        let mut paint = Paint::default();
        paint.set_color(Color::from(self.color));
        
        // Draw 3 lines
        ctx.canvas.draw_line(Point::new(self.p1.0, self.p1.1), Point::new(self.p2.0, self.p2.1), &paint);
        ctx.canvas.draw_line(Point::new(self.p2.0, self.p2.1), Point::new(self.p3.0, self.p3.1), &paint);
        ctx.canvas.draw_line(Point::new(self.p3.0, self.p3.1), Point::new(self.p1.0, self.p1.1), &paint);
    }

    // fn set_state(&mut self) {
    //     self.p1.0 += 1.0;
    //     self.p1.1 += 1.0;
    //     self.p2.0 += 1.0;
    //     self.p2.1 += 1.0;
    //     self.p3.0 += 1.0;
    //     self.p3.1 += 1.0;
    // }
}
