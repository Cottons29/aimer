use crate::ZeroSizedBox;
use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_macro::{EventElement, Rebuildable, WidgetConstructor};
use aimer_widget::base::*;
use aimer_widget::{Drawable, Element, LayoutCache, LayoutElement, Reconcilable, VisitorElement, Widget, base::Color};

#[derive(WidgetConstructor)]
pub struct SizedBox<W: Widget + 'static = ZeroSizedBox> {
    #[constructor(default, into)]
    width: Dimension,
    #[constructor(default, into)]
    height: Dimension,
    #[constructor(default, into)]
    color: Color,
    #[constructor(default = SizedBox::PLACE_HOLDER)]
    child: W,
}

impl SizedBox {
    pub const PLACE_HOLDER: ZeroSizedBox = ZeroSizedBox;
}

impl<W: Widget + 'static> Widget for SizedBox<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawSizedBox {
            width: self.width,
            height: self.height,
            child: self.child.to_element(ctx),
            color: self.color,
            cache: LayoutCache::new(),
            debug_name: "SizedBox",
            bounds: std::cell::Cell::new(None),
        })
    }
}
#[derive(Rebuildable, EventElement)]
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
            if aimer_widget::inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx.canvas.get_transform_translation();
                let end_x = start_x + width;
                let end_y = start_y + height;

                let scale = ctx.scale;
                let l_start = Vec2d { x: start_x / scale, y: start_y / scale };
                let l_end = Vec2d { x: end_x / scale, y: end_y / scale };
                self.bounds.set(Some((l_start, l_end)));

                let cp = ctx.cursor_pos;
                if cp.x >= l_start.x && cp.x <= l_end.x && cp.y >= l_start.y && cp.y <= l_end.y &&  let Ok(mut hovered) = aimer_widget::inspector_overlay::HOVERED_WIDGET.write(){
                    *hovered = Some((self.debug_name, l_start, l_end));
                }
            }
        }

        ctx.canvas
            .fill_color_rect(Vec2d { x: 0.0, y: 0.0 }, ResolvedSize { width, height }, self.color, [0.0; 4]);
    }
}

impl<E: Element> VisitorElement for RawSizedBox<E> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}

impl<E: Element> LayoutElement for RawSizedBox<E> {
    fn size(&self) -> Option<Size> {
        match (self.width, self.height) {
            (Dimension::Px(w), Dimension::Px(h)) => Some(Size { width: Dimension::Px(w), height: Dimension::Px(h) }),
            _ => None,
        }
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
        self.cache.set_computed(ctx.box_constraint, scale_bits, result);
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

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bounds.get()
    }
}

impl<E: Element + 'static> Reconcilable for RawSizedBox<E> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_from_widget(&self, _new_element: &dyn Element, _ctx: &BuildContext) -> bool {
        false
    }
}
