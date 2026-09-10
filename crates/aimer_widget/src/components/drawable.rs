use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;

use crate::base::BuildContext;
use crate::components::element::Element;

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
