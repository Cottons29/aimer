use constructor::Constructor;
use skia_safe::{Color, Font, FontMgr, FontStyle, Paint, Rect, TextBlob};
use std::slice;
use std::sync::{Arc, RwLock};
use widget::{StatefulWidget, StatelessWidget, Widget};
#[derive(Constructor)]


pub struct MyStatefulWidget {
    num: Arc<RwLock<u32>>,
    child: Box<dyn Widget>,
}

impl StatelessWidget for MyStatefulWidget {
    fn draw(&self, ctx: &widget::base::BuildContext) {
        let size = &ctx.size;
        let center_x = size.width as f32 / 2.0;
        let center_y = size.height as f32 / 2.0;

        let width = size.width as f32;
        let height = size.width as f32;

        let rect = Rect::from_xywh(center_x - width / 2.0, center_y - height / 2.0, width, height);

        let mut paint = Paint::default();
        paint.set_color(Color::CYAN);
        ctx.canvas.draw_rect(rect, &paint);

        let font_mgr = FontMgr::default();
        let typeface = font_mgr
            .match_family_style("Arial", FontStyle::normal())
            .unwrap();
        let font = Font::new(typeface, 30.0);

        let text = format!("Count: {}", self.num.read().unwrap());
        if let Some(blob) = TextBlob::from_str(&text, &font) {
            let text_width = font.measure_text(&text, Some(&paint)).1.width();
            let text_x = center_x - text_width / 2.0;
            let text_y = center_y + 10.0;

            let mut text_paint = Paint::default();
            text_paint.set_color(Color::BLACK);
            ctx.canvas
                .draw_text_blob(&blob, (text_x, text_y), &text_paint);
        }
    }
}

impl Widget for MyStatefulWidget {
    fn draw(&self, ctx: &widget::base::BuildContext) {
        StatelessWidget::draw(self, ctx);
    }

    fn child(&self) -> &[Box<dyn Widget>] {
        slice::from_ref(&self.child)
    }
}
