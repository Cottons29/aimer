use attribute::position::Vec2d;
use attribute::size::{ResolvedSize, Size};
use constructor::WidgetConstructor;
use std::cell::Cell;
use utils::debug;
use widget::{Drawable, Element, LayoutCache, LayoutSpacing, Widget, base::BuildContext};
use canvas::CanvasRendering;

#[cfg(target_arch = "wasm32")]
type Float = f64;
#[cfg(not(target_arch = "wasm32"))]
type Float = f32;
use crate::flex::{BoxAlignment, LayoutDirection, OverflowBehavior};

/// a flexible layout container
#[allow(dead_code)]
#[derive(WidgetConstructor)]
pub struct Flex<W: Widget + 'static> {
    #[constructor(default)]
    pub(crate) direction: LayoutDirection,
    #[constructor(default)]
    pub(crate) vertical_alignment: BoxAlignment,
    #[constructor(default)]
    pub(crate) horizontal_alignment: BoxAlignment,
    #[constructor(default)]
    pub(crate) gaps: LayoutSpacing,
    #[constructor(default)]
    pub(crate) overflow: OverflowBehavior,
    #[constructor(default)]
    pub(crate) children: Vec<W>,
}

impl<W: Widget + 'static> Widget for Flex<W> {
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
            debug_name: "Flex",
            bounds: Cell::new(None),
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
    pub(crate) direction: LayoutDirection,
    pub(crate) vertical_alignment: BoxAlignment,
    pub(crate) horizontal_alignment: BoxAlignment,
    pub(crate) gaps: LayoutSpacing,
    pub(crate) children: Vec<Box<dyn Element>>,
    pub(crate) cache: LayoutCache,
    pub(crate) overflow_behavior: OverflowBehavior,
    pub(crate) debug_name: &'static str,
    pub(crate) bounds: Cell<Option<(Vec2d, Vec2d)>>,
}

impl RawFlex {
    fn render_child(widget: &dyn Element, ctx: &BuildContext) {
        ctx.canvas.save();
        widget.draw(ctx);
        let content = widget.content_size(ctx);
        let child_ctx = BuildContext {
            parent_size: content,
            canvas: ctx.canvas.clone(),
            scale: ctx.scale,
            parent_pos: Default::default(),
            cursor_pos: ctx.cursor_pos,
            box_constraint: widget::style::BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: content.width,
                max_height: content.height,
            },
            visible_rect: ctx.visible_rect,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
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
}

impl Drawable for RawFlex {
    fn draw(&self, ctx: &BuildContext) {
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
            LayoutDirection::Row | LayoutDirection::Inherit => {
                current_x = match self.horizontal_alignment {
                    BoxAlignment::Start => 0.0,
                    BoxAlignment::Center => extra_w / 2.0,
                    BoxAlignment::End => extra_w,
                };
            }
            LayoutDirection::Column => {
                current_y = match self.vertical_alignment {
                    BoxAlignment::Start => 0.0,
                    BoxAlignment::Center => extra_h / 2.0,
                    BoxAlignment::End => extra_h,
                };
            }
        }

        ctx.canvas.save();

        #[cfg(debug_assertions)]
        {
            if widget::inspector_overlay::is_enabled() {
                // TODO: expose transform position from AimerCanvas for inspector
                let (start_x, start_y): (Float, Float) = (0.0, 0.0);
                let end_x = start_x + size.width;
                let end_y = start_y + size.height;

                let parent_pos = ctx.parent_pos;
                let max_width = ctx.box_constraint.max_width;
                let max_height = ctx.box_constraint.max_height;

                let scale = ctx.scale;

                let l_start = Vec2d { x: parent_pos.x + (start_x / scale), y: parent_pos.y + (start_y / scale) };
                let l_end = Vec2d {
                    x: parent_pos.x + (max_width / scale),
                    y: parent_pos.y + (max_height / scale) + (start_y / scale),
                };

                self.bounds.set(Some((l_start, l_end)));

                let cp = ctx.cursor_pos;
                if cp.x >= start_x && cp.x <= end_x && cp.y >= start_y && cp.y <= end_y {
                    if let Ok(mut hovered) = widget::inspector_overlay::HOVERED_WIDGET.write() {
                        *hovered = Some((self.debug_name, l_start, l_end));
                    }
                }
            }
        }

        // Apply clipping for overflow hidden
        self.overflow_behavior.apply_overflow_behave(ctx);

        let mut child_ctx = BuildContext {
            parent_size: ctx.parent_size,
            canvas: ctx.canvas.clone(),
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: ctx.box_constraint,
            visible_rect: ctx.visible_rect,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
        };

        // Pass 1: measure sized children to find remaining space for unsized ones
        let child_count = self.children.len();
        let total_gap = if child_count > 1 {
            match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => gap_x * (child_count - 1) as Float,
                LayoutDirection::Column => gap_y * (child_count - 1) as Float,
            }
        } else {
            0.0
        };

        let mut sized_main: Float = 0.0;
        let mut unsized_count: usize = 0;
        let mut child_has_size: Vec<bool> = Vec::with_capacity(child_count);

        for child in &self.children {
            // ptr -> box -> heap
            let has_explicit_main = match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => {
                    let has_width = |s: Size| !s.is_auto_width();

                    child.size().is_some_and(has_width) || child.get_size_from_child().is_some_and(has_width)
                }
                LayoutDirection::Column => {
                    let has_height = |s: Size| !s.is_auto_height();

                    child.size().is_some_and(has_height) || child.get_size_from_child().is_some_and(has_height)
                }
            };
            child_has_size.push(has_explicit_main);
            if has_explicit_main {
                match self.direction {
                    LayoutDirection::Row | LayoutDirection::Inherit => {
                        child_ctx.box_constraint.max_width = Float::MAX;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    LayoutDirection::Column => {
                        child_ctx.box_constraint.max_height = Float::MAX;
                        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                    }
                }
                let s = child.computed_size(&child_ctx);
                match self.direction {
                    LayoutDirection::Row | LayoutDirection::Inherit => sized_main += s.width,
                    LayoutDirection::Column => sized_main += s.height,
                }
            } else {
                unsized_count += 1;
            }
        }

        let remaining_main = match self.direction {
            LayoutDirection::Row | LayoutDirection::Inherit => (max_w - sized_main - total_gap).max(0.0),
            LayoutDirection::Column => (max_h - sized_main - total_gap).max(0.0),
        };
        let per_unsized = if unsized_count > 0 { remaining_main / unsized_count as Float } else { 0.0 };

        let mut draw_commands = Vec::with_capacity(self.children.len());

        for (i, child) in self.children.iter().enumerate() {
            if child_has_size[i] {
                match self.direction {
                    LayoutDirection::Row | LayoutDirection::Inherit => {
                        child_ctx.box_constraint.max_width = Float::MAX;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    LayoutDirection::Column => {
                        child_ctx.box_constraint.max_height = Float::MAX;
                        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                    }
                }
            } else {
                match self.direction {
                    LayoutDirection::Row | LayoutDirection::Inherit => {
                        child_ctx.box_constraint.max_width = per_unsized;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    LayoutDirection::Column => {
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
                LayoutDirection::Row | LayoutDirection::Inherit => {
                    offset_y = match self.vertical_alignment {
                        BoxAlignment::Start => 0.0,
                        BoxAlignment::Center => (max_h - c_h).max(0.0) / 2.0,
                        BoxAlignment::End => (max_h - c_h).max(0.0),
                    };
                }
                LayoutDirection::Column => {
                    offset_x = match self.horizontal_alignment {
                        BoxAlignment::Start => 0.0,
                        BoxAlignment::Center => (max_w - c_w).max(0.0) / 2.0,
                        BoxAlignment::End => (max_w - c_w).max(0.0),
                    };
                }
            }

            let mut is_visible = true;
            if let Some((vx, vy, vw, vh)) = ctx.visible_rect {
                if (offset_x as Float) + (c_w as Float) < vx
                    || (offset_x as Float) > vx + vw
                    || (offset_y as Float) + (c_h as Float) < vy
                    || (offset_y as Float) > vy + vh
                {
                    is_visible = false;
                }
            }

            if is_visible {
                let draw_ctx = BuildContext {
                    parent_size: child_size,
                    canvas: ctx.canvas.clone(),
                    scale: ctx.scale,
                    parent_pos: ctx.parent_pos,
                    cursor_pos: ctx.cursor_pos,
                    box_constraint: widget::style::BoxConstraint {
                        min_width: 0.0,
                        min_height: 0.0,
                        max_width: c_w,
                        max_height: c_h,
                    },
                    visible_rect: ctx
                        .visible_rect
                        .map(|(vx, vy, vw, vh)| (vx - offset_x as Float, vy - offset_y as Float, vw, vh)),
                    window: ctx.window,
                    #[cfg(not(target_arch = "wasm32"))]
                    async_handle: ctx.async_handle.clone(),
                    inherited_states: ctx.inherited_states.clone(),
                };

                draw_commands.push((child.layer(), offset_x, offset_y, draw_ctx, child.as_ref()));
            }

            match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => {
                    current_x += c_w + gap_x;
                }
                LayoutDirection::Column => {
                    current_y += c_h + gap_y;
                }
            }
        }

        draw_commands.sort_by_key(|cmd| cmd.0);

        for cmd in draw_commands {
            let (_, offset_x, offset_y, draw_ctx, child) = cmd;

            draw_ctx.canvas.save();
            draw_ctx.canvas.translate(Vec2d { x: offset_x, y: offset_y });
            Self::render_child(child, &draw_ctx);
            draw_ctx.canvas.restore();
        }

        // Pop the clip pushed by overflow_behavior.apply_overflow_behave()
        if self.overflow_behavior == OverflowBehavior::Hidden {
            ctx.canvas.clear_clip();
        }
        ctx.canvas.restore();
    }
}

impl Element for RawFlex {
    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bounds.get()
    }

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
                LayoutDirection::Row | LayoutDirection::Inherit => gap_x * (child_count - 1) as Float,
                LayoutDirection::Column => gap_y * (child_count - 1) as Float,
            }
        } else {
            0.0
        };

        let max_main = match self.direction {
            LayoutDirection::Row | LayoutDirection::Inherit => ctx.box_constraint.max_width,
            LayoutDirection::Column => ctx.box_constraint.max_height,
        };

        // Pass 1: measure sized children, count unsized
        let mut sized_main: Float = 0.0;
        let mut unsized_count: usize = 0;
        let mut child_sizes: Vec<Option<ResolvedSize>> = Vec::with_capacity(child_count);

        let mut child_ctx = BuildContext {
            parent_size: ctx.parent_size,
            canvas: ctx.canvas.clone(),
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: ctx.box_constraint,
            visible_rect: ctx.visible_rect,
            window: ctx.window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
        };

        for child in &self.children {
            #[allow(clippy::unnecessary_map_or)]
            let has_explicit_main = match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => {
                    child.size().map_or(false, |s| !s.is_auto_width())
                        || child
                            .get_size_from_child()
                            .map_or(false, |s| !s.is_auto_width())
                }
                LayoutDirection::Column => {
                    child.size().map_or(false, |s| !s.is_auto_height())
                        || child
                            .get_size_from_child()
                            .map_or(false, |s| !s.is_auto_height())
                }
            };
            if has_explicit_main {
                match self.direction {
                    LayoutDirection::Row | LayoutDirection::Inherit => {
                        child_ctx.box_constraint.max_width = Float::MAX;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    LayoutDirection::Column => {
                        child_ctx.box_constraint.max_height = Float::MAX;
                        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                    }
                }
                let s = child.computed_size(&child_ctx);
                match self.direction {
                    LayoutDirection::Row | LayoutDirection::Inherit => sized_main += s.width,
                    LayoutDirection::Column => sized_main += s.height,
                }
                child_sizes.push(Some(s));
            } else {
                unsized_count += 1;
                child_sizes.push(None);
            }
        }

        // Pass 2: distribute remaining space to unsized children
        let per_unsized = if unsized_count > 0 {
            if max_main == Float::MAX {
                Float::MAX
            } else {
                let remaining = (max_main - sized_main - total_gap).max(0.0);
                remaining / unsized_count as Float
            }
        } else {
            0.0
        };

        let mut total_width: Float = 0.0;
        let mut total_height: Float = 0.0;

        for (i, child) in self.children.iter().enumerate() {
            let s = if let Some(s) = child_sizes[i] {
                s
            } else {
                match self.direction {
                    LayoutDirection::Row | LayoutDirection::Inherit => {
                        child_ctx.box_constraint.max_width = per_unsized;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    LayoutDirection::Column => {
                        child_ctx.box_constraint.max_height = per_unsized;
                        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                    }
                }
                child.computed_size(&child_ctx)
            };
            match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => {
                    total_width += s.width;
                    total_height = total_height.max(s.height);
                }
                LayoutDirection::Column => {
                    total_height += s.height;
                    total_width = total_width.max(s.width);
                }
            }
        }

        if child_count > 1 {
            match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => {
                    total_width += gap_x * (child_count - 1) as Float;
                }
                LayoutDirection::Column => {
                    total_height += gap_y * (child_count - 1) as Float;
                }
            }
        }

        let result = ResolvedSize { width: total_width, height: total_height };
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

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}
