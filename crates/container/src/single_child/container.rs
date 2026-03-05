use attribute::dimension::Dimension;
use attribute::size::{ResolvedSize, Size};
use constructor::Constructor;
#[cfg(not(target_arch = "wasm32"))]
use skia_safe::{Color as SkColor, Paint, Rect, paint::Style};
use widget::{Drawable, Element, LayoutCache, LayoutSpacing, Spacing, Widget, base::*, style::border::BoxBorder};

#[cfg(target_arch = "wasm32")]
type FLOAT = f64;
#[cfg(not(target_arch = "wasm32"))]
type FLOAT = f32;
#[derive(Constructor)]
pub struct Container<T: Widget> {
    #[constructor(into, default)]
    width: Dimension,
    #[constructor(into, default)]
    height: Dimension,
    #[constructor(into, default)]
    color: Color,
    #[constructor(default)]
    pub padding: LayoutSpacing,
    #[constructor(default)]
    pub margin: LayoutSpacing,
    #[constructor(default)]
    pub border: BoxBorder,
    child: T,
}

impl<W: Widget> Widget for Container<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let parent_width = ctx.box_constraint.max_width;
        let parent_height = ctx.box_constraint.max_height;
        let scale = ctx.scale;

        let m_left = self.margin.left.value(parent_width, scale);
        let m_right = self.margin.right.value(parent_width, scale);
        let m_top = self.margin.top.value(parent_height, scale);
        let m_bottom = self.margin.bottom.value(parent_height, scale);

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

        let p_left = self.padding.left.value(box_width, scale);
        let p_right = self.padding.right.value(box_width, scale);
        let p_top = self.padding.top.value(box_height, scale);
        let p_bottom = self.padding.bottom.value(box_height, scale);

        let get_stroke = |dim: Dimension, parent_val: FLOAT| -> FLOAT {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };

        let b_left = get_stroke(self.border.left.stroke, box_width).max(0.0);
        let b_right = get_stroke(self.border.right.stroke, box_width).max(0.0);
        let b_top = get_stroke(self.border.top.stroke, box_height).max(0.0);
        let b_bottom = get_stroke(self.border.bottom.stroke, box_height).max(0.0);

        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.max_width = (box_width - p_left - b_left - p_right - b_right).max(0.0);
        child_ctx.box_constraint.max_height = (box_height - p_top - b_top - p_bottom - b_bottom).max(0.0);

        let child = self.child.to_element(&child_ctx);
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
/// #### Low level container element.
///
/// - **Container**: safe wrapper for RawContainer
///
/// - **SizedBox**: fixed size container or place holder
pub struct RawContainer<T: Element> {
    pub padding: LayoutSpacing,
    pub margin: LayoutSpacing,
    pub width: Dimension,
    pub height: Dimension,
    pub color: Color,
    pub border: BoxBorder,
    pub child: T,
    pub cache: LayoutCache,
}

impl<T: Element> RawContainer<T> {
    fn margin(&self, ctx: &BuildContext) -> (FLOAT, FLOAT, FLOAT, FLOAT) {
        let parent_width = ctx.box_constraint.max_width;
        let parent_height = ctx.box_constraint.max_height;
        let scale = ctx.scale;

        let m_left = self.margin.left.value(parent_width, scale);
        let m_top = self.margin.top.value(parent_height, scale);
        let m_right = self.margin.right.value(parent_width, scale);
        let m_bottom = self.margin.bottom.value(parent_height, scale);

        (m_left, m_top, m_right, m_bottom)
    }
}

impl<T: Element> Drawable for RawContainer<T> {
    fn draw(&self, ctx: &BuildContext) {
        // debug!("RawContainer::draw");
        #[cfg(not(target_arch = "wasm32"))]
        ctx.canvas.save();
        #[cfg(target_arch = "wasm32")]
        ctx.canvas.save();

        let constraint = ctx.box_constraint;

        let parent_width = constraint.max_width;
        let parent_height = constraint.max_height;
        let scale = ctx.scale;

        let (m_left, m_top, m_right, m_bottom) = self.margin(ctx);

        #[cfg(not(target_arch = "wasm32"))]
        ctx.canvas.translate((m_left, m_top));
        #[cfg(target_arch = "wasm32")]
        match ctx.canvas.translate(m_left, m_top) {
            Ok(_) => {}
            Err(err) => {
                utils::error!("Failed to translate canvas: {:?}", err);
            }
        }

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
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut paint = Paint::default();
            paint.set_anti_alias(true);
            paint.set_color(SkColor::from(self.color));
            paint.set_style(Style::Fill);

            let rect = Rect::from_xywh(0.0, 0.0, box_width, box_height);

            let has_radius = self.border.get_uniform_radius(box_width, box_height, scale);

            if let Some(radius) = has_radius {
                let rrect = skia_safe::RRect::new_rect_xy(rect, radius, radius);
                ctx.canvas.draw_rrect(rrect, &paint);
            } else {
                ctx.canvas.draw_rect(rect, &paint);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let color_str = self.color.to_css_color();
            ctx.canvas.set_fill_style_str(&color_str);

            let has_radius = self
                .border
                .get_uniform_radius(box_width, box_height, scale);

            if let Some(radius) = has_radius {
                ctx.canvas.begin_path();
                let _ = ctx
                    .canvas
                    .round_rect_with_f64(0.0, 0.0, box_width, box_height, radius);
                ctx.canvas.fill();
            } else {
                ctx.canvas.fill_rect(0.0, 0.0, box_width, box_height);
            }
        }

        self.border.draw(ctx.canvas, box_width, box_height, scale);

        let p_left = self.padding.left.value(box_width, scale);
        let p_top = self.padding.top.value(box_height, scale);
        let _p_right = self.padding.right.value(box_width, scale);
        let _p_bottom = self.padding.bottom.value(box_height, scale);

        let mut b_left = 0.0;
        let mut b_top = 0.0;
        let border = self.border;

        let get_stroke = |dim: Dimension, parent_val: FLOAT| -> FLOAT {
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

        #[cfg(not(target_arch = "wasm32"))]
        let inset_rect = Rect::from_ltrb(
            b_left / 2.0,
            b_top / 2.0,
            (box_width - b_right / 2.0).max(0.0),
            (box_height - b_bottom / 2.0).max(0.0),
        );

        if let Some(radius) = border.get_uniform_radius(box_width, box_height, scale) {
            let inner_radius = (radius - (b_left / 2.0)).max(0.0);
            #[cfg(not(target_arch = "wasm32"))]
            {
                let rrect = skia_safe::RRect::new_rect_xy(inset_rect, inner_radius, inner_radius);
                ctx.canvas
                    .clip_rrect(rrect, skia_safe::ClipOp::Intersect, true);
            }
            #[cfg(target_arch = "wasm32")]
            {
                ctx.canvas.begin_path();
                let _ = ctx.canvas.round_rect_with_f64(
                    b_left / 2.0,
                    b_top / 2.0,
                    (box_width - b_right / 2.0).max(0.0) - (b_left / 2.0),
                    (box_height - b_bottom / 2.0).max(0.0) - (b_top / 2.0),
                    inner_radius,
                );
                ctx.canvas.clip();
            }
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            {
                ctx.canvas
                    .clip_rect(inset_rect, skia_safe::ClipOp::Intersect, true);
            }
            #[cfg(target_arch = "wasm32")]
            {
                ctx.canvas.begin_path();
                ctx.canvas.rect(
                    b_left / 2.0,
                    b_top / 2.0,
                    (box_width - b_right / 2.0).max(0.0) - (b_left / 2.0),
                    (box_height - b_bottom / 2.0).max(0.0) - (b_top / 2.0),
                );
                ctx.canvas.clip();
            }
        }

        // #[cfg(not(target_arch = "wasm32"))]
        // ctx.canvas
        //     .clip_rect(Rect::from_xywh(0.0, 0.0, box_width, box_height), skia_safe::ClipOp::Intersect, true);
        // #[cfg(target_arch = "wasm32")]
        // {
        //     ctx.canvas.begin_path();
        //     ctx.canvas.rect(0.0, 0.0, box_width, box_height);
        //     ctx.canvas.clip();
        // }

        #[cfg(not(target_arch = "wasm32"))]
        ctx.canvas.translate((p_left + b_left, p_top + b_top));

        #[cfg(target_arch = "wasm32")]
        match ctx.canvas.translate(p_left + b_left, p_top + b_top) {
            Ok(_) => {}
            Err(err) => {
                utils::error!("Failed to translate canvas: {:?}", err);
            }
        }

        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.max_width = (box_width - p_left - b_left - _p_right - b_right).max(0.0);
        child_ctx.box_constraint.max_height = (box_height - p_top - b_top - _p_bottom - b_bottom).max(0.0);

        self.child.draw(&child_ctx);
        #[cfg(not(target_arch = "wasm32"))]
        ctx.canvas.restore();
        #[cfg(target_arch = "wasm32")]
        ctx.canvas.restore();
    }
}

impl<T: Element> Element for RawContainer<T> {
    fn size(&self) -> Option<Size> {
        Some(Size { width: self.width, height: self.height })
    }

    fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Container handles its own child rendering in draw() with proper offset,
        // so we don't expose children here to avoid double-rendering.
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
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

        let m_left = self.margin.left.value(p_w, scale);
        let m_right = self.margin.right.value(p_w, scale);
        let m_top = self.margin.top.value(p_h, scale);
        let m_bottom = self.margin.bottom.value(p_h, scale);

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

        let m_left = self.margin.left.value(p_w, scale);
        let m_right = self
            .margin
            .right.value(p_w, scale);
        let m_top = self.margin.top.value(p_h, scale);
        let m_bottom = self
            .margin
            .bottom.value(p_h, scale);

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

        let p_left = self
            .padding
            .left.value(b_w, scale);
        let p_right = self
            .padding
            .right.value(b_w, scale);
        let p_top = self.padding.top.value(b_h, scale);
        let p_bottom = self
            .padding
            .bottom.value(b_h, scale);

        let get_stroke = |dim: Dimension, parent_val: FLOAT| -> FLOAT {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };

        let border = self.border;

        let b_left = get_stroke(border.left.stroke, b_w).max(0.0);
        let b_right = get_stroke(border.right.stroke, b_w).max(0.0);
        let b_top = get_stroke(border.top.stroke, b_h).max(0.0);
        let b_bottom = get_stroke(border.bottom.stroke, b_h).max(0.0);

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

        let m_w: FLOAT = 0.0;
        let m_h: FLOAT = 0.0;
        let mut p_w: FLOAT = 0.0;
        let mut p_h: FLOAT = 0.0;
        let mut b_w: FLOAT = 0.0;
        let mut b_h: FLOAT = 0.0;

        // Note: For get_size_from_child, we don't have a parent size to resolve percentages,
        // so we can only accurately add Px values. Percentages will be ignored or should be
        // handled by the layout system during actual resolution.

        if let Spacing::Px(v) = self.padding.left {
            p_w += v as FLOAT;
        }
        if let Spacing::Px(v) = self.padding.right {
            p_w += v as FLOAT;
        }
        if let Spacing::Px(v) = self.padding.top {
            p_h += v as FLOAT;
        }
        if let Spacing::Px(v) = self.padding.bottom {
            p_h += v as FLOAT;
        }

        if let Dimension::Px(v) = self.border.left.stroke {
            b_w += v;
        }
        if let Dimension::Px(v) = self.border.right.stroke {
            b_w += v;
        }
        if let Dimension::Px(v) = self.border.top.stroke {
            b_h += v;
        }
        if let Dimension::Px(v) = self.border.bottom.stroke {
            b_h += v;
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
