use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_macro::WidgetConstructor;
use aimer_widget::{base::*, Drawable, Element, LayoutCache, Widget};
pub use aimer_style::*;

#[derive(WidgetConstructor)]
pub struct Container<T: Widget + 'static> {
    #[constructor(into, default)]
    pub(crate) width: Dimension,
    #[constructor(into, default)]
    pub(crate) height: Dimension,
    #[constructor(default)]
    pub padding: LayoutSpacing,
    #[constructor(default)]
    pub margin: LayoutSpacing,
    #[constructor(default)]
    pub box_decoration: BoxDecoration,
    #[constructor(default = Option::None, into)]
    pub color: Option<Color>,
    pub(crate) child: T,
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

        let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };

        let b_left = get_stroke(self.box_decoration.border.left.stroke, box_width).max(0.0);
        let b_right = get_stroke(self.box_decoration.border.right.stroke, box_width).max(0.0);
        let b_top = get_stroke(self.box_decoration.border.top.stroke, box_height).max(0.0);
        let b_bottom = get_stroke(self.box_decoration.border.bottom.stroke, box_height).max(0.0);

        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.max_width = (box_width - p_left - b_left - p_right - b_right).max(0.0);
        child_ctx.box_constraint.max_height = (box_height - p_top - b_top - p_bottom - b_bottom).max(0.0);

        let child = self.child.to_element(&child_ctx);
        Box::new(RawContainer {
            width: self.width,
            height: self.height,
            child,
            padding: self.padding,
            margin: self.margin,
            box_decoration: self.box_decoration.clone(),
            cache: LayoutCache::new(),
            debug_name: "Container",
            bounds: std::cell::Cell::new(None),
            color: self.color,
        })
    }
}
/// #### Low level container element.
///
/// - **Container**: safe wrapper for RawContainer
///
/// - **SizedBox**: fixed size container or place holder
#[derive(Default)]
pub struct RawContainer<T: Element> {
    pub padding: LayoutSpacing,
    pub margin: LayoutSpacing,
    pub width: Dimension,
    pub height: Dimension,
    pub box_decoration: BoxDecoration,
    pub child: T,
    pub cache: LayoutCache,
    pub debug_name: &'static str,
    pub color: Option<Color>,
    pub bounds: std::cell::Cell<Option<(Vec2d, Vec2d)>>,
}

impl<E: Element > RawContainer<E> {
    pub fn new(child: E) -> Self {
        Self {
            child,
            padding: Default::default(),
            margin: Default::default(),
            width: Default::default(),
            height: Default::default(),
            box_decoration: Default::default(),
            cache: LayoutCache::new(),
            debug_name: "Container",
            bounds: std::cell::Cell::new(None),
            color: None,
        }
    }
}

impl<T: Element> RawContainer<T> {
    fn margin(&self, ctx: &BuildContext) -> (f32, f32, f32, f32) {
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
        ctx.canvas.save();

        let constraint = ctx.box_constraint;

        let parent_width = constraint.max_width;
        let parent_height = constraint.max_height;
        let scale = ctx.scale;

        let (m_left, m_top, m_right, m_bottom) = self.margin(ctx);

        ctx.canvas.translate(Vec2d { x: m_left, y: m_top });

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

        // Use computed_size to get correct dimensions (handles unbounded/scrollable case)
        let computed = self.computed_size(ctx);
        let (m_left_v, m_top_v, m_right_v, m_bottom_v) = self.margin(ctx);
        let draw_width = (computed.width - m_left_v - m_right_v).max(0.0);
        let draw_height = (computed.height - m_top_v - m_bottom_v).max(0.0);

        #[cfg(debug_assertions)]
        {
            if aimer_widget::inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx.canvas.get_transform_translation();
                let end_x = start_x + box_width;
                let end_y = start_y + box_height;

                let scale = ctx.scale;
                let l_start = Vec2d { x: start_x  / scale, y: start_y  / scale };
                let l_end = Vec2d { x: end_x  / scale, y: end_y  / scale };
                self.bounds.set(Some((l_start, l_end)));

                let cp = ctx.cursor_pos;
                if cp.x >= l_start.x && cp.x <= l_end.x && cp.y >= l_start.y && cp.y <= l_end.y {
                    if let Ok(mut hovered) = aimer_widget::inspector_overlay::HOVERED_WIDGET.write() {
                        *hovered = Some((self.debug_name, l_start, l_end));
                    }
                }
            }
        }

        let decoration_ctx = BuildContext {
            parent_size: ResolvedSize { width: draw_width, height: draw_height },
            ..ctx.clone()
        };

        if self.color.is_some() && self.box_decoration.background_color.is_none() {
            self.box_decoration.update_color(self.color.unwrap())
        }

        self.box_decoration.draw(&decoration_ctx);


        let p_left = self.padding.left.value(box_width, scale);
        let p_top = self.padding.top.value(box_height, scale);
        let _p_right = self.padding.right.value(box_width, scale);
        let _p_bottom = self.padding.bottom.value(box_height, scale);

        let border = self.box_decoration.border;
        let radii = self.box_decoration.border_radius.resolve(box_width, box_height, scale);

        let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };
        let b_left = get_stroke(border.left.stroke, box_width).max(0.0);
        let b_right = get_stroke(border.right.stroke, box_width).max(0.0);
        let b_top = get_stroke(border.top.stroke, box_height).max(0.0);
        let b_bottom = get_stroke(border.bottom.stroke, box_height).max(0.0);

        // Draw decoration (background, border, outline)


        // Clip to inset rect (inside borders)
        let clip_x = b_left;
        let clip_y = b_top;
        let clip_w = (box_width - b_right - clip_x).max(0.0);
        let clip_h = (box_height - b_bottom - clip_y).max(0.0);
        
        let inner_radii = [
            (radii[0] - b_top.max(b_left)).max(0.0),     // top-left
            (radii[1] - b_top.max(b_right)).max(0.0),    // top-right
            (radii[2] - b_bottom.max(b_right)).max(0.0), // bottom-right
            (radii[3] - b_bottom.max(b_left)).max(0.0),  // bottom-left
        ];

        ctx.canvas.set_clip_rounded(
            Vec2d { x: clip_x, y: clip_y },
            ResolvedSize { width: clip_w, height: clip_h },
            inner_radii,
        );

        ctx.canvas.translate(Vec2d { x: p_left + b_left, y: p_top + b_top });

        let mut child_ctx = ctx.clone();
        let content_w = (box_width - p_left - b_left - _p_right - b_right).max(0.0);
        let content_h = (box_height - p_top - b_top - _p_bottom - b_bottom).max(0.0);
        child_ctx.box_constraint.max_width = content_w;
        child_ctx.box_constraint.max_height = content_h;
        child_ctx.parent_size = ResolvedSize { width: content_w, height: content_h };

        self.child.draw(&child_ctx);
        ctx.canvas.clear_clip();
        ctx.canvas.restore();
    }
}

impl<T: Element> Element for RawContainer<T> {
    fn size(&self) -> Option<Size> {
        Some(Size { width: self.width, height: self.height })
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bounds.get()
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
        let threshold = 1_000_000.0f32;

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

        // When Auto dimension is unbounded (e.g. inside scrollable), derive size from child
        let width_unbounded = matches!(self.width, Dimension::Auto) && box_width > threshold;
        let height_unbounded = matches!(self.height, Dimension::Auto) && box_height > threshold;

        let result = if width_unbounded || height_unbounded {
            let capped_w = box_width.min(threshold);
            let capped_h = box_height.min(threshold);

            let p_left = self.padding.left.value(capped_w, scale);
            let p_right = self.padding.right.value(capped_w, scale);
            let p_top = self.padding.top.value(capped_h, scale);
            let p_bottom = self.padding.bottom.value(capped_h, scale);

            let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
                match dim {
                    Dimension::Px(w) => w * scale,
                    Dimension::Percent(p) => parent_val * (p / 100.0),
                    Dimension::Auto => 0.0,
                }
            };
            let bl = get_stroke(self.box_decoration.border.left.stroke, capped_w).max(0.0);
            let br = get_stroke(self.box_decoration.border.right.stroke, capped_w).max(0.0);
            let bt = get_stroke(self.box_decoration.border.top.stroke, capped_h).max(0.0);
            let bb = get_stroke(self.box_decoration.border.bottom.stroke, capped_h).max(0.0);

            let mut child_ctx = ctx.clone();
            child_ctx.box_constraint.max_width = if width_unbounded { f32::MAX } else { (box_width - p_left - bl - p_right - br).max(0.0) };
            child_ctx.box_constraint.max_height = if height_unbounded { f32::MAX } else { (box_height - p_top - bt - p_bottom - bb).max(0.0) };
            let child_size = self.child.computed_size(&child_ctx);

            let final_w = if width_unbounded {
                child_size.width + p_left + p_right + bl + br + m_left + m_right
            } else {
                box_width + m_left + m_right
            };
            let final_h = if height_unbounded {
                child_size.height + p_top + p_bottom + bt + bb + m_top + m_bottom
            } else {
                box_height + m_top + m_bottom
            };

            ResolvedSize {
                width: final_w.max(0.0),
                height: final_h.max(0.0),
            }
        } else {
            ResolvedSize {
                width: (box_width + m_left + m_right).max(0.0),
                height: (box_height + m_top + m_bottom).max(0.0),
            }
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
        let threshold = 1_000_000.0f32;

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

        let width_unbounded = matches!(self.width, Dimension::Auto) && box_width > threshold;
        let height_unbounded = matches!(self.height, Dimension::Auto) && box_height > threshold;

        let b_w = box_width.max(0.0);
        let b_h = box_height.max(0.0);
        let capped_w = b_w.min(threshold);
        let capped_h = b_h.min(threshold);

        let p_left = self.padding.left.value(capped_w, scale);
        let p_right = self.padding.right.value(capped_w, scale);
        let p_top = self.padding.top.value(capped_h, scale);
        let p_bottom = self.padding.bottom.value(capped_h, scale);

        let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };

        let border = self.box_decoration.border;

        let b_left = get_stroke(border.left.stroke, capped_w).max(0.0);
        let b_right = get_stroke(border.right.stroke, capped_w).max(0.0);
        let b_top = get_stroke(border.top.stroke, capped_h).max(0.0);
        let b_bottom = get_stroke(border.bottom.stroke, capped_h).max(0.0);

        let result = if width_unbounded || height_unbounded {
            let mut child_ctx = ctx.clone();
            child_ctx.box_constraint.max_width = if width_unbounded { f32::MAX } else { (b_w - p_left - b_left - p_right - b_right).max(0.0) };
            child_ctx.box_constraint.max_height = if height_unbounded { f32::MAX } else { (b_h - p_top - b_top - p_bottom - b_bottom).max(0.0) };
            let child_size = self.child.computed_size(&child_ctx);

            ResolvedSize {
                width: if width_unbounded { child_size.width } else { (b_w - p_left - p_right - b_left - b_right).max(0.0) },
                height: if height_unbounded { child_size.height } else { (b_h - p_top - p_bottom - b_top - b_bottom).max(0.0) },
            }
        } else {
            ResolvedSize {
                width: (b_w - p_left - p_right - b_left - b_right).max(0.0),
                height: (b_h - p_top - p_bottom - b_top - b_bottom).max(0.0),
            }
        };
        self.cache
            .set_content(ctx.box_constraint, scale_bits, result);
        result
    }

    fn get_size_from_child(&self) -> Option<Size> {
        let mut size = self.child.get_size_from_child().unwrap_or_default();

        let m_w: f32 = 0.0;
        let m_h: f32 = 0.0;
        let mut p_w: f32 = 0.0;
        let mut p_h: f32 = 0.0;
        let mut b_w: f32 = 0.0;
        let mut b_h: f32 = 0.0;

        // Note: For get_size_from_child, we don't have a parent size to resolve percentages,
        // so we can only accurately add Px values. Percentages will be ignored or should be
        // handled by the layout system during actual resolution.

        if let Spacing::Px(v) = self.padding.left {
            p_w += v as f32;
        }
        if let Spacing::Px(v) = self.padding.right {
            p_w += v as f32;
        }
        if let Spacing::Px(v) = self.padding.top {
            p_h += v as f32;
        }
        if let Spacing::Px(v) = self.padding.bottom {
            p_h += v as f32;
        }

        if let Dimension::Px(v) = self.box_decoration.border.left.stroke {
            b_w += v;
        }
        if let Dimension::Px(v) = self.box_decoration.border.right.stroke {
            b_w += v;
        }
        if let Dimension::Px(v) = self.box_decoration.border.top.stroke {
            b_h += v;
        }
        if let Dimension::Px(v) = self.box_decoration.border.bottom.stroke {
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

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}
