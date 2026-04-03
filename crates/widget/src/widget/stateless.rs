use crate::{base::*, Drawable, Element, Widget};
use attribute::position::Vec2d;
use attribute::size::{ResolvedSize, Size};
// StatelessWidget is effectively just a Widget.
// We rely on direct Widget implementation to avoid blanket implementation conflicts.
// The trait is kept for backward compatibility if needed, but generally users should implement Widget directly.

pub trait StatelessWidget {
    fn build(&self, ctx: &BuildContext) -> impl Widget;
}

/// Wraps any [`Widget`] and attaches a static name used by the inspector overlay.
/// Used by `#[derive(WidgetConstructor)]` to provide inspector support.
pub struct NamedWidget {
    inner: Box<dyn Widget>,
    name: &'static str,
}

impl NamedWidget {
    pub fn new(inner: Box<dyn Widget>, name: &'static str) -> Self {
        Self { inner, name }
    }
}

impl Widget for NamedWidget {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self.inner.to_element(ctx);
        if child.debug_name() == self.name {
            return child;
        }
        Box::new(StatelessElement {
            child,
            debug_name: self.name,
            bounds: std::cell::Cell::new(None),
        })
    }

    fn debug_name(&self) -> &'static str {
        self.name
    }
}

pub struct StatelessElement {
    pub child: Box<dyn Element>,
    pub debug_name: &'static str,
    pub bounds: std::cell::Cell<Option<(Vec2d, Vec2d)>>,
}

impl Drawable for StatelessElement {
    fn draw(&self, ctx: &BuildContext) {
        #[cfg(debug_assertions)]
        {
            if crate::inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx.canvas.get_transform_translation();
                let size = self.content_size(ctx);
                let end_x = start_x + size.width;
                let end_y = start_y + size.height;

                let scale = ctx.scale;
                let l_start = crate::base::Vec2d { x: start_x / scale, y: start_y / scale };
                let l_end = crate::base::Vec2d { x: end_x / scale, y: end_y / scale };
                self.bounds.set(Some((l_start, l_end)));

                let cp = ctx.cursor_pos;
                if cp.x >= l_start.x && cp.x <= l_end.x && cp.y >= l_start.y && cp.y <= l_end.y {
                    if let Ok(mut hovered) = crate::inspector_overlay::HOVERED_WIDGET.write() {
                        *hovered = Some((self.debug_name, l_start, l_end));
                    }
                }
            }
        }
        self.child.draw(ctx);
    }
}

impl Element for StatelessElement {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }

    fn size(&self) -> Option<Size> {
        self.child.size()
    }
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        if self.bounds.get().is_some() {
            return self.bounds.get();
        }
        self.child.pos_start_end()
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }
    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }
    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}





