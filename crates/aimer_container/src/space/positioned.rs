use aimer_attribute::BoxConstraint;
use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyWidget, Drawable, Element, EventElement, LayoutElement, Rebuildable, VisitorElement, Widget,
};

use crate::ZeroSizedBox;

#[allow(dead_code)]
/// Positions and transforms one child relative to its parent, typically a [`crate::Stack`].
///
/// Attach a child with [`Positioned::child`] to retain its concrete type, or
/// with [`Positioned::box_child`] when branches need a shared erased type.
pub struct Positioned<W: Widget + 'static = ZeroSizedBox> {
    pub child: W,
    pub position: Position,
    pub left: Dimension,
    pub top: Dimension,
    pub right: Dimension,
    pub bottom: Dimension,
    pub transform: Transform,
    pub layer: u32,
}

impl Default for Positioned {
    fn default() -> Self {
        Self::new()
    }
}

impl Positioned {
    /// Creates a relative, untransformed positioned widget on layer zero.
    ///
    /// The placeholder is already a valid widget; use [`Positioned::child`] or
    /// [`Positioned::box_child`] to attach content.
    pub fn new() -> Self {
        Self {
            child: ZeroSizedBox,
            position: Position::default(),
            left: Dimension::default(),
            top: Dimension::default(),
            right: Dimension::default(),
            bottom: Dimension::default(),
            transform: Transform::default(),
            layer: 0,
        }
    }
}

impl<W: Widget + 'static> Positioned<W> {
    // pub fn box() -> Box<Self> {
    //     Box::new(Self::new())
    // }

    /// Records the positioning mode associated with this widget.
    ///
    /// The default is [`Position::Relative`]. The current renderer stores this
    /// value for inspection but resolves edge offsets identically for every
    /// [`Position`] variant.
    pub fn position(mut self, position: Position) -> Self {
        self.position = position;
        self
    }

    /// Sets the logical left inset or offset.
    ///
    /// The default is [`Dimension::Auto`]. Pixel values are logical pixels and
    /// percentage values resolve against the parent's width. If both left and
    /// right are specified, left takes precedence for painting.
    pub fn left(mut self, left: impl Into<Dimension>) -> Self {
        self.left = left.into();
        self
    }

    /// Sets the logical top inset or offset.
    ///
    /// The default is [`Dimension::Auto`]. Pixel values are logical pixels and
    /// percentage values resolve against the parent's height. If both top and
    /// bottom are specified, top takes precedence for painting.
    pub fn top(mut self, top: impl Into<Dimension>) -> Self {
        self.top = top.into();
        self
    }

    /// Sets the logical right inset.
    ///
    /// The default is [`Dimension::Auto`]. Pixel values are logical pixels and
    /// percentage values resolve against the parent's width. A right-only inset
    /// positions the child from the parent's right edge.
    pub fn right(mut self, right: impl Into<Dimension>) -> Self {
        self.right = right.into();
        self
    }

    /// Sets the logical bottom inset.
    ///
    /// The default is [`Dimension::Auto`]. Pixel values are logical pixels and
    /// percentage values resolve against the parent's height. A bottom-only
    /// inset positions the child from the parent's bottom edge.
    pub fn bottom(mut self, bottom: impl Into<Dimension>) -> Self {
        self.bottom = bottom.into();
        self
    }

    /// Replaces the additional paint transform.
    ///
    /// The default is [`Transform::None`]. Translation values use logical pixels,
    /// rotation uses radians, and scale values are dimensionless. The transform
    /// affects painting rather than the child's measured layout size.
    pub fn transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }

    /// Sets the z-order layer used by layered parents such as [`crate::Stack`].
    ///
    /// The default is `0`; higher layers paint later in a normal-direction stack.
    pub fn layer(mut self, layer: u32) -> Self {
        self.layer = layer;
        self
    }

    /// Attaches or replaces the child while preserving positioning settings.
    ///
    /// `Positioned::new()` is already valid with a zero-sized placeholder. This
    /// operation preserves the new child's concrete type; use
    /// [`Positioned::box_child`] for branch type erasure.
    pub fn child<C: Widget>(self, child: C) -> Positioned<C> {
        Positioned {
            child,
            position: self.position,
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
            transform: self.transform,
            layer: self.layer,
        }
    }

    /// Attaches `child` and erases the resulting widget's concrete type.
    ///
    /// This is equivalent to calling [`Positioned::child`] followed by
    /// [`Widget::boxed`]. Use it when different branches must return one
    /// [`AnyWidget`] type.
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child)
            .boxed()
    }
}

impl<W: Widget> Widget for Positioned<W> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let child = self
            .child
            .to_element(ctx);
        Box::new(RawPositionedElement {
            child,
            position: self.position,
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
            transform: self.transform,
            layer: self.layer,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Default, Copy)]
pub enum Transform {
    Translate(f32, f32),
    TranslateX(f32),
    TranslateY(f32),
    Scale(f32, f32),
    ScaleX(f32),
    ScaleY(f32),
    // Matrix(Vec<f32>),
    Rotate(f32), // radians
    #[default]
    None,
}

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub enum Position {
    Absolute,
    #[default]
    Relative,
    Static,
    Sticky,
    Fixed,
}

#[allow(dead_code)]
pub struct RawPositionedElement<E: Element> {
    pub(crate) child: E,
    pub(crate) position: Position,
    pub(crate) left: Dimension,
    pub(crate) top: Dimension,
    pub(crate) right: Dimension,
    pub(crate) bottom: Dimension,
    pub(crate) transform: Transform,
    pub(crate) layer: u32,
}

impl<E: Element> Drawable for RawPositionedElement<E> {
    fn draw(&self, ctx: &BuildContext) {
        // debug!("Positioned::draw");
        let is_auto = self.top == Dimension::Auto
            && self.left == Dimension::Auto
            && self.right == Dimension::Auto
            && self.bottom == Dimension::Auto;

        if is_auto && self.transform == Transform::None {
            self.child
                .draw(ctx);
            return;
        }

        ctx.canvas
            .save();

        let mut offset_x = 0.0;
        let mut offset_y = 0.0;
        let child_size = self
            .child
            .content_size(ctx);

        if !is_auto {
            if self.left != Dimension::Auto {
                offset_x = self
                    .left
                    .resolve(
                        ctx.parent_size
                            .width,
                        ctx.scale,
                    );
            } else if self.right != Dimension::Auto {
                offset_x = ctx
                    .parent_size
                    .width
                    - self
                        .right
                        .resolve(
                            ctx.parent_size
                                .width,
                            ctx.scale,
                        )
                    - child_size.width;
            }

            if self.top != Dimension::Auto {
                offset_y = self
                    .top
                    .resolve(
                        ctx.parent_size
                            .height,
                        ctx.scale,
                    );
            } else if self.bottom != Dimension::Auto {
                offset_y = ctx
                    .parent_size
                    .height
                    - self
                        .bottom
                        .resolve(
                            ctx.parent_size
                                .height,
                            ctx.scale,
                        )
                    - child_size.height;
            }
        }

        ctx.canvas
            .translate(Vec2d { x: offset_x, y: offset_y });

        match &self.transform {
            Transform::Translate(tx, ty) => {
                ctx.canvas
                    .translate(Vec2d { x: *tx, y: *ty });
            }
            Transform::TranslateX(tx) => {
                ctx.canvas
                    .translate(Vec2d { x: *tx, y: 0.0 });
            }
            Transform::TranslateY(ty) => {
                ctx.canvas
                    .translate(Vec2d { x: 0.0, y: *ty });
            }
            Transform::Scale(sx, sy) => {
                ctx.canvas
                    .scale(*sx, *sy);
            }
            Transform::ScaleX(sx) => {
                ctx.canvas
                    .scale(*sx, 1.0);
            }
            Transform::ScaleY(sy) => {
                ctx.canvas
                    .scale(1.0, *sy);
            }
            Transform::Rotate(rad) => {
                ctx.canvas
                    .rotate(*rad);
            }
            Transform::None => {}
        }

        if is_auto {
            self.child
                .draw(ctx);
        } else {
            let parent_pos = ctx.parent_pos;

            let parent_size = child_size;

            let child_constraint = BoxConstraint {
                min_width: 0.0,
                min_height: 0.0,
                max_width: parent_size.width,
                max_height: parent_size.height,
            };

            // The child is drawn after translating the canvas by the position
            // offset (and any translation transform), so the visibility rect (used
            // for scroll culling) must be shifted by the same amount. Otherwise, a
            // positioned block's top children (e.g. a title above a body) are
            // culled too early and disappear while the taller body survives.
            let child_visible_rect =
                shift_visible_rect(ctx.visible_rect, offset_x, offset_y, &self.transform);

            let child_ctx = BuildContext {
                parent_size,
                canvas: ctx
                    .canvas
                    .clone(),
                scale: ctx.scale,
                parent_pos,
                cursor_pos: ctx.cursor_pos,
                box_constraint: child_constraint,
                visible_rect: child_visible_rect,
                window: ctx
                    .window
                    .clone(),
                #[cfg(not(target_arch = "wasm32"))]
                async_handle: ctx
                    .async_handle
                    .clone(),
                inherited_states: ctx
                    .inherited_states
                    .clone(),
            };

            self.child
                .draw(&child_ctx);
        }
        ctx.canvas
            .restore();
    }
}

/// Shift the scroll-culling `visible_rect` into the positioned child's local
/// coordinate space, i.e. by the same offset the canvas was translated:
/// the position offset plus any translate transform. Scale/Rotate transforms
/// leave the rect unchanged (culling stays conservative — never over-culls).
fn shift_visible_rect(
    visible_rect: Option<(f32, f32, f32, f32)>,
    offset_x: f32,
    offset_y: f32,
    transform: &Transform,
) -> Option<(f32, f32, f32, f32)> {
    let (t_tx, t_ty) = match transform {
        Transform::Translate(tx, ty) => (*tx, *ty),
        Transform::TranslateX(tx) => (*tx, 0.0),
        Transform::TranslateY(ty) => (0.0, *ty),
        _ => (0.0, 0.0),
    };
    let shift_x = offset_x + t_tx;
    let shift_y = offset_y + t_ty;
    visible_rect.map(|(vx, vy, vw, vh)| (vx - shift_x, vy - shift_y, vw, vh))
}

impl<E: Element> VisitorElement for RawPositionedElement<E> {
    fn visit_children<'a>(&'a self, _visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Positioned handles its own child rendering in draw() with proper
        // offset, so we don't expose children here to avoid
        // double-rendering at (0,0).
    }

    fn debug_name(&self) -> &'static str {
        "RawPositionedElement"
    }
}

impl<E: Element> EventElement for RawPositionedElement<E> {
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }
}

impl<E: Element> LayoutElement for RawPositionedElement<E> {
    fn layer(&self) -> u32 {
        self.layer
    }
}

// `visit_children` is intentionally empty so `Drawable::draw` doesn't double-
// render the child. The default `Rebuildable::mark_needs_rebuild` and
// `rebuild_if_dirty` both walk `visit_children`, which means in a
// `Stack → Positioned → Scrollable → Stateful` chain a resize cascade would
// stop at the `Positioned` and never reach the inner `StatefulElement` — the
// `dirty=true` flag set by `adopt_state_from` would sit unused, and the
// rebuilt tree from the adopted `rebuild_fn` (with restored state) would
// never be produced. Visual symptom: stateful child "snaps back" on resize.
impl<E: Element + 'static> Rebuildable for RawPositionedElement<E> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        // eprintln!("[diag] Positioned.rebuild_if_dirty -> child");
        self.child
            .rebuild_if_dirty(ctx);
    }

    fn mark_needs_rebuild(&self) {
        // eprintln!("[diag] Positioned.mark_needs_rebuild -> child");
        self.child
            .mark_needs_rebuild();
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Cull test used by the flex layout: a child at local `[y, y+h)` is drawn
    /// only if it overlaps the visible window `[vy, vy+vh)`.
    fn visible(child_y: f32, child_h: f32, rect: (f32, f32, f32, f32)) -> bool {
        let (_, vy, _, vh) = rect;
        !(child_y + child_h < vy || child_y > vy + vh)
    }

    // Reproduces the reported bug: a feature block positioned 110px down inside a
    // 500px stack, with a short title (0..30) above a tall body (40..300). The
    // scrollable's viewport shows y 200..800 of the stack.
    #[test]
    fn positioned_offset_keeps_title_visible() {
        let stack_visible = Some((0.0, 200.0, 1000.0, 600.0));

        // Before the fix: visible_rect forwarded unchanged. In block-local space
        // the title (0..30) is treated as far above the viewport and culled,
        // while the taller body straddles the line and survives — exactly the
        // "title clipped, body left" symptom.
        let unshifted = stack_visible.unwrap();
        assert!(!visible(0.0, 30.0, unshifted), "buggy path should cull the title");
        assert!(visible(40.0, 260.0, unshifted), "buggy path keeps the body");

        // After the fix: shift by the 110px top offset. The title's real position
        // is stack y 110..140, still above the viewport (200), so culling it is
        // now *correct*; both title and body are judged in the same, correct space.
        let shifted = shift_visible_rect(stack_visible, 0.0, 110.0, &Transform::None).unwrap();
        assert_eq!(shifted, (0.0, 90.0, 1000.0, 600.0));

        // A block near the top of the stack (offset 10px) whose title IS on screen
        // must keep its title after the fix, where the unshifted path wrongly culls it.
        let top_block =
            shift_visible_rect(Some((0.0, 5.0, 1000.0, 600.0)), 0.0, 10.0, &Transform::None)
                .unwrap();
        assert!(visible(0.0, 30.0, top_block), "on-screen title must not be culled");
    }

    #[test]
    fn shift_includes_translate_transform() {
        let r = shift_visible_rect(
            Some((0.0, 100.0, 10.0, 10.0)),
            5.0,
            20.0,
            &Transform::Translate(1.0, 2.0),
        )
        .unwrap();
        assert_eq!(r, (-6.0, 78.0, 10.0, 10.0));
        // Scale/Rotate leave the rect untouched (conservative — never over-cull).
        let s = shift_visible_rect(
            Some((0.0, 100.0, 10.0, 10.0)),
            0.0,
            0.0,
            &Transform::Scale(2.0, 2.0),
        )
        .unwrap();
        assert_eq!(s, (0.0, 100.0, 10.0, 10.0));
    }

    /// Regression for the `Positioned → Scrollable → Stateful` resize bug:
    /// `RawPositionedElement::visit_children` is empty (intentional, to avoid
    /// double-render), so the default `Rebuildable::mark_needs_rebuild` /
    /// `rebuild_if_dirty` walk `visit_children` and stop at `Positioned`. The
    /// inner `StatefulElement` then never receives the resize cascade and the
    /// `dirty=true` flag set by `adopt_state_from` is never consumed. This
    /// test pins the structural trap and the divergence between the two child
    /// accessors — the actual rebuild behavior is covered by the framework
    /// reconcile tests in `aimer_widget`.
    #[test]
    fn positioned_propagates_dirty_into_child() {
        use crate::ZeroSizedBox;
        let positioned: RawPositionedElement<ZeroSizedBox> = RawPositionedElement {
            child: ZeroSizedBox,
            position: Default::default(),
            left: Default::default(),
            top: Default::default(),
            right: Default::default(),
            bottom: Default::default(),
            transform: Default::default(),
            layer: 0,
        };

        // `visit_children` is intentionally empty (no double-render).
        let mut visit_count = 0;
        positioned.visit_children(&mut |_| visit_count += 1);
        assert_eq!(
            visit_count, 0,
            "visit_children must stay empty so draw() doesn't double-render"
        );

        // `event_children` DOES surface the wrapped child — events still need
        // to reach it. The Rebuildable fix above must NOT change this.
        let mut event_count = 0;
        positioned.event_children(&mut |_| event_count += 1);
        assert_eq!(event_count, 1, "event_children must keep surfacing the wrapped child");
    }
}
