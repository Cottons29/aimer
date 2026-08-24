pub(crate) mod children_source;
#[doc(hidden)]
pub mod flex_child;
pub(crate) mod flex_layout;
pub mod flex_list;
#[cfg(test)]
mod lazy_tests {
    //! Tests for the viewport-proportional behaviour of [`RawFlex`].
    //!
    //! A flex container measures and paints through one cached main-axis table, so
    //! the work a frame does is meant to follow the viewport rather than the child
    //! count. These tests pin that with a hundred-thousand-child column: the probe
    //! children count every measure and every paint they receive, so a regression to
    //! an `O(children)` pass shows up as a number instead of a slowdown.

    use std::cell::Cell;
    use std::rc::Rc;

    use aimer_widget::{Drawable, Element, EventElement, LayoutElement, Rebuildable};

    use crate::flex::raw_flex::RawFlex;
    use crate::flex::test_support::{
        CountingChild, ResizingChild, dummy_build_context, replace_a_generated_subtree,
    };
    use crate::flex::FlexDirection;

    const CHILD_COUNT: usize = 100_000;
    const CHILD_HEIGHT: f32 = 80.0;
    const VIEWPORT: f32 = 600.0;

    fn tall_column(measured: &Rc<Cell<usize>>, drawn: &Rc<Cell<usize>>) -> RawFlex {
        let children = (0..CHILD_COUNT)
            .map(|_| CountingChild::boxed_new(200.0, CHILD_HEIGHT, measured, drawn))
            .collect();
        RawFlex::new(FlexDirection::Column, children, "Column")
    }

    /// A `Column` under a viewport must paint only the children intersecting it,
    /// and must not re-measure the whole list to find them.
    #[test]
    fn draw_only_touches_the_visible_children() {
        let measured = Rc::new(Cell::new(0));
        let drawn = Rc::new(Cell::new(0));
        let column = tall_column(&measured, &drawn);
        let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));

        // The first pass has to size the list once to know the scroll extent.
        column.computed_size(&ctx);
        measured.set(0);
        drawn.set(0);

        column.draw(&ctx);

        let visible = (VIEWPORT / CHILD_HEIGHT).ceil() as usize + 1;
        assert!(
            drawn.get() <= visible,
            "painted {} children for a {VIEWPORT}px viewport",
            drawn.get()
        );
        assert!(
            measured.get() <= visible,
            "measured {} children while painting {} of them",
            measured.get(),
            drawn.get()
        );
    }

    /// Scrolling only changes the offset, so a later frame must stay cheap and
    /// paint the slice that the offset exposes.
    #[test]
    fn scrolled_draw_stays_proportional_to_the_viewport() {
        let measured = Rc::new(Cell::new(0));
        let drawn = Rc::new(Cell::new(0));
        let column = tall_column(&measured, &drawn);
        let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));
        column.draw(&ctx);

        // Emulate a `Scrollable` that has scrolled 4_000 children down.
        let offset = 4_000.0 * CHILD_HEIGHT;
        let scrolled = dummy_build_context(200.0, VIEWPORT, Some((0.0, offset, 200.0, VIEWPORT)));
        measured.set(0);
        drawn.set(0);
        column.draw(&scrolled);

        let visible = (VIEWPORT / CHILD_HEIGHT).ceil() as usize + 1;
        assert!(
            drawn.get() > 0 && drawn.get() <= visible,
            "painted {} children after scrolling",
            drawn.get()
        );
        assert!(
            measured.get() <= visible,
            "measured {} children after scrolling",
            measured.get()
        );
    }

    /// Hit testing must only consider the children of the last painted frame;
    /// nothing else can be under the pointer.
    #[test]
    fn hit_testing_visits_only_the_painted_children() {
        let measured = Rc::new(Cell::new(0));
        let drawn = Rc::new(Cell::new(0));
        let column = tall_column(&measured, &drawn);
        let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));
        column.draw(&ctx);

        let mut hit_tested = 0;
        column.hit_test_children(&mut |_| hit_tested += 1);

        assert_eq!(hit_tested, drawn.get());
    }

    /// Focus and broadcast delivery must still reach every child, painted or
    /// not, otherwise an off-screen input field would stop receiving keys.
    #[test]
    fn event_children_still_visits_the_whole_list() {
        let measured = Rc::new(Cell::new(0));
        let drawn = Rc::new(Cell::new(0));
        let column = tall_column(&measured, &drawn);
        let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));
        column.draw(&ctx);

        let mut visited = 0;
        column.event_children(&mut |_| visited += 1);

        assert_eq!(visited, CHILD_COUNT);
    }

    /// Before the first frame nothing is known to be off-screen, so hit testing
    /// must fall back to the whole list.
    #[test]
    fn hit_testing_before_the_first_frame_visits_everything() {
        let measured = Rc::new(Cell::new(0));
        let drawn = Rc::new(Cell::new(0));
        let column = tall_column(&measured, &drawn);

        let mut hit_tested = 0;
        column.hit_test_children(&mut |_| hit_tested += 1);

        assert_eq!(hit_tested, CHILD_COUNT);
    }

    /// A child that resizes itself between frames — an implicitly animated
    /// container does exactly that inside its own `draw` — must still push its
    /// siblings, even though the cached table was measured before the change.
    #[test]
    fn a_resized_child_moves_its_siblings_on_the_next_frame() {
        let height = Rc::new(Cell::new(20.0));
        let first_at = Rc::new(Cell::new((0.0, 0.0)));
        let second_at = Rc::new(Cell::new((0.0, 0.0)));
        let column = RawFlex::new(
            FlexDirection::Column,
            vec![
                ResizingChild::boxed_new(&height, &first_at),
                ResizingChild::boxed_new(&Rc::new(Cell::new(20.0)), &second_at),
            ],
            "Column",
        );
        let ctx = dummy_build_context(200.0, 600.0, Some((0.0, 0.0, 200.0, 600.0)));

        column.draw(&ctx);
        assert_eq!(second_at.get().1, 20.0);

        height.set(50.0);
        column.draw(&ctx);

        assert_eq!(first_at.get().1, 0.0);
        assert_eq!(second_at.get().1, 50.0);
    }

    /// Replacing the children below a container must retire the cached table.
    ///
    /// A list that grew — an `AsyncBuilder` swapping a spinner for the rows it
    /// waited on — is measured by a `Scrollable` through the container above it. If
    /// that container answers from a table measured before the swap, the scroll
    /// view is told the content still fits and the page will not scroll.
    #[test]
    fn a_replaced_child_list_is_measured_again() {
        let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));

        // The frame that only had the loading state to show.
        let height = Rc::new(Cell::new(CHILD_HEIGHT));
        let placement = Rc::new(Cell::new((0.0, 0.0)));
        let column = RawFlex::new(
            FlexDirection::Column,
            vec![ResizingChild::boxed_new(&height, &placement)],
            "Column",
        );
        assert_eq!(column.computed_size(&ctx).height, CHILD_HEIGHT);

        // The frame the completed future produced: taller content under the very
        // same container.
        height.set(CHILD_HEIGHT * 20.0);
        replace_a_generated_subtree(&ctx);

        assert_eq!(
            column.computed_size(&ctx).height,
            CHILD_HEIGHT * 20.0,
            "the container reported the extent it had before the rebuild"
        );
    }

    /// A table measured from rows that did not survive the rebuild describes
    /// nothing and must not be trusted, however well the two containers match.
    ///
    /// This is the `Column` an `AsyncBuilder` rebuilds when the data arrives: same
    /// direction, same gaps, same number of children — and completely different
    /// children. Trusting the table there is a page that paints its content and
    /// refuses to scroll, because the `Scrollable` above is told the old extent.
    #[test]
    fn an_adopted_table_is_not_trusted_when_the_rows_were_rebuilt() {
        let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));
        let placement = Rc::new(Cell::new((0.0, 0.0)));

        // The container that measured the loading state.
        let short = Rc::new(Cell::new(CHILD_HEIGHT));
        let old = RawFlex::new(
            FlexDirection::Column,
            vec![
                ResizingChild::boxed_new(&short, &placement),
                ResizingChild::boxed_new(&short, &placement),
            ],
            "Column",
        );
        assert_eq!(old.computed_size(&ctx).height, CHILD_HEIGHT * 2.0);

        // The container the completed request produced: the same shape, rows built
        // from scratch out of the data that arrived.
        let tall = Rc::new(Cell::new(CHILD_HEIGHT * 20.0));
        let new = RawFlex::new(
            FlexDirection::Column,
            vec![
                ResizingChild::boxed_new(&tall, &placement),
                ResizingChild::boxed_new(&tall, &placement),
            ],
            "Column",
        );
        new.adopt_runtime_state_from(&old as &dyn Element);
        replace_a_generated_subtree(&ctx);

        assert_eq!(
            new.computed_size(&ctx).height,
            CHILD_HEIGHT * 40.0,
            "the rebuilt container answered from the table its predecessor measured"
        );
    }

    /// The total main-axis extent must survive the lazy path: 100_000 children
    /// of 80px are 8_000_000px tall, which `f32` cannot accumulate exactly.
    #[test]
    fn computed_size_reports_the_full_extent() {
        let measured = Rc::new(Cell::new(0));
        let drawn = Rc::new(Cell::new(0));
        let column = tall_column(&measured, &drawn);
        let ctx = dummy_build_context(200.0, VIEWPORT, Some((0.0, 0.0, 200.0, VIEWPORT)));

        let size = column.computed_size(&ctx);

        assert_eq!(size.width, 200.0);
        assert_eq!(size.height, CHILD_COUNT as f32 * CHILD_HEIGHT);
    }
}
#[doc(hidden)]
pub mod raw_flex;
pub mod row_column;
#[cfg(test)]
pub(crate) mod test_support;
pub(crate) mod wrap_layout;

// pub use raw_flex::RawFlex;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_widget::base::BuildContext;
use aimer_widget::portable::{
    PortableMaterializeError, PortableMaterializeProperty, PortableProperty,
    PortablePropertyConversion, PortablePropertyReflection,
};
#[cfg(feature = "portable-guest")]
use aimer_widget::portable::{PortableBuildContext, PortableBuildError, PortableEncodeProperty};
pub use flex_child::Expanded;
pub use flex_list::{FlexList, ListFlex};

#[cfg(test)]
mod portable_materializer_tests {
    use aimer_widget::portable::PortableNativeWidget;

    use super::Expanded;

    #[test]
    fn expanded_exposes_a_native_materializer_for_hot_reload_hosts() {
        fn assert_materializer<T: PortableNativeWidget>() {}

        assert_materializer::<Expanded>();
    }
}
pub use raw_flex::Flex;
pub use row_column::{Column, Row};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
    #[default]
    Inherit,
}
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum BoxAlignment {
    #[default]
    Start,
    Center,
    End,
}

/// Controls how children are placed along a flex container's main axis.
///
/// `Start`, `Center`, and `End` place the group without changing the spacing
/// between children. The remaining variants distribute positive free space
/// between and around the children. When the children use all available space
/// or overflow it, every variant falls back to the closest position possible.
///
/// # Examples
///
/// ```rust
/// use aimer_flex::JustifyContent;
///
/// let start = JustifyContent::Start;
/// let centered = JustifyContent::Center;
/// let end = JustifyContent::End;
/// let between = JustifyContent::SpaceBetween;
/// let around = JustifyContent::SpaceAround;
/// let evenly = JustifyContent::SpaceEvenly;
/// ```
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    #[default]
    Hidden,
    Wrap,
    Visible,
}

impl PortableProperty for BoxAlignment {
    const REFLECTION: PortablePropertyReflection = PortablePropertyReflection::new(
        aimer_widget::portable::__anteros::PropertyValueKind::I64,
        PortablePropertyConversion::SignedInteger {
            minimum: 0,
            maximum: 2,
        },
    );
}

impl PortableProperty for JustifyContent {
    const REFLECTION: PortablePropertyReflection = PortablePropertyReflection::new(
        aimer_widget::portable::__anteros::PropertyValueKind::I64,
        PortablePropertyConversion::SignedInteger {
            minimum: 0,
            maximum: 5,
        },
    );
}

impl PortableProperty for OverflowBehavior {
    const REFLECTION: PortablePropertyReflection = PortablePropertyReflection::new(
        aimer_widget::portable::__anteros::PropertyValueKind::I64,
        PortablePropertyConversion::SignedInteger {
            minimum: 0,
            maximum: 2,
        },
    );
}

#[cfg(feature = "portable-guest")]
impl PortableEncodeProperty for BoxAlignment {
    #[inline]
    fn encode_property(
        self,
        _context: &mut PortableBuildContext,
    ) -> Result<aimer_widget::portable::__anteros::PropertyValue, PortableBuildError> {
        Ok(aimer_widget::portable::__anteros::PropertyValue::I64(match self {
            Self::Start => 0,
            Self::Center => 1,
            Self::End => 2,
        }))
    }
}

#[cfg(feature = "portable-guest")]
impl PortableEncodeProperty for JustifyContent {
    #[inline]
    fn encode_property(
        self,
        _context: &mut PortableBuildContext,
    ) -> Result<aimer_widget::portable::__anteros::PropertyValue, PortableBuildError> {
        Ok(aimer_widget::portable::__anteros::PropertyValue::I64(match self {
            Self::Start => 0,
            Self::Center => 1,
            Self::End => 2,
            Self::SpaceBetween => 3,
            Self::SpaceAround => 4,
            Self::SpaceEvenly => 5,
        }))
    }
}

#[cfg(feature = "portable-guest")]
impl PortableEncodeProperty for OverflowBehavior {
    #[inline]
    fn encode_property(
        self,
        _context: &mut PortableBuildContext,
    ) -> Result<aimer_widget::portable::__anteros::PropertyValue, PortableBuildError> {
        Ok(aimer_widget::portable::__anteros::PropertyValue::I64(match self {
            Self::Hidden => 0,
            Self::Wrap => 1,
            Self::Visible => 2,
        }))
    }
}

impl PortableMaterializeProperty for BoxAlignment {
    fn from_awir(
        _document: &aimer_widget::portable::__anteros::WidgetDocumentView<'_>,
        property: aimer_widget::portable::__anteros::PropertyId,
        value: aimer_widget::portable::__anteros::PropertyValue,
    ) -> Result<Self, PortableMaterializeError> {
        match value {
            aimer_widget::portable::__anteros::PropertyValue::I64(0) => Ok(Self::Start),
            aimer_widget::portable::__anteros::PropertyValue::I64(1) => Ok(Self::Center),
            aimer_widget::portable::__anteros::PropertyValue::I64(2) => Ok(Self::End),
            aimer_widget::portable::__anteros::PropertyValue::I64(_) => {
                Err(PortableMaterializeError::InvalidPropertyValue { property })
            }
            _ => Err(PortableMaterializeError::InvalidPropertyType { property }),
        }
    }
}

impl PortableMaterializeProperty for JustifyContent {
    fn from_awir(
        _document: &aimer_widget::portable::__anteros::WidgetDocumentView<'_>,
        property: aimer_widget::portable::__anteros::PropertyId,
        value: aimer_widget::portable::__anteros::PropertyValue,
    ) -> Result<Self, PortableMaterializeError> {
        match value {
            aimer_widget::portable::__anteros::PropertyValue::I64(0) => Ok(Self::Start),
            aimer_widget::portable::__anteros::PropertyValue::I64(1) => Ok(Self::Center),
            aimer_widget::portable::__anteros::PropertyValue::I64(2) => Ok(Self::End),
            aimer_widget::portable::__anteros::PropertyValue::I64(3) => Ok(Self::SpaceBetween),
            aimer_widget::portable::__anteros::PropertyValue::I64(4) => Ok(Self::SpaceAround),
            aimer_widget::portable::__anteros::PropertyValue::I64(5) => Ok(Self::SpaceEvenly),
            aimer_widget::portable::__anteros::PropertyValue::I64(_) => {
                Err(PortableMaterializeError::InvalidPropertyValue { property })
            }
            _ => Err(PortableMaterializeError::InvalidPropertyType { property }),
        }
    }
}

impl PortableMaterializeProperty for OverflowBehavior {
    fn from_awir(
        _document: &aimer_widget::portable::__anteros::WidgetDocumentView<'_>,
        property: aimer_widget::portable::__anteros::PropertyId,
        value: aimer_widget::portable::__anteros::PropertyValue,
    ) -> Result<Self, PortableMaterializeError> {
        match value {
            aimer_widget::portable::__anteros::PropertyValue::I64(0) => Ok(Self::Hidden),
            aimer_widget::portable::__anteros::PropertyValue::I64(1) => Ok(Self::Wrap),
            aimer_widget::portable::__anteros::PropertyValue::I64(2) => Ok(Self::Visible),
            aimer_widget::portable::__anteros::PropertyValue::I64(_) => {
                Err(PortableMaterializeError::InvalidPropertyValue { property })
            }
            _ => Err(PortableMaterializeError::InvalidPropertyType { property }),
        }
    }
}

impl OverflowBehavior {
    fn apply_overflow_behave(&self, ctx: &BuildContext) {
        match self {
            Self::Hidden => {
                ctx.canvas.set_clip(
                    Vec2d { x: 0.0, y: 0.0 },
                    ResolvedSize {
                        width: ctx.box_constraint.max_width,
                        height: ctx.box_constraint.max_height,
                    },
                );
            }
            Self::Wrap | Self::Visible => {}
        }
    }
}

#[cfg(all(test, feature = "portable-guest"))]
mod portable_layout_tests {
    use aimer_widget::base::BuildContext;
    use aimer_widget::portable::{
        PortableBuildContext, PortableLimits, PortableWidgetLimits, PortableWidgetSchema,
        SourceFingerprint, StableId128,
    };
    use aimer_widget::portable::__anteros::{
        PropertyValue, Version, WIDGET_SIZED_BOX, WidgetDocumentView,
    };
    use aimer_widget::{AnyElement, ErrorWidget, PortableWidget, Widget};

    use super::{Expanded, Flex, FlexDirection, ListFlex};

    struct Leaf;

    impl Widget for Leaf {
        fn to_element(self, ctx: &BuildContext) -> AnyElement {
            ErrorWidget::new("portable leaf").to_element(ctx)
        }
    }

    impl PortableWidget for Leaf {
        fn to_portable_node(
            self,
            ctx: &mut PortableBuildContext,
            source: SourceFingerprint,
        ) -> Result<aimer_widget::portable::PortableNodeId, aimer_widget::portable::PortableBuildError>
        {
            ctx.push_node(
                WIDGET_SIZED_BOX,
                Version::new(1, 0),
                None,
                source,
                &[],
                &[],
            )
        }
    }

    fn context() -> PortableBuildContext {
        PortableBuildContext::new(
            1,
            1,
            PortableWidgetLimits::new(32, 32, 32, 32, 1_024, 8_192),
            PortableLimits::new(8, 16, 64, 128, 1_024),
        )
        .unwrap()
    }

    fn source() -> SourceFingerprint {
        SourceFingerprint::new(StableId128::from_bytes([0x21; 16]))
    }

    fn assert_lowered<W>(widget: W, expected_children: usize, expected_properties: usize)
    where
        W: Widget + PortableWidget + PortableWidgetSchema,
    {
        let mut ctx = context();
        let root = widget.to_portable_node(&mut ctx, source()).unwrap();
        let document = ctx.finish_document(root).unwrap();
        let bytes = document.encode().unwrap();
        let view = WidgetDocumentView::decode(&bytes, document.model_limits()).unwrap();
        let node = view.node(root.index()).unwrap();
        assert_eq!(node.widget_type(), W::SCHEMA.widget().id());
        assert_eq!(node.properties().count(), expected_properties);
        assert_eq!(node.children().count(), expected_children);
    }

    #[test]
    fn expanded_lowers_flex_property_and_required_child() {
        let mut ctx = context();
        let root = Expanded::new().flex(2.5).child(Leaf).to_portable_node(&mut ctx, source()).unwrap();
        let document = ctx.finish_document(root).unwrap();
        let bytes = document.encode().unwrap();
        let view = WidgetDocumentView::decode(&bytes, document.model_limits()).unwrap();
        let node = view.node(root.index()).unwrap();
        assert_eq!(node.widget_type(), <Expanded<Leaf> as PortableWidgetSchema>::SCHEMA.widget().id());
        assert_eq!(node.properties().next().unwrap().value(), PropertyValue::F64(2.5));
        assert_eq!(node.children().count(), 1);
    }

    #[test]
    fn flex_lowers_collection_children_without_native_construction() {
        assert_lowered(Flex::new().direction(FlexDirection::Row).children([Leaf]), 1, 0);
    }

    #[test]
    fn list_flex_lowers_data_builder_children() {
        let list: ListFlex<u8, _> = super::Column::new()
            .list([1_u8, 2])
            .builder(|_| Leaf);
        assert_lowered(list, 2, 0);
    }
}
