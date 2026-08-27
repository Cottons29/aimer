use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;

use crate::base::BuildContext;
use crate::components::element::Element;

pub trait Drawable {
    fn draw(&self, ctx: &BuildContext);

    /// Returns whether this element's paint can be recorded once and replayed
    /// under a different transform without running `draw` again.
    ///
    /// Implementors must return `true` only when drawing has no observable
    /// side effects outside the command stream: it must not update event or
    /// hit-test geometry, advance animation/input state, start asynchronous
    /// work, or depend on the current viewport/cursor. Structural, style,
    /// text, image, and scale changes are still invalidated by the owner of a
    /// retained stream. The conservative default keeps custom and dynamic
    /// elements on the normal draw path.
    #[inline]
    fn is_paint_stable(&self) -> bool {
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
}

impl Drawable for Box<dyn Drawable> {
    fn draw(&self, ctx: &BuildContext) {
        self.as_ref().draw(ctx);
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        self.as_ref().is_paint_stable()
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
}
