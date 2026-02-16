use constructor::Constructor;
use widget::{StatelessWidget, Widget, base::*};
use skia_safe::{Paint, Rect, paint::Style, Color as SkColor};


#[derive(Constructor)]
pub struct Container<T: Widget> {
    size: Option<Size>,
    color: Option<Color>,
    child: T
}

impl<T: Widget> Widget for Container<T> {
    fn draw(&self, ctx: &BuildContext) {
        StatelessWidget::draw(self, ctx);
    }
}

impl<T:Widget> StatelessWidget for Container<T> {
    fn draw(&self, ctx: &BuildContext) {
        let item = if let Some(item) = self.size {
            item
        } else if let Some(item) = self.get_size_from_child() {
            item
        } else {
            return;
        };

        if let Some(color) = self.color {
            let mut paint = Paint::default();
            paint.set_color(SkColor::from(color));
            paint.set_style(Style::Fill);

            let rect = Rect::from_xywh(0.0, 0.0, ctx.size.width as f32, item.height as f32);
            ctx.canvas.draw_rect(rect, &paint);
        }

        self.child.draw(ctx);
    }
}


impl<T: Widget>  Container<T> {
    pub fn get_size_from_child(&self) -> Option<Size> {
        self.child.size().or(self.child.get_size_from_child())
    }
}

