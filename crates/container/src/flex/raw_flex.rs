use widget::{Constructor, Element, LayoutSpacing, Widget};

use crate::flex::{BoxAlignment, FlexDirection};

#[derive(Constructor)]
pub struct Flex {
    #[constructor(default)]
    direction: FlexDirection,
    #[constructor(default)]
    vertical_alignment: BoxAlignment,
    #[constructor(default)]
    horizontal_alignment: BoxAlignment,
    #[constructor(default)]
    gaps: LayoutSpacing,
    #[constructor(default)]
    children: Vec<Box<dyn Widget>>,
}

pub struct RawFlex {
    direction: FlexDirection,
    vertical_alignment: BoxAlignment,
    horizontal_alignment: BoxAlignment,
    gaps: LayoutSpacing,
    children: Vec<Box<dyn Element>>,
}

impl Widget for Flex {
    fn to_element(&self, ctx: &widget::base::BuildContext) -> Box<dyn Element> {
        let elements = self.children.iter().map(|c| c.to_element(ctx)).collect();
        Box::new(RawFlex {
            direction: self.direction,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            children: elements,
        })
    }
}

impl RawFlex {
    fn render_child(widget: &dyn Element, ctx: &widget::base::BuildContext) {
        ctx.canvas.save();
        widget.draw(ctx);
        let child_ctx = widget::base::BuildContext {
            parent_size: widget.content_size(ctx),
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: Default::default(),
            box_constraint: Default::default(),
        };
        widget.visit_children(&mut |child| {
            Self::render_child(child, &child_ctx);
        });
        ctx.canvas.restore();
    }
}

impl Element for RawFlex {
    fn draw(&self, ctx: &widget::base::BuildContext) {
        let size = self.computed_size(ctx);
        let gap_x = self
            .gaps
            .left
            .value(ctx.box_constraint.max_width as f32, ctx.scale)
            + self
                .gaps
                .right
                .value(ctx.box_constraint.max_width as f32, ctx.scale);
        let gap_y = self
            .gaps
            .top
            .value(ctx.box_constraint.max_height as f32, ctx.scale)
            + self
                .gaps
                .bottom
                .value(ctx.box_constraint.max_height as f32, ctx.scale);

        let max_w = ctx.box_constraint.max_width as f32;
        let max_h = ctx.box_constraint.max_height as f32;

        let actual_w = size.width as f32;
        let actual_h = size.height as f32;

        let extra_w = (max_w - actual_w).max(0.0);
        let extra_h = (max_h - actual_h).max(0.0);

        let mut current_x = 0.0;
        let mut current_y = 0.0;

        match self.direction {
            FlexDirection::Row | FlexDirection::Inherit => {
                current_x = match self.horizontal_alignment {
                    BoxAlignment::Start => 0.0,
                    BoxAlignment::Center => extra_w / 2.0,
                    BoxAlignment::End => extra_w,
                };
            }
            FlexDirection::Column => {
                current_y = match self.vertical_alignment {
                    BoxAlignment::Start => 0.0,
                    BoxAlignment::Center => extra_h / 2.0,
                    BoxAlignment::End => extra_h,
                };
            }
        }

        ctx.canvas.save();

        let mut child_ctx = widget::base::BuildContext {
            parent_size: ctx.parent_size,
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            box_constraint: ctx.box_constraint,
        };

        println!("Child Count : {}", self.children.len());

        for child in &self.children {
            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    child_ctx.box_constraint.max_width = u32::MAX;
                    child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                }
                FlexDirection::Column => {
                    child_ctx.box_constraint.max_height = u32::MAX;
                    child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                }
            }

            let child_size = child.computed_size(&child_ctx);
            let c_w = child_size.width as f32;
            let c_h = child_size.height as f32;

            let mut offset_x = current_x;
            let mut offset_y = current_y;

            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    offset_y = match self.vertical_alignment {
                        BoxAlignment::Start => 0.0,
                        BoxAlignment::Center => (max_h - c_h).max(0.0) / 2.0,
                        BoxAlignment::End => (max_h - c_h).max(0.0),
                    };
                }
                FlexDirection::Column => {
                    offset_x = match self.horizontal_alignment {
                        BoxAlignment::Start => 0.0,
                        BoxAlignment::Center => (max_w - c_w).max(0.0) / 2.0,
                        BoxAlignment::End => (max_w - c_w).max(0.0),
                    };
                }
            }

            // println!("Drawing child");

            let draw_ctx = widget::base::BuildContext {
                parent_size: child_size,
                canvas: ctx.canvas,
                scale: ctx.scale,
                parent_pos: ctx.parent_pos, // you may want to update parent_pos too
                box_constraint: widget::style::BoxConstraint {
                    min_width: 0,
                    min_height: 0,
                    max_width: c_w as u32,
                    max_height: c_h as u32,
                },
            };

            draw_ctx.canvas.save();
            draw_ctx.canvas.translate((offset_x, offset_y));
            Self::render_child(child.as_ref(), &draw_ctx);
            draw_ctx.canvas.restore();

            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    current_x += c_w + gap_x;
                }
                FlexDirection::Column => {
                    current_y += c_h + gap_y;
                }
            }
        }

        ctx.canvas.restore();
    }

    fn computed_size(&self, ctx: &widget::base::BuildContext) -> widget::base::Size {
        let mut width: f32 = 0.0;
        let mut height: f32 = 0.0;
        let mut child_count = 0;

        let gap_x = self
            .gaps
            .left
            .value(ctx.box_constraint.max_width as f32, ctx.scale)
            + self
                .gaps
                .right
                .value(ctx.box_constraint.max_width as f32, ctx.scale);
        let gap_y = self
            .gaps
            .top
            .value(ctx.box_constraint.max_height as f32, ctx.scale)
            + self
                .gaps
                .bottom
                .value(ctx.box_constraint.max_height as f32, ctx.scale);

        let mut child_ctx = widget::base::BuildContext {
            parent_size: ctx.parent_size,
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            box_constraint: ctx.box_constraint,
        };

        for child in &self.children {
            // In Flutter flex algorithm, inflexible children are laid out with unbounded main axis.
            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    child_ctx.box_constraint.max_width = u32::MAX;
                    child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                }
                FlexDirection::Column => {
                    child_ctx.box_constraint.max_height = u32::MAX;
                    child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                }
            }

            let s = child.computed_size(&child_ctx);
            child_count += 1;
            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    width += s.width as f32;
                    height = height.max(s.height as f32);
                }
                FlexDirection::Column => {
                    height += s.height as f32;
                    width = width.max(s.width as f32);
                }
            }
        }

        if child_count > 1 {
            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    width += gap_x * (child_count - 1) as f32;
                }
                FlexDirection::Column => {
                    height += gap_y * (child_count - 1) as f32;
                }
            }
        }

        widget::base::Size { width: width as u32, height: height as u32 }
    }

    fn content_size(&self, ctx: &widget::base::BuildContext) -> widget::base::Size {
        self.computed_size(ctx)
    }

    // We don't implement visit_children here.
    // If we did, the engine's `render_widget_tree` would visit the children and draw them
    // again at the top-left (0,0) of the Flex container.
    // Instead, we manually traverse and render the children in `draw()` with the correct translations.
}
