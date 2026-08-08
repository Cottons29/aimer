//! The panel a menu's content sits on.
//!
//! It is deliberately *not* a [`aimer_container::Container`]: a container with
//! automatic dimensions only shrinks to its child when the space around it is
//! unbounded, and a floating menu is handed the whole viewport, so a container
//! would paint the panel over the entire window. The panel therefore measures
//! its child itself and is exactly as large as the child plus the style's
//! padding — which is also what tells [`aimer_modal::Floating`] where to place
//! it.

use aimer_attribute::{ResolvedSize, Vec2d};
use aimer_events::element::ElementEvent;
use aimer_macro::Rebuildable;
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, Drawable, Element, EventElement, EventResult, LayoutElement, VisitorElement,
};

use crate::style::ContextMenuStyle;

/// The styled panel wrapped around a menu's content.
#[derive(Rebuildable)]
pub(crate) struct RawContextMenuPanel {
    child: AnyElement,
    style: ContextMenuStyle,
}

impl RawContextMenuPanel {
    /// Creates a panel drawing `style` around `child`.
    #[inline]
    pub(crate) fn element(child: AnyElement, style: ContextMenuStyle) -> AnyElement {
        Self { child, style }.boxed()
    }

    /// The padding around the content, in physical pixels.
    fn insets(&self, ctx: &BuildContext) -> (f32, f32, f32, f32) {
        let scale = if ctx.scale > 0.0 { ctx.scale } else { 1.0 };
        let width = ctx.box_constraint.max_width;
        let height = ctx.box_constraint.max_height;
        (
            self.style.padding.left.value(width, scale),
            self.style.padding.top.value(height, scale),
            self.style.padding.right.value(width, scale),
            self.style.padding.bottom.value(height, scale),
        )
    }

    /// The panel's size: its content plus its padding.
    ///
    /// An empty content collapses the panel to nothing, so a menu opened with no
    /// verbs paints no background rather than a bare rounded rectangle.
    fn panel_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let (left, top, right, bottom) = self.insets(ctx);
        let content = self.child.content_size(ctx);
        if content.width <= 0.0 || content.height <= 0.0 {
            return ResolvedSize {
                width: 0.0,
                height: 0.0,
            };
        }
        ResolvedSize {
            width: content.width + left + right,
            height: content.height + top + bottom,
        }
    }
}

impl Drawable for RawContextMenuPanel {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.panel_size(ctx);
        if size.width <= 0.0 || size.height <= 0.0 {
            return;
        }

        let mut panel_ctx = ctx.clone();
        panel_ctx.parent_size = size;
        self.style.panel.draw(&panel_ctx);

        let (left, top, right, bottom) = self.insets(ctx);
        ctx.canvas.save();
        ctx.canvas.translate(Vec2d { x: left, y: top });
        let mut child_ctx = ctx.clone();
        child_ctx.parent_size = ResolvedSize {
            width: (size.width - left - right).max(0.0),
            height: (size.height - top - bottom).max(0.0),
        };
        child_ctx.box_constraint.max_width = child_ctx.parent_size.width;
        child_ctx.box_constraint.max_height = child_ctx.parent_size.height;
        self.child.draw(&child_ctx);
        ctx.canvas.restore();
    }
}

impl EventElement for RawContextMenuPanel {
    fn on_event(&self, _event: &ElementEvent) -> EventResult {
        EventResult::ignored()
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl LayoutElement for RawContextMenuPanel {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.panel_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.panel_size(ctx)
    }
}

impl VisitorElement for RawContextMenuPanel {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "ContextMenu"
    }
}
