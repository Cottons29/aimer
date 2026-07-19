use aimer_attribute::size::{ResolvedSize, Size};
use aimer_macro::{EventElement, Rebuildable};
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyWidget, Drawable, Element, LayoutElement, RequiredChild, VisitorElement, Widget,
};

use crate::ZeroSizedBox;

/// A flex child that fills the remaining main-axis space inside a flex
/// container (`Row`, `Column`, `Flex`), mirroring Flutter's `Expanded` widget.
///
/// The `flex` factor controls how the free space of the flex container is
/// shared between the expanding children:
///
/// - In a `Row` with a single `Expanded`, the child fills the whole width.
/// - In a `Row` with two `Expanded` children (both `flex = 1`), each child gets
///   half of the width.
/// - In a `Row` with two `Expanded` children of `flex = 1` and `flex = 2`, the
///   first child gets `1 / (1 + 2)` and the second `2 / (1 + 2)` of the free
///   space.
///
/// Attach a child with [`Expanded::child`] to retain its concrete type, or with
/// [`Expanded::box_child`] when branches need to return the same erased type.
///
/// # Example
///
/// ```rust
/// use aimer_container::SizedBox;
/// use aimer_container::flex::{Expanded, Row};
///
/// let row = Row::new()
///     .children(vec![
///         Expanded::new().child(SizedBox::new()),
///         Expanded::new().flex(2.0).child(SizedBox::new()),
///     ]);
/// ```
pub struct Expanded<W = RequiredChild> {
    /// The flex factor: the child's share of the free main-axis space is
    /// `flex / sum_of_all_flex_factors`. Defaults to `1.0`.
    flex: f32,
    /// The widget that expands to fill the assigned space.
    child: W,
}

impl Default for Expanded {
    fn default() -> Self {
        Self::new()
    }
}

impl Expanded {
    /// Creates an expanding child with a flex factor of `1.0`.
    ///
    /// Finish the builder with [`Expanded::child`] or [`Expanded::box_child`].
    pub fn new() -> Self {
        Self { flex: 1.0, child: RequiredChild }
    }

    /// Sets this child's weight when a flex parent distributes remaining space.
    ///
    /// The default is `1.0`. At element construction, negative values are
    /// clamped to `0.0`; a zero-weight child receives no share of the remaining
    /// main-axis space.
    pub fn flex(mut self, flex: f32) -> Self {
        self.flex = flex;
        self
    }
    /// Attaches the required child and makes the builder a valid [`Widget`].
    ///
    /// This terminal operation preserves the child's concrete type. Use
    /// [`Expanded::box_child`] instead when branch type erasure is needed.
    pub fn child<W: Widget + 'static>(self, child: W) -> Expanded<W> {
        Expanded { child, flex: self.flex }
    }

    /// Attaches `child` and erases the resulting widget's concrete type.
    ///
    /// This is equivalent to calling [`Expanded::child`] followed by
    /// [`Widget::boxed`]. Use it when different branches must return one
    /// [`AnyWidget`] type.
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child)
            .boxed()
    }
}

impl<W: Widget + 'static> Widget for Expanded<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        Box::new(RawExpanded {
            child: self
                .child
                .to_element(ctx),
            flex: self
                .flex
                .max(0.0),
            debug_name: "Expanded",
        })
    }

    fn debug_name(&self) -> &'static str {
        "Expanded"
    }
}

/// Lower level element backing [`Expanded`].
///
/// It carries a `flex` factor that its flex parent (`RawFlex`) reads through
/// [`LayoutElement::flex`] to distribute the remaining main-axis space. On
/// layout it simply fills whatever bounded constraint the parent hands it and
/// delegates painting to its child.
#[derive(Rebuildable, EventElement)]
pub struct RawExpanded<E: Element> {
    pub(crate) child: E,
    pub(crate) flex: f32,
    pub(crate) debug_name: &'static str,
}

impl<E: Element> RawExpanded<E> {
    /// Constraints at or above this value are treated as unbounded (the same
    /// threshold `Container` uses), in which case there is no "remaining space"
    /// to fill and the element falls back to its child's intrinsic size.
    const UNBOUNDED: f32 = 1_000_000.0;
}

impl<E: Element> Drawable for RawExpanded<E> {
    fn draw(&self, ctx: &BuildContext) {
        self.child
            .draw(ctx);
    }
}

impl<E: Element> VisitorElement for RawExpanded<E> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}

impl<E: Element> LayoutElement for RawExpanded<E> {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let child = self
            .child
            .computed_size(ctx);
        let max_w = ctx
            .box_constraint
            .max_width;
        let max_h = ctx
            .box_constraint
            .max_height;

        // Fill every bounded axis; fall back to the child on unbounded axes
        // (e.g. inside a `Scrollable`) where there is no space to expand into.
        let width = if max_w < Self::UNBOUNDED { max_w } else { child.width };
        let height = if max_h < Self::UNBOUNDED { max_h } else { child.height };

        ResolvedSize { width, height }
    }

    fn flex(&self) -> Option<f32> {
        Some(self.flex)
    }

    /// An `Expanded` has no intrinsic main-axis size of its own — it is sized
    /// by its flex parent — so it must not report a fixed size to
    /// ancestors.
    fn get_size_from_child(&self) -> Option<Size> {
        None
    }

    fn invalidate_layout(&self) {
        self.child
            .invalidate_layout();
    }
}
/// Distribute `remaining` main-axis space across children according to their
/// flex `weights`.
///
/// `weights[i]` is the flex factor of child `i`, or `0.0` for a non-flex
/// (regular) child. The returned vector has the same length: each flex child
/// receives `remaining * weight / total_weight`, and every non-flex child
/// receives `0.0`. When no child is flexible (all weights `<= 0`) the result is
/// all zeros.
pub(crate) fn distribute_flex_space(remaining: f32, weights: &[f32]) -> Vec<f32> {
    let total: f32 = weights
        .iter()
        .copied()
        .filter(|w| *w > 0.0)
        .sum();
    if total <= 0.0 {
        return vec![0.0; weights.len()];
    }
    weights
        .iter()
        .map(|&w| if w > 0.0 { remaining * (w / total) } else { 0.0 })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_flex_child_fills_all_remaining_space() {
        // A single Expanded in a Row: it fills the whole main axis.
        let shares = distribute_flex_space(300.0, &[1.0]);
        assert_eq!(shares, vec![300.0]);
    }

    #[test]
    fn two_equal_flex_children_split_evenly() {
        // Two Expanded children, both flex = 1 => each gets parent / 2.
        let shares = distribute_flex_space(300.0, &[1.0, 1.0]);
        assert_eq!(shares, vec![150.0, 150.0]);
    }

    #[test]
    fn weighted_flex_children_split_proportionally() {
        // flex = 1 and flex = 2 => 1/3 and 2/3 of the free space.
        let shares = distribute_flex_space(300.0, &[1.0, 2.0]);
        assert_eq!(shares, vec![100.0, 200.0]);
    }

    #[test]
    fn non_flex_children_receive_no_space() {
        // A sized (non-flex) child in the middle gets nothing; the flex
        // children share everything.
        let shares = distribute_flex_space(300.0, &[1.0, 0.0, 2.0]);
        assert_eq!(shares, vec![100.0, 0.0, 200.0]);
    }

    #[test]
    fn no_flex_children_yields_zeros() {
        let shares = distribute_flex_space(300.0, &[0.0, 0.0]);
        assert_eq!(shares, vec![0.0, 0.0]);
    }
}
