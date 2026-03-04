use attribute::size::{ResolvedSize, Size};
use widget::{Constructor, Drawable, Element, LayoutCache, LayoutSpacing, Widget, base::BuildContext};

#[cfg(target_arch = "wasm32")]
type Float = f64;
#[cfg(not(target_arch = "wasm32"))]
type Float = f32;
use crate::flex::{BoxAlignment, FlexDirection, OverflowBehavior};


type DrawCmd<'a> = (u32, Float, Float, BuildContext<'a>, &'a dyn Element);
/// a flexible layout container
#[allow(dead_code)]
#[derive(Constructor)]
pub struct Flex {
    #[constructor(default)]
    pub(crate) direction: FlexDirection,
    #[constructor(default)]
    pub(crate) vertical_alignment: BoxAlignment,
    #[constructor(default)]
    pub(crate) horizontal_alignment: BoxAlignment,
    #[constructor(default)]
    pub(crate) gaps: LayoutSpacing,
    #[constructor(default)]
    pub(crate) overflow: OverflowBehavior,
    #[constructor(default)]
    pub(crate) children: Vec<Box<dyn Widget>>,
}

impl Widget for Flex {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let elements = self.children.iter().map(|c| c.to_element(ctx)).collect();
        Box::new(RawFlex {
            direction: self.direction,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            gaps: self.gaps,
            children: elements,
            cache: LayoutCache::new(),
            overflow_behavior: self.overflow,
        })
    }
}
/// #### lower level flex container also the base of the flex layout such as
///
/// - Flex: layout that aligns children in horizontal and vertical
///
/// - Column: layout that always aligns children in a vertical direction
///
/// - Row: layout that always aligns children in a horizontal direction
#[allow(dead_code)]
pub struct RawFlex {
    pub(crate) direction: FlexDirection,
    pub(crate) vertical_alignment: BoxAlignment,
    pub(crate) horizontal_alignment: BoxAlignment,
    pub(crate) gaps: LayoutSpacing,
    pub(crate) children: Vec<Box<dyn Element>>,
    pub(crate) cache: LayoutCache,
    pub(crate) overflow_behavior: OverflowBehavior,
}

impl RawFlex {
    fn render_child(widget: &dyn Element, ctx: &BuildContext) {
        ctx.canvas.save();
        widget.draw(ctx);
        let content = widget.content_size(ctx);
        let child_ctx = BuildContext {
            parent_size: content,
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: Default::default(),
            cursor_pos: ctx.cursor_pos,
            box_constraint: widget::style::BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: content.width,
                max_height: content.height,
            },
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
        };
        widget.visit_children(&mut |child| {
            Self::render_child(child, &child_ctx);
        });
        ctx.canvas.restore();
    }
}

impl RawFlex {
    #[inline]
    fn resole_gaps(&self, ctx: &BuildContext) -> (Float, Float) {
        let gap_x = self
            .gaps
            .left
            .value(ctx.box_constraint.max_width, ctx.scale)
            + self
                .gaps
                .right
                .value(ctx.box_constraint.max_width, ctx.scale);
        let gap_y = self
            .gaps
            .top
            .value(ctx.box_constraint.max_height, ctx.scale)
            + self
                .gaps
                .bottom
                .value(ctx.box_constraint.max_height, ctx.scale);

        (gap_x, gap_y)
    }

    fn build_draw_cmd<'a>(&self, ctx: &BuildContext<'a>) -> Vec<(u32, Float, Float, BuildContext<'a>, &dyn Element)> {
        let size = self.computed_size(ctx);
        let (gap_x, gap_y) = self.resole_gaps(ctx);
        let max_w = ctx.box_constraint.max_width;
        let max_h = ctx.box_constraint.max_height;

        let actual_w = size.width;
        let actual_h = size.height;

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

        // Apply clipping for overflow hidden
        self.overflow_behavior.apply_overflow_behave(ctx);

        let mut child_ctx = BuildContext {
            parent_size: ctx.parent_size,
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: ctx.box_constraint,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
        };

        // Pass 1: measure sized children to find remaining space for unsized ones
        let child_count = self.children.len();
        let total_gap = if child_count > 1 {
            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => gap_x * (child_count - 1) as Float,
                FlexDirection::Column => gap_y * (child_count - 1) as Float,
            }
        } else {
            0.0
        };

        let mut sized_main: Float = 0.0;
        let mut unsized_count: usize = 0;
        let mut child_has_size: Vec<bool> = Vec::with_capacity(child_count);

        for child in &self.children {
            let has_explicit_main = match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    let has_width = |s: Size| !s.is_auto_width();

                    child.size().is_some_and(has_width) || child.get_size_from_child().is_some_and(has_width)
                }
                FlexDirection::Column => {
                    let has_height = |s: Size| !s.is_auto_height();

                    child.size().is_some_and(has_height) || child.get_size_from_child().is_some_and(has_height)
                }
            };
            child_has_size.push(has_explicit_main);
            if has_explicit_main {
                match self.direction {
                    FlexDirection::Row | FlexDirection::Inherit => {
                        child_ctx.box_constraint.max_width = Float::MAX;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    FlexDirection::Column => {
                        child_ctx.box_constraint.max_height = Float::MAX;
                        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                    }
                }
                let s = child.computed_size(&child_ctx);
                match self.direction {
                    FlexDirection::Row | FlexDirection::Inherit => sized_main += s.width,
                    FlexDirection::Column => sized_main += s.height,
                }
            } else {
                unsized_count += 1;
            }
        }

        let remaining_main = match self.direction {
            FlexDirection::Row | FlexDirection::Inherit => (max_w - sized_main - total_gap).max(0.0),
            FlexDirection::Column => (max_h - sized_main - total_gap).max(0.0),
        };
        let per_unsized = if unsized_count > 0 { remaining_main / unsized_count as Float } else { 0.0 };

        let mut draw_commands = Vec::with_capacity(self.children.len());

        for (i, child) in self.children.iter().enumerate() {
            if child_has_size[i] {
                match self.direction {
                    FlexDirection::Row | FlexDirection::Inherit => {
                        child_ctx.box_constraint.max_width = Float::MAX;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    FlexDirection::Column => {
                        child_ctx.box_constraint.max_height = Float::MAX;
                        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                    }
                }
            } else {
                match self.direction {
                    FlexDirection::Row | FlexDirection::Inherit => {
                        child_ctx.box_constraint.max_width = per_unsized;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    FlexDirection::Column => {
                        child_ctx.box_constraint.max_height = per_unsized;
                        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                    }
                }
            }

            let child_size = child.computed_size(&child_ctx);
            let c_w = child_size.width;
            let c_h = child_size.height;

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

            let draw_ctx = BuildContext {
                parent_size: child_size,
                canvas: ctx.canvas,
                scale: ctx.scale,
                parent_pos: ctx.parent_pos,
                cursor_pos: ctx.cursor_pos,
                box_constraint: widget::style::BoxConstraint {
                    min_width: 0.0,
                    min_height: 0.0,
                    max_width: c_w,
                    max_height: c_h,
                },
                window: ctx.window,
                #[cfg(not(target_arch = "wasm32"))]
                async_handle: ctx.async_handle.clone(),
            };

            draw_commands.push((child.layer(), offset_x, offset_y, draw_ctx, child.as_ref()));

            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    current_x += c_w + gap_x;
                }
                FlexDirection::Column => {
                    current_y += c_h + gap_y;
                }
            }
        }

        draw_commands.sort_by_key(|cmd| cmd.0);
        draw_commands
    }
}

impl Drawable for RawFlex {
    fn draw(&self, ctx: &BuildContext) {
        for cmd in self.build_draw_cmd(ctx) {
            let (_, offset_x, offset_y, draw_ctx, child) = cmd;
            // non wasm
            #[cfg(not(target_arch = "wasm32"))]
            draw_ctx.canvas.save();
            #[cfg(target_arch = "wasm32")]
            draw_ctx.canvas.save();
            #[cfg(not(target_arch = "wasm32"))]
            draw_ctx.canvas.translate((offset_x, offset_y));
            #[cfg(target_arch = "wasm32")]
            let _ =  draw_ctx.canvas.translate(offset_x, offset_y);
            Self::render_child(child, &draw_ctx);
            #[cfg(not(target_arch = "wasm32"))]
            draw_ctx.canvas.restore();
        }
        ctx.canvas.restore();
    }
}

impl Element for RawFlex {
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        for child in &self.children {
            visitor(child.as_ref());
        }
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_computed(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let mut width: Float = 0.0;
        let mut height: Float = 0.0;
        let child_count = self.children.len();

        let gap_x = self
            .gaps
            .left
            .value(ctx.box_constraint.max_width, ctx.scale)
            + self
                .gaps
                .right
                .value(ctx.box_constraint.max_width, ctx.scale);
        let gap_y = self
            .gaps
            .top
            .value(ctx.box_constraint.max_height, ctx.scale)
            + self
                .gaps
                .bottom
                .value(ctx.box_constraint.max_height, ctx.scale);

        let total_gap = if child_count > 1 {
            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => gap_x * (child_count - 1) as Float,
                FlexDirection::Column => gap_y * (child_count - 1) as Float,
            }
        } else {
            0.0
        };

        let max_main = match self.direction {
            FlexDirection::Row | FlexDirection::Inherit => ctx.box_constraint.max_width,
            FlexDirection::Column => ctx.box_constraint.max_height,
        };

        // Pass 1: measure sized children, count unsized
        let mut sized_main: Float = 0.0;
        let mut unsized_count: usize = 0;
        let mut child_sizes: Vec<Option<ResolvedSize>> = Vec::with_capacity(child_count);

        let mut child_ctx = BuildContext {
            parent_size: ctx.parent_size,
            canvas: ctx.canvas,
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: ctx.box_constraint,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
        };

        for child in &self.children {
            #[allow(clippy::unnecessary_map_or)]
            let has_explicit_main = match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    child.size().map_or(false, |s| !s.is_auto_width())
                        || child
                            .get_size_from_child()
                            .map_or(false, |s| !s.is_auto_width())
                }
                FlexDirection::Column => {
                    child.size().map_or(false, |s| !s.is_auto_height())
                        || child
                            .get_size_from_child()
                            .map_or(false, |s| !s.is_auto_height())
                }
            };
            if has_explicit_main {
                match self.direction {
                    FlexDirection::Row | FlexDirection::Inherit => {
                        child_ctx.box_constraint.max_width = Float::MAX;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    FlexDirection::Column => {
                        child_ctx.box_constraint.max_height = Float::MAX;
                        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                    }
                }
                let s = child.computed_size(&child_ctx);
                match self.direction {
                    FlexDirection::Row | FlexDirection::Inherit => sized_main += s.width,
                    FlexDirection::Column => sized_main += s.height,
                }
                child_sizes.push(Some(s));
            } else {
                unsized_count += 1;
                child_sizes.push(None);
            }
        }

        // Pass 2: distribute remaining space to unsized children
        let remaining = (max_main - sized_main - total_gap).max(0.0);
        let per_unsized = if unsized_count > 0 { remaining / unsized_count as Float } else { 0.0 };

        for (i, child) in self.children.iter().enumerate() {
            let s = if let Some(s) = child_sizes[i] {
                s
            } else {
                match self.direction {
                    FlexDirection::Row | FlexDirection::Inherit => {
                        child_ctx.box_constraint.max_width = per_unsized;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    FlexDirection::Column => {
                        child_ctx.box_constraint.max_height = per_unsized;
                        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                    }
                }
                child.computed_size(&child_ctx)
            };
            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    width += s.width;
                    height = height.max(s.height);
                }
                FlexDirection::Column => {
                    height += s.height;
                    width = width.max(s.width);
                }
            }
        }

        if child_count > 1 {
            match self.direction {
                FlexDirection::Row | FlexDirection::Inherit => {
                    width += gap_x * (child_count - 1) as Float;
                }
                FlexDirection::Column => {
                    height += gap_y * (child_count - 1) as Float;
                }
            }
        }

        let result = ResolvedSize { width, height };
        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);
        result
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.computed_size(ctx)
    }

    fn invalidate_layout(&self) {
        self.cache.invalidate();
        for child in &self.children {
            child.invalidate_layout();
        }
    }
}
