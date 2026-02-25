use constructor::Constructor;
use skia_safe::{Color as SkColor, Paint, Rect, paint::Style};
use widget::{Element, LayoutCache, LayoutSpacing, Spacing, Widget, base::*, style::border::BoxBorder};

#[derive(Constructor)]
pub struct Container<T: Widget> {
    #[constructor(into, default)]
    width: Dimension,
    #[constructor(into, default)]
    height: Dimension,
    #[constructor(into, default)]
    color: Color,
    #[constructor(default)]
    pub padding: Option<LayoutSpacing>,
    #[constructor(default)]
    pub margin: Option<LayoutSpacing>,
    #[constructor(default)]
    pub border: Option<BoxBorder>,
    child: T,
}

impl<W: Widget> Widget for Container<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.child.to_element(ctx);
        Box::new(RawContainer {
            width: self.width,
            height: self.height,
            color: self.color,
            child,
            padding: self.padding,
            margin: self.margin,
            border: self.border,
            cache: LayoutCache::new(),
        })
    }
}

pub struct RawContainer<T> {
    pub padding: Option<LayoutSpacing>,
    pub margin: Option<LayoutSpacing>,
    pub width: Dimension,
    pub height: Dimension,
    pub color: Color,
    pub border: Option<BoxBorder>,
    pub child: T,
    pub cache: LayoutCache,
}


impl<T: Element> RawContainer<T> {
    fn margin(&self, ctx: &BuildContext) -> (f32,f32,f32,f32) {
        let parent_width = ctx.box_constraint.max_width;
        let parent_height = ctx.box_constraint.max_height;
        let scale = ctx.scale;

        let m_left = self
            .margin
            .map(|m| m.left.value(parent_width, scale))
            .unwrap_or(0.0);
        let m_top = self
            .margin
            .map(|m| m.top.value(parent_height, scale))
            .unwrap_or(0.0);
        let m_right = self
            .margin
            .map(|m| m.right.value(parent_width, scale))
            .unwrap_or(0.0);
        let m_bottom = self
            .margin
            .map(|m| m.bottom.value(parent_height, scale))
            .unwrap_or(0.0);

        (m_left, m_top, m_right, m_bottom)

    }
}

impl<T: Element> Element for RawContainer<T> {
    fn draw(&self, ctx: &BuildContext) {
        let constraint = ctx.box_constraint;

        let parent_width = constraint.max_width;
        let parent_height = constraint.max_height;
        let scale = ctx.scale;

        let (m_left, m_top, m_right, m_bottom) = self.margin(ctx);

        ctx.canvas.translate((m_left, m_top));

        let box_width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => parent_width * (p / 100.0) - (m_left + m_right),
            Dimension::Auto => parent_width - m_left - m_right,
        };

        let box_height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => parent_height * (p / 100.0) - (m_top + m_bottom),
            Dimension::Auto => parent_height - m_top - m_bottom,
        };

        let box_width = box_width.max(0.0);
        let box_height = box_height.max(0.0);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_color(SkColor::from(self.color));
        paint.set_style(Style::Fill);

        let rect = Rect::from_xywh(0.0, 0.0, box_width, box_height);

        let has_radius = self
            .border
            .and_then(|b| b.get_uniform_radius(box_width, box_height, scale));

        if let Some(radius) = has_radius {
            let rrect = skia_safe::RRect::new_rect_xy(rect, radius, radius);
            ctx.canvas.draw_rrect(rrect, &paint);
        } else {
            ctx.canvas.draw_rect(rect, &paint);
        }

        if let Some(border) = self.border {
            border.draw(ctx.canvas, box_width, box_height, scale);
        }

        let p_left = self.padding.map(|p| p.left.value(box_width, scale)).unwrap_or(0.0);
        let p_top = self.padding.map(|p| p.top.value(box_height, scale)).unwrap_or(0.0);
        let _p_right = self.padding.map(|p| p.right.value(box_width, scale)).unwrap_or(0.0);
        let _p_bottom = self.padding.map(|p| p.bottom.value(box_height, scale)).unwrap_or(0.0);

        let mut b_left = 0.0;
        let mut b_top = 0.0;

        if let Some(border) = self.border {
            let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
                match dim {
                    Dimension::Px(w) => w * scale,
                    Dimension::Percent(p) => parent_val * (p / 100.0),
                    Dimension::Auto => 0.0,
                }
            };
            b_left = get_stroke(border.left.stroke, box_width).max(0.0);
            let b_right = get_stroke(border.right.stroke, box_width).max(0.0);
            b_top = get_stroke(border.top.stroke, box_height).max(0.0);
            let b_bottom = get_stroke(border.bottom.stroke, box_height).max(0.0);

            let inset_rect = Rect::from_ltrb(
                b_left / 2.0,
                b_top / 2.0,
                (box_width - b_right / 2.0).max(0.0),
                (box_height - b_bottom / 2.0).max(0.0),
            );

            if let Some(radius) = border.get_uniform_radius(box_width, box_height, scale) {
                let inner_radius = (radius - (b_left / 2.0)).max(0.0);
                let rrect = skia_safe::RRect::new_rect_xy(inset_rect, inner_radius, inner_radius);
                ctx.canvas.clip_rrect(rrect, skia_safe::ClipOp::Intersect, true);
            } else {
                ctx.canvas.clip_rect(inset_rect, skia_safe::ClipOp::Intersect, true);
            }
        } else {
            ctx.canvas.clip_rect(
                Rect::from_xywh(0.0, 0.0, box_width, box_height),
                skia_safe::ClipOp::Intersect,
                true,
            );
        }

        ctx.canvas.translate((p_left + b_left, p_top + b_top));
    }

    fn size(&self) -> Option<Size> {
        Some(Size { width: self.width, height: self.height })
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
        let p_w = ctx.box_constraint.max_width;
        let p_h = ctx.box_constraint.max_height;

        let m_left = self.margin.map(|m| m.left.value(p_w, scale)).unwrap_or(0.0);
        let m_right = self.margin.map(|m| m.right.value(p_w, scale)).unwrap_or(0.0);
        let m_top = self.margin.map(|m| m.top.value(p_h, scale)).unwrap_or(0.0);
        let m_bottom = self.margin.map(|m| m.bottom.value(p_h, scale)).unwrap_or(0.0);

        let box_width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => p_w * (p / 100.0) - (m_left + m_right),
            Dimension::Auto => p_w - (m_left + m_right),
        };

        let box_height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => p_h * (p / 100.0) - (m_top + m_bottom),
            Dimension::Auto => p_h - (m_top + m_bottom),
        };

        let result = ResolvedSize {
            width: (box_width + m_left + m_right).max(0.0),
            height: (box_height + m_top + m_bottom).max(0.0),
        };
        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);
        result
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_content(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let scale = ctx.scale;
        let p_w = ctx.box_constraint.max_width;
        let p_h = ctx.box_constraint.max_height;

        let m_left = self.margin.map(|m| m.left.value(p_w, scale)).unwrap_or(0.0);
        let m_right = self.margin.map(|m| m.right.value(p_w, scale)).unwrap_or(0.0);
        let m_top = self.margin.map(|m| m.top.value(p_h, scale)).unwrap_or(0.0);
        let m_bottom = self.margin.map(|m| m.bottom.value(p_h, scale)).unwrap_or(0.0);

        let box_width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => p_w * (p / 100.0) - (m_left + m_right),
            Dimension::Auto => p_w - (m_left + m_right),
        };
        let box_height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => p_h * (p / 100.0) - (m_top + m_bottom),
            Dimension::Auto => p_h - (m_top + m_bottom),
        };

        let b_w = box_width.max(0.0);
        let b_h = box_height.max(0.0);

        let p_left = self.padding.map(|p| p.left.value(b_w, scale)).unwrap_or(0.0);
        let p_right = self.padding.map(|p| p.right.value(b_w, scale)).unwrap_or(0.0);
        let p_top = self.padding.map(|p| p.top.value(b_h, scale)).unwrap_or(0.0);
        let p_bottom = self.padding.map(|p| p.bottom.value(b_h, scale)).unwrap_or(0.0);

        let mut b_left = 0.0;
        let mut b_right = 0.0;
        let mut b_top = 0.0;
        let mut b_bottom = 0.0;

        if let Some(border) = self.border {
            let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
                match dim {
                    Dimension::Px(w) => w * scale,
                    Dimension::Percent(p) => parent_val * (p / 100.0),
                    Dimension::Auto => 0.0,
                }
            };
            b_left = get_stroke(border.left.stroke, b_w).max(0.0);
            b_right = get_stroke(border.right.stroke, b_w).max(0.0);
            b_top = get_stroke(border.top.stroke, b_h).max(0.0);
            b_bottom = get_stroke(border.bottom.stroke, b_h).max(0.0);
        }

        let result = ResolvedSize {
            width: (b_w - p_left - p_right - b_left - b_right).max(0.0),
            height: (b_h - p_top - p_bottom - b_top - b_bottom).max(0.0),
        };
        self.cache
            .set_content(ctx.box_constraint, scale_bits, result);
        result
    }

    fn get_size_from_child(&self) -> Option<Size> {
        let mut size = self.child.get_size_from_child().unwrap_or_default();

        let mut m_w: f32 = 0.0;
        let mut m_h: f32 = 0.0;
        let mut p_w: f32 = 0.0;
        let mut p_h: f32 = 0.0;
        let mut b_w: f32 = 0.0;
        let mut b_h: f32 = 0.0;

        // Note: For get_size_from_child, we don't have a parent size to resolve percentages,
        // so we can only accurately add Px values. Percentages will be ignored or should be
        // handled by the layout system during actual resolution.
        if let Some(m) = self.margin {
            if let Spacing::Px(v) = m.left { m_w += v as f32; }
            if let Spacing::Px(v) = m.right { m_w += v as f32; }
            if let Spacing::Px(v) = m.top { m_h += v as f32; }
            if let Spacing::Px(v) = m.bottom { m_h += v as f32; }
        }
        if let Some(p) = self.padding {
            if let Spacing::Px(v) = p.left { p_w += v as f32; }
            if let Spacing::Px(v) = p.right { p_w += v as f32; }
            if let Spacing::Px(v) = p.top { p_h += v as f32; }
            if let Spacing::Px(v) = p.bottom { p_h += v as f32; }
        }
        if let Some(border) = self.border {
            if let Dimension::Px(v) = border.left.stroke { b_w += v; }
            if let Dimension::Px(v) = border.right.stroke { b_w += v; }
            if let Dimension::Px(v) = border.top.stroke { b_h += v; }
            if let Dimension::Px(v) = border.bottom.stroke { b_h += v; }
        }

        if let Dimension::Px(w) = self.width {
            size.width = Dimension::Px(w + m_w);
        } else {
            size.width = match size.width {
                Dimension::Px(v) => Dimension::Px(v + m_w + p_w + b_w),
                other => other,
            };
        }

        if let Dimension::Px(h) = self.height {
            size.height = Dimension::Px(h + m_h);
        } else {
            size.height = match size.height {
                Dimension::Px(v) => Dimension::Px(v + m_h + p_h + b_h),
                other => other,
            };
        }

        Some(size)
    }

    fn invalidate_layout(&self) {
        self.cache.invalidate();
        self.child.invalidate_layout();
    }
}
