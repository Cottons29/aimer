use utils::debug;
use widget::base::BuildContext;
use widget::{Element, Widget};
use widget::Constructor;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum StackDirection {
    #[default]
    Normal,
    Reverse,
    Inherit,
}
#[derive(Constructor)]
pub struct Stack {
    pub children: Vec<Box<dyn Widget>>,
    #[constructor(default)]
    pub direction: StackDirection,
}

impl Widget for Stack {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let children = self.children.iter().map(|c| c.to_element(ctx)).collect();
        Box::new(RawStackElement {
            children,
            direction: self.direction,
        })
    }
}

pub struct RawStackElement {
    pub children: Vec<Box<dyn Element>>,
    pub direction: StackDirection,
}

impl Element for RawStackElement {
    fn draw(&self, ctx: &BuildContext) {
        // debug!("RawStackElement::draw");
        let content_size = self.content_size(ctx);
        let child_ctx = BuildContext {
            parent_size: content_size,
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: widget::style::BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: content_size.width,
                max_height: content_size.height,
            },
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
        };

        let mut sorted_children: Vec<_> = self.children.iter().collect();
        // if self.direction == StackDirection::Reverse {
        //     sorted_children.reverse();
        // }
        sorted_children.sort_by_key(|child| child.layer());
        
        for child in sorted_children {
            child.draw(&child_ctx);
        }
    }

    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        for child in &self.children {
            visitor(child.as_ref());
        }
    }
}
