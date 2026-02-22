use crate::base::*;

#[allow(dead_code)]
pub trait Element: Send + Sync {
    fn draw(&self, ctx: &BuildContext);
    fn pos(&self) -> Option<Vec2d> {
        None
    }
    fn size(&self) -> Option<Size> {
        None
    }
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        if self.size().is_none() || self.pos().is_none() {
            return None;
        }
        let start = self.pos().unwrap();
        let size = self.size().unwrap();
        let resolved = ResolvedSize {
            width: match size.width {
                Dimension::Px(v) => v,
                _ => 0.0,
            },
            height: match size.height {
                Dimension::Px(v) => v,
                _ => 0.0,
            },
        };
        let end = start.get_end(resolved);
        Some((start, end))
    }
    fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {
        // default no children
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.computed_size(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.size()
            .map(|s| s.resolve(&ResolvedSize {
                width: ctx.box_constraint.max_width,
                height: ctx.box_constraint.max_height,
            }, ctx.scale))
            .unwrap_or(ctx.parent_size)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.computed_size(ctx)
    }

    fn get_size_from_child(&self) -> Option<Size> {
        if let Some(s) = self.size() {
            return Some(s);
        }

        let mut result_w = Dimension::Auto;
        let mut result_h = Dimension::Auto;
        let mut found = false;

        self.visit_children(&mut |item| {
            if let Some(child_size) = item.size().or_else(|| item.get_size_from_child()) {
                // For Px values, take the max; otherwise keep what we have
                result_w = match (result_w, child_size.width) {
                    (Dimension::Px(a), Dimension::Px(b)) => Dimension::Px(a.max(b)),
                    (Dimension::Auto, w) => w,
                    (w, _) => w,
                };
                result_h = match (result_h, child_size.height) {
                    (Dimension::Px(a), Dimension::Px(b)) => Dimension::Px(a.max(b)),
                    (Dimension::Auto, h) => h,
                    (h, _) => h,
                };
                found = true;
            }
        });

        if found { Some(Size { width: result_w, height: result_h }) } else { None }
    }

    /// Invalidate cached layout data for this element and all children.
    /// Called at the start of each frame to ensure fresh layout.
    fn invalidate_layout(&self) {
        self.visit_children(&mut |child| {
            child.invalidate_layout();
        });
    }
}

impl Element for Box<dyn Element> {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx);
    }
    fn pos(&self) -> Option<Vec2d> {
        self.as_ref().pos()
    }
    fn size(&self) -> Option<Size> {
        self.as_ref().size()
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.as_ref().visit_children(visitor)
    }
    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().layout(ctx)
    }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.as_ref().content_size(ctx)
    }
    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref().get_size_from_child()
    }
    fn invalidate_layout(&self) {
        self.as_ref().invalidate_layout()
    }
}

