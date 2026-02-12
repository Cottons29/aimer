use skia_safe::{Canvas, Paint, Color, Rect, Point};

pub trait Widget {
    fn draw(&self, canvas: &Canvas);
}

pub struct Square {
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: u32,
}

impl Widget for Square {
    fn draw(&self, canvas: &Canvas) {
        let mut paint = Paint::default();
        paint.set_color(Color::from(self.color));
        let rect = Rect::from_xywh(self.x, self.y, self.size, self.size);
        canvas.draw_rect(rect, &paint);
    }
}

pub struct Circle {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub color: u32,
}

impl Widget for Circle {
    fn draw(&self, canvas: &Canvas) {
        let mut paint = Paint::default();
        paint.set_color(Color::from(self.color));
        canvas.draw_circle(Point::new(self.cx, self.cy), self.radius, &paint);
    }
}

pub struct Triangle {
    pub p1: (f32, f32),
    pub p2: (f32, f32),
    pub p3: (f32, f32),
    pub color: u32,
}

impl Widget for Triangle {
    fn draw(&self, canvas: &Canvas) {
        let mut paint = Paint::default();
        paint.set_color(Color::from(self.color));
        
        // Draw 3 lines
        canvas.draw_line(Point::new(self.p1.0, self.p1.1), Point::new(self.p2.0, self.p2.1), &paint);
        canvas.draw_line(Point::new(self.p2.0, self.p2.1), Point::new(self.p3.0, self.p3.1), &paint);
        canvas.draw_line(Point::new(self.p3.0, self.p3.1), Point::new(self.p1.0, self.p1.1), &paint);
    }
}
