use std::fmt::{Display, Formatter};

use aimer_attribute::{BoxConstraint, ResolvedSize, Vec2d};
use aimer_widget::base::BuildContext;
use aimer_widget::{
    Drawable, Element, ErrorElement, EventElement, LayoutElement, Rebuildable, VisitorElement,
    detect_overflow, paint_overflow_indicator,
};

use super::grid::{GridAlignment, GridOverflow};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridTrack {
    Px(f32),
    Fr(f32),
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridPlacement {
    pub row: Option<usize>,
    pub column: Option<usize>,
    pub row_span: usize,
    pub column_span: usize,
}

impl Default for GridPlacement {
    fn default() -> Self {
        Self { row: None, column: None, row_span: 1, column_span: 1 }
    }
}

impl GridPlacement {
    pub fn at(mut self, row: usize, column: usize) -> Self {
        self.row = Some(row);
        self.column = Some(column);
        self
    }

    pub fn row(mut self, row: usize) -> Self {
        self.row = Some(row);
        self
    }

    pub fn column(mut self, column: usize) -> Self {
        self.column = Some(column);
        self
    }

    pub fn row_span(mut self, span: usize) -> Self {
        self.row_span = span;
        self
    }

    pub fn column_span(mut self, span: usize) -> Self {
        self.column_span = span;
        self
    }

    pub(crate) fn resolved(row: usize, column: usize, row_span: usize, column_span: usize) -> Self {
        Self { row: Some(row), column: Some(column), row_span, column_span }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GridError {
    MissingColumns,
    ZeroSpan { item: usize },
    ColumnOutOfRange { item: usize, column: usize, span: usize, columns: usize },
    OverlappingItems { first: usize, second: usize },
    InvalidPixels { axis: &'static str, index: usize, value: f32 },
    InvalidFraction { axis: &'static str, index: usize, value: f32 },
    UnboundedFractionalTrack { axis: &'static str },
}

impl Display for GridError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingColumns => formatter.write_str("Grid requires at least one column"),
            Self::ZeroSpan { item } => {
                write!(formatter, "Grid item {item} has a zero row or column span")
            }
            Self::ColumnOutOfRange { item, column, span, columns } => write!(
                formatter,
                "Grid item {item} starts at column {column} with span {span}, outside {columns} columns"
            ),
            Self::OverlappingItems { first, second } => {
                write!(formatter, "Grid items {first} and {second} overlap")
            }
            Self::InvalidPixels { axis, index, value } => {
                write!(formatter, "Grid {axis} track {index} has invalid pixel size {value}")
            }
            Self::InvalidFraction { axis, index, value } => {
                write!(formatter, "Grid {axis} track {index} has invalid fraction {value}")
            }
            Self::UnboundedFractionalTrack { axis } => {
                write!(
                    formatter,
                    "Grid cannot resolve fractional {axis} tracks on an unbounded axis"
                )
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedPlacements {
    pub items: Vec<GridPlacement>,
    pub row_count: usize,
}

fn ensure_rows(occupied: &mut Vec<Vec<Option<usize>>>, rows: usize, columns: usize) {
    occupied.resize_with(rows, || vec![None; columns]);
}

fn fits(
    occupied: &[Vec<Option<usize>>],
    row: usize,
    column: usize,
    row_span: usize,
    column_span: usize,
    columns: usize,
) -> bool {
    column + column_span <= columns
        && (row..row + row_span).all(|r| {
            occupied
                .get(r)
                .is_none_or(|cells| {
                    cells[column..column + column_span]
                        .iter()
                        .all(Option::is_none)
                })
        })
}

fn occupy(
    occupied: &mut Vec<Vec<Option<usize>>>,
    placement: GridPlacement,
    item: usize,
    columns: usize,
) {
    let row = placement
        .row
        .unwrap();
    let column = placement
        .column
        .unwrap();
    ensure_rows(occupied, row + placement.row_span, columns);
    for cells in occupied
        .iter_mut()
        .skip(row)
        .take(placement.row_span)
    {
        for slot in cells
            .iter_mut()
            .skip(column)
            .take(placement.column_span)
        {
            *slot = Some(item);
        }
    }
}

pub(crate) fn resolve_placements(
    items: &[GridPlacement],
    columns: usize,
    explicit_rows: usize,
) -> Result<ResolvedPlacements, GridError> {
    if columns == 0 {
        return Err(GridError::MissingColumns);
    }

    let items = items.to_vec();
    let mut occupied = Vec::new();
    ensure_rows(&mut occupied, explicit_rows, columns);
    let mut resolved = vec![GridPlacement::default(); items.len()];

    for (index, placement) in items
        .iter()
        .copied()
        .enumerate()
    {
        if placement.row_span == 0 || placement.column_span == 0 {
            return Err(GridError::ZeroSpan { item: index });
        }
        if let Some(column) = placement.column
            && column + placement.column_span > columns
        {
            return Err(GridError::ColumnOutOfRange {
                item: index,
                column,
                span: placement.column_span,
                columns,
            });
        }
        if placement
            .row
            .is_none()
            && placement
                .column
                .is_none()
        {
            continue;
        }

        let (row, column) = match (placement.row, placement.column) {
            (Some(row), Some(column)) => {
                ensure_rows(&mut occupied, row + placement.row_span, columns);
                if !fits(&occupied, row, column, placement.row_span, placement.column_span, columns)
                {
                    let first = occupied
                        .iter()
                        .skip(row)
                        .take(placement.row_span)
                        .flat_map(|cells| {
                            cells
                                .iter()
                                .skip(column)
                                .take(placement.column_span)
                        })
                        .find_map(|slot| *slot)
                        .unwrap();
                    return Err(GridError::OverlappingItems { first, second: index });
                }
                (row, column)
            }
            (Some(row), None) => {
                ensure_rows(&mut occupied, row + placement.row_span, columns);
                let column = (0..columns)
                    .find(|column| {
                        fits(
                            &occupied,
                            row,
                            *column,
                            placement.row_span,
                            placement.column_span,
                            columns,
                        )
                    })
                    .ok_or(GridError::ColumnOutOfRange {
                        item: index,
                        column: 0,
                        span: placement.column_span,
                        columns,
                    })?;
                (row, column)
            }
            (None, Some(column)) => {
                let row = (0..)
                    .find(|row| {
                        fits(
                            &occupied,
                            *row,
                            column,
                            placement.row_span,
                            placement.column_span,
                            columns,
                        )
                    })
                    .unwrap();
                (row, column)
            }
            (None, None) => unreachable!(),
        };
        let placement =
            GridPlacement::resolved(row, column, placement.row_span, placement.column_span);
        occupy(&mut occupied, placement, index, columns);
        resolved[index] = placement;
    }

    let mut cursor = 0;
    for (index, placement) in items
        .iter()
        .copied()
        .enumerate()
    {
        if placement
            .row
            .is_some()
            || placement
                .column
                .is_some()
        {
            continue;
        }
        let (row, column) = (cursor..)
            .map(|cell| (cell / columns, cell % columns))
            .find(|(row, column)| {
                fits(&occupied, *row, *column, placement.row_span, placement.column_span, columns)
            })
            .unwrap();
        let placement =
            GridPlacement::resolved(row, column, placement.row_span, placement.column_span);
        occupy(&mut occupied, placement, index, columns);
        resolved[index] = placement;
        cursor = row * columns + column + 1;
    }

    Ok(ResolvedPlacements {
        items: resolved,
        row_count: occupied
            .len()
            .max(explicit_rows),
    })
}

pub(crate) fn resolve_tracks(
    tracks: &[GridTrack],
    available: f32,
    gap: f32,
    auto_minima: &[f32],
    axis: &'static str,
) -> Result<Vec<f32>, GridError> {
    let mut resolved = vec![0.0; tracks.len()];
    let mut consumed = gap.max(0.0)
        * tracks
            .len()
            .saturating_sub(1) as f32;
    let mut fraction_sum = 0.0;

    for (index, track) in tracks
        .iter()
        .copied()
        .enumerate()
    {
        match track {
            GridTrack::Px(value) if !value.is_finite() || value < 0.0 => {
                return Err(GridError::InvalidPixels { axis, index, value });
            }
            GridTrack::Px(value) => {
                resolved[index] = value;
                consumed += value;
            }
            GridTrack::Fr(value) if !value.is_finite() || value <= 0.0 => {
                return Err(GridError::InvalidFraction { axis, index, value });
            }
            GridTrack::Fr(value) => fraction_sum += value,
            GridTrack::Auto => {
                let value = auto_minima
                    .get(index)
                    .copied()
                    .unwrap_or(0.0)
                    .max(0.0);
                resolved[index] = value;
                consumed += value;
            }
        }
    }

    if fraction_sum > 0.0 {
        if available == f32::MAX || !available.is_finite() {
            return Err(GridError::UnboundedFractionalTrack { axis });
        }
        let unit = (available - consumed).max(0.0) / fraction_sum;
        for (index, track) in tracks
            .iter()
            .enumerate()
        {
            if let GridTrack::Fr(value) = track {
                resolved[index] = unit * value;
            }
        }
    }

    Ok(resolved)
}

fn apply_auto_minimum(
    tracks: &[GridTrack],
    minima: &mut [f32],
    start: usize,
    span: usize,
    desired: f32,
    gap: f32,
) {
    let covered = &tracks[start..start + span];
    if covered
        .iter()
        .any(|track| matches!(track, GridTrack::Fr(_)))
    {
        return;
    }
    let fixed = covered
        .iter()
        .filter_map(|track| match track {
            GridTrack::Px(value) => Some(*value),
            GridTrack::Fr(_) | GridTrack::Auto => None,
        })
        .sum::<f32>();
    let auto_count = covered
        .iter()
        .filter(|track| **track == GridTrack::Auto)
        .count();
    if auto_count == 0 {
        return;
    }
    let contribution =
        (desired - fixed - gap * span.saturating_sub(1) as f32).max(0.0) / auto_count as f32;
    for index in start..start + span {
        if tracks[index] == GridTrack::Auto {
            minima[index] = minima[index].max(contribution);
        }
    }
}

pub(crate) struct RawGridItem {
    pub child: Box<dyn Element>,
    pub placement: GridPlacement,
    pub horizontal_alignment: Option<GridAlignment>,
    pub vertical_alignment: Option<GridAlignment>,
}

pub(crate) struct RawGrid {
    pub columns: Vec<GridTrack>,
    pub rows: Vec<GridTrack>,
    pub column_gap: f32,
    pub row_gap: f32,
    pub horizontal_alignment: GridAlignment,
    pub vertical_alignment: GridAlignment,
    pub overflow: GridOverflow,
    pub children: Vec<RawGridItem>,
}

struct GridLayout {
    placements: Vec<GridPlacement>,
    columns: Vec<f32>,
    rows: Vec<f32>,
    size: ResolvedSize,
}

impl RawGrid {
    fn layout_grid(&self, ctx: &BuildContext) -> Result<GridLayout, GridError> {
        let placements = self
            .children
            .iter()
            .map(|item| item.placement)
            .collect::<Vec<_>>();
        let resolved_placements = resolve_placements(
            &placements,
            self.columns
                .len(),
            self.rows
                .len(),
        )?;
        let mut rows = self
            .rows
            .clone();
        rows.resize(resolved_placements.row_count, GridTrack::Auto);

        let intrinsic = self
            .children
            .iter()
            .map(|item| {
                let mut child_ctx = ctx.clone();
                child_ctx.parent_size = ResolvedSize::default();
                child_ctx.box_constraint = BoxConstraint {
                    min_width: 0.0,
                    min_height: 0.0,
                    max_width: f32::MAX,
                    max_height: f32::MAX,
                };
                item.child
                    .computed_size(&child_ctx)
            })
            .collect::<Vec<_>>();

        let mut column_minima = vec![
            0.0_f32;
            self.columns
                .len()
        ];
        for (size, placement) in intrinsic
            .iter()
            .zip(&resolved_placements.items)
        {
            let start = placement
                .column
                .unwrap();
            apply_auto_minimum(
                &self.columns,
                &mut column_minima,
                start,
                placement.column_span,
                size.width,
                self.column_gap,
            );
        }
        let columns = resolve_tracks(
            &self.columns,
            ctx.box_constraint
                .max_width,
            self.column_gap,
            &column_minima,
            "columns",
        )?;

        let mut row_minima = vec![0.0_f32; rows.len()];
        for (item, placement) in self
            .children
            .iter()
            .zip(&resolved_placements.items)
        {
            let cell_width = span_size(
                &columns,
                placement
                    .column
                    .unwrap(),
                placement.column_span,
                self.column_gap,
            );
            let mut child_ctx = ctx.clone();
            child_ctx.parent_size = ResolvedSize { width: cell_width, height: 0.0 };
            child_ctx.box_constraint = BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: cell_width,
                max_height: f32::MAX,
            };
            let size = item
                .child
                .computed_size(&child_ctx);
            let start = placement
                .row
                .unwrap();
            apply_auto_minimum(
                &rows,
                &mut row_minima,
                start,
                placement.row_span,
                size.height,
                self.row_gap,
            );
        }
        let rows = resolve_tracks(
            &rows,
            ctx.box_constraint
                .max_height,
            self.row_gap,
            &row_minima,
            "rows",
        )?;
        let size = ResolvedSize {
            width: tracks_size(&columns, self.column_gap),
            height: tracks_size(&rows, self.row_gap),
        };

        Ok(GridLayout { placements: resolved_placements.items, columns, rows, size })
    }

    fn draw_item(
        &self,
        ctx: &BuildContext,
        item: &RawGridItem,
        placement: GridPlacement,
        layout: &GridLayout,
    ) {
        let cell_pos = Vec2d {
            x: track_offset(
                &layout.columns,
                placement
                    .column
                    .unwrap(),
                self.column_gap,
            ),
            y: track_offset(
                &layout.rows,
                placement
                    .row
                    .unwrap(),
                self.row_gap,
            ),
        };
        let cell_size = ResolvedSize {
            width: span_size(
                &layout.columns,
                placement
                    .column
                    .unwrap(),
                placement.column_span,
                self.column_gap,
            ),
            height: span_size(
                &layout.rows,
                placement
                    .row
                    .unwrap(),
                placement.row_span,
                self.row_gap,
            ),
        };
        let horizontal = item
            .horizontal_alignment
            .unwrap_or(self.horizontal_alignment);
        let vertical = item
            .vertical_alignment
            .unwrap_or(self.vertical_alignment);
        let mut child_ctx = ctx.clone();
        child_ctx.parent_size = cell_size;
        child_ctx.box_constraint = BoxConstraint {
            min_width: if horizontal == GridAlignment::Stretch { cell_size.width } else { 0.0 },
            min_height: if vertical == GridAlignment::Stretch { cell_size.height } else { 0.0 },
            max_width: cell_size.width,
            max_height: cell_size.height,
        };
        let child_size = item
            .child
            .computed_size(&child_ctx);
        let offset = Vec2d {
            x: alignment_offset(horizontal, cell_size.width, child_size.width),
            y: alignment_offset(vertical, cell_size.height, child_size.height),
        };
        child_ctx.visible_rect = ctx
            .visible_rect
            .map(|(x, y, width, height)| {
                (x - cell_pos.x - offset.x, y - cell_pos.y - offset.y, width, height)
            });
        let overflow = detect_overflow(child_size, cell_size, offset);

        ctx.canvas
            .save();
        ctx.canvas
            .translate(cell_pos);
        if self.overflow == GridOverflow::Clip {
            ctx.canvas
                .set_clip(Vec2d::default(), cell_size);
        }
        ctx.canvas
            .save();
        ctx.canvas
            .translate(offset);
        item.child
            .draw(&child_ctx);
        ctx.canvas
            .restore();
        paint_overflow_indicator(
            ctx,
            cell_size,
            overflow,
            item.child
                .debug_name(),
        );
        if self.overflow == GridOverflow::Clip {
            ctx.canvas
                .clear_clip();
        }
        ctx.canvas
            .restore();
    }
}

impl Drawable for RawGrid {
    fn draw(&self, ctx: &BuildContext) {
        match self.layout_grid(ctx) {
            Ok(layout) => {
                for (item, placement) in self
                    .children
                    .iter()
                    .zip(&layout.placements)
                {
                    self.draw_item(ctx, item, *placement, &layout);
                }
            }
            Err(error) => ErrorElement::draw_message(ctx, &format!("Grid layout error: {error}")),
        }
    }
}

impl EventElement for RawGrid {
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        for item in &self.children {
            visitor(
                item.child
                    .as_ref(),
            );
        }
    }
}

impl Rebuildable for RawGrid {}

impl VisitorElement for RawGrid {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        for item in &self.children {
            visitor(
                item.child
                    .as_ref(),
            );
        }
    }

    fn debug_name(&self) -> &'static str {
        "Grid"
    }
}

impl LayoutElement for RawGrid {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.layout_grid(ctx)
            .map_or_else(|_| fallback_size(ctx), |layout| layout.size)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.computed_size(ctx)
    }

    fn invalidate_layout(&self) {
        for item in &self.children {
            item.child
                .invalidate_layout();
        }
    }
}

fn tracks_size(tracks: &[f32], gap: f32) -> f32 {
    tracks
        .iter()
        .sum::<f32>()
        + gap
            * tracks
                .len()
                .saturating_sub(1) as f32
}

fn span_size(tracks: &[f32], start: usize, span: usize, gap: f32) -> f32 {
    tracks[start..start + span]
        .iter()
        .sum::<f32>()
        + gap * span.saturating_sub(1) as f32
}

fn track_offset(tracks: &[f32], index: usize, gap: f32) -> f32 {
    tracks[..index]
        .iter()
        .sum::<f32>()
        + gap * index as f32
}

fn alignment_offset(alignment: GridAlignment, available: f32, child: f32) -> f32 {
    match alignment {
        GridAlignment::Start | GridAlignment::Stretch => 0.0,
        GridAlignment::Center => (available - child) / 2.0,
        GridAlignment::End => available - child,
    }
}

fn fallback_size(ctx: &BuildContext) -> ResolvedSize {
    ResolvedSize {
        width: if ctx
            .box_constraint
            .max_width
            == f32::MAX
        {
            ctx.parent_size
                .width
        } else {
            ctx.box_constraint
                .max_width
        },
        height: if ctx
            .box_constraint
            .max_height
            == f32::MAX
        {
            ctx.parent_size
                .height
        } else {
            ctx.box_constraint
                .max_height
        },
    }
}

#[cfg(test)]
mod tests {
    use std::any::{Any, TypeId};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::{OnceLock, RwLock};

    use aimer_attribute::{BoxConstraint, ResolvedSize};
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_cupid::draw_cmd::DrawCommand;
    use aimer_widget::base::{BuildContext, WindowHandle};
    use aimer_widget::{Drawable, EventElement, LayoutElement, Rebuildable, VisitorElement};

    use crate::ZeroSizedBox;

    use super::{
        GridAlignment, GridError, GridOverflow, GridPlacement, GridTrack, RawGrid, RawGridItem,
        apply_auto_minimum, resolve_placements, resolve_tracks,
    };

    struct VisibleRectRecorder {
        visible_rect: Rc<RefCell<Option<(f32, f32, f32, f32)>>>,
    }

    impl Drawable for VisibleRectRecorder {
        fn draw(&self, ctx: &BuildContext) {
            *self
                .visible_rect
                .borrow_mut() = ctx.visible_rect;
        }
    }

    impl EventElement for VisibleRectRecorder {}
    impl LayoutElement for VisibleRectRecorder {
        fn computed_size(&self, _ctx: &BuildContext) -> ResolvedSize {
            ResolvedSize { width: 20.0, height: 20.0 }
        }
    }
    impl Rebuildable for VisibleRectRecorder {}
    impl VisitorElement for VisibleRectRecorder {
        fn debug_name(&self) -> &'static str {
            "VisibleRectRecorder"
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn dummy_async_handle() -> tokio::runtime::Handle {
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        RUNTIME
            .get_or_init(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
            })
            .handle()
            .clone()
    }

    fn build_context(canvas: &'static InnerCanvas) -> BuildContext<'static> {
        BuildContext {
            parent_size: ResolvedSize { width: 200.0, height: 100.0 },
            canvas: Canvas::new(canvas),
            scale: 1.0,
            parent_pos: Default::default(),
            cursor_pos: Default::default(),
            box_constraint: BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: 200.0,
                max_height: 100.0,
            },
            visible_rect: None,
            window: WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 100), 1.0),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: dummy_async_handle(),
            inherited_states: Rc::new(RwLock::new(HashMap::<TypeId, Rc<dyn Any>>::new())),
        }
    }

    #[test]
    fn clipped_grid_balances_each_child_clip() {
        let canvas = Box::leak(Box::new(InnerCanvas::new()));
        let ctx = build_context(canvas);
        let grid = RawGrid {
            columns: vec![GridTrack::Px(100.0), GridTrack::Px(100.0)],
            rows: vec![GridTrack::Px(100.0)],
            column_gap: 0.0,
            row_gap: 0.0,
            horizontal_alignment: GridAlignment::Stretch,
            vertical_alignment: GridAlignment::Stretch,
            overflow: GridOverflow::Clip,
            children: vec![
                RawGridItem {
                    child: Box::new(ZeroSizedBox),
                    placement: GridPlacement::default(),
                    horizontal_alignment: None,
                    vertical_alignment: None,
                },
                RawGridItem {
                    child: Box::new(ZeroSizedBox),
                    placement: GridPlacement::default(),
                    horizontal_alignment: None,
                    vertical_alignment: None,
                },
            ],
        };

        grid.draw(&ctx);

        let commands = canvas.draw_list();
        let pushes = commands
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::PushClip { .. }))
            .count();
        let pops = commands
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::PopClip))
            .count();
        assert_eq!(pushes, 2);
        assert_eq!(pops, pushes);
    }

    #[test]
    fn grid_shifts_visible_rect_into_child_cell_coordinates() {
        let canvas = Box::leak(Box::new(InnerCanvas::new()));
        let mut ctx = build_context(canvas);
        ctx.visible_rect = Some((0.0, 60.0, 200.0, 20.0));
        let visible_rect = Rc::new(RefCell::new(None));
        let grid = RawGrid {
            columns: vec![GridTrack::Px(200.0)],
            rows: vec![GridTrack::Px(50.0), GridTrack::Px(50.0)],
            column_gap: 0.0,
            row_gap: 0.0,
            horizontal_alignment: GridAlignment::Center,
            vertical_alignment: GridAlignment::Center,
            overflow: GridOverflow::Clip,
            children: vec![RawGridItem {
                child: Box::new(VisibleRectRecorder { visible_rect: visible_rect.clone() }),
                placement: GridPlacement::default().at(1, 0),
                horizontal_alignment: None,
                vertical_alignment: None,
            }],
        };

        grid.draw(&ctx);

        assert_eq!(*visible_rect.borrow(), Some((-90.0, -5.0, 200.0, 20.0)));
    }

    #[test]
    fn explicit_items_reserve_cells_before_sparse_auto_placement() {
        let items = [
            GridPlacement::default(),
            GridPlacement::default().at(0, 1),
            GridPlacement::default(),
            GridPlacement::default(),
        ];

        let layout = resolve_placements(&items, 2, 1).unwrap();

        assert_eq!(layout.row_count, 2);
        assert_eq!(layout.items[0], GridPlacement::resolved(0, 0, 1, 1));
        assert_eq!(layout.items[1], GridPlacement::resolved(0, 1, 1, 1));
        assert_eq!(layout.items[2], GridPlacement::resolved(1, 0, 1, 1));
        assert_eq!(layout.items[3], GridPlacement::resolved(1, 1, 1, 1));
    }

    #[test]
    fn sparse_auto_placement_does_not_backfill_holes_before_cursor() {
        let items = [
            GridPlacement::default().column_span(2),
            GridPlacement::default(),
            GridPlacement::default().at(0, 1),
        ];

        let layout = resolve_placements(&items, 3, 1).unwrap();

        assert_eq!(layout.items[0], GridPlacement::resolved(1, 0, 1, 2));
        assert_eq!(layout.items[1], GridPlacement::resolved(1, 2, 1, 1));
        assert_eq!(layout.items[2], GridPlacement::resolved(0, 1, 1, 1));
    }

    #[test]
    fn invalid_spans_and_explicit_overlaps_are_reported() {
        let zero_span = resolve_placements(&[GridPlacement::default().column_span(0)], 2, 1);
        assert_eq!(zero_span, Err(GridError::ZeroSpan { item: 0 }));

        let overlap = resolve_placements(
            &[GridPlacement::default().at(0, 0), GridPlacement::default().at(0, 0)],
            2,
            1,
        );
        assert_eq!(overlap, Err(GridError::OverlappingItems { first: 0, second: 1 }));

        let outside = resolve_placements(
            &[GridPlacement::default()
                .at(0, 1)
                .column_span(2)],
            2,
            1,
        );
        assert_eq!(
            outside,
            Err(GridError::ColumnOutOfRange { item: 0, column: 1, span: 2, columns: 2 })
        );
    }

    #[test]
    fn tracks_resolve_fixed_auto_and_fractional_space() {
        let tracks = [GridTrack::Px(20.0), GridTrack::Auto, GridTrack::Fr(1.0), GridTrack::Fr(2.0)];

        let resolved =
            resolve_tracks(&tracks, 140.0, 5.0, &[0.0, 30.0, 0.0, 0.0], "columns").unwrap();

        assert_eq!(resolved, vec![20.0, 30.0, 25.0, 50.0]);
    }

    #[test]
    fn fractional_tracks_require_a_bounded_axis() {
        let result = resolve_tracks(&[GridTrack::Fr(1.0)], f32::MAX, 0.0, &[0.0], "columns");

        assert_eq!(result, Err(GridError::UnboundedFractionalTrack { axis: "columns" }));
    }

    #[test]
    fn track_values_must_be_valid() {
        assert_eq!(
            resolve_tracks(&[GridTrack::Fr(0.0)], 100.0, 0.0, &[0.0], "columns"),
            Err(GridError::InvalidFraction { axis: "columns", index: 0, value: 0.0 })
        );
        assert_eq!(
            resolve_tracks(&[GridTrack::Px(-1.0)], 100.0, 0.0, &[0.0], "columns"),
            Err(GridError::InvalidPixels { axis: "columns", index: 0, value: -1.0 })
        );
    }

    #[test]
    fn spanning_content_subtracts_fixed_tracks_before_sizing_auto_tracks() {
        let tracks = [GridTrack::Px(20.0), GridTrack::Auto, GridTrack::Auto];
        let mut minima = vec![0.0; tracks.len()];

        apply_auto_minimum(&tracks, &mut minima, 0, 3, 120.0, 5.0);

        assert_eq!(minima, vec![0.0, 45.0, 45.0]);
    }
}
