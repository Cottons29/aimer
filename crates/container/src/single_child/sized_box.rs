use crate::ZeroSizedBox;
use attribute::dimension::Dimension;
use attribute::size::{ResolvedSize, Size};
use widget::base::*;
use widget::{base::Color, Constructor, Drawable, Element, LayoutCache, Widget};

#[cfg(target_arch = "wasm32")]
type FLOAT = f64;
#[cfg(not(target_arch = "wasm32"))]
type FLOAT = f32;


#[derive(Constructor)]
pub struct SizedBox {
    #[constructor(default, into)]
    width: Dimension,
    #[constructor(default, into)]
    height: Dimension,
    #[constructor(default, into)]
    color: Color,
    child: Option<Box<dyn Widget>>,
}

impl Widget for SizedBox {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = match self.child.as_ref() {
            Some(item) => item.to_element(ctx),
            None => ZeroSizedBox.to_element(ctx),
        };
        Box::new(RawSizedBox {
            width: self.width,
            height: self.height,
            child,
            color: self.color,
            cache: LayoutCache::new(),
        })
    }
}

pub struct RawSizedBox {
    width: Dimension,
    height: Dimension,
    color: Color,
    child: Box<dyn Element>,
    cache: LayoutCache,
}

impl Drawable for RawSizedBox {
    #[cfg(not(target_arch = "wasm32"))]
    fn draw(&self, ctx: &BuildContext) {
        use skia_safe::Paint;
        use skia_safe::{paint::Style, Color as SkColor, Rect};
        let size = self.computed_size(ctx);
        let width = size.width;
        let height = size.height;

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(SkColor::from(self.color));
        paint.set_style(Style::Fill);
        {
            let rect = Rect::from_xywh(0.0, 0.0, width, height);
            ctx.canvas.draw_rect(rect, &paint);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let width = size.width;
        let height = size.height;

        let color_str = self.color.to_css_color();
        ctx.canvas.set_fill_style_str(&color_str);
        ctx.canvas.fill_rect(0.0, 0.0, width, height);
    }
}

impl Element for RawSizedBox {

    fn size(&self) -> Option<Size> {
        match (self.width, self.height) {
            (Dimension::Px(w), Dimension::Px(h)) => Some(Size { width: Dimension::Px(w), height: Dimension::Px(h) }),
            _ => None,
        }
    }

    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
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
            cursor_pos: ctx.cursor_pos,
            box_constraint: ctx.box_constraint,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
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

        let result = ResolvedSize { width, height };
        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);
        result
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

    fn invalidate_layout(&self) {
        self.cache.invalidate();
        self.child.invalidate_layout();
    }
}
