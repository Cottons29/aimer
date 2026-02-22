use crate::ZeroSizedBox;
use skia_safe::Paint;
use skia_safe::{Color as SkColor, Rect, paint::Style};
use widget::base::*;
use widget::{
    Constructor, Element, LayoutCache, Widget,
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
        Box::new(RawSizedBox { width: self.width, height: self.height, child, color: self.color, cache: LayoutCache::new() })
    }
}

pub struct RawSizedBox {
    width: Dimension,
    height: Dimension,
    color: Color,
    child: Box<dyn Element>,
    cache: LayoutCache,
}

impl Element for RawSizedBox {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let width = size.width;
        let height = size.height;

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(SkColor::from(self.color));
        paint.set_style(Style::Fill);

        let rect = Rect::from_xywh(0.0, 0.0, width, height);
        ctx.canvas.draw_rect(rect, &paint);
    }

    fn computed_size(&self, ctx: &BuildContext) -> widget::base::ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_computed(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let scale = ctx.scale;
        
        let mut child_ctx = BuildContext {
            parent_size: ctx.parent_size,
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            box_constraint: ctx.box_constraint,
        };

        child_ctx.box_constraint.max_width = self.width.resolve(ctx.box_constraint.max_width, scale);
        child_ctx.box_constraint.max_height = self.height.resolve(ctx.box_constraint.max_height, scale);

        let width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => ctx.box_constraint.max_width * (p / 100.0),
            Dimension::Auto => self.child.computed_size(&child_ctx).width,
        };

        let height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => ctx.box_constraint.max_height * (p / 100.0),
            Dimension::Auto => self.child.computed_size(&child_ctx).height,
        };

        let result = widget::base::ResolvedSize { width, height };
        self.cache.set_computed(ctx.box_constraint, scale_bits, result);
        result
    }

    fn invalidate_layout(&self) {
        self.cache.invalidate();
        self.child.invalidate_layout();
    }

    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    fn size(&self) -> Option<Size> {
        match (self.width, self.height) {
            (Dimension::Px(w), Dimension::Px(h)) => Some(Size { width: Dimension::Px(w), height: Dimension::Px(h) }),
            _ => None,
        }
    }

    fn get_size_from_child(&self) -> Option<Size> {
        let mut size = self.child.get_size_from_child().unwrap_or_default();
        if let Dimension::Px(_) = self.width {
            size.width = self.width;
        }
        if let Dimension::Px(_) = self.height {
            size.height = self.height;
        }
        Some(size)
    }
}
