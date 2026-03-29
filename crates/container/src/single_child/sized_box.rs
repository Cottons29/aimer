use crate::ZeroSizedBox;
use attribute::dimension::Dimension;
use attribute::position::Vec2d;
use attribute::size::{ResolvedSize, Size};
use constructor::WidgetConstructor;
use widget::base::*;
use widget::{base::Color, Constructor, Drawable, Element, LayoutCache, Widget};
use canvas::CanvasRendering;


#[derive(WidgetConstructor)]
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
            debug_name: "SizedBox",
            bounds: std::cell::Cell::new(None),
        })
    }
}

pub struct RawSizedBox<E: Element> {
    pub(crate) width: Dimension,
    pub(crate) height: Dimension,
    pub(crate) color: Color,
    pub(crate) child: E,
    pub(crate) cache: LayoutCache,
    pub(crate) debug_name: &'static str,
    pub(crate) bounds: std::cell::Cell<Option<(Vec2d, Vec2d)>>,
}

impl<E: Element> Drawable for RawSizedBox<E> {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let width = size.width;
        let height = size.height;

        #[cfg(debug_assertions)]
        {
            if widget::inspector_overlay::is_enabled() {
                // TODO: expose transform position from AimerCanvas for inspector
                let (start_x, start_y): (f32, f32) = (0.0, 0.0);
                let end_x = start_x + width;
                let end_y = start_y + height;

                let scale = ctx.scale;
                let l_start = Vec2d { x: start_x/ scale, y: start_y  / scale };
                let l_end = Vec2d { x: end_x  / scale, y: end_y  / scale };
                self.bounds.set(Some((l_start, l_end)));

                let cp = ctx.cursor_pos;
                if (cp.x as f32) >= start_x && (cp.x as f32) <= end_x && (cp.y as f32) >= start_y && (cp.y as f32) <= end_y {
                    if let Ok(mut hovered) = widget::inspector_overlay::HOVERED_WIDGET.write() {
                        *hovered = Some((self.debug_name, l_start, l_end));
                    }
                }
            }
        }

        ctx.canvas.fill_color_rect(
            Vec2d { x: 0.0, y: 0.0 },
            ResolvedSize { width, height },
            self.color,
            0.0,
        );
    }
}

impl<E: Element> Element for RawSizedBox<E> {
    fn size(&self) -> Option<Size> {
        match (self.width, self.height) {
            (Dimension::Px(w), Dimension::Px(h)) => Some(Size { width: Dimension::Px(w), height: Dimension::Px(h) }),
            _ => None,
        }
    }

    fn pos_start_end(&self) -> Option<(attribute::position::Vec2d, attribute::position::Vec2d)> {
        self.bounds.get()
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
            canvas: ctx.canvas.clone(),
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: ctx.box_constraint,
            visible_rect: ctx.visible_rect,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
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

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}
