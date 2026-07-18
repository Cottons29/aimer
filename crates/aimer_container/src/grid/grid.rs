use aimer_widget::base::BuildContext;
use aimer_widget::{AnyWidget, Element, Widget};

use super::raw_grid::{
    GridPlacement, GridTrack, RawGrid, RawGridItem, resolve_placements, resolve_tracks,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridAlignment {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridOverflow {
    #[default]
    Clip,
    Visible,
}

pub struct GridItem<W: Widget + 'static> {
    pub(crate) child: W,
    pub(crate) placement: GridPlacement,
    pub(crate) horizontal_alignment: Option<GridAlignment>,
    pub(crate) vertical_alignment: Option<GridAlignment>,
}

impl<W: Widget + 'static> GridItem<W> {
    pub fn new(child: W) -> Self {
        Self {
            child,
            placement: GridPlacement::default(),
            horizontal_alignment: None,
            vertical_alignment: None,
        }
    }

    pub fn row(mut self, row: usize) -> Self {
        self.placement
            .row = Some(row);
        self
    }

    pub fn column(mut self, column: usize) -> Self {
        self.placement
            .column = Some(column);
        self
    }

    pub fn at(mut self, row: usize, column: usize) -> Self {
        self.placement
            .row = Some(row);
        self.placement
            .column = Some(column);
        self
    }

    pub fn row_span(mut self, span: usize) -> Self {
        self.placement
            .row_span = span;
        self
    }

    pub fn column_span(mut self, span: usize) -> Self {
        self.placement
            .column_span = span;
        self
    }

    pub fn horizontal_alignment(mut self, alignment: GridAlignment) -> Self {
        self.horizontal_alignment = Some(alignment);
        self
    }

    pub fn vertical_alignment(mut self, alignment: GridAlignment) -> Self {
        self.vertical_alignment = Some(alignment);
        self
    }
}

pub struct Grid<W: Widget + 'static = AnyWidget> {
    columns: Vec<GridTrack>,
    rows: Vec<GridTrack>,
    column_gap: f32,
    row_gap: f32,
    horizontal_alignment: GridAlignment,
    vertical_alignment: GridAlignment,
    overflow: GridOverflow,
    children: Vec<GridItem<W>>,
}

impl Default for Grid<AnyWidget> {
    fn default() -> Self {
        Self::new()
    }
}

impl Grid<AnyWidget> {
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            column_gap: 0.0,
            row_gap: 0.0,
            horizontal_alignment: GridAlignment::Stretch,
            vertical_alignment: GridAlignment::Stretch,
            overflow: GridOverflow::Clip,
            children: Vec::new(),
        }
    }
}

impl<W: Widget + 'static> Grid<W> {
    pub fn columns(mut self, tracks: impl IntoIterator<Item = GridTrack>) -> Self {
        self.columns = tracks
            .into_iter()
            .collect();
        self
    }

    pub fn rows(mut self, tracks: impl IntoIterator<Item = GridTrack>) -> Self {
        self.rows = tracks
            .into_iter()
            .collect();
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.column_gap = gap.max(0.0);
        self.row_gap = gap.max(0.0);
        self
    }

    pub fn column_gap(mut self, gap: f32) -> Self {
        self.column_gap = gap.max(0.0);
        self
    }

    pub fn row_gap(mut self, gap: f32) -> Self {
        self.row_gap = gap.max(0.0);
        self
    }

    pub fn horizontal_alignment(mut self, alignment: GridAlignment) -> Self {
        self.horizontal_alignment = alignment;
        self
    }

    pub fn vertical_alignment(mut self, alignment: GridAlignment) -> Self {
        self.vertical_alignment = alignment;
        self
    }

    pub fn overflow(mut self, overflow: GridOverflow) -> Self {
        self.overflow = overflow;
        self
    }

    pub fn children<C: Widget + 'static>(
        self,
        children: impl IntoIterator<Item = GridItem<C>>,
    ) -> Grid<C> {
        Grid {
            columns: self.columns,
            rows: self.rows,
            column_gap: self.column_gap,
            row_gap: self.row_gap,
            horizontal_alignment: self.horizontal_alignment,
            vertical_alignment: self.vertical_alignment,
            overflow: self.overflow,
            children: children
                .into_iter()
                .collect(),
        }
    }
}

impl<W: Widget + 'static> Widget for Grid<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let placements = self
            .children
            .iter()
            .map(|item| item.placement)
            .collect::<Vec<_>>();
        let validation = resolve_placements(
            &placements,
            self.columns
                .len(),
            self.rows
                .len(),
        )
        .and_then(|_| {
            resolve_tracks(&self.columns, 1.0, self.column_gap, &[], "columns").map(|_| ())
        })
        .and_then(|_| resolve_tracks(&self.rows, 1.0, self.row_gap, &[], "rows").map(|_| ()));

        if let Err(error) = validation {
            return aimer_widget::ErrorWidget::new(format!("Grid layout error: {error}"))
                .to_element(ctx);
        }

        let children = self
            .children
            .iter()
            .map(|item| RawGridItem {
                child: item
                    .child
                    .to_element(ctx),
                placement: item.placement,
                horizontal_alignment: item.horizontal_alignment,
                vertical_alignment: item.vertical_alignment,
            })
            .collect();

        Box::new(RawGrid {
            columns: self
                .columns
                .clone(),
            rows: self
                .rows
                .clone(),
            column_gap: self.column_gap,
            row_gap: self.row_gap,
            horizontal_alignment: self.horizontal_alignment,
            vertical_alignment: self.vertical_alignment,
            overflow: self.overflow,
            children,
        })
    }

    fn debug_name(&self) -> &'static str {
        "Grid"
    }
}
