use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_attribute::{BoxConstraint, CacheBounds};
use aimer_macro::{Rebuildable, WidgetConstructor};
use aimer_style::LayoutSpacing;
use aimer_widget::{Drawable, Element, EventElement, LayoutCache, LayoutElement, Reconcilable, VisitorElement, Widget, base::BuildContext};

use crate::flex::flex_child::distribute_flex_space;
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
            cache_bound: CacheBounds::new(),
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
#[derive(Rebuildable)]
pub struct RawFlex {
    pub(crate) direction: LayoutDirection,
    pub(crate) vertical_alignment: BoxAlignment,
    pub(crate) horizontal_alignment: BoxAlignment,
    pub(crate) gaps: LayoutSpacing,
    pub(crate) children: Vec<Box<dyn Element>>,
    pub(crate) cache: LayoutCache,
    pub(crate) overflow_behavior: OverflowBehavior,
    pub(crate) debug_name: &'static str,
    pub(crate) cache_bound: CacheBounds,
}

impl RawFlex {
    fn render_child(widget: &dyn Element, ctx: &BuildContext) {
        ctx.canvas.save();
        // `draw` already paints the widget's *entire* subtree: every `Drawable`
        // draws its own children (see `Container`, `Stack`, `MouseRegion`,
        // `StatefulElement`, ...). Re-walking `visit_children` here painted that
        // subtree a *second* time. For plain children (`Container`, `Text`)
        // `visit_children` is empty so it was a no-op, which is why it went
        // unnoticed — but any direct flex child with a non-empty `visit_children`
        // (a `#[widget(Stateful)]`/`Stateless` section, `MouseRegion`, `Stack`)
        // rendered twice, offset by the second pass's different constraints.
        // `visit_children` is for event/hit-test traversal, not painting.
        widget.draw(ctx);
        ctx.canvas.restore();
    }
}

impl RawFlex {
    #[inline]
    fn resole_gaps(&self, ctx: &BuildContext) -> (f32, f32) {
        let gap_x =
            self.gaps.left.value(ctx.box_constraint.max_width, ctx.scale) + self.gaps.right.value(ctx.box_constraint.max_width, ctx.scale);
        let gap_y = self.gaps.top.value(ctx.box_constraint.max_height, ctx.scale)
            + self.gaps.bottom.value(ctx.box_constraint.max_height, ctx.scale);

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
            if aimer_widget::inspector_overlay::is_enabled() {
                let parent_pos: Vec2d = ctx.canvas.get_transform_translation().into();

                self.cache_bound
                    .save(ctx.scale, parent_pos.x, parent_pos.y, ctx.box_constraint.max_width, ctx.box_constraint.max_height);

                let cp = ctx.cursor_pos;
                if self.cache_bound.is_inside(cp.x, cp.y) {
                    let (l_start, l_end) = self.cache_bound.pos_start_end().unwrap();
                    if let Ok(mut hovered) = aimer_widget::inspector_overlay::HOVERED_WIDGET.write() {
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
                LayoutDirection::Row | LayoutDirection::Inherit => gap_x * (child_count - 1) as f32,
                LayoutDirection::Column => gap_y * (child_count - 1) as f32,
            }
        } else {
            0.0
        };

        let mut sized_main: f32 = 0.0;
        // Flex weight per child: the flex factor for flexible (`Expanded`)
        // children, or `0.0` for a regular child that keeps its own size.
        let mut weights: Vec<f32> = Vec::with_capacity(child_count);

        for child in &self.children {
            // An `Expanded`-style child advertises its flex factor directly.
            if let Some(flex) = child.flex() {
                weights.push(flex.max(0.0));
                continue;
            }

            // Any other child is *not* flexible: it keeps its own intrinsic
            // main-axis size (measured with an unbounded main axis) and simply
            // consumes that much space before the flex children get their share.
            // This matches Flutter, where only `Expanded`/`Flexible` children
            // grow — a plain `Text`, `Container`, etc. never stretches on its
            // own.
            match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => {
                    child_ctx.box_constraint.max_width = f32::MAX;
                    child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                }
                LayoutDirection::Column => {
                    child_ctx.box_constraint.max_height = f32::MAX;
                    child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                }
            }
            let s = child.computed_size(&child_ctx);
            match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => sized_main += s.width,
                LayoutDirection::Column => sized_main += s.height,
            }
            weights.push(0.0);
        }

        let remaining_main = match self.direction {
            LayoutDirection::Row | LayoutDirection::Inherit => (max_w - sized_main - total_gap).max(0.0),
            LayoutDirection::Column => (max_h - sized_main - total_gap).max(0.0),
        };
        let flex_shares = distribute_flex_space(remaining_main, &weights);

        let mut draw_commands = Vec::with_capacity(self.children.len());

        for (i, child) in self.children.iter().enumerate() {
            if weights[i] > 0.0 {
                match self.direction {
                    LayoutDirection::Row | LayoutDirection::Inherit => {
                        child_ctx.box_constraint.max_width = flex_shares[i];
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    LayoutDirection::Column => {
                        child_ctx.box_constraint.max_height = flex_shares[i];
                        child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                    }
                }
            } else {
                match self.direction {
                    LayoutDirection::Row | LayoutDirection::Inherit => {
                        child_ctx.box_constraint.max_width = f32::MAX;
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    LayoutDirection::Column => {
                        child_ctx.box_constraint.max_height = f32::MAX;
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
            #[allow(clippy::collapsible_if)]
            if let Some((vx, vy, vw, vh)) = ctx.visible_rect {
                if (offset_x) + (c_w) < vx || (offset_x) > vx + vw || (offset_y) + (c_h) < vy || (offset_y) > vy + vh {
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
                    box_constraint: BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: c_w, max_height: c_h },
                    visible_rect: ctx.visible_rect.map(|(vx, vy, vw, vh)| (vx - offset_x, vy - offset_y, vw, vh)),
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

impl VisitorElement for RawFlex {
    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}

impl EventElement for RawFlex {
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        for child in &self.children {
            visitor(child.as_ref());
        }
    }
}
impl LayoutElement for RawFlex {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_computed(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let child_count = self.children.len();

        let gap_x =
            self.gaps.left.value(ctx.box_constraint.max_width, ctx.scale) + self.gaps.right.value(ctx.box_constraint.max_width, ctx.scale);
        let gap_y = self.gaps.top.value(ctx.box_constraint.max_height, ctx.scale)
            + self.gaps.bottom.value(ctx.box_constraint.max_height, ctx.scale);

        let total_gap = if child_count > 1 {
            match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => gap_x * (child_count - 1) as f32,
                LayoutDirection::Column => gap_y * (child_count - 1) as f32,
            }
        } else {
            0.0
        };

        let max_main = match self.direction {
            LayoutDirection::Row | LayoutDirection::Inherit => ctx.box_constraint.max_width,
            LayoutDirection::Column => ctx.box_constraint.max_height,
        };

        // Pass 1: measure sized children, collect flex weights
        let mut sized_main: f32 = 0.0;
        let mut weights: Vec<f32> = Vec::with_capacity(child_count);
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
            // An `Expanded`-style child advertises its flex factor directly.
            if let Some(flex) = child.flex() {
                weights.push(flex.max(0.0));
                child_sizes.push(None);
                continue;
            }

            // Any other child keeps its own intrinsic main-axis size (measured
            // with an unbounded main axis) and consumes that space before the
            // flex children are laid out — only `Expanded`/`Flexible` grow.
            match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => {
                    child_ctx.box_constraint.max_width = f32::MAX;
                    child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                }
                LayoutDirection::Column => {
                    child_ctx.box_constraint.max_height = f32::MAX;
                    child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
                }
            }
            let s = child.computed_size(&child_ctx);
            match self.direction {
                LayoutDirection::Row | LayoutDirection::Inherit => sized_main += s.width,
                LayoutDirection::Column => sized_main += s.height,
            }
            weights.push(0.0);
            child_sizes.push(Some(s));
        }

        // Pass 2: distribute remaining space to flexible children by weight.
        // Under an unbounded main axis there is no space to share, so flex
        // children fall back to their intrinsic size (measured with `f32::MAX`).
        let flex_shares = if max_main == f32::MAX {
            vec![f32::MAX; child_count]
        } else {
            let remaining = (max_main - sized_main - total_gap).max(0.0);
            distribute_flex_space(remaining, &weights)
        };

        let mut total_width: f32 = 0.0;
        let mut total_height: f32 = 0.0;

        for (i, child) in self.children.iter().enumerate() {
            let s = if let Some(s) = child_sizes[i] {
                s
            } else {
                match self.direction {
                    LayoutDirection::Row | LayoutDirection::Inherit => {
                        child_ctx.box_constraint.max_width = flex_shares[i];
                        child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
                    }
                    LayoutDirection::Column => {
                        child_ctx.box_constraint.max_height = flex_shares[i];
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
                    total_width += gap_x * (child_count - 1) as f32;
                }
                LayoutDirection::Column => {
                    total_height += gap_y * (child_count - 1) as f32;
                }
            }
        }

        let result = ResolvedSize { width: total_width, height: total_height };
        self.cache.set_computed(ctx.box_constraint, scale_bits, result);
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

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.cache_bound.pos_start_end()
    }
}

impl Reconcilable for RawFlex {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn update_from_widget(&self, _new_element: &dyn Element, _ctx: &BuildContext) -> bool {
        // TODO: reconcile children by key matching
        false
    }
}
