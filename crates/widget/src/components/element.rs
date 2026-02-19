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
        let end = start.get_end(self.size().unwrap());
        Some((start, end))
    }
    fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {
        // default no children
    }

    fn computed_size(&self, ctx: &BuildContext) -> Size {
        self.size().unwrap_or(ctx.parent_size)
    }

    fn content_size(&self, ctx: &BuildContext) -> Size {
        self.computed_size(ctx)
    }

    fn get_size_from_child(&self) -> Option<Size> {
        if let Some(s) = self.size() {
            return Some(s);
        }

        let mut max_w = 0;
        let mut max_h = 0;
        let mut found = false;

        self.visit_children(&mut |item| {
            if let Some(child_size) = item.size().or_else(|| item.get_size_from_child()) {
                max_w = max_w.max(child_size.width);
                max_h = max_h.max(child_size.height);
                found = true;
            }
        });

        if found { Some(Size { width: max_w, height: max_h }) } else { None }
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
    fn computed_size(&self, ctx: &BuildContext) -> Size {
        self.as_ref().computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> Size {
        self.as_ref().content_size(ctx)
    }
    fn get_size_from_child(&self) -> Option<Size> {
        self.as_ref().get_size_from_child()
    }
}

