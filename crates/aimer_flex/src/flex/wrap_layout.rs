//! Wrapping flex layout.
//!
//! [`OverflowBehavior::Wrap`] turns a single flex line into as many lines as the
//! constraints require: children fill the main axis until the next one no longer
//! fits, then continue on a fresh line offset along the cross axis. Positions
//! are resolved for the whole child list, so wrapping is intentionally eager —
//! a line break depends on every preceding child.

use aimer_attribute::BoxConstraint;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_widget::base::BuildContext;

use crate::flex::raw_flex::RawFlex;
use crate::flex::{BoxAlignment, LayoutDirection};

/// Positions and total size of a wrapped flex line set.
pub(crate) struct WrapLayout {
    /// Top-left offset of every child, in child order.
    offsets: Vec<(f32, f32)>,
    /// Size occupied by all lines together, gaps included.
    pub(crate) size: ResolvedSize,
}

/// Lays children out onto as many lines as `max_width` / `max_height` allow.
fn compute_wrap_layout(
    children: &[ResolvedSize],
    direction: LayoutDirection,
    max_width: f32,
    max_height: f32,
    gap_x: f32,
    gap_y: f32,
) -> WrapLayout {
    let mut offsets = Vec::with_capacity(children.len());
    let mut main_offset: f32 = 0.0;
    let mut cross_offset: f32 = 0.0;
    let mut line_cross: f32 = 0.0;
    let mut max_line_main: f32 = 0.0;

    for child in children {
        let (child_main, child_cross, max_main, main_gap, cross_gap) = match direction {
            LayoutDirection::Row | LayoutDirection::Inherit => {
                (child.width, child.height, max_width, gap_x, gap_y)
            }
            LayoutDirection::Column => (child.height, child.width, max_height, gap_y, gap_x),
        };
        let required_main = if main_offset > 0.0 {
            main_offset + main_gap + child_main
        } else {
            child_main
        };
        if main_offset > 0.0 && required_main > max_main {
            max_line_main = max_line_main.max(main_offset);
            cross_offset += line_cross + cross_gap;
            main_offset = 0.0;
            line_cross = 0.0;
        }
        if main_offset > 0.0 {
            main_offset += main_gap;
        }
        let offset = match direction {
            LayoutDirection::Row | LayoutDirection::Inherit => (main_offset, cross_offset),
            LayoutDirection::Column => (cross_offset, main_offset),
        };
        offsets.push(offset);
        main_offset += child_main;
        line_cross = line_cross.max(child_cross);
    }

    max_line_main = max_line_main.max(main_offset);
    let total_cross = if children.is_empty() {
        0.0
    } else {
        cross_offset + line_cross
    };
    let size = match direction {
        LayoutDirection::Row | LayoutDirection::Inherit => ResolvedSize {
            width: max_line_main,
            height: total_cross,
        },
        LayoutDirection::Column => ResolvedSize {
            width: total_cross,
            height: max_line_main,
        },
    };
    WrapLayout { offsets, size }
}

impl RawFlex {
    pub(crate) fn wrapped_layout(
        &self,
        ctx: &BuildContext,
        gap_x: f32,
        gap_y: f32,
    ) -> (Vec<ResolvedSize>, WrapLayout) {
        // A line break depends on every preceding child, so wrapping cannot be
        // windowed: a data-driven source has to hand over its whole range.
        self.materialize_all(ctx);
        let mut child_ctx = ctx.clone();
        match self.direction {
            LayoutDirection::Row | LayoutDirection::Inherit => {
                child_ctx.box_constraint.max_width = f32::MAX;
            }
            LayoutDirection::Column => {
                child_ctx.box_constraint.max_height = f32::MAX;
            }
        }
        let sizes = (0..self.children.len())
            .map(|index| match self.children.get(index) {
                Some(child) => child.computed_size(&child_ctx),
                None => ResolvedSize::default(),
            })
            .collect::<Vec<_>>();
        let layout = compute_wrap_layout(
            &sizes,
            self.direction,
            ctx.box_constraint.max_width,
            ctx.box_constraint.max_height,
            gap_x,
            gap_y,
        );
        (sizes, layout)
    }

    pub(crate) fn draw_wrapped(&self, ctx: &BuildContext, gap_x: f32, gap_y: f32) {
        let (sizes, layout) = self.wrapped_layout(ctx, gap_x, gap_y);
        let extra_width = (ctx.box_constraint.max_width - layout.size.width).max(0.0);
        let extra_height = (ctx.box_constraint.max_height - layout.size.height).max(0.0);
        let base_x = match self.horizontal_alignment {
            BoxAlignment::Start => 0.0,
            BoxAlignment::Center => extra_width / 2.0,
            BoxAlignment::End => extra_width,
        };
        let base_y = match self.vertical_alignment {
            BoxAlignment::Start => 0.0,
            BoxAlignment::Center => extra_height / 2.0,
            BoxAlignment::End => extra_height,
        };
        let mut draw_commands = Vec::with_capacity(self.children.len());

        for (index, child_size) in sizes.iter().copied().enumerate() {
            let Some(child) = self.children.get(index) else {
                continue;
            };
            let offset_x = layout.offsets[index].0 + base_x;
            let offset_y = layout.offsets[index].1 + base_y;
            if let Some((vx, vy, vw, vh)) = ctx.visible_rect
                && (offset_x + child_size.width < vx
                    || offset_x > vx + vw
                    || offset_y + child_size.height < vy
                    || offset_y > vy + vh)
            {
                continue;
            }

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
            draw_commands.push((child.layer(), offset_x, offset_y, child_ctx, child));
        }

        draw_commands.sort_by_key(|command| command.0);
        let scale = ctx.scale.max(1.0);
        for (_, offset_x, offset_y, child_ctx, child) in draw_commands {
            let x = (offset_x * scale).round() / scale;
            let y = (offset_y * scale).round() / scale;
            ctx.canvas.save();
            ctx.canvas.translate(Vec2d { x, y });
            Self::render_child(child, &child_ctx);
            ctx.canvas.restore();
        }
    }
}

#[cfg(test)]
mod wrap_tests {
    use aimer_attribute::size::ResolvedSize;

    use super::{LayoutDirection, compute_wrap_layout};

    #[test]
    fn row_wraps_children_onto_additional_lines() {
        let children = vec![
            ResolvedSize {
                width: 60.0,
                height: 10.0,
            },
            ResolvedSize {
                width: 50.0,
                height: 20.0,
            },
            ResolvedSize {
                width: 40.0,
                height: 15.0,
            },
        ];

        let layout = compute_wrap_layout(&children, LayoutDirection::Row, 100.0, 100.0, 5.0, 3.0);

        assert_eq!(
            layout.size,
            ResolvedSize {
                width: 95.0,
                height: 33.0
            }
        );
        assert_eq!(layout.offsets, vec![(0.0, 0.0), (0.0, 13.0), (55.0, 13.0)]);
    }

    #[test]
    fn column_wraps_children_onto_additional_columns() {
        let children = vec![
            ResolvedSize {
                width: 10.0,
                height: 60.0,
            },
            ResolvedSize {
                width: 20.0,
                height: 50.0,
            },
            ResolvedSize {
                width: 15.0,
                height: 40.0,
            },
        ];

        let layout =
            compute_wrap_layout(&children, LayoutDirection::Column, 100.0, 100.0, 3.0, 5.0);

        assert_eq!(
            layout.size,
            ResolvedSize {
                width: 33.0,
                height: 95.0
            }
        );
        assert_eq!(layout.offsets, vec![(0.0, 0.0), (13.0, 0.0), (13.0, 55.0)]);
    }
}
