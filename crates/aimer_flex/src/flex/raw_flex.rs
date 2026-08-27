use std::ops::Range;
use std::rc::Rc;

use crate::flex::children_source::{ChildrenSource, EagerChildren};
use crate::flex::flex_layout::{FlexLayout, FlexLayoutCache, LayerOrder};
use crate::flex::flex_list::FlexList;
use crate::flex::{BoxAlignment, FlexDirection, JustifyContent, OverflowBehavior};
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_attribute::{BoxConstraint, CacheBounds, Dimension};
use aimer_style::LayoutSpacing;
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, LayoutCache, LayoutElement,
    Rebuildable, VisitorElement, Widget,
};

/// Arranges a homogeneous collection of children along a configurable main
/// axis.
///
/// [`FlexDirection::Row`] uses a horizontal main axis and
/// [`FlexDirection::Column`] a vertical one. Alignment controls placement on
/// each physical axis, while [`OverflowBehavior`] clips, exposes, or wraps
/// children that exceed the available constraints. Spacing is expressed as
/// logical pixels through [`LayoutSpacing`].
///
/// `Flex::new()` defaults to [`FlexDirection::Inherit`], start alignment on
/// both axes, zero gaps, [`OverflowBehavior::Hidden`], and no children. Supply
/// children with [`Flex::children`] or append to an existing erased collection
/// with [`Flex::add_child`].
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(id = "aimer_flex::flex::Flex", schema_only)]
#[allow(dead_code)]
pub struct Flex<W: Widget + 'static = AnyWidget> {
    #[portable_skip]
    pub(crate) direction: FlexDirection,
    #[portable_skip]
    pub(crate) vertical_alignment: BoxAlignment,
    #[portable_skip]
    pub(crate) horizontal_alignment: BoxAlignment,
    #[portable_skip]
    pub(crate) justify_content: Option<JustifyContent>,
    #[portable_skip]
    pub(crate) gaps: LayoutSpacing,
    #[portable_skip]
    pub(crate) overflow: OverflowBehavior,
    #[portable_children]
    pub(crate) children: Vec<W>,
}

impl Default for Flex {
    /// Creates an empty flex layout with inherited direction.
    ///
    /// Both alignments default to [`BoxAlignment::Start`], gaps to zero, and
    /// overflow to [`OverflowBehavior::Hidden`]. An empty flex is already a
    /// valid [`Widget`].
    fn default() -> Self {
        Self::new()
    }
}

impl Flex {
    /// Creates an empty flex layout with inherited direction.
    ///
    /// Both alignments default to [`BoxAlignment::Start`], gaps to zero, and
    /// overflow to [`OverflowBehavior::Hidden`]. An empty flex is already a
    /// valid [`Widget`].
    #[inline]
    pub fn new() -> Self {
        Self {
            direction: FlexDirection::default(),
            vertical_alignment: BoxAlignment::default(),
            horizontal_alignment: BoxAlignment::default(),
            justify_content: None,
            gaps: LayoutSpacing::default(),
            overflow: OverflowBehavior::default(),
            children: Vec::new(),
        }
    }
}

impl<W: Widget + 'static> Flex<W> {
    /// Sets the main-axis direction used to place children.
    ///
    /// The default is [`FlexDirection::Inherit`]. Use
    /// [`FlexDirection::Row`] for left-to-right placement or
    /// [`FlexDirection::Column`] for top-to-bottom placement.
    #[inline]
    pub fn direction(mut self, direction: FlexDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Sets alignment on the physical vertical axis.
    ///
    /// The default is [`BoxAlignment::Start`].
    #[inline]
    pub fn vertical_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.vertical_alignment = alignment;
        self
    }

    /// Sets alignment on the physical horizontal axis.
    ///
    /// The default is [`BoxAlignment::Start`].
    #[inline]
    pub fn horizontal_alignment(mut self, alignment: BoxAlignment) -> Self {
        self.horizontal_alignment = alignment;
        self
    }

    /// Sets placement along the flex container's main axis.
    ///
    /// The default preserves the physical-axis alignment methods. Once set,
    /// this semantic value takes precedence over the old main-axis alignment:
    /// [`FlexDirection::Row`] uses the horizontal axis and
    /// [`FlexDirection::Column`] uses the vertical axis.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use aimer_flex::{Flex, FlexDirection, JustifyContent};
    ///
    /// let flex = Flex::new()
    ///     .direction(FlexDirection::Row)
    ///     .justify_content(JustifyContent::SpaceEvenly);
    /// ```
    #[inline]
    pub fn justify_content(mut self, justify_content: JustifyContent) -> Self {
        self.justify_content = Some(justify_content);
        self
    }

    /// Sets the spacing between adjacent children.
    ///
    /// Values are logical pixels represented by [`LayoutSpacing`]; the default
    /// is zero spacing. Horizontal sides contribute to row gaps and vertical
    /// sides contribute to column gaps.
    #[inline]
    pub fn gaps(mut self, gaps: impl Into<LayoutSpacing>) -> Self {
        self.gaps = gaps.into();
        self
    }

    /// Sets how children exceeding the available constraints are handled.
    ///
    /// [`OverflowBehavior::Hidden`] is the default and clips to the flex
    /// bounds. [`OverflowBehavior::Visible`] paints outside them, while
    /// [`OverflowBehavior::Wrap`] creates additional rows or columns.
    #[inline]
    pub fn overflow(mut self, overflow: OverflowBehavior) -> Self {
        self.overflow = overflow;
        self
    }

    /// Supplies a data source instead of a widget collection.
    ///
    /// The returned [`FlexList`] is not yet a widget: pair it with
    /// [`FlexList::builder`] to map each datum to a child. Prefer this over
    /// [`Flex::children`] for long lists — the container then retains the data
    /// rather than one widget per item. Children supplied earlier are replaced,
    /// exactly as [`Flex::children`] replaces them.
    ///
    /// # Example
    ///
    /// ```rust
    /// use aimer_container::SizedBox;
    /// use aimer_flex::{Flex, FlexDirection};
    ///
    /// let flex = Flex::new().direction(FlexDirection::Column)
    ///                       .list(0..120_000)
    ///                       .builder(|i| SizedBox::new().height(*i % 40));
    /// ```
    #[inline]
    pub fn list<T>(self, items: impl IntoIterator<Item = T>) -> FlexList<T> {
        FlexList::new(
            self.direction,
            self.vertical_alignment,
            self.horizontal_alignment,
            self.justify_content,
            self.gaps,
            self.overflow,
            items,
        )
    }

    /// Replaces all children with the supplied homogeneous collection.
    ///
    /// This is not an append operation. The returned [`Flex`] adopts the item
    /// type of the iterator and is immediately a valid [`Widget`], including
    /// when the iterator is empty.
    #[inline]
    pub fn children<C: Widget>(self, children: impl IntoIterator<Item = C>) -> Flex<C> {
        Flex {
            direction: self.direction,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            justify_content: self.justify_content,
            gaps: self.gaps,
            overflow: self.overflow,
            children: children.into_iter().collect(),
        }
    }

    /// Appends one child to the existing collection.
    ///
    /// The child must have the same type as the collection's existing items.
    /// Use [`Flex::children`] to replace the collection or establish a new item
    /// type.
    #[inline]
    pub fn add_child(mut self, child: W) -> Self {
        self.children.push(child);
        self
    }
}

impl<W: Widget + 'static> Widget for Flex<W> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let elements = self
            .children
            .into_iter()
            .map(|c| c.to_element(ctx))
            .collect();
        RawFlex {
            direction: self.direction,
            vertical_alignment: self.vertical_alignment,
            horizontal_alignment: self.horizontal_alignment,
            justify_content: self.justify_content,
            gaps: self.gaps,
            children: Box::new(EagerChildren(elements)),
            cache: LayoutCache::new(),
            layout: FlexLayoutCache::new(),
            item_extent: None,
            overflow_behavior: self.overflow,
            debug_name: "Flex",
            cache_bound: CacheBounds::new(),
        }
        .boxed()
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
    pub(crate) justify_content: Option<JustifyContent>,
    pub(crate) gaps: LayoutSpacing,
    /// Supplies the children by index.
    ///
    /// A widget collection produces
    /// [`EagerChildren`](crate::flex::children_source::EagerChildren), which
    /// simply owns the vector. A data source with a declared item extent
    /// produces a windowed one that materializes a viewport at a time — see the
    /// [sparse-children contract](crate::flex::children_source).
    pub(crate) children: Box<dyn ChildrenSource>,
    pub(crate) cache: LayoutCache,
    pub(crate) layout: FlexLayoutCache,
    /// Main-axis extent every child is declared to occupy, when known.
    ///
    /// Set through [`FlexList::item_extent`], it replaces the measuring pass
    /// with arithmetic.
    pub(crate) item_extent: Option<Dimension>,
    pub(crate) overflow_behavior: OverflowBehavior,
    pub(crate) debug_name: &'static str,
    pub(crate) cache_bound: CacheBounds,
}

impl RawFlex {
    /// Creates a low-level flex element with default alignment, spacing, and
    /// clipping.
    #[doc(hidden)]
    #[inline]
    pub fn new(
        direction: FlexDirection,
        children: Vec<AnyElement>,
        debug_name: &'static str,
    ) -> Self {
        Self {
            direction,
            vertical_alignment: Default::default(),
            horizontal_alignment: Default::default(),
            justify_content: None,
            gaps: Default::default(),
            children: Box::new(EagerChildren(children)),
            cache: Default::default(),
            layout: Default::default(),
            item_extent: None,
            overflow_behavior: Default::default(),
            debug_name,
            cache_bound: CacheBounds::new(),
        }
    }

    pub(crate) fn render_child(widget: &dyn Element, ctx: &BuildContext) {
        ctx.canvas.save();
        widget.draw(ctx);
        ctx.canvas.restore();
    }
}

impl RawFlex {
    /// Prepares the same layout table used by ordinary painting, without
    /// tying the result to one visible rectangle. Dynamic-island painting is
    /// only enabled for eager children, so the complete order can be reused
    /// for the static prefix while the live context still culls dynamic rows.
    fn prepare_paint_partition(
        &self,
        ctx: &BuildContext,
    ) -> Option<(Rc<FlexLayout>, (f32, f32))> {
        let (gap_x, gap_y) = self.resole_gaps(ctx);
        let mut layout = self.flex_layout(ctx);
        let mut distribution = self.main_distribution(ctx, layout.total(), layout.len());
        let mut range = self.painted_range(ctx, &layout, distribution);

        self.children.window(range.clone(), ctx);

        for _ in 0..RECONCILE_PASSES {
            let outcome = self.reconcile(ctx, &layout, &range);
            if matches!(outcome, Reconciled::Matched) {
                break;
            }
            let rebuilt = matches!(outcome, Reconciled::Stale);
            if rebuilt {
                layout = self.measure_layout(ctx);
            } else {
                ctx.window.request_redraw();
            }
            distribution = self.main_distribution(ctx, layout.total(), layout.len());
            range = self.painted_range(ctx, &layout, distribution);
            self.children.window(range.clone(), ctx);
            if rebuilt {
                break;
            }
        }

        self.cache
            .set_computed(ctx.box_constraint, scale_bits_of(ctx), layout.total());
        self.layout.set_painted(&range);
        Some((layout, distribution))
    }

    /// Visits a flex's complete paint order while giving stable and dynamic
    /// children the context appropriate to their consumer. The static context
    /// deliberately has no viewport rectangle; the live context keeps the
    /// normal cache-window culling contract.
    fn visit_paint_partition(
        &self,
        retained_ctx: &BuildContext,
        live_ctx: &BuildContext,
        layout: &FlexLayout,
        distribution: (f32, f32),
        order: &LayerOrder,
        range: Range<usize>,
        draw_stable: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
        draw_dynamic: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
    ) {
        let is_row = self.is_row();
        let max_w = retained_ctx.box_constraint.max_width;
        let max_h = retained_ctx.box_constraint.max_height;
        let scale = retained_ctx.scale.max(1.0);
        let clip = (self.overflow_behavior == OverflowBehavior::Hidden).then_some(
            ResolvedSize {
                width: max_w,
                height: max_h,
            },
        );

        order.visit(range, |index| {
            let Some(child) = self.children.get(index) else {
                return;
            };
            let child_size = layout.size(index);
            let main = distribution.0
                + layout.offset(index) as f32
                + distribution.1 * index as f32;
            let (offset_x, offset_y) = if is_row {
                (
                    main,
                    align_offset(self.vertical_alignment, (max_h - child_size.height).max(0.0)),
                )
            } else {
                (
                    align_offset(self.horizontal_alignment, (max_w - child_size.width).max(0.0)),
                    main,
                )
            };
            let offset = Vec2d {
                x: (offset_x * scale).round() / scale,
                y: (offset_y * scale).round() / scale,
            };
            let stable = child.is_paint_stable();
            let base_ctx = if stable { retained_ctx } else { live_ctx };
            if !stable
                && !live_ctx.is_rect_visible(
                    offset_x,
                    offset_y,
                    child_size.width,
                    child_size.height,
                )
            {
                return;
            }

            let child_ctx = BuildContext {
                parent_size: child_size,
                box_constraint: BoxConstraint {
                    min_width: 0.0,
                    min_height: 0.0,
                    max_width: child_size.width,
                    max_height: child_size.height,
                },
                visible_rect: base_ctx.visible_rect.map(|(x, y, width, height)| {
                    (x - offset_x, y - offset_y, width, height)
                }),
                ..base_ctx.clone()
            };

            if stable {
                draw_stable(child, &child_ctx, offset, clip);
            } else {
                draw_dynamic(child, &child_ctx, offset, clip);
            }
        });
    }
}

/// How often one frame reconciles its table with the children it is about to
/// paint.
///
/// Correcting a prediction moves the children after the correction, which can
/// bring another one into the viewport, and that one has to be checked too. Two
/// rounds settle any single correction; the third only exists so a cascade
/// terminates rather than being chased forever.
const RECONCILE_PASSES: usize = 3;

/// Outcome of comparing the children about to be painted with the cached table.
enum Reconciled {
    /// The table describes them, so the frame can paint straight away.
    Matched,
    /// A prediction absorbed their exact extents. Offsets after the first
    /// correction moved, so the painted range has to be resolved again.
    Refined,
    /// The table cannot be corrected in place and has to be rebuilt.
    Stale,
}

/// Whether `value` is a real extent rather than an "unbounded" sentinel.
#[inline]
fn is_bounded(value: f32) -> bool {
    value.is_finite() && value < f32::MAX
}

/// Cache key component derived from the context's device scale.
#[inline]
fn scale_bits_of(ctx: &BuildContext) -> u32 {
    ctx.scale.to_bits()
}

/// Distance a child is pushed along one axis to satisfy `alignment`.
///
/// `extra` is the unused space on that axis and is never negative.
#[inline]
fn align_offset(alignment: BoxAlignment, extra: f32) -> f32 {
    match alignment {
        BoxAlignment::Start => 0.0,
        BoxAlignment::Center => extra / 2.0,
        BoxAlignment::End => extra,
    }
}

/// Returns the leading space and additional space between adjacent children.
///
/// `free_space` excludes the gaps already recorded in the layout table. A
/// non-positive or unbounded amount cannot be distributed, so children remain
/// at their measured positions.
#[inline]
pub(crate) fn justify_distribution(
    justify_content: JustifyContent,
    free_space: f32,
    child_count: usize,
) -> (f32, f32) {
    if child_count == 0 || !free_space.is_finite() || free_space <= 0.0 {
        return (0.0, 0.0);
    }

    match justify_content {
        JustifyContent::Start => (0.0, 0.0),
        JustifyContent::Center => (free_space / 2.0, 0.0),
        JustifyContent::End => (free_space, 0.0),
        JustifyContent::SpaceBetween if child_count > 1 => {
            (0.0, free_space / (child_count - 1) as f32)
        }
        JustifyContent::SpaceAround => {
            let between = free_space / child_count as f32;
            (between / 2.0, between)
        }
        JustifyContent::SpaceEvenly => {
            let space = free_space / (child_count + 1) as f32;
            (space, space)
        }
        JustifyContent::SpaceBetween => (0.0, 0.0),
    }
}

impl RawFlex {
    #[inline]
    fn resole_gaps(&self, ctx: &BuildContext) -> (f32, f32) {
        let max_width = ctx.box_constraint.max_width;
        let max_height = ctx.box_constraint.max_height;
        let gap_x = self.gaps.left.value(max_width, ctx.scale)
            + self.gaps.right.value(max_width, ctx.scale);
        let gap_y = self.gaps.top.value(max_height, ctx.scale)
            + self.gaps.bottom.value(max_height, ctx.scale);

        (gap_x, gap_y)
    }

    /// `true` when children run along the horizontal axis.
    #[inline]
    fn is_row(&self) -> bool {
        !matches!(self.direction, FlexDirection::Column)
    }

    /// Picks the gap that separates two adjacent children.
    #[inline]
    fn gap_main(&self, gap_x: f32, gap_y: f32) -> f32 {
        if self.is_row() { gap_x } else { gap_y }
    }

    /// Returns the main-axis table for `ctx`, measuring the children only when
    /// the constraint or the scale changed since the last pass.
    ///
    /// Measuring and painting share the result, so a scroll — which changes the
    /// visible rectangle and nothing else — never re-measures the child list.
    fn flex_layout(&self, ctx: &BuildContext) -> Rc<FlexLayout> {
        if let Some(layout) = self.layout.get(ctx.box_constraint, scale_bits_of(ctx)) {
            return layout;
        }
        if let Some(layout) = self
            .layout
            .get_stale_stable(ctx.box_constraint, scale_bits_of(ctx))
            && layout.stable_children_match(self.children.as_ref())
        {
            self.layout
                .set(ctx.box_constraint, scale_bits_of(ctx), Rc::clone(&layout));
            return layout;
        }
        self.measure_layout(ctx)
    }

    /// Rebuilds the cached table, measuring the children only when their extent
    /// can be neither declared nor predicted.
    ///
    /// The three sources are tried in order of what they cost: a declared extent
    /// is free, a prediction costs one child, and measuring costs all of them.
    ///
    /// Measuring is the one pass that needs every child at once, so a windowed
    /// source is asked to materialize its whole range first. That is the escape
    /// hatch for a declared extent that cannot be resolved under the current
    /// constraints — the container reports an honest size instead of a total
    /// derived from the handful of rows it happens to hold.
    fn measure_layout(&self, ctx: &BuildContext) -> Rc<FlexLayout> {
        let (gap_x, gap_y) = self.resole_gaps(ctx);
        let gap_main = self.gap_main(gap_x, gap_y);
        let known = self
            .declared_layout(ctx, gap_main)
            .or_else(|| self.estimated_layout(ctx, gap_main));
        let layout = Rc::new(match known {
            Some(table) => table,
            None => {
                self.materialize_all(ctx);
                FlexLayout::build(self.direction, self.children.as_ref(), ctx, gap_main)
            }
        });
        self.layout
            .set(ctx.box_constraint, scale_bits_of(ctx), Rc::clone(&layout));
        layout
    }

    /// Builds the main-axis table from [`RawFlex::item_extent`] alone.
    ///
    /// A declared extent turns the whole pass into arithmetic: the total is
    /// `(extent + gap) * len - gap`, so a list of a hundred thousand rows
    /// reports its scroll extent without a single child being measured, and the
    /// first frame only touches the handful of children it paints. Children are
    /// laid out across the container's full cross-axis maximum, since nothing
    /// measured them.
    ///
    /// Returns `None` — falling back to measuring — when the extent states
    /// nothing ([`Dimension::Auto`]) or cannot be resolved, which happens for a
    /// percentage extent under an unbounded main axis and when the cross axis
    /// itself is unbounded.
    fn declared_layout(&self, ctx: &BuildContext, gap_main: f32) -> Option<FlexLayout> {
        let is_row = self.is_row();
        let extent = self.item_extent?;

        let (max_main, max_cross) = if is_row {
            (ctx.box_constraint.max_width, ctx.box_constraint.max_height)
        } else {
            (ctx.box_constraint.max_height, ctx.box_constraint.max_width)
        };

        let main = match extent {
            // `Auto` declares nothing, so there is no table to build.
            Dimension::Auto => return None,
            Dimension::Px(value) => value * ctx.scale,
            // A share of an unbounded axis is not a size.
            Dimension::Percent(_) if !is_bounded(max_main) => return None,
            percent => percent.resolve(max_main, ctx.scale),
        };
        if !is_bounded(main) || main < 0.0 || !is_bounded(max_cross) {
            return None;
        }

        Some(FlexLayout::declared(
            self.children.len(),
            main,
            max_cross.max(0.0),
            is_row,
            gap_main,
        ))
    }

    /// Predicts the main-axis table from a single probed child.
    ///
    /// Without a declared extent the container would have to measure every child
    /// before it could report its own size, which is what makes a long list hang
    /// on its first frame. Measuring *one* child and assuming the rest match it
    /// costs one measure instead of `len`, and it is exact for the shape a long
    /// scrolled list almost always has: uniform rows. The guess is not taken on
    /// faith — [`RawFlex::reconcile`] re-measures the children each frame paints,
    /// and a child that disagrees has its exact extent recorded in the table, so
    /// a list of varying rows converges on its true extent as it is scrolled
    /// without ever having to be measured whole.
    ///
    /// Returns `None`, leaving the exact measuring pass in charge, when:
    ///
    /// - the children are not built on demand, so measuring them is what the
    ///   container already paid for, and a wrong guess would only add a visible
    ///   correction to an ordinary [`Row`](crate::Row) or
    ///   [`Column`](crate::Column);
    /// - the main axis is bounded, which means the container is not inside a
    ///   scroll viewport and its total is not what a scroll extent is derived
    ///   from — there is nothing to win and a correction to lose;
    /// - the probe is a flex child, whose size comes from distributing the
    ///   leftover space rather than from the child itself.
    ///
    /// The probe is taken from the live window rather than from index zero: at a
    /// deep scroll offset row zero does not exist, so probing it would build a
    /// row only to drop it, and a first row is often the atypical one.
    fn estimated_layout(&self, ctx: &BuildContext, gap_main: f32) -> Option<FlexLayout> {
        let len = self.children.len();
        if len == 0 || !self.children.is_windowed() {
            return None;
        }

        let is_row = self.is_row();
        let (max_main, max_cross) = if is_row {
            (ctx.box_constraint.max_width, ctx.box_constraint.max_height)
        } else {
            (ctx.box_constraint.max_height, ctx.box_constraint.max_width)
        };
        if is_bounded(max_main) {
            return None;
        }

        let index = self.children.live_start().unwrap_or(0);
        if self.children.get(index).is_none() {
            self.children.window(index..index + 1, ctx);
        }
        let probe = self.children.get(index)?;
        if probe.flex().is_some() {
            // Distribution decides the size, so there is nothing to extrapolate
            // from — and every share depends on the other children anyway.
            self.children.materialize_all(ctx);
            return None;
        }

        // Measured exactly as `FlexLayout::build` would, so a later
        // revalidation compares like with like.
        let mut child_ctx = ctx.clone();
        if is_row {
            child_ctx.box_constraint.max_width = f32::MAX;
            child_ctx.box_constraint.max_height = max_cross;
        } else {
            child_ctx.box_constraint.max_height = f32::MAX;
            child_ctx.box_constraint.max_width = max_cross;
        }
        let probed = probe.computed_size(&child_ctx);

        Some(FlexLayout::estimated(len, probed, is_row, gap_main))
    }

    /// Compares the children about to be painted with the cached table, and
    /// corrects the table where it can.
    ///
    /// Nothing invalidates a flex container when a descendant resizes itself —
    /// an implicitly animated child rebuilds inside its own `draw` — so the
    /// children that are about to be painted are re-measured and compared. That
    /// keeps the check proportional to what is on screen: a child that changed
    /// size while off-screen cannot be seen to be mispositioned, and it is
    /// picked up as soon as it scrolls into range.
    ///
    /// A *predicted* table absorbs a disagreement instead of being thrown away:
    /// [`FlexLayout::refine`] records the child's exact extent, the children
    /// before it keep the offsets they were painted at, and the container stays
    /// windowed. A long list of genuinely varying rows therefore converges on its
    /// true extent as it is scrolled, rather than paying for measuring all of it
    /// the moment one row disagrees.
    ///
    /// A table containing flex children is never trusted across frames, because
    /// each share depends on what every other child consumed.
    fn reconcile(
        &self,
        ctx: &BuildContext,
        layout: &FlexLayout,
        range: &Range<usize>,
    ) -> Reconciled {
        // A declared extent is authoritative, so nothing is re-measured.
        if layout.is_declared() {
            return Reconciled::Matched;
        }

        if layout.has_flex() {
            return Reconciled::Stale;
        }

        let is_row = self.is_row();
        let mut child_ctx = ctx.clone();
        if is_row {
            child_ctx.box_constraint.max_width = f32::MAX;
            child_ctx.box_constraint.max_height = ctx.box_constraint.max_height;
        } else {
            child_ctx.box_constraint.max_height = f32::MAX;
            child_ctx.box_constraint.max_width = ctx.box_constraint.max_width;
        }

        let predicted = layout.is_estimated();
        let mut refined = false;
        for index in range.clone() {
            let Some(child) = self.children.get(index) else {
                continue;
            };
            // A flex child's size comes from distributing the leftover space, so
            // no per-child correction can describe it and the whole list has to
            // be measured together.
            if child.flex().is_some() {
                self.materialize_all(ctx);
                return Reconciled::Stale;
            }
            let size = child.computed_size(&child_ctx);
            if size == layout.size(index) {
                continue;
            }
            if !predicted {
                return Reconciled::Stale;
            }
            layout.refine(index, size);
            refined = true;
        }

        if refined {
            Reconciled::Refined
        } else {
            Reconciled::Matched
        }
    }

    /// Forces the source to hold every child, permanently.
    ///
    /// Only the measuring and wrapping passes need this: both derive a result
    /// from the whole list, so a partial window would produce a wrong size. The
    /// source stops windowing rather than merely widening its range, so the next
    /// frame cannot drop what the measured table describes — and a container
    /// that had to measure once never goes back to predicting.
    pub(crate) fn materialize_all(&self, ctx: &BuildContext) {
        self.children.materialize_all(ctx);
    }

    /// Main-axis leading and inter-child space added by the container's
    /// justification.
    #[inline]
    fn main_distribution(&self, ctx: &BuildContext, total: ResolvedSize, child_count: usize) -> (f32, f32) {
        let (max_main, total_main, legacy_alignment) = if self.is_row() {
            (
                ctx.box_constraint.max_width,
                total.width,
                self.horizontal_alignment,
            )
        } else {
            (
                ctx.box_constraint.max_height,
                total.height,
                self.vertical_alignment,
            )
        };
        let justify_content = self.justify_content.unwrap_or(match legacy_alignment {
            BoxAlignment::Start => JustifyContent::Start,
            BoxAlignment::Center => JustifyContent::Center,
            BoxAlignment::End => JustifyContent::End,
        });
        justify_distribution(
            justify_content,
            (max_main - total_main).max(0.0),
            child_count,
        )
    }

    /// Resolves the children that `ctx.visible_rect` exposes.
    ///
    /// `base_main` is the main-axis shift the container's own alignment applies.
    /// Without a visible rectangle every child is in range, which keeps an
    /// unclipped flex behaving exactly as before.
    fn painted_range(
        &self,
        ctx: &BuildContext,
        layout: &FlexLayout,
        distribution: (f32, f32),
    ) -> Range<usize> {
        let (start, extent) = match ctx.visible_rect {
            Some((vx, vy, vw, vh)) => {
                if self.is_row() {
                    (vx, vw)
                } else {
                    (vy, vh)
                }
            }
            None if self.overflow_behavior == OverflowBehavior::Hidden => {
                // Hidden overflow establishes a finite painting box even at
                // the root, where no ancestor has supplied visible_rect. Keep
                // the child window aligned with that box so painting and hit
                // testing do not walk eager children that the clip discards.
                let extent = if self.is_row() {
                    ctx.box_constraint.max_width
                } else {
                    ctx.box_constraint.max_height
                };
                (0.0, extent)
            }
            None => return 0..layout.len(),
        };
        let (leading, between) = distribution;
        if between == 0.0 {
            let start = (start - leading) as f64;
            return layout.visible_range(start, start + extent as f64);
        }
        layout.visible_range_with_extra_space(
            start as f64,
            (start + extent) as f64,
            leading as f64,
            between as f64,
        )
    }
}

impl Drawable for RawFlex {
    fn draw(&self, ctx: &BuildContext) {
        let (gap_x, gap_y) = self.resole_gaps(ctx);
        let max_w = ctx.box_constraint.max_width;
        let max_h = ctx.box_constraint.max_height;

        ctx.canvas.save();

        #[cfg(debug_assertions)]
        {
            if aimer_widget::inspector_overlay::is_enabled() {
                let parent_pos: Vec2d = ctx.canvas.get_transform_translation().into();

                self.cache_bound.save(
                    ctx.scale,
                    parent_pos.x,
                    parent_pos.y,
                    ctx.box_constraint.max_width,
                    ctx.box_constraint.max_height,
                );

                let cp = ctx.cursor_pos;
                if self.cache_bound.is_inside(cp.x, cp.y) {
                    let (l_start, l_end) = self.cache_bound.pos_start_end().unwrap();
                    if let Ok(mut hovered) = aimer_widget::inspector_overlay::HOVERED_WIDGET.write()
                    {
                        *hovered = Some((self.debug_name, l_start, l_end));
                    }
                }
            }
        }

        // Apply clipping for overflow hidden
        self.overflow_behavior.apply_overflow_behave(ctx);

        if self.overflow_behavior == OverflowBehavior::Wrap {
            self.draw_wrapped(ctx, gap_x, gap_y);
            ctx.canvas.restore();
            return;
        }

        let is_row = self.is_row();
        let mut layout = self.flex_layout(ctx);
        // The container's justification distributes free space along the main
        // axis; the cross axis is aligned per child.
        let mut distribution = self.main_distribution(ctx, layout.total(), layout.len());
        let mut range = self.painted_range(ctx, &layout, distribution);

        // Bring the slice about to be painted into existence before anything
        // borrows a child. A windowed source drops what left the range here, so
        // this must stay the last point at which children can be dropped.
        self.children.window(range.clone(), ctx);

        for _ in 0..RECONCILE_PASSES {
            let outcome = self.reconcile(ctx, &layout, &range);
            if matches!(outcome, Reconciled::Matched) {
                break;
            }
            let rebuilt = matches!(outcome, Reconciled::Stale);
            if rebuilt {
                layout = self.measure_layout(ctx);
            } else {
                // The container's own size changed underneath a parent that has
                // already asked for it this frame — a scroll view reads
                // `content_size` before it draws its child. Asking for another
                // frame is what lets the scroll range catch up. It cannot loop:
                // a recorded child agrees with the table from now on.
                ctx.window.request_redraw();
            }
            distribution = self.main_distribution(ctx, layout.total(), layout.len());
            range = self.painted_range(ctx, &layout, distribution);
            self.children.window(range.clone(), ctx);
            if rebuilt {
                // A rebuilt table measured every child, so there is nothing left
                // to disagree with.
                break;
            }
        }

        // Painting resolved the whole table, so let a later `computed_size`
        // read the result straight back.
        self.cache
            .set_computed(ctx.box_constraint, scale_bits_of(ctx), layout.total());
        self.layout.set_painted(&range);

        // Children paint in layer order. The order is structural for this
        // element generation and the current painted range, so cached frames
        // reuse it without rebuilding or sorting a vector.
        let order = self.layout.cached_layer_order(range.clone(), |index| {
            self.children.get(index).map(|child| child.layer())
        });

        // Round child positions to device pixels so that adjacent backgrounds
        // always tile without sub-pixel seams.  Without this, a fractional
        // scroll offset combined with a float main-axis offset can place two
        // sibling rectangles on fractional device-pixel boundaries, and the
        // GPU anti-aliasing blends the gap with the parent background (white).
        let scale = ctx.scale.max(1.0);

        order.visit(range, |index| {
            let Some(child) = self.children.get(index) else {
                return;
            };
            let child_size = layout.size(index);
            let c_w = child_size.width;
            let c_h = child_size.height;
            let main = distribution.0
                + layout.offset(index) as f32
                + distribution.1 * index as f32;

            let (offset_x, offset_y) = if is_row {
                (
                    main,
                    align_offset(self.vertical_alignment, (max_h - c_h).max(0.0)),
                )
            } else {
                (
                    align_offset(self.horizontal_alignment, (max_w - c_w).max(0.0)),
                    main,
                )
            };

            // The index range only bounds the main axis, so a child can still
            // sit outside the viewport across it.
            if !ctx.is_rect_visible(offset_x, offset_y, c_w, c_h) {
                return;
            }

            let draw_ctx = BuildContext {
                parent_size: child_size,
                box_constraint: BoxConstraint {
                    min_width: 0.0,
                    min_height: 0.0,
                    max_width: c_w,
                    max_height: c_h,
                },
                visible_rect: ctx
                    .visible_rect
                    .map(|(vx, vy, vw, vh)| (vx - offset_x, vy - offset_y, vw, vh)),
                ..ctx.clone()
            };

            let rx = (offset_x * scale).round() / scale;
            let ry = (offset_y * scale).round() / scale;

            draw_ctx.canvas.save();
            draw_ctx.canvas.translate(Vec2d { x: rx, y: ry });
            Self::render_child(child, &draw_ctx);
            draw_ctx.canvas.restore();
        });

        // Pop the clip pushed by overflow_behavior.apply_overflow_behave()
        if self.overflow_behavior == OverflowBehavior::Hidden {
            ctx.canvas.clear_clip();
        }
        ctx.canvas.restore();
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        self.overflow_behavior != OverflowBehavior::Wrap && self.children.is_paint_stable()
    }

    #[doc(hidden)]
    fn draw_paint_islands(
        &self,
        retained_ctx: &BuildContext,
        live_ctx: &BuildContext,
        draw_stable: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
        draw_dynamic: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
    ) -> bool {
        // A windowed source's live children are created and retired as the
        // viewport moves, so retaining one of its rows independently would
        // outlive the source's structural contract. Wrapping also changes the
        // two-dimensional placement in a way this one-axis partition cannot
        // represent. Both cases remain on the ordinary direct path.
        if self.overflow_behavior == OverflowBehavior::Wrap || self.children.is_windowed() {
            return false;
        }

        let Some((layout, distribution)) = self.prepare_paint_partition(live_ctx) else {
            return false;
        };
        let range = 0..self.children.len();
        let order = self
            .layout
            .cached_layer_order(range.clone(), |index| {
                self.children.get(index).map(|child| child.layer())
            });

        // One retained layer is emitted before the dynamic suffix. A dynamic
        // child interleaved with a later stable child would require several
        // independent layers (and can multiply the texture budget), so it is
        // deliberately rejected before either callback can paint.
        let mut saw_stable = false;
        let mut saw_dynamic = false;
        let mut dynamic_started = false;
        let mut stable_after_dynamic = false;
        order.visit(range.clone(), |index| {
            let Some(child) = self.children.get(index) else {
                return;
            };
            if child.is_paint_stable() {
                saw_stable = true;
                stable_after_dynamic |= dynamic_started;
            } else {
                saw_dynamic = true;
                dynamic_started = true;
            }
        });
        if !saw_stable || !saw_dynamic || stable_after_dynamic {
            return false;
        }

        self.visit_paint_partition(
            retained_ctx,
            live_ctx,
            &layout,
            distribution,
            &order,
            range,
            draw_stable,
            draw_dynamic,
        );
        true
    }
}

impl VisitorElement for RawFlex {
    /// Visits the children that exist.
    ///
    /// An eager container holds all of them. A windowed one holds the rows the
    /// last frame needed, which is the [sparse-children
    /// contract](crate::flex::children_source): a row outside the window has no
    /// element yet, so there is nothing to visit.
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.children.visit(visitor);
    }

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}

impl EventElement for RawFlex {
    /// The event and visual child views are the same retained source.
    #[inline]
    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.children.visit(visitor);
    }

    /// Offers every child that exists, painted or not.
    ///
    /// Focus and broadcast delivery use this, so an off-screen input field of an
    /// eager container still receives keys. A windowed container can only offer
    /// its live window — see the [sparse-children
    /// contract](crate::flex::children_source).
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        self.children.visit(visitor);
    }

    /// Offers only the children the last frame actually painted.
    ///
    /// A pointer can only land on something that was drawn, so a clipped list
    /// hit-tests its viewport slice instead of its whole child vector — the
    /// difference between a few dozen and a few hundred thousand visits per
    /// pointer move. Keyboard focus and broadcasts keep using
    /// [`EventElement::event_children`], so an off-screen field still receives
    /// them.
    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        let Some(range) = self.layout.painted() else {
            return self.event_children(visitor);
        };
        for index in range.start..range.end.min(self.children.len()) {
            if let Some(child) = self.children.get(index) {
                visitor(child);
            }
        }
    }

    /// Visits the painted range in the order pointer routing consumes it, so a
    /// large sibling group does not materialize a temporary reverse-order
    /// buffer for every event.
    #[inline]
    fn hit_test_children_reversed<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        let Some(range) = self.layout.painted() else {
            return self.children.visit_reversed(visitor);
        };
        for index in (range.start..range.end.min(self.children.len())).rev() {
            if let Some(child) = self.children.get(index) {
                visitor(child);
            }
        }
    }
}
impl Rebuildable for RawFlex {
    fn option_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// Takes over the rows and the measured table of the container being
    /// replaced.
    ///
    /// Both escape ordinary reconciliation. A windowed container starts life
    /// empty, so the positional walk finds no rows to pair — see the
    /// [sparse-children contract](crate::flex::children_source). The table is not
    /// a child at all, and it holds every extent the list learned by painting, so
    /// dropping it would snap a predicted scroll extent back to its prediction.
    ///
    /// The rows are claimed by identity, so they transfer even when the data
    /// changed. The table describes positions, so it only transfers when nothing
    /// that decides them did: the same number of children, laid out under the
    /// same rules.
    ///
    /// The rows the replacement builds are new elements either way, so only the
    /// part of the table that never described a particular child survives the
    /// handover — see [`FlexLayoutCache::adopt`].
    fn adopt_runtime_state_from(&self, old: &dyn Element) {
        let Some(old) = old
            .option_any()
            .and_then(|any| any.downcast_ref::<RawFlex>())
        else {
            return;
        };

        self.children.adopt_retained(old.children.take_retained());

        if old.children.len() == self.children.len()
            && old.direction == self.direction
            && old.gaps == self.gaps
            && old.overflow_behavior == self.overflow_behavior
            && old.item_extent == self.item_extent
        {
            self.layout.adopt(&old.layout);
        }
    }
}

impl LayoutElement for RawFlex {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_computed(ctx.box_constraint, scale_bits) {
            return cached;
        }

        if self.overflow_behavior == OverflowBehavior::Wrap {
            let (gap_x, gap_y) = self.resole_gaps(ctx);
            let (_, layout) = self.wrapped_layout(ctx, gap_x, gap_y);
            self.cache
                .set_computed(ctx.box_constraint, scale_bits, layout.size);
            return layout.size;
        }

        let layout = self.flex_layout(ctx);
        let result = layout.total();
        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);
        result
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.computed_size(ctx)
    }

    /// Drops this container's tables and those of every child that exists.
    ///
    /// The live window itself is kept: a resize invalidates layout on every
    /// frame it produces, and rebuilding the visible rows each time would reset
    /// their state while the user is still dragging the window edge.
    fn invalidate_layout(&self) {
        self.cache.invalidate();
        self.layout.invalidate();
        self.children.visit(&mut |child| child.invalidate_layout());
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.cache_bound.pos_start_end()
    }
}
