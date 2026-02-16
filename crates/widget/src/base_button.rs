use crate::base::BuildContext;
use crate::attribute::size::Size;
use crate::{StatelessWidget, base::Vec2d};
use color::prelude::ColorMixer;
use constructor::Constructor;
use skia_safe::{Color, Font, Paint, Point, Rect, TextBlob};
pub trait IntoButton {
    fn to_button(self) -> ButtonTemplate;
}
#[derive(Constructor)]
pub struct ButtonTemplate {
    pub pos: Vec2d,
    pub size: Size,
    pub label: String,
    pub background: color::prelude::Color,
    pub on_click: Box<dyn Fn() + Send + Sync>,
}

impl StatelessWidget for ButtonTemplate {
    fn pos(&self) -> Option<Vec2d> {
        Some(self.pos)
    }
    fn size(&self) -> Option<Size> {
        Some(self.size)
    }

    fn draw(&self, ctx: &BuildContext) {
        let mut paint = Paint::default();
        paint.set_color(Color::from(self.background.to_u32()));

        let rect = Rect::from_xywh(
            self.pos.x,
            self.pos.y,
            self.size.width as f32,
            self.size.height as f32,
        );
        ctx.canvas.draw_rect(rect, &paint);

        let mut font = Font::default();
        font.set_size(20.0);

        if let Some(blob) = TextBlob::from_str(&self.label, &font) {
            let mut paint = Paint::default();
            paint.set_color(skia_safe::Color::WHITE);
            ctx.canvas.draw_text_blob(
                &blob,
                Point::new(self.pos.x * 0.5,self. pos.y * 0.5),
                &paint,
            );
        }
    }

    fn on_click(&self) -> Option<&Box<dyn Fn() + Send + Sync>> {
        Some(&self.on_click)
    }
}
