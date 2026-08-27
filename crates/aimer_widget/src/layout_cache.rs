use std::cell::UnsafeCell;

use aimer_attribute::BoxConstraint;
use aimer_attribute::size::ResolvedSize;

use crate::components::element::{element_tree_generation, layout_invalidation_generation};

/// One memoized measurement together with everything that decides whether it
/// still describes the element.
#[derive(Clone, Copy)]
struct Measurement {
    constraint: BoxConstraint,
    scale_bits: u32,
    tree_generation: u64,
    layout_generation: u64,
    size: ResolvedSize,
}

impl Measurement {
    /// `true` when the measurement was taken under `constraint` and
    /// `scale_bits`, and nothing has replaced a generated subtree since.
    #[inline]
    fn describes(
        &self,
        constraint: BoxConstraint,
        scale_bits: u32,
        tree_generation: u64,
        layout_generation: u64,
    ) -> bool {
        self.constraint == constraint
            && self.scale_bits == scale_bits
            && self.tree_generation == tree_generation
            && self.layout_generation == layout_generation
    }
}

/// Caches the result of `computed_size` and `content_size` between frames.
///
/// The cache is keyed by `(BoxConstraint, scale)` so that if the same element
/// is queried multiple times with the same inputs, the result is returned
/// instantly. yeah it saves the CPU and GPU and reduces power consuming :))
///
/// The key also carries the [element-tree
/// generation](crate::element_tree_generation) and layout-invalidation
/// generation the measurement was taken in. An element measures its children,
/// so a measurement only describes the element for as long as the subtree below
/// it is the one that was measured. Replacing a generated subtree — a
/// `setState`, or an
/// [`AsyncBuilder`](crate::AsyncBuilder) swapping its loading state for the data
/// it waited on — advances that generation, which retires every measurement
/// taken before it. Without this an ancestor that itself never rebuilt, such as
/// the `Container` between a `Scrollable` and its content, would keep handing
/// out the height the content had while it was still loading, and the scroll
/// view would decide there is nothing to scroll.
///
/// The tree generation only moves when the element tree changes shape, while
/// the layout generation moves when a caller explicitly invalidates layout. A
/// frame that merely animates or scrolls still reads straight from the cache.
///
/// # Examples
///
/// ```
/// use aimer_attribute::BoxConstraint;
/// use aimer_attribute::size::ResolvedSize;
/// use aimer_widget::LayoutCache;
///
/// let cache = LayoutCache::new();
/// let constraint = BoxConstraint::new();
/// let scale_bits = 1.0_f32.to_bits();
/// let size = ResolvedSize { width: 100.0, height: 40.0 };
///
/// cache.set_computed(constraint, scale_bits, size);
///
/// assert_eq!(cache.get_computed(constraint, scale_bits), Some(size));
/// ```
pub struct LayoutCache {
    computed: UnsafeCell<Option<Measurement>>,
    content: UnsafeCell<Option<Measurement>>,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self {
            computed: UnsafeCell::new(None),
            content: UnsafeCell::new(None),
        }
    }

    /// Returns cached computed_size if constraint, scale, element tree, and
    /// layout invalidation generation are unchanged, otherwise None.
    pub fn get_computed(&self, constraint: BoxConstraint, scale_bits: u32) -> Option<ResolvedSize> {
        let guard = unsafe { &*self.computed.get() };
        Self::read(guard, constraint, scale_bits)
    }

    /// Stores computed_size result.
    pub fn set_computed(&self, constraint: BoxConstraint, scale_bits: u32, size: ResolvedSize) {
        let guard = unsafe { &mut *self.computed.get() };
        *guard = Some(Self::measurement(constraint, scale_bits, size));
    }

    /// Returns cached content_size if constraint, scale, element tree, and
    /// layout invalidation generation are unchanged, otherwise None.
    pub fn get_content(&self, constraint: BoxConstraint, scale_bits: u32) -> Option<ResolvedSize> {
        let guard = unsafe { &*self.content.get() };
        Self::read(guard, constraint, scale_bits)
    }

    /// Stores content_size result.
    pub fn set_content(&self, constraint: BoxConstraint, scale_bits: u32, size: ResolvedSize) {
        let guard = unsafe { &mut *self.content.get() };
        *guard = Some(Self::measurement(constraint, scale_bits, size));
    }

    /// Clears all cached values (call at the start of each frame).
    pub fn invalidate(&self) {
        unsafe {
            *self.computed.get() = None;
            *self.content.get() = None;
        }
    }

    #[inline]
    fn measurement(
        constraint: BoxConstraint,
        scale_bits: u32,
        size: ResolvedSize,
    ) -> Measurement {
        Measurement {
            constraint,
            scale_bits,
            tree_generation: element_tree_generation(),
            layout_generation: layout_invalidation_generation(),
            size,
        }
    }

    #[inline]
    fn read(
        slot: &Option<Measurement>,
        constraint: BoxConstraint,
        scale_bits: u32,
    ) -> Option<ResolvedSize> {
        let measurement = slot.as_ref()?;
        measurement
            .describes(
                constraint,
                scale_bits,
                element_tree_generation(),
                layout_invalidation_generation(),
            )
            .then_some(measurement.size)
    }
}

impl Default for LayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::BuildContext;
    use crate::components::element::reconcile_generated_tree;
    use crate::{Drawable, EventElement, LayoutElement, Rebuildable, VisitorElement};

    struct Leaf;

    impl VisitorElement for Leaf {
        fn debug_name(&self) -> &'static str {
            "Leaf"
        }
    }

    impl Drawable for Leaf {
        fn draw(&self, _ctx: &BuildContext) {}
    }

    impl EventElement for Leaf {}
    impl LayoutElement for Leaf {}
    impl Rebuildable for Leaf {}

    fn size(height: f32) -> ResolvedSize {
        ResolvedSize {
            width: 100.0,
            height,
        }
    }

    #[test]
    fn a_measurement_taken_under_other_inputs_is_not_reused() {
        let cache = LayoutCache::new();
        let constraint = BoxConstraint::new();
        let wider = BoxConstraint {
            max_width: 200.0,
            ..constraint
        };
        cache.set_computed(constraint, 1.0_f32.to_bits(), size(40.0));

        assert_eq!(cache.get_computed(wider, 1.0_f32.to_bits()), None);
        assert_eq!(cache.get_computed(constraint, 2.0_f32.to_bits()), None);
    }

    #[test]
    fn replacing_a_generated_subtree_retires_a_cached_measurement() {
        let cache = LayoutCache::new();
        let constraint = BoxConstraint::new();
        let scale_bits = 1.0_f32.to_bits();
        cache.set_computed(constraint, scale_bits, size(40.0));
        cache.set_content(constraint, scale_bits, size(40.0));

        // What an `AsyncBuilder` does when the future it waited on completes.
        reconcile_generated_tree(&Leaf, &Leaf);

        assert_eq!(cache.get_computed(constraint, scale_bits), None);
        assert_eq!(cache.get_content(constraint, scale_bits), None);
    }

    #[test]
    fn layout_invalidation_retires_a_cached_measurement() {
        let cache = LayoutCache::new();
        let constraint = BoxConstraint::new();
        let scale_bits = 1.0_f32.to_bits();
        cache.set_computed(constraint, scale_bits, size(40.0));
        assert_eq!(cache.get_computed(constraint, scale_bits), Some(size(40.0)));

        crate::components::element::advance_layout_invalidation_generation();

        assert_eq!(cache.get_computed(constraint, scale_bits), None);
    }
}
