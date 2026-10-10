use std::cell::Cell;
use std::rc::Rc;

mod backdrop_filter;
mod magnification;
pub use magnification::{Magnification, MagnificationStyle};
pub use backdrop_filter::BackdropFilter;

use aimer_attribute::{ResolvedSize, Size, Vec2d};
use aimer_canvas::Canvas;
use aimer_style::BoxDecoration;
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, EventTreeRole, LayoutElement,
    PortableWidget, Rebuildable, RequiredChild, VisitorElement, Widget,
};

/// Paints the background of a [`DecorationBox`] without changing child layout.
///
/// Decorations record element-local commands through an already open [`Canvas`].
/// Coordinates are logical pixels; layout sizes have already been divided by
/// the device scale. Implementations need neither `Clone` nor thread-safe state.
///
/// # Examples
///
/// ```
/// use aimer_attribute::{ResolvedSize, Vec2d};
/// use aimer_canvas::Canvas;
/// use aimer_container::{Decoration, DecorationBox, SizedBox};
/// use aimer_widget::base::Color;
///
/// struct Background;
/// impl Decoration for Background {
///     fn paint(&self, canvas: &Canvas, origin: Vec2d, size: ResolvedSize, _scale: f32) {
///         canvas.fill_color_rect(origin, size, Color::BLUE);
///     }
/// }
/// let widget = DecorationBox::new()
///     .decoration(Background)
///     .child(SizedBox::new().width(120).height(40));
/// ```
pub trait Decoration {
    /// Records decoration commands behind the child.
    ///
    /// `origin` locates the decorated box inside its retained paint list,
    /// including any left/top paint outsets. `size` is finite and nonnegative,
    /// and `scale` is finite and positive. Add `origin` to local coordinates.
    /// Balance any canvas state scopes; unbalanced scopes panic when the
    /// wrapper commits the list. Do not draw children, open or finish another
    /// canvas, change layout, or perform application side effects here.
    fn paint(&self, canvas: &Canvas, origin: Vec2d, size: ResolvedSize, scale: f32);

    /// Returns conservative logical paint outsets in left/top/right/bottom order.
    ///
    /// Include every pixel outside `size`, such as shadows or outlines, across
    /// all retained revisions. The default confines paint to the box. Negative
    /// or nonfinite values are treated as zero. For unchanged size and scale,
    /// changing these declared bounds requires rebuilding the widget.
    #[inline]
    fn paint_outsets(&self, _size: ResolvedSize, _scale: f32) -> [f32; 4] {
        [0.0; 4]
    }

    /// Identifies the current paint values for retained command reuse.
    ///
    /// The default is suitable for immutable decorations. Interior-mutable
    /// implementations must return a different value when their paint changes
    /// and have their owner request a redraw, for example through
    /// [`aimer_widget::base::WindowHandle::request_redraw`]. Revision checks do
    /// not schedule frames. This query runs during retained traversal and
    /// should be cheap and side-effect-free.
    #[inline]
    fn revision(&self) -> u64 {
        0
    }
}

impl Decoration for BoxDecoration {
    #[inline]
    fn paint(&self, canvas: &Canvas, origin: Vec2d, size: ResolvedSize, scale: f32) {
        self.record_local_v2(canvas, origin, size.width * scale, size.height * scale, scale);
    }

    #[inline]
    fn paint_outsets(&self, size: ResolvedSize, scale: f32) -> [f32; 4] {
        self.local_v2_paint_outsets(size.width * scale, size.height * scale, scale)
    }

    #[inline]
    fn revision(&self) -> u64 {
        self.background_color.get().map_or(0, |color| u64::from(color.as_u32()) + 1)
    }
}

impl<D: Decoration + ?Sized> Decoration for Box<D> {
    #[inline]
    fn paint(&self, canvas: &Canvas, origin: Vec2d, size: ResolvedSize, scale: f32) {
        self.as_ref().paint(canvas, origin, size, scale);
    }

    #[inline]
    fn paint_outsets(&self, size: ResolvedSize, scale: f32) -> [f32; 4] {
        self.as_ref().paint_outsets(size, scale)
    }

    #[inline]
    fn revision(&self) -> u64 {
        self.as_ref().revision()
    }
}

impl<D: Decoration + ?Sized> Decoration for Rc<D> {
    #[inline]
    fn paint(&self, canvas: &Canvas, origin: Vec2d, size: ResolvedSize, scale: f32) {
        self.as_ref().paint(canvas, origin, size, scale);
    }

    #[inline]
    fn paint_outsets(&self, size: ResolvedSize, scale: f32) -> [f32; 4] {
        self.as_ref().paint_outsets(size, scale)
    }

    #[inline]
    fn revision(&self) -> u64 {
        self.as_ref().revision()
    }
}

/// Paints one child's background while preserving its layout and interaction.
///
/// Decoration borders do not inset the child, and corner radii do not clip it.
/// Compose with [`crate::Padding`], [`crate::Margin`], or [`crate::SizedBox`] for
/// layout. All decoration commands paint before the child; the wrapper never
/// consumes pointer or scroll events, even with an opaque background.
/// Custom decoration widgets do not support portable lowering.
///
/// # Examples
///
/// ```
/// use aimer_container::{DecorationBox, SizedBox};
/// use aimer_style::BoxDecoration;
/// use aimer_widget::base::Color;
///
/// let widget = DecorationBox::new()
///     .decoration(BoxDecoration::new().background_color(Color::BLUE).border_radius(8.0))
///     .child(SizedBox::new().width(120).height(40));
/// ```
pub struct DecorationBox<D = BoxDecoration, W = RequiredChild> {
    decoration: D,
    child: W,
}

impl Default for DecorationBox {
    fn default() -> Self {
        Self::new()
    }
}

impl DecorationBox {
    /// Creates an empty decoration builder that requires a child.
    ///
    /// Finish with [`DecorationBox::child`] or [`DecorationBox::box_child`].
    #[inline]
    pub fn new() -> Self {
        Self { decoration: BoxDecoration::new(), child: RequiredChild }
    }
}

impl<D: Decoration> DecorationBox<D> {
    /// Replaces the decoration, preserving its concrete type without boxing.
    #[inline]
    pub fn decoration<E: Decoration>(self, decoration: E) -> DecorationBox<E> {
        DecorationBox { decoration, child: self.child }
    }

    /// Attaches the required child and completes the builder.
    #[inline]
    pub fn child<W: Widget>(self, child: W) -> DecorationBox<D, W> {
        DecorationBox { decoration: self.decoration, child }
    }

    /// Attaches a child and erases the completed widget's concrete type.
    #[inline]
    pub fn box_child<W: Widget + 'static>(self, child: W) -> AnyWidget
    where
        D: 'static,
    {
        self.child(child).boxed()
    }
}

impl<D: Decoration + 'static, W: Widget> PortableWidget for DecorationBox<D, W> {}

impl<D: Decoration + 'static, W: Widget> Widget for DecorationBox<D, W> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        RawDecorationBox {
            decoration: self.decoration,
            child: self.child.to_element(ctx),
            recorded: Cell::new(None),
        }
        .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "DecorationBox"
    }
}

struct RawDecorationBox<D> {
    decoration: D,
    child: AnyElement,
    recorded: Cell<Option<PaintKey>>,
}

#[derive(Clone, Copy, PartialEq)]
struct PaintKey {
    revision: u64,
    geometry: Option<PaintGeometry>,
}

#[derive(Clone, Copy, PartialEq)]
struct PaintGeometry {
    size: ResolvedSize,
    scale: f32,
}

impl PaintGeometry {
    #[inline]
    fn logical_size(self) -> ResolvedSize {
        ResolvedSize { width: self.size.width / self.scale, height: self.size.height / self.scale }
    }
}

impl<D: Decoration> RawDecorationBox<D> {
    fn paint_geometry(&self, ctx: &BuildContext) -> Option<PaintGeometry> {
        if !ctx.scale.is_finite() || ctx.scale <= 0.0 {
            return None;
        }
        let geometry = PaintGeometry { size: self.child.computed_size(ctx), scale: ctx.scale };
        let logical = geometry.logical_size();
        (geometry.size.width.is_finite() && geometry.size.height.is_finite()
            && geometry.size.width >= 0.0 && geometry.size.height >= 0.0
            && logical.width.is_finite() && logical.height.is_finite()
            && logical.width >= 0.0 && logical.height >= 0.0)
            .then_some(geometry)
    }

    fn paint_outsets(&self, geometry: PaintGeometry) -> [f32; 4] {
        self.decoration.paint_outsets(geometry.logical_size(), geometry.scale)
            .map(|outset| if outset.is_finite() { outset.max(0.0) } else { 0.0 })
    }

    #[inline]
    fn paint_key(&self, ctx: &BuildContext) -> PaintKey {
        PaintKey {
            revision: self.decoration.revision(),
            geometry: self.paint_geometry(ctx),
        }
    }
}

impl<D: Decoration> Rebuildable for RawDecorationBox<D> {}

impl<D: Decoration> Drawable for RawDecorationBox<D> {
    fn update(&self, ctx: &BuildContext) {
        self.child.update(ctx);
    }

    #[inline]
    fn sync_paint_geometry(&self, ctx: &BuildContext) {
        self.child.sync_paint_geometry(ctx);
    }

    fn can_paint_local_v2(&self, _ctx: &BuildContext) -> bool {
        true
    }

    fn paint_local_v2(&self, ctx: &BuildContext) {
        let key = self.paint_key(ctx);
        let canvas = Canvas::of(ctx);
        if let Some(geometry) = key.geometry {
            let size = geometry.logical_size();
            let [left, top, _, _] = self.paint_outsets(geometry);
            // The render node starts at the outer paint corner. Offset only
            // our commands; the retained walker compensates the child's origin.
            self.decoration.paint(&canvas, Vec2d { x: left, y: top }, size, geometry.scale);
        }
        canvas.finish();
        self.recorded.set(Some(key));
    }

    #[inline]
    fn local_v2_paint_needs_recording(&self, ctx: &BuildContext) -> bool {
        self.recorded.get() != Some(self.paint_key(ctx))
    }

    fn retained_v2_bounds(&self, ctx: &BuildContext) -> Option<ResolvedSize> {
        Some(self.paint_geometry(ctx).map_or(ResolvedSize::default(), |geometry| geometry.size))
    }

    fn retained_v2_paint_outsets(&self, ctx: &BuildContext) -> Option<[f32; 4]> {
        Some(self.paint_geometry(ctx).map_or([0.0; 4], |geometry| {
            self.paint_outsets(geometry)
        }))
    }

    fn retained_v2_child_context<'a>(
        &self,
        ctx: &BuildContext<'a>,
        child: &dyn Element,
    ) -> Option<BuildContext<'a>> {
        // Paint bounds can be larger than content size, but must never feed
        // back into the child's constraints or parent size.
        std::ptr::eq(child, self.child.as_ref()).then(|| ctx.clone())
    }
}

impl<D: Decoration> EventElement for RawDecorationBox<D> {
    #[inline]
    fn event_tree_role(&self) -> EventTreeRole {
        EventTreeRole::Transparent
    }

    #[inline]
    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }
}

impl<D: Decoration> VisitorElement for RawDecorationBox<D> {
    #[inline]
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "DecorationBox"
    }
}

impl<D: Decoration> LayoutElement for RawDecorationBox<D> {
    fn size(&self) -> Option<Size> {
        self.child.size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.layout(ctx)
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(ctx)
    }

    fn layer(&self) -> u32 {
        self.child.layer()
    }

    fn flex(&self) -> Option<f32> {
        self.child.flex()
    }

    #[inline]
    fn is_layout_stable(&self) -> bool {
        self.child.is_layout_stable()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.child.invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.child.pos_start_end()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod magnification_tests;
