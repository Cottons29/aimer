use std::cell::UnsafeCell;
use std::rc::Rc;

use aimer_attribute::BoxConstraint;
use aimer_attribute::position::Vec2d;
use aimer_macro::{LayoutElement, Rebuildable};
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, LayoutElement, VisitorElement, Widget,
};

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum StackDirection {
    #[default]
    Normal,
    Reverse,
    Inherit,
}
/// Paints children on top of one another in the same constrained area.
///
/// Every child receives the stack's content size and constraints. Before
/// painting, children are sorted by their [`Widget`] element layer; the default
/// [`StackDirection::Normal`] paints lower layers first, while
/// [`StackDirection::Reverse`] reverses that order. `Inherit` currently behaves
/// like `Normal`.
///
/// `Stack::new()` is an empty, valid widget. [`Stack::children`] replaces the
/// collection with homogeneous values, while [`Stack::add_child`] appends and
/// boxes values so different concrete widget types can be mixed.
///
/// # Example
///
/// ```rust
/// use aimer_container::SizedBox;
/// use aimer_space::{Align, Alignment, Stack};
///
/// let stack = Stack::new().add_child(SizedBox::new().width(200).height(120))
///                         .add_child(Align::new().alignment(Alignment::MidCenter)
///                                                .child(SizedBox::new().width(40).height(40)));
/// ```
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(id = "aimer_space::space::Stack", schema_only)]
pub struct Stack<W = AnyWidget> {
    #[portable_children]
    pub children: Vec<W>,
    #[portable_skip]
    pub direction: StackDirection,
}

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "illumos",
))]
#[aimer_widget::portable::__linkme::distributed_slice(
    aimer_widget::portable::materializer::PORTABLE_NATIVE_WIDGET_SCHEMAS
)]
#[linkme(crate = aimer_widget::portable::__linkme)]
#[allow(non_upper_case_globals)]
static __AIMER_PORTABLE_NATIVE_SCHEMA_FOR_STACK:
    aimer_widget::portable::__anteros::PortableWidgetSchemaMetadata<'static> =
    <Stack as aimer_widget::portable::PortableWidgetSchema>::SCHEMA;

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

impl Stack {
    /// Creates an empty stack in [`StackDirection::Normal`] painting order.
    ///
    /// The empty stack is already a valid [`Widget`].
    #[inline]
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            direction: StackDirection::default(),
        }
    }

    /// Replaces all children with a homogeneous collection.
    ///
    /// This is not an append operation. The returned [`Stack`] adopts the
    /// iterator's item type; callers that need it to satisfy the current
    /// concrete [`Widget`] implementation should supply erased [`AnyWidget`]
    /// values, or use [`Stack::add_child`] instead.
    #[inline]
    pub fn children<W: Widget>(self, children: impl IntoIterator<Item = W>) -> Stack<W> {
        Stack {
            children: children.into_iter().collect(),
            direction: self.direction,
        }
    }

    /// Appends a child, boxing it into the stack's erased collection.
    ///
    /// Existing children are retained, and successive calls may use different
    /// concrete widget types.
    #[inline]
    pub fn add_child(mut self, child: impl Widget + 'static) -> Self {
        self.children.push(child.boxed());
        self
    }

    /// Sets the layer-sorted painting order.
    ///
    /// The default is [`StackDirection::Normal`]. Reverse order affects
    /// painting only; it does not change layout constraints or child storage.
    #[inline]
    pub fn direction(mut self, direction: StackDirection) -> Self {
        self.direction = direction;
        self
    }
}

impl Widget for Stack {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let children = self.children.into_iter().map(|c| c.to_element(ctx)).collect();
        RawStackElement {
            children,
            direction: self.direction,
            paint_order: UnsafeCell::new(None),
            hit_test_index: UnsafeCell::new(None),
        }
        .boxed()
    }
}

struct CachedChildOrder {
    generation: u64,
    child_count: usize,
    order: Rc<[usize]>,
}

const HIT_TEST_BIN_COUNT: usize = 64;

struct CachedHitTestIndex {
    generation: u64,
    child_count: usize,
    min_y: f32,
    max_y: f32,
    bin_height: f32,
    dense: bool,
    offsets: Rc<[usize]>,
    children: Rc<[usize]>,
    outside: Rc<[usize]>,
}

impl CachedHitTestIndex {
    #[inline]
    fn candidates(&self, y: f32) -> Option<&[usize]> {
        if self.dense || !y.is_finite() {
            return None;
        }

        let candidates = if y < self.min_y || y > self.max_y {
            self.outside.as_ref()
        } else {
            let bin = ((y - self.min_y) / self.bin_height)
                .floor()
                .clamp(0.0, (HIT_TEST_BIN_COUNT - 1) as f32)
                as usize;
            let start = self.offsets[bin];
            let end = self.offsets[bin + 1];
            &self.children[start..end]
        };

        (candidates.len().saturating_mul(2) <= self.child_count).then_some(candidates)
    }
}

#[derive(Rebuildable, LayoutElement)]
pub struct RawStackElement {
    pub children: Vec<AnyElement>,
    pub direction: StackDirection,
    paint_order: UnsafeCell<Option<CachedChildOrder>>,
    hit_test_index: UnsafeCell<Option<CachedHitTestIndex>>,
}

impl RawStackElement {
    /// Returns child indices in ascending layer order.
    ///
    /// Layer is structural element state, so the element-tree generation is
    /// enough to retire this order when a generated child subtree changes.
    #[inline]
    fn sorted_child_indices(&self) -> Rc<[usize]> {
        let generation = aimer_widget::element_tree_generation();
        let cached = unsafe { &*self.paint_order.get() };
        if let Some(cached) = cached.as_ref()
            && cached.generation == generation
            && cached.child_count == self.children.len()
        {
            return Rc::clone(&cached.order);
        }

        let mut order: Vec<_> = (0..self.children.len()).collect();
        order.sort_by_key(|&index| self.children[index].layer());
        let order = Rc::from(order.into_boxed_slice());

        let slot = unsafe { &mut *self.paint_order.get() };
        *slot = Some(CachedChildOrder {
            generation,
            child_count: self.children.len(),
            order: Rc::clone(&order),
        });
        order
    }

    /// Retires y-range candidates after children have redrawn their retained
    /// bounds. The GUI tree is single-threaded, so replacing this cache does
    /// not race with an event traversal.
    #[inline]
    fn invalidate_hit_test_index(&self) {
        unsafe {
            *self.hit_test_index.get() = None;
        }
    }

    /// Builds a coarse retained y index in painted topmost-first order.
    ///
    /// A y-only index is intentional: x is still validated by the normal
    /// bounded descent at the child boundary, while the stack avoids scanning
    /// siblings that are vertically disjoint from the pointer. Unknown bounds
    /// remain candidates in every bin, and dense bins use the exact fallback.
    #[inline]
    fn ensure_hit_test_index(&self) {
        let generation = aimer_widget::element_tree_generation();
        let child_count = self.children.len();
        let cached = unsafe { &*self.hit_test_index.get() };
        if let Some(cached) = cached.as_ref()
            && cached.generation == generation
            && cached.child_count == child_count
        {
            return;
        }

        let sorted = self.sorted_child_indices();
        let mut bounds_by_child = vec![None; child_count];
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut known_count = 0;

        for &index in sorted.iter() {
            let bounds = self.children[index]
                .pos_start_end()
                .filter(|(start, end)| {
                    start.y.is_finite() && end.y.is_finite() && start.y <= end.y
                })
                .map(|(start, end)| (start.y, end.y));
            if let Some((start_y, end_y)) = bounds {
                min_y = min_y.min(start_y);
                max_y = max_y.max(end_y);
                known_count += 1;
            }
            bounds_by_child[index] = bounds;
        }

        let span = max_y - min_y;
        let span_is_indexable = known_count > 0 && span.is_finite();
        let bin_height = if span_is_indexable && span > 0.0 {
            span / HIT_TEST_BIN_COUNT as f32
        } else {
            1.0
        };
        let bin_range = |start_y: f32, end_y: f32| {
            let first_bin = ((start_y - min_y) / bin_height)
                .floor()
                .clamp(0.0, (HIT_TEST_BIN_COUNT - 1) as f32)
                as usize;
            let last_bin = ((end_y - min_y) / bin_height)
                .floor()
                .clamp(0.0, (HIT_TEST_BIN_COUNT - 1) as f32)
                as usize;
            (first_bin, last_bin)
        };
        let mut estimated_entries = 0usize;
        if span_is_indexable {
            for bounds in &bounds_by_child {
                let entries = bounds.map_or(HIT_TEST_BIN_COUNT, |(start_y, end_y)| {
                    let (first_bin, last_bin) = bin_range(start_y, end_y);
                    last_bin - first_bin + 1
                });
                estimated_entries = estimated_entries.saturating_add(entries);
            }
        }
        let dense = !span_is_indexable
            || estimated_entries > child_count.saturating_mul(4);
        let (offsets, indexed_children, outside): (Vec<usize>, Vec<usize>, Vec<usize>) =
            if dense {
                (vec![0; HIT_TEST_BIN_COUNT + 1], Vec::new(), Vec::new())
            } else {
                let mut bins: Vec<Vec<usize>> = (0..HIT_TEST_BIN_COUNT)
                    .map(|_| Vec::new())
                    .collect();
                let mut outside = Vec::new();

                for &index in sorted.iter().rev() {
                    if let Some((start_y, end_y)) = bounds_by_child[index] {
                        let (first_bin, last_bin) = bin_range(start_y, end_y);
                        for bin in first_bin..=last_bin {
                            bins[bin].push(index);
                        }
                    } else {
                        outside.push(index);
                        for bin in &mut bins {
                            bin.push(index);
                        }
                    }
                }

                let mut offsets = Vec::with_capacity(HIT_TEST_BIN_COUNT + 1);
                let mut indexed_children = Vec::with_capacity(estimated_entries);
                offsets.push(0);
                for bin in bins {
                    indexed_children.extend(bin);
                    offsets.push(indexed_children.len());
                }
                (offsets, indexed_children, outside)
            };

        let slot = unsafe { &mut *self.hit_test_index.get() };
        *slot = Some(CachedHitTestIndex {
            generation,
            child_count,
            min_y: if span_is_indexable { min_y } else { 0.0 },
            max_y: if span_is_indexable { max_y } else { 0.0 },
            bin_height,
            dense,
            offsets: Rc::from(offsets.into_boxed_slice()),
            children: Rc::from(indexed_children.into_boxed_slice()),
            outside: Rc::from(outside.into_boxed_slice()),
        });
    }

    #[inline]
    fn visit_indexed<'a>(
        &'a self,
        pos: Vec2d,
        reversed: bool,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.ensure_hit_test_index();
        let candidates = unsafe {
            (&*self.hit_test_index.get())
                .as_ref()
                .and_then(|index| index.candidates(pos.y))
        };
        if let Some(candidates) = candidates {
            if reversed {
                for &index in candidates {
                    visitor(self.children[index].as_ref());
                }
            } else {
                for &index in candidates.iter().rev() {
                    visitor(self.children[index].as_ref());
                }
            }
            return;
        }

        let sorted = self.sorted_child_indices();
        if reversed {
            for &index in sorted.iter().rev() {
                visitor(self.children[index].as_ref());
            }
        } else {
            for &index in sorted.iter() {
                visitor(self.children[index].as_ref());
            }
        }
    }
}

impl Drawable for RawStackElement {
    fn draw(&self, ctx: &BuildContext) {
        self.invalidate_hit_test_index();
        let content_size = self.content_size(ctx);
        let child_ctx = BuildContext {
            parent_size: content_size,
            canvas: ctx.canvas.clone(),
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: content_size.width,
                max_height: content_size.height,
            },
            visible_rect: ctx.visible_rect,
            window: ctx.window.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
        };

        let sorted_children = self.sorted_child_indices();

        if self.direction == StackDirection::Reverse {
            for &index in sorted_children.iter().rev() {
                self.children[index].draw(&child_ctx);
            }
        } else {
            for &index in sorted_children.iter() {
                self.children[index].draw(&child_ctx);
            }
        }
    }
}

impl VisitorElement for RawStackElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        for child in &self.children {
            visitor(child.as_ref());
        }
    }

    fn debug_name(&self) -> &'static str {
        "RawStackElement"
    }
}

impl EventElement for RawStackElement {
    /// The structural order is the retained child order; layer sorting is only
    /// a paint and hit-test concern.
    #[inline]
    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        for child in &self.children {
            visitor(child.as_ref());
        }
    }

    /// Offer the topmost layer first, matching paint order.
    ///
    /// Position-based dispatch walks the child list in reverse, so visiting
    /// children in ascending layer order makes the highest layer answer a press
    /// before anything painted beneath it. Without this a full-area bottom
    /// layer — a `Scrollable` under a floating `Align`, say — swallows the press
    /// aimed at the button above it.
    fn hit_test_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        let sorted = self.sorted_child_indices();
        for &index in sorted.iter() {
            visitor(self.children[index].as_ref());
        }
    }

    /// Visits the painted order in the order pointer routing consumes it,
    /// avoiding a temporary reverse-order sibling buffer.
    #[inline]
    fn hit_test_children_reversed<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        let sorted = self.sorted_child_indices();
        for &index in sorted.iter().rev() {
            visitor(self.children[index].as_ref());
        }
    }

    /// Uses the retained y index when it can reject enough siblings; otherwise
    /// preserves the exact painted traversal.
    #[inline]
    fn hit_test_children_at<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.visit_indexed(pos, false, visitor);
    }

    /// Visits indexed candidates in topmost-first order for pointer routing.
    #[inline]
    fn hit_test_children_at_reversed<'a>(
        &'a self,
        pos: Vec2d,
        visitor: &mut dyn FnMut(&'a dyn Element),
    ) {
        self.visit_indexed(pos, true, visitor);
    }

}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;
    use aimer_widget::{Drawable, EventElement, LayoutElement, Rebuildable, VisitorElement};

    struct LayeredElement(u32);

    impl Drawable for LayeredElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl VisitorElement for LayeredElement {
        fn debug_name(&self) -> &'static str {
            "LayeredElement"
        }
    }

    impl EventElement for LayeredElement {}

    impl LayoutElement for LayeredElement {
        fn layer(&self) -> u32 {
            self.0
        }
    }

    impl Rebuildable for LayeredElement {}

    struct BoundedElement {
        layer: u32,
        name: &'static str,
        bounds: Rc<Cell<Option<(
            aimer_attribute::position::Vec2d,
            aimer_attribute::position::Vec2d,
        )>>>,
    }

    impl Drawable for BoundedElement {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl VisitorElement for BoundedElement {
        fn debug_name(&self) -> &'static str {
            self.name
        }
    }

    impl EventElement for BoundedElement {}

    impl LayoutElement for BoundedElement {
        fn layer(&self) -> u32 {
            self.layer
        }

        fn pos_start_end(
            &self,
        ) -> Option<(
            aimer_attribute::position::Vec2d,
            aimer_attribute::position::Vec2d,
        )> {
            self.bounds.get()
        }
    }

    impl Rebuildable for BoundedElement {}

    #[test]
    fn cached_child_order_is_stable_and_layer_sorted() {
        let stack = RawStackElement {
            children: vec![
                LayeredElement(3).boxed(),
                LayeredElement(1).boxed(),
                LayeredElement(2).boxed(),
            ],
            direction: StackDirection::Normal,
            paint_order: UnsafeCell::new(None),
            hit_test_index: UnsafeCell::new(None),
        };

        let first = stack.sorted_child_indices();
        assert_eq!(first.as_ref(), &[1, 2, 0]);

        let second = stack.sorted_child_indices();
        assert!(Rc::ptr_eq(&first, &second));
    }

    #[test]
    fn indexed_hit_test_keeps_topmost_order_and_unknown_bounds() {
        let mut children = vec![
            BoundedElement {
                layer: 1,
                name: "outside",
                    bounds: Rc::new(Cell::new(Some((
                        aimer_attribute::position::Vec2d { x: 20.0, y: 20.0 },
                        aimer_attribute::position::Vec2d { x: 30.0, y: 30.0 },
                    )))),
            }
            .boxed(),
            BoundedElement {
                layer: 3,
                name: "unknown",
                    bounds: Rc::new(Cell::new(None)),
            }
            .boxed(),
            BoundedElement {
                layer: 2,
                name: "inside",
                bounds: Rc::new(Cell::new(Some((
                    aimer_attribute::position::Vec2d { x: 0.0, y: 0.0 },
                    aimer_attribute::position::Vec2d { x: 10.0, y: 10.0 },
                )))),
            }
            .boxed(),
        ];
        for layer in 4..33 {
            children.push(
                BoundedElement {
                    layer,
                    name: "filler",
                    bounds: Rc::new(Cell::new(Some((
                        aimer_attribute::position::Vec2d { x: 0.0, y: 100.0 },
                        aimer_attribute::position::Vec2d { x: 10.0, y: 101.0 },
                    )))),
                }
                .boxed(),
            );
        }

        let stack = RawStackElement {
            children,
            direction: StackDirection::Normal,
            paint_order: UnsafeCell::new(None),
            hit_test_index: UnsafeCell::new(None),
        };

        stack.ensure_hit_test_index();
        let cached = unsafe { &*stack.hit_test_index.get() };
        assert!(!cached.as_ref().expect("index should be built").dense);

        let mut names = Vec::new();
        stack.hit_test_children_at_reversed(Vec2d { x: 5.0, y: 5.0 }, &mut |child| {
            names.push(child.debug_name())
        });

        assert_eq!(names, ["unknown", "inside"]);
    }

    #[test]
    fn hit_test_index_rebuilds_after_bounds_invalidation() {
        let bounds = Rc::new(Cell::new(Some((
            Vec2d { x: 0.0, y: 0.0 },
            Vec2d { x: 10.0, y: 10.0 },
        ))));
        let mut children = vec![
            BoundedElement {
                layer: 1,
                name: "child",
                bounds: Rc::clone(&bounds),
            }
            .boxed(),
        ];
        for layer in 2..33 {
            children.push(
                BoundedElement {
                    layer,
                    name: "filler",
                    bounds: Rc::new(Cell::new(Some((
                        Vec2d { x: 0.0, y: 100.0 },
                        Vec2d { x: 10.0, y: 101.0 },
                    )))),
                }
                .boxed(),
            );
        }
        let stack = RawStackElement {
            children,
            direction: StackDirection::Normal,
            paint_order: UnsafeCell::new(None),
            hit_test_index: UnsafeCell::new(None),
        };

        let mut names = Vec::new();
        stack.hit_test_children_at_reversed(Vec2d { x: 5.0, y: 5.0 }, &mut |child| {
            names.push(child.debug_name())
        });
        assert_eq!(names, ["child"]);

        bounds.set(Some((
            Vec2d { x: 20.0, y: 20.0 },
            Vec2d { x: 30.0, y: 30.0 },
        )));
        assert_eq!(
            stack.children[0].pos_start_end(),
            Some((
                Vec2d { x: 20.0, y: 20.0 },
                Vec2d { x: 30.0, y: 30.0 },
            ))
        );
        stack.invalidate_hit_test_index();
        stack.ensure_hit_test_index();
        let cached = unsafe { &*stack.hit_test_index.get() };
        assert_eq!(
            (cached.as_ref().unwrap().min_y, cached.as_ref().unwrap().max_y),
            (20.0, 101.0)
        );
        names.clear();
        stack.hit_test_children_at_reversed(Vec2d { x: 5.0, y: 5.0 }, &mut |child| {
            names.push(child.debug_name())
        });
        assert!(names.is_empty());
    }

}
