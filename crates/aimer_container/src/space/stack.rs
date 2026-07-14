use aimer_attribute::BoxConstraint;
use aimer_macro::{EventElement, LayoutElement, Rebuildable};
use aimer_widget::base::BuildContext;
use aimer_widget::{Drawable, Element, LayoutElement, VisitorElement, Widget};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum StackDirection {
    #[default]
    Normal,
    Reverse,
    Inherit,
}
pub struct Stack {
    pub children: Vec<Box<dyn Widget>>,
    pub direction: StackDirection,
}

impl Stack {
    pub fn new() -> Self {
        Self { children: Vec::new(), direction: StackDirection::default() }
    }

    pub fn children(mut self, children: Vec<Box<dyn Widget>>) -> Self {
        self.children = children;
        self
    }

    pub fn add_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(Box::new(child));
        self
    }

    pub fn direction(mut self, direction: StackDirection) -> Self {
        self.direction = direction;
        self
    }
}

impl Widget for Stack {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let children = self.children.iter().map(|c| c.to_element(ctx)).collect();
        Box::new(RawStackElement { children, direction: self.direction })
    }
}

#[derive(Rebuildable, LayoutElement, EventElement)]
pub struct RawStackElement {
    pub children: Vec<Box<dyn Element>>,
    pub direction: StackDirection,
}

impl Drawable for RawStackElement {
    fn draw(&self, ctx: &BuildContext) {
        let content_size = self.content_size(ctx);
        let child_ctx = BuildContext {
            parent_size: content_size,
            canvas: ctx.canvas.clone(),
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: content_size.width,
                max_height: content_size.height,
            },
            visible_rect: ctx.visible_rect,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
        };

        let mut sorted_children: Vec<_> = self.children.iter().collect();

        sorted_children.sort_by_key(|child| child.layer());

        if self.direction == StackDirection::Reverse {
            for child in sorted_children.iter().rev() {
                child.draw(&child_ctx);
            }
        } else {
            for child in sorted_children {
                child.draw(&child_ctx);
            }
        }
    }
}

impl VisitorElement for RawStackElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        for child in &self.children {
            visitor(child.as_ref());
        }
    }

    fn debug_name(&self) -> &'static str {
        "RawStackElement"
    }
}
