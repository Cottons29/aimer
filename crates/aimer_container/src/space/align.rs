use aimer_attribute::BoxConstraint;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_macro::{EventElement, Rebuildable};
use aimer_style::TextAlign;
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, LayoutElement, RequiredChild, VisitorElement, Widget,
};

pub type Alignment = TextAlign;

/// Positions a single child within the space supplied by its parent.
///
/// Attach a child with [`Align::child`] to retain its concrete type, or with
/// [`Align::box_child`] when branches need a shared erased type.
pub struct Align<W = RequiredChild> {
    child: W,
    layer: u32,
    alignment: Alignment,
}

impl Default for Align {
    fn default() -> Self {
        Self::new()
    }
}

impl Align {
    /// Creates a top-centered alignment builder on layer zero.
    ///
    /// Finish the builder with [`Align::child`] or [`Align::box_child`].
    pub fn new() -> Self {
        Self { child: RequiredChild, layer: 0, alignment: Alignment::TopCenter }
    }

    /// Sets where the child is placed within the parent's available size.
    ///
    /// The default is [`Alignment::TopCenter`]. If the child exceeds an axis,
    /// that axis receives no negative offset, so placement starts at zero.
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Sets the child's z-order layer when used in layered layouts.
    ///
    /// The default is `0`. Higher layers are painted later by [`crate::Stack`]
    /// in its normal direction; this value does not affect the child's size.
    pub fn layer(mut self, layer: u32) -> Self {
        self.layer = layer;
        self
    }

    /// Attaches the required child and completes this builder.
    ///
    /// The child receives the parent's constraints and is translated according
    /// to the selected alignment. Its concrete type is preserved; use
    /// [`Align::box_child`] for branch type erasure.
    pub fn child<W: Widget>(self, child: W) -> Align<W> {
        Align { child, layer: self.layer, alignment: self.alignment }
    }

    /// Attaches `child` and erases the resulting widget's concrete type.
    ///
    /// This is equivalent to calling [`Align::child`] followed by
    /// [`Widget::boxed`]. Use it when different branches must return one
    /// [`AnyWidget`] type.
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child)
            .boxed()
    }
}

impl<W: Widget + 'static> Widget for Align<W> {
    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        Box::new(RawAlign {
            child: self
                .child
                .to_element(ctx),
            layer: self.layer,
            alignment: self.alignment,
        })
    }
}

fn alignment_offset(alignment: Alignment, parent: ResolvedSize, child: ResolvedSize) -> (f32, f32) {
    let remaining_width = (parent.width - child.width).max(0.0);
    let remaining_height = (parent.height - child.height).max(0.0);
    let x = match alignment {
        Alignment::TopLeft | Alignment::MidLeft | Alignment::BotLeft => 0.0,
        Alignment::TopCenter | Alignment::MidCenter | Alignment::BotCenter => remaining_width / 2.0,
        Alignment::TopRight | Alignment::MidRight | Alignment::BotRight => remaining_width,
    };
    let y = match alignment {
        Alignment::TopLeft | Alignment::TopCenter | Alignment::TopRight => 0.0,
        Alignment::MidLeft | Alignment::MidCenter | Alignment::MidRight => remaining_height / 2.0,
        Alignment::BotLeft | Alignment::BotCenter | Alignment::BotRight => remaining_height,
    };
    (x, y)
}

#[derive(EventElement, Rebuildable)]
struct RawAlign {
    child: Box<dyn Element>,
    layer: u32,
    alignment: Alignment,
}

impl Drawable for RawAlign {
    fn draw(&self, ctx: &BuildContext) {
        let child_size = self
            .child
            .computed_size(ctx);
        let (offset_x, offset_y) = alignment_offset(self.alignment, ctx.parent_size, child_size);
        let mut child_ctx = ctx.clone();
        child_ctx.parent_size = child_size;
        child_ctx.box_constraint = BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: child_size.width,
            max_height: child_size.height,
        };
        child_ctx.visible_rect = ctx
            .visible_rect
            .map(|(x, y, width, height)| (x - offset_x, y - offset_y, width, height));

        ctx.canvas
            .save();
        ctx.canvas
            .translate(Vec2d { x: offset_x, y: offset_y });
        self.child
            .draw(&child_ctx);
        ctx.canvas
            .restore();
    }
}

impl LayoutElement for RawAlign {
    fn size(&self) -> Option<Size> {
        self.child
            .size()
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child
            .computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child
            .content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.layer
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child
            .get_size_from_child()
    }
}

impl VisitorElement for RawAlign {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(
            self.child
                .as_ref(),
        );
    }

    fn debug_name(&self) -> &'static str {
        "Align"
    }
}

#[cfg(test)]
mod tests {
    use aimer_attribute::size::ResolvedSize;
    use aimer_widget::LayoutElement;

    use crate::ZeroSizedBox;

    use super::{Alignment, RawAlign, alignment_offset};

    #[test]
    fn alignment_offsets_cover_each_axis_position() {
        let parent = ResolvedSize { width: 100.0, height: 80.0 };
        let child = ResolvedSize { width: 20.0, height: 10.0 };

        assert_eq!(alignment_offset(Alignment::TopLeft, parent, child), (0.0, 0.0));
        assert_eq!(alignment_offset(Alignment::TopCenter, parent, child), (40.0, 0.0));
        assert_eq!(alignment_offset(Alignment::MidCenter, parent, child), (40.0, 35.0));
        assert_eq!(alignment_offset(Alignment::BotRight, parent, child), (80.0, 70.0));
    }

    #[test]
    fn alignment_does_not_produce_negative_offsets_for_oversized_child() {
        let parent = ResolvedSize { width: 10.0, height: 10.0 };
        let child = ResolvedSize { width: 20.0, height: 30.0 };

        assert_eq!(alignment_offset(Alignment::MidCenter, parent, child), (0.0, 0.0));
    }

    #[test]
    fn configured_layer_is_exposed_to_stack_ordering() {
        let align =
            RawAlign { child: Box::new(ZeroSizedBox), layer: 10, alignment: Alignment::TopLeft };

        assert_eq!(align.layer(), 10);
    }
}
