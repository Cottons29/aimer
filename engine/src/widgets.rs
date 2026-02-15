use color::prelude::{Color, ColorMixer};
use constructor::Constructor;
use skia_safe::{Font, Paint, Point, Rect, TextBlob};
use widget::{
    Widget,
    base::{BuildContext, ButtonTemplate, IntoButton, Size, Vec2d},
};
#[derive(Constructor)]
pub struct DemoButton {
    label: String,
    size: Size,
    background: Color,
    on_click: Box<dyn Fn() + Send + Sync>,
}

impl IntoButton for DemoButton {
    fn to_button(self) -> widget::base::ButtonTemplate {
        widget::ButtonTemplate!(
             pos : Vec2d {x : 300.0, y : 500.0},
             size: self.size,
             label: self.label,
             background: self.background,
             on_click: self.on_click,
        )
    }
}

impl Widget for DemoButton {
    fn draw(&self, ctx: &BuildContext) {
        let mut paint = Paint::default();
        paint.set_color(skia_safe::Color::from(self.background.to_u32()));

        let pos = self.pos().unwrap_or(Vec2d { x: 0.0, y: 0.0 });

        let rect = Rect::from_xywh(pos.x, pos.y, self.size.width as f32, self.size.height as f32);
        ctx.canvas.draw_rect(rect, &paint);

        let mut font = Font::default();
        font.set_size(20.0);

        if let Some(blob) = TextBlob::from_str(&self.label, &font) {
            let mut paint = Paint::default();
            paint.set_color(skia_safe::Color::WHITE);
            ctx.canvas.draw_text_blob(
                &blob,
                Point::new(
                    self.pos().unwrap_or(Vec2d { x: 300.0, y: 500.0 }).x * 0.5,
                    self.pos().unwrap_or(Vec2d { x: 300.0, y: 500.0 }).y * 0.5,
                ),
                &paint,
            );
        }
    }

    fn pos(&self) -> Option<Vec2d> {
        Some(Vec2d { x: 300.0, y: 500.0 })
    }

    fn size(&self) -> Option<Size> {
        Some(self.size)
    }

    fn on_click(&self) -> Option<&(dyn Fn() + Send + Sync)> {
        Some(&self.on_click)
    }
}
