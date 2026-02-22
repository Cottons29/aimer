use crate::ZeroSizedBox;
use skia_safe::Paint;
use skia_safe::{Color as SkColor, Rect, paint::Style};
use widget::base::*;
use widget::{
    Constructor, Element, Widget,
    base::{Color, Dimension},
};

#[derive(Constructor)]
pub struct SizedBox {
    #[constructor(default, into)]
    width: Dimension,
    #[constructor(default, into)]
    height: Dimension,
    #[constructor(default,into)]
    color: Color,
    child: Option<Box<dyn Widget>>,
}

impl Widget for SizedBox {
    fn to_element(&self, ctx: &widget::base::BuildContext) -> Box<dyn widget::Element> {
        let child = match self.child.as_ref() {
            Some(item) => item.to_element(ctx),
            None => ZeroSizedBox.to_element(ctx),
        };
        Box::new(RawSizedBox { width: self.width, height: self.height, child, color: self.color })
    }
}

pub struct RawSizedBox {
    width: Dimension,
    height: Dimension,
    color: Color,
    child: Box<dyn Element>,
}

impl Element for RawSizedBox {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let width = size.width as f32;
        let height = size.height as f32;

        println!("SizedBox color: {:?}", self.color);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(SkColor::from(self.color));
        paint.set_style(Style::Fill);

        let rect = Rect::from_xywh(0.0, 0.0, width, height);
        ctx.canvas.draw_rect(rect, &paint);
    }

    fn computed_size(&self, ctx: &BuildContext) -> Size {
        let scale = ctx.scale;
        let width = match self.width {
            Dimension::Px(w) => w * ctx.scale,
            Dimension::Percent(p) => ctx.box_constraint.max_width as f32 * (p / 100.0),
            Dimension::Auto => self.child.computed_size(ctx).width as f32,
        };

        let height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => ctx.box_constraint.max_height as f32 * (p / 100.0),
            Dimension::Auto => self.child.computed_size(ctx).height as f32,
        };

        Size { width: width as u32, height: height as u32 }
    }

    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    fn size(&self) -> Option<Size> {
        match (self.width, self.height) {
            (Dimension::Px(w), Dimension::Px(h)) => Some(Size { width: w as u32, height: h as u32 }),
            _ => None,
        }
    }

    fn get_size_from_child(&self) -> Option<Size> {
        let mut size = self.child.get_size_from_child().unwrap_or_default();
        if let Dimension::Px(w) = self.width {
            size.width = w as u32;
        }
        if let Dimension::Px(h) = self.height {
            size.height = h as u32;
        }
        Some(size)
    }
}
