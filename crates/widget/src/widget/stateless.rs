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
        Box::new(StatelessElement {
            child,
            debug_name: self.name,
        })
    }
}

pub struct StatelessElement {
    pub child: Box<dyn Element>,
    pub debug_name: &'static str,
}

impl Drawable for StatelessElement {
    fn draw(&self, ctx: &BuildContext) {
        self.child.draw(ctx);
        #[cfg(not(target_arch = "wasm32"))]
        if crate::inspector_overlay::is_enabled() {
            let size = self.child.computed_size(ctx);
            crate::widget::draw_inspector_box(ctx, size, self.debug_name);
        }
    }
}

impl Element for StatelessElement {

    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }
    fn size(&self) -> Option<Size> {
        self.child.size()
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





