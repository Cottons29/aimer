use std::ptr::slice_from_raw_parts;

use constructor::Constructor;
use skia_safe::{Color as SkColor, Paint, Rect, paint::Style};
use widget::{Element, Widget, base::*};


#[derive(Constructor)]
pub struct Container<T> {
    size: Option<Size>,
    color: Option<Color>,
    child: T,
}

impl<W: Widget> Widget for Container<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);
        Box::new(Container {
            size: self.size,
            color: self.color,
            child,
        })
    }
}

impl<T: Element> Element for Container<T> {
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
    }

    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }
    
}


impl<T: Element> Container<T> {
    pub fn get_size_from_child(&self) -> Option<Size> {
        if let Some(item) = self.size {
            return Some(item);
        }
        self.child.get_size_from_child()
    }    
}
