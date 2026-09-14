use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;

use crate::base::BuildContext;
use crate::components::element::Element;

/// A transform that can be applied without changing retained paint commands.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompositorTransform {
    /// Leaves the current transform unchanged.
    Identity,
    /// Translates the retained content by `(x, y)`.
    Translate { x: f32, y: f32 },
    /// Scales around `(origin_x, origin_y)`.
    Scale {
        sx: f32,
        sy: f32,
        origin_x: f32,
        origin_y: f32,
    },
    /// Rotates around `(origin_x, origin_y)` in radians.
    Rotate {
        radians: f32,
        origin_x: f32,
        origin_y: f32,
    },
}

/// One sampled frame of a compositor-safe animation.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompositorAnimationFrame {
    /// The sampled controller value used for damage tracking.
    pub progress: f32,
    /// The transform to apply to the retained paint.
    pub transform: CompositorTransform,
    /// An opacity override, when the animation changes opacity.
    pub opacity: Option<f32>,
    /// A local rectangular clip, when the animation requires one.
    pub clip: Option<ResolvedSize>,
    /// Whether another frame must be requested after this one.
    pub active: bool,
    /// Whether all compositor properties are finite and safe to apply.
    pub valid: bool,
}

impl CompositorAnimationFrame {
    /// Creates a sampled compositor frame.
    #[inline]
    #[doc(hidden)]
    pub const fn new(
        progress: f32,
        transform: CompositorTransform,
        opacity: Option<f32>,
        clip: Option<ResolvedSize>,
        active: bool,
        valid: bool,
    ) -> Self {
        Self {
            progress,
            transform,
            opacity,
            clip,
            active,
            valid,
        }
    }

    /// Applies this frame to the current canvas state.
    #[inline]
    #[doc(hidden)]
    pub fn apply(self, ctx: &BuildContext) {
        if !self.valid {
            return;
        }

        if let Some(size) = self.clip {
            ctx.canvas.set_clip((0.0, 0.0).into(), size);
        }

        match self.transform {
            CompositorTransform::Identity => {}
            CompositorTransform::Translate { x, y } => {
                ctx.canvas.translate((x, y).into());
            }
            CompositorTransform::Scale {
                sx,
                sy,
                origin_x,
                origin_y,
            } => {
                ctx.canvas.translate((origin_x, origin_y).into());
                ctx.canvas.scale(sx, sy);
                ctx.canvas.translate((-origin_x, -origin_y).into());
            }
            CompositorTransform::Rotate {
                radians,
                origin_x,
                origin_y,
            } => {
                ctx.canvas.translate((origin_x, origin_y).into());
                ctx.canvas.rotate(radians);
                ctx.canvas.translate((-origin_x, -origin_y).into());
            }
        }

        if let Some(opacity) = self.opacity {
            ctx.canvas.set_alpha(opacity);
        }
    }

    /// Removes the clip and opacity state installed by [`Self::apply`].
    #[inline]
    #[doc(hidden)]
    pub fn clear(self, ctx: &BuildContext) {
        if !self.valid {
            return;
        }
        if self.clip.is_some() {
            ctx.canvas.clear_clip();
        }
        if self.opacity.is_some() {
            ctx.canvas.restore_alpha();
        }
    }
}

/// Describes whether a drawable can move its current animation to the
/// compositor while retaining its static paint commands.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompositorAnimationDecision {
    /// This drawable has no compositor animation provider.
    None,
    /// The sampled frame must be drawn live, but the provider already sampled
    /// it and can avoid ticking its controller a second time.
    Live(CompositorAnimationFrame),
    /// The sampled frame may use the retained paint path.
    Compositor(CompositorAnimationFrame),
}

pub trait Drawable {
    fn draw(&self, ctx: &BuildContext);

    /// Emits only the visual commands for this element.
    ///
    /// This is the paint-only half of [`Self::draw`]. It may be recorded and
    /// replayed by an internal retained-paint owner, so it must not rebuild a
    /// child, update hit-test or focus geometry, advance animation/input
    /// state, start asynchronous work, or depend on the cursor or viewport.
    /// The default keeps existing custom elements on the ordinary live path;
    /// an implementation must override this method before opting into
    /// [`Self::is_paint_stable`].
    #[doc(hidden)]
    #[inline]
    fn paint(&self, ctx: &BuildContext) {
        self.draw(ctx);
    }

    /// Synchronizes live geometry needed by interaction and hit testing before
    /// a retained paint replay.
    ///
    /// This hook is deliberately separate from [`Self::paint`]. A cached
    /// visual subtree must still publish current bounds and layout-derived
    /// interaction state even when no descendant paint commands are emitted.
    /// The default is a no-op for leaves whose geometry is already available.
    #[doc(hidden)]
    #[inline]
    fn sync_paint_geometry(&self, _ctx: &BuildContext) {}

    /// Gives layout-aware containers a chance to reconcile viewport-dependent
    /// geometry before the visible paint pass. The return value is `true` when
    /// the element has made its content extent authoritative for this context;
    /// callers may then update an end-anchored scroll position before painting.
    /// The default is intentionally a no-op: ordinary leaves have no layout
    /// work that can be prepared without painting.
    #[doc(hidden)]
    #[inline]
    fn prepare_layout(&self, _ctx: &BuildContext) -> bool {
        false
    }

    /// Returns whether this element's paint can be recorded once and replayed
    /// under a different transform without running its live `draw` lifecycle
    /// again.
    ///
    /// Implementors must return `true` only when drawing has no observable
    /// side effects outside the command stream: it must not update event or
    /// hit-test geometry, advance animation/input state, start asynchronous
    /// work, or depend on the current viewport/cursor. The matching
    /// [`Self::paint`] implementation must emit the complete visual command
    /// stream without those side effects. Structural, style, text, image, and
    /// scale changes are still invalidated by the owner of a retained stream.
    /// The conservative default keeps custom and dynamic elements on the
    /// normal draw path.
    #[inline]
    fn is_paint_stable(&self) -> bool {
        false
    }

    /// Returns whether live drawing stays within the element's current
    /// [`LayoutElement::content_size`](crate::LayoutElement::content_size)
    /// rectangle.
    ///
    /// This contract is intentionally weaker than [`Self::is_paint_stable`].
    /// A bounded element may still perform live work, rebuild children, or
    /// start asynchronous loading; it is only promising that its visual
    /// output remains inside its current layout bounds. Animation damage
    /// tracking uses this to invalidate a local rectangle without opting the
    /// element into retained paint replay.
    ///
    /// Implementations must return `false` when painting can extend outside
    /// the layout rectangle, or when that fact cannot be established
    /// conservatively.
    #[doc(hidden)]
    #[inline]
    fn is_paint_bounded(&self) -> bool {
        false
    }

    /// Draws a subtree whose stable prefix and dynamic suffix can be composed
    /// independently by a retained viewport.
    ///
    /// The default is conservative: the caller must use [`Self::draw`] for
    /// the complete subtree. Implementors may opt in only when they can
    /// preserve their normal paint order and provide child contexts that are
    /// valid both for a full retained recording (`retained_ctx`) and for the
    /// currently visible frame (`live_ctx`). The callbacks receive the child,
    /// its un-translated context, its device-snapped local offset, and an
    /// optional parent clip. A caller owns the actual recording/compositing
    /// policy; this method only exposes the safe partition.
    #[doc(hidden)]
    fn draw_paint_islands(
        &self,
        _retained_ctx: &BuildContext,
        _live_ctx: &BuildContext,
        _draw_stable: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
        _draw_dynamic: &mut dyn FnMut(
            &dyn Element,
            &BuildContext,
            Vec2d,
            Option<ResolvedSize>,
        ),
    ) -> bool {
        false
    }

    /// Samples the compositor animation for the current frame.
    #[doc(hidden)]
    #[inline]
    fn compositor_animation(&self, _ctx: &BuildContext) -> CompositorAnimationDecision {
        CompositorAnimationDecision::None
    }

    /// Draws a sampled animation on the live path without ticking it again.
    #[doc(hidden)]
    #[inline]
    fn draw_with_compositor_animation(
        &self,
        ctx: &BuildContext,
        _frame: CompositorAnimationFrame,
    ) {
        self.draw(ctx);
    }

    /// Updates damage bookkeeping after a retained compositor frame was
    /// replayed.
    #[doc(hidden)]
    #[inline]
    fn update_compositor_animation_damage(
        &self,
        _ctx: &BuildContext,
        _frame: CompositorAnimationFrame,
    ) {
    }
}

impl Drawable for Box<dyn Drawable> {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx);
    }

    #[inline]
    fn paint(&self, ctx: &BuildContext) {
        self.as_ref().paint(ctx);
    }

    #[inline]
    fn sync_paint_geometry(&self, ctx: &BuildContext) {
        self.as_ref().sync_paint_geometry(ctx);
    }

    #[inline]
    fn prepare_layout(&self, ctx: &BuildContext) -> bool {
        self.as_ref().prepare_layout(ctx)
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        self.as_ref().is_paint_stable()
    }

    #[inline]
    fn is_paint_bounded(&self) -> bool {
        self.as_ref().is_paint_bounded()
    }

    #[inline]
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
        self.as_ref().draw_paint_islands(
            retained_ctx,
            live_ctx,
            draw_stable,
            draw_dynamic,
        )
    }

    #[inline]
    fn compositor_animation(&self, ctx: &BuildContext) -> CompositorAnimationDecision {
        self.as_ref().compositor_animation(ctx)
    }

    #[inline]
    fn draw_with_compositor_animation(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.as_ref().draw_with_compositor_animation(ctx, frame);
    }

    #[inline]
    fn update_compositor_animation_damage(
        &self,
        ctx: &BuildContext,
        frame: CompositorAnimationFrame,
    ) {
        self.as_ref()
            .update_compositor_animation_damage(ctx, frame);
    }
}
