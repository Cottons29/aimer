use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_macro::{PortableWidget, Rebuildable};
pub use aimer_style::*;
use aimer_widget::InteractionBounds;
use aimer_widget::base::*;
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, EventResult, LayoutCache,
    EventTreeRole, LayoutElement, RequiredChild, VisitorElement, Widget,
};

const LOCAL_V2_CONTAINER_EXTENT_LIMIT: f32 = 1_000_000.0;

#[cfg(feature = "portable-guest")]
use aimer_widget::portable::{
    PortableBuildContext, PortableBuildError, SourceFingerprint,
};

/// A decorated single-child layout box with optional size, spacing, and color.
///
/// Width and height default to [`Dimension::Auto`] and resolve within the
/// parent's constraints. Margin contributes outside the painted box; padding
/// and decoration borders inset the child. All spacing and pixel dimensions are
/// logical units that are scaled by the build context.
///
/// Attach a child with [`Container::child`] to retain its concrete type, or
/// with [`Container::box_child`] when branches need a shared erased type.
#[derive(PortableWidget)]
#[portable_widget(
    id = "aimer_container::single_child::Container",
    validate = validate_portable_container
)]
pub struct Container<T = RequiredChild> {
    #[portable_optional]
    pub(crate) width: Dimension,
    #[portable_optional]
    pub(crate) height: Dimension,
    #[portable_optional]
    pub padding: LayoutSpacing,
    #[portable_optional]
    pub margin: LayoutSpacing,
    #[portable_optional]
    pub box_decoration: BoxDecoration,
    pub color: Option<Color>,
    #[portable_child]
    pub child: T,
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Container {
    /// Creates an automatically sized container with no spacing or decoration.
    ///
    /// Finish the builder with [`Container::child`] or
    /// [`Container::box_child`].
    #[inline]
    pub fn new() -> Self {
        Self {
            width: Dimension::Auto,
            height: Dimension::Auto,
            padding: LayoutSpacing::default(),
            margin: LayoutSpacing::default(),
            box_decoration: BoxDecoration::default(),
            color: None,
            child: RequiredChild,
        }
    }

    /// Sets the outer width before margin is added.
    ///
    /// The default is [`Dimension::Auto`]. Pixel dimensions use logical pixels,
    /// percentages resolve against the parent's maximum width, and the final
    /// value is clamped to the available constraints.
    #[inline]
    pub fn width(mut self, width: impl Into<Dimension>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the outer height before margin is added.
    ///
    /// The default is [`Dimension::Auto`]. Pixel dimensions use logical pixels,
    /// percentages resolve against the parent's maximum height, and the final
    /// value is clamped to the available constraints.
    #[inline]
    pub fn height(mut self, height: impl Into<Dimension>) -> Self {
        self.height = height.into();
        self
    }

    /// Replaces the spacing between the decoration edge and the child.
    ///
    /// Padding is measured in logical pixels and defaults to zero on every
    /// side. It reduces the constraints available to the child.
    #[inline]
    pub fn padding(mut self, padding: LayoutSpacing) -> Self {
        self.padding = padding;
        self
    }

    /// Replaces the transparent spacing outside the decorated box.
    ///
    /// Margin is measured in logical pixels and defaults to zero on every side.
    /// It contributes to the container's layout footprint but is not painted.
    #[inline]
    pub fn margin(mut self, margin: LayoutSpacing) -> Self {
        self.margin = margin;
        self
    }

    /// Replaces the complete background, border, and corner decoration.
    ///
    /// The default decoration is empty. Its border width is included when
    /// deriving the child's inset and clipping radius.
    #[inline]
    pub fn box_decoration(mut self, box_decoration: BoxDecoration) -> Self {
        self.box_decoration = box_decoration;
        self
    }

    /// Sets a solid background color.
    ///
    /// By default no separate color is painted. Setting a color also makes the
    /// container opaque to scroll-event fall-through within its bounds.
    #[inline]
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Attaches the required child and completes this builder.
    ///
    /// The child is laid out inside padding and decoration borders, and its
    /// concrete type is preserved. Use [`Container::box_child`] when different
    /// branches need one erased return type.
    #[inline]
    pub fn child<W: Widget>(self, child: W) -> Container<W> {
        Container {
            width: self.width,
            height: self.height,
            padding: self.padding,
            margin: self.margin,
            box_decoration: self.box_decoration,
            color: self.color,
            child,
        }
    }

    /// Attaches `child` and erases the resulting widget's concrete type.
    ///
    /// This is equivalent to calling [`Container::child`] followed by
    /// [`Widget::boxed`]. Use it when different branches must return one
    /// [`AnyWidget`] type.
    #[inline]
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl<W: Widget> Widget for Container<W> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let child = self.child.to_element(ctx);
        let box_decoration = self.box_decoration;
        if let Some(color) = self.color
            && box_decoration.background_color.get().is_none()
        {
            box_decoration.update_color(color);
        }
        RawContainer {
            width: self.width,
            height: self.height,
            child,
            padding: self.padding,
            margin: self.margin,
            box_decoration,
            cache: LayoutCache::new(),
            debug_name: "Container",
            bounds: InteractionBounds::new(),
            color: None,
            local_v2_background_color: std::cell::Cell::new(None),
        }
        .boxed()
    }
}

#[cfg(feature = "portable-guest")]
fn validate_portable_container<W: Widget>(
    container: &Container<W>,
    ctx: &PortableBuildContext,
    source: SourceFingerprint,
) -> Result<(), PortableBuildError> {
    for (dimension, property) in [
        (container.width, "Container.width"),
        (container.height, "Container.height"),
    ] {
        let valid = match dimension {
            Dimension::Auto => true,
            Dimension::Px(value) => value.is_finite() && value >= 0.0,
            Dimension::Percent(_) => false,
        };
        if !valid {
            return Err(ctx.unsupported_widget(property, source));
        }
    }
    Ok(())
}

/// #### Low level container element.
///
/// - **Container**: safe wrapper for RawContainer
///
/// - **SizedBox**: fixed size container or placeholder
#[derive(Default, Rebuildable)]
pub struct RawContainer<T: Element> {
    pub padding: LayoutSpacing,
    pub margin: LayoutSpacing,
    pub width: Dimension,
    pub height: Dimension,
    pub box_decoration: BoxDecoration,
    pub child: T,
    pub cache: LayoutCache,
    pub debug_name: &'static str,
    pub color: Option<Color>,
    pub bounds: InteractionBounds,
    local_v2_background_color: std::cell::Cell<Option<Option<(u8, u8, u8, u8)>>>,
}

#[derive(Clone, Copy)]
struct LocalV2ContainerGeometry {
    clip_origin: Vec2d,
    clip_size: ResolvedSize,
    child_origin: Vec2d,
    content_size: ResolvedSize,
    /// Corner radii of the child clip in device pixels (top-left, top-right,
    /// bottom-right, bottom-left): the outer radii shrunk by the border, as
    /// the live path computes them.
    clip_radii: [f32; 4],
}

impl<E: Element> RawContainer<E> {
    /// Returns whether this container only forwards layout and a rectangular
    /// clip. Such a wrapper can expose the child's paint islands without
    /// recording its own decoration or translating an inset.
    #[inline]
    fn can_delegate_paint_islands(&self) -> bool {
        self.width == Dimension::Auto
            && self.height == Dimension::Auto
            && self.padding == LayoutSpacing::default()
            && self.margin == LayoutSpacing::default()
            && self.color.is_none()
            && self.box_decoration == BoxDecoration::default()
    }

    /// A container is *opaque* when it paints a background — either an explicit
    /// `color` or a `box_decoration.background_color`. An opaque container
    /// visually covers whatever sits behind it in a `Stack`, so it must also
    /// occlude it for hit-testing (Flutter's `HitTestBehavior::opaque`).
    fn is_opaque(&self) -> bool {
        self.color.is_some() || self.box_decoration.background_color.get().is_some()
    }

    fn local_v2_paint_size(&self, ctx: &BuildContext) -> Option<ResolvedSize> {
        if self.color.is_some() {
            return None;
        }

        let scale = ctx.scale;
        let parent_width = ctx.box_constraint.max_width;
        let parent_height = ctx.box_constraint.max_height;
        if !scale.is_finite()
            || scale <= 0.0
            || !parent_width.is_finite()
            || !parent_height.is_finite()
            || parent_width < 0.0
            || parent_height < 0.0
        {
            return None;
        }

        let (margin_left, margin_top, margin_right, margin_bottom) = self.margin(ctx);
        let width = match self.width {
            Dimension::Px(width) => width * scale,
            Dimension::Percent(percent) => {
                parent_width * (percent / 100.0) - margin_left - margin_right
            }
            Dimension::Auto => parent_width - margin_left - margin_right,
        };
        let height = match self.height {
            Dimension::Px(height) => height * scale,
            Dimension::Percent(percent) => {
                parent_height * (percent / 100.0) - margin_top - margin_bottom
            }
            Dimension::Auto => parent_height - margin_top - margin_bottom,
        };
        // An unbounded axis (a scroll view offers `f32::MAX`) stays huge here,
        // exactly as in the live `render`: children size themselves to their
        // content only while their constraint stays unbounded. Rectangles handed
        // to the render tree are clamped separately with `render_extent`.
        if !width.is_finite() || !height.is_finite() {
            return None;
        }

        // Rounded corners are supported: the box decoration records its own
        // radii, and `retained_v2_child_clip_radius` rounds the child's clip.
        if self
            .box_decoration
            .border_radius
            .resolve(width, height, scale)
            .into_iter()
            .any(|radius| !radius.is_finite() || radius < 0.0)
        {
            return None;
        }

        Some(ResolvedSize {
            width: width.max(0.0),
            height: height.max(0.0),
        })
    }

    #[allow(dead_code)]
    pub(crate) fn new(child: E) -> Self {
        Self {
            margin: LayoutSpacing::default(),
            padding: LayoutSpacing::default(),
            width: Dimension::Auto,
            height: Dimension::Auto,
            box_decoration: BoxDecoration::default(),
            child,
            cache: LayoutCache::default(),
            debug_name: "RawContainer",
            color: None,
            bounds: InteractionBounds::new(),
            local_v2_background_color: std::cell::Cell::new(None),
        }
    }

    fn margin(&self, ctx: &BuildContext) -> (f32, f32, f32, f32) {
        let parent_width = ctx.box_constraint.max_width;
        let parent_height = ctx.box_constraint.max_height;
        let scale = ctx.scale;

        let m_left = self.margin.left.value(parent_width, scale);
        let m_top = self.margin.top.value(parent_height, scale);
        let m_right = self.margin.right.value(parent_width, scale);
        let m_bottom = self.margin.bottom.value(parent_height, scale);

        (m_left, m_top, m_right, m_bottom)
    }

    fn local_v2_geometry(&self, ctx: &BuildContext) -> Option<LocalV2ContainerGeometry> {
        let scale = ctx.scale;
        let size = self.local_v2_paint_size(ctx)?;
        let (margin_left, margin_top, _, _) = self.margin(ctx);
        let width = size.width;
        let height = size.height;
        let padding_left = self.padding.left.value(width, scale);
        let padding_top = self.padding.top.value(height, scale);
        let padding_right = self.padding.right.value(width, scale);
        let padding_bottom = self.padding.bottom.value(height, scale);
        let border = self.box_decoration.border;
        let border_left = border_stroke(border.left.stroke, width, scale);
        let border_right = border_stroke(border.right.stroke, width, scale);
        let border_top = border_stroke(border.top.stroke, height, scale);
        let border_bottom = border_stroke(border.bottom.stroke, height, scale);
        let clip_size = ResolvedSize {
            width: (width - border_left - border_right).max(0.0),
            height: (height - border_top - border_bottom).max(0.0),
        };
        let child_origin = Vec2d {
            x: margin_left + padding_left + border_left,
            y: margin_top + padding_top + border_top,
        };
        let content_size = ResolvedSize {
            width: (clip_size.width - padding_left - padding_right).max(0.0),
            height: (clip_size.height - padding_top - padding_bottom).max(0.0),
        };
        let radii = self
            .box_decoration
            .border_radius
            .resolve(width, height, scale);
        let clip_radii = [
            (radii[0] - border_top.max(border_left)).max(0.0),
            (radii[1] - border_top.max(border_right)).max(0.0),
            (radii[2] - border_bottom.max(border_right)).max(0.0),
            (radii[3] - border_bottom.max(border_left)).max(0.0),
        ];
        Some(LocalV2ContainerGeometry {
            clip_origin: Vec2d {
                x: margin_left + border_left,
                y: margin_top + border_top,
            },
            clip_size,
            child_origin,
            content_size,
            clip_radii,
        })
    }

    fn local_v2_child_context<'a>(&self, ctx: &BuildContext<'a>) -> Option<BuildContext<'a>> {
        let geometry = self.local_v2_geometry(ctx)?;
        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.max_width = geometry.content_size.width;
        child_ctx.box_constraint.max_height = geometry.content_size.height;
        child_ctx.parent_size = geometry.content_size;
        child_ctx.visible_rect = ctx.visible_rect.map(|(x, y, width, height)| {
            (
                x - geometry.child_origin.x,
                y - geometry.child_origin.y,
                width,
                height,
            )
        });
        Some(child_ctx)
    }
}

#[inline]
fn border_stroke(dimension: Dimension, parent: f32, scale: f32) -> f32 {
    match dimension {
        Dimension::Px(width) => width * scale,
        Dimension::Percent(percent) => parent * (percent / 100.0),
        Dimension::Auto => 0.0,
    }
    .max(0.0)
}

/// Clamps a device-pixel extent to what the render tree can carry.
///
/// A scroll view offers an unbounded axis as `f32::MAX`; scaled to device
/// pixels (or added to a position) that overflows to infinity, which the tree
/// rejects. `LOCAL_V2_CONTAINER_EXTENT_LIMIT` logical pixels is far beyond any
/// viewport, so clamping to it does not change what is visible.
#[inline]
fn render_extent(device_extent: f32, scale: f32) -> f32 {
    device_extent.min(LOCAL_V2_CONTAINER_EXTENT_LIMIT * scale)
}

#[inline]
fn intersect_retained_clip(
    left: aimer_cupid::draw_cmd_v2::Rect,
    right: aimer_cupid::draw_cmd_v2::Rect,
) -> aimer_cupid::draw_cmd_v2::Rect {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    let right_edge = (left.x + left.width).min(right.x + right.width);
    let bottom_edge = (left.y + left.height).min(right.y + right.height);
    aimer_cupid::draw_cmd_v2::Rect::new(
        x,
        y,
        (right_edge - x).max(0.0),
        (bottom_edge - y).max(0.0),
    )
}

impl<T: Element> RawContainer<T> {
    fn render(&self, ctx: &BuildContext, paint_only: bool) {
        ctx.canvas.save();

        let constraint = ctx.box_constraint;

        let parent_width = constraint.max_width;
        let parent_height = constraint.max_height;
        let scale = ctx.scale;

        let (m_left, m_top, m_right, m_bottom) = self.margin(ctx);

        let box_width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => parent_width * (p / 100.0) - (m_left + m_right),
            Dimension::Auto => parent_width - m_left - m_right,
        };

        let box_height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => parent_height * (p / 100.0) - (m_top + m_bottom),
            Dimension::Auto => parent_height - m_top - m_bottom,
        };

        let box_width = box_width.max(0.0);
        let box_height = box_height.max(0.0);

        // Use computed_size to get correct dimensions (handles unbounded/scrollable
        // case)
        let computed = self.computed_size(ctx);
        let (m_left_v, m_top_v, m_right_v, m_bottom_v) = self.margin(ctx);
        let draw_width = (computed.width - m_left_v - m_right_v).max(0.0);
        let draw_height = (computed.height - m_top_v - m_bottom_v).max(0.0);

        // Record the on-screen (logical) bounds every frame. `dispatch_event`
        // uses `pos_start_end` to decide whether an event lands on this element;
        // without live bounds an opaque container could never occlude an event
        // at a specific position (see `on_event`). Bounds start after the margin
        // translate and span the actually-drawn size (`draw_width`/`draw_height`).
        if !paint_only {
            let (start_x, start_y) = ctx.canvas.get_transform_translation();
            self.bounds.save(
                scale,
                start_x + m_left,
                start_y + m_top,
                draw_width,
                draw_height,
            );
        }

        // Translate to the decorated box before painting. Margin is layout
        // space outside the complete decoration; it must not be filled by the
        // background, border, or outline.
        ctx.canvas.translate(Vec2d {
            x: m_left,
            y: m_top,
        });

        let p_left = self.padding.left.value(box_width, scale);
        let p_top = self.padding.top.value(box_height, scale);
        let _p_right = self.padding.right.value(box_width, scale);
        let _p_bottom = self.padding.bottom.value(box_height, scale);

        let border = self.box_decoration.border;
        let radii = self
            .box_decoration
            .border_radius
            .resolve(box_width, box_height, scale);

        let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };
        let b_left = get_stroke(border.left.stroke, box_width).max(0.0);
        let b_right = get_stroke(border.right.stroke, box_width).max(0.0);
        let b_top = get_stroke(border.top.stroke, box_height).max(0.0);
        let b_bottom = get_stroke(border.bottom.stroke, box_height).max(0.0);

        // Draw decoration (background, border, outline)

        // Clip to inset rect (inside borders)
        let clip_x = b_left;
        let clip_y = b_top;
        let clip_w = (box_width - b_right - clip_x).max(0.0);
        let clip_h = (box_height - b_bottom - clip_y).max(0.0);

        let inner_radii = [
            (radii[0] - b_top.max(b_left)).max(0.0),     // top-left
            (radii[1] - b_top.max(b_right)).max(0.0),    // top-right
            (radii[2] - b_bottom.max(b_right)).max(0.0), // bottom-right
            (radii[3] - b_bottom.max(b_left)).max(0.0),  /* bottom-left */
        ];

        ctx.canvas.set_clip_rounded(
            Vec2d {
                x: clip_x,
                y: clip_y,
            },
            ResolvedSize {
                width: clip_w,
                height: clip_h,
            },
            inner_radii,
        );

        ctx.canvas.translate(Vec2d {
            x: p_left + b_left,
            y: p_top + b_top,
        });

        let mut child_ctx = ctx.clone();
        let content_w = (box_width - p_left - b_left - _p_right - b_right).max(0.0);
        let content_h = (box_height - p_top - b_top - _p_bottom - b_bottom).max(0.0);
        child_ctx.box_constraint.max_width = content_w;
        child_ctx.box_constraint.max_height = content_h;
        child_ctx.parent_size = ResolvedSize {
            width: content_w,
            height: content_h,
        };

        // The child is drawn after translating the canvas by the margin and the
        // padding + border inset, so the visibility rect (used for scroll
        // culling) must be shifted by the same offset. Otherwise children of a
        // padded/margined container are culled too early and disappear before
        // they actually leave the viewport.
        let inset_x = m_left + p_left + b_left;
        let inset_y = m_top + p_top + b_top;
        child_ctx.visible_rect = ctx
            .visible_rect
            .map(|(vx, vy, vw, vh)| (vx - inset_x, vy - inset_y, vw, vh));

        // The child may intentionally paint outside its measured content into
        // the padding area, so use the complete clipping region rather than
        // the child's nominal content bounds. That keeps unknown child bounds
        // conservative while still skipping the whole subtree when the clip
        // itself is outside the ancestor viewport.
        if ctx.is_rect_visible(
            m_left + clip_x,
            m_top + clip_y,
            clip_w,
            clip_h,
        ) {
            if paint_only {
                self.child.paint(&child_ctx);
            } else {
                self.child.update(&child_ctx);
            }
        }
        ctx.canvas.clear_clip();
        ctx.canvas.restore();
    }
}

impl<T: Element> Drawable for RawContainer<T> {
    /// Hit-tested over the box that is drawn: the computed size less the
    /// margins, which start `retained_v2_interaction_offset` in from the node.
    fn retained_v2_interaction_size(&self, ctx: &BuildContext) -> Option<ResolvedSize> {
        let (m_left, m_top, m_right, m_bottom) = self.margin(ctx);
        let computed = self.computed_size(ctx);
        Some(ResolvedSize {
            width: (computed.width - m_left - m_right).max(0.0),
            height: (computed.height - m_top - m_bottom).max(0.0),
        })
    }

    #[inline]
    fn retained_v2_interaction_offset(&self, ctx: &BuildContext) -> (f32, f32) {
        let (m_left, m_top, _, _) = self.margin(ctx);
        (m_left, m_top)
    }

    #[inline]
    fn adopt_retained_v2_interaction_source(&self, source: aimer_widget::InteractionSource) {
        self.bounds.adopt(source);
    }

    #[inline]
    fn retained_v2_interaction_disagreement(&self) -> Option<aimer_widget::InteractionDisagreement> {
        self.bounds.disagreement("Container")
    }

    fn update(&self, ctx: &BuildContext) {
        self.render(ctx, false);
    }

    fn can_paint_local_v2(&self, ctx: &BuildContext) -> bool {
        self.local_v2_paint_size(ctx).is_some()
    }

    fn paint_local_v2(&self, ctx: &BuildContext) {
        let Some(size) = self.local_v2_paint_size(ctx) else {
            unreachable!("RawContainer v2 support must be checked before painting")
        };

        let canvas = aimer_canvas::Canvas::of(ctx);
        let (margin_left, margin_top, _, _) = self.margin(ctx);
        self.box_decoration.record_local_v2(
            &canvas,
            Vec2d {
                x: margin_left / ctx.scale,
                y: margin_top / ctx.scale,
            },
            render_extent(size.width, ctx.scale),
            render_extent(size.height, ctx.scale),
            ctx.scale,
        );
        canvas.finish();
        self.local_v2_background_color.set(Some(
            self.box_decoration
                .background_color
                .get()
                .map(|color| color.to_rgba()),
        ));
    }

    fn local_v2_paint_needs_recording(&self, _ctx: &BuildContext) -> bool {
        self.local_v2_background_color.get()
            != Some(
                self.box_decoration
                    .background_color
                    .get()
                    .map(|color| color.to_rgba()),
            )
    }

    fn retained_clip(&self, ctx: &BuildContext) -> Option<aimer_cupid::draw_cmd_v2::Rect> {
        let _ = ctx;
        None
    }

    fn retained_v2_paint_outsets(&self, ctx: &BuildContext) -> Option<[f32; 4]> {
        let size = self.local_v2_paint_size(ctx)?;
        Some(self.box_decoration.local_v2_paint_outsets(
            render_extent(size.width, ctx.scale),
            render_extent(size.height, ctx.scale),
            ctx.scale,
        ))
    }

    fn retained_v2_child_context<'a>(
        &self,
        ctx: &BuildContext<'a>,
        child: &dyn Element,
    ) -> Option<BuildContext<'a>> {
        if !std::ptr::eq(child, &self.child as &dyn Element) {
            return None;
        }
        self.local_v2_child_context(ctx)
    }

    fn retained_v2_child_clip_radius(&self, ctx: &BuildContext, child: &dyn Element) -> [f32; 4] {
        if !std::ptr::eq(child, &self.child as &dyn Element) {
            return [0.0; 4];
        }
        self.local_v2_geometry(ctx)
            .map_or([0.0; 4], |geometry| {
                geometry
                    .clip_radii
                    .map(|radius| (radius / ctx.scale).min(LOCAL_V2_CONTAINER_EXTENT_LIMIT))
            })
    }

    fn retained_v2_child_geometry(
        &self,
        ctx: &BuildContext,
        child: &dyn Element,
    ) -> Option<(
        aimer_cupid::draw_cmd_v2::Rect,
        Option<aimer_cupid::draw_cmd_v2::Rect>,
    )> {
        if !std::ptr::eq(child, &self.child as &dyn Element) {
            return None;
        }
        let scale = ctx.scale;
        let geometry = self.local_v2_geometry(ctx)?;
        let child_ctx = self.local_v2_child_context(ctx)?;
        let child_size = child.content_size(&child_ctx);
        let position = child.pos().unwrap_or_default();
        let x = geometry.child_origin.x + position.x;
        let y = geometry.child_origin.y + position.y;
        let bounds = aimer_cupid::draw_cmd_v2::Rect::new(
            x / scale,
            y / scale,
            child_size.width / scale,
            child_size.height / scale,
        );
        let clip = aimer_cupid::draw_cmd_v2::Rect::new(
            (geometry.clip_origin.x - x) / scale,
            (geometry.clip_origin.y - y) / scale,
            render_extent(geometry.clip_size.width, scale) / scale,
            render_extent(geometry.clip_size.height, scale) / scale,
        );
        let clip = child
            .retained_clip(&child_ctx)
            .map(|child_clip| intersect_retained_clip(clip, child_clip))
            .unwrap_or(clip);
        Some((bounds, Some(clip)))
    }

    fn paint(&self, ctx: &BuildContext) {
        self.render(ctx, true);
    }

    #[inline]
    fn is_paint_bounded(&self) -> bool {
        // The child is painted inside the container's inset clip. Shadows and
        // outlines are the two decoration features that can extend beyond the
        // container's own layout rectangle.
        self.box_decoration.box_shadow.is_empty()
            && self.box_decoration.outline == Default::default()
    }

    #[inline]
    fn sync_paint_geometry(&self, ctx: &BuildContext) {
        let constraint = ctx.box_constraint;
        let scale = ctx.scale;
        let (m_left, m_top, m_right, m_bottom) = self.margin(ctx);
        let box_width = match self.width {
            Dimension::Px(width) => width * scale,
            Dimension::Percent(percent) => {
                constraint.max_width * (percent / 100.0) - (m_left + m_right)
            }
            Dimension::Auto => constraint.max_width - m_left - m_right,
        }
        .max(0.0);
        let box_height = match self.height {
            Dimension::Px(height) => height * scale,
            Dimension::Percent(percent) => {
                constraint.max_height * (percent / 100.0) - (m_top + m_bottom)
            }
            Dimension::Auto => constraint.max_height - m_top - m_bottom,
        }
        .max(0.0);
        let computed = self.computed_size(ctx);
        let draw_width = (computed.width - m_left - m_right).max(0.0);
        let draw_height = (computed.height - m_top - m_bottom).max(0.0);
        let (start_x, start_y) = ctx.canvas.get_transform_translation();
        if scale.is_finite() && scale > 0.0 {
            self.bounds.save(
                scale,
                start_x + m_left,
                start_y + m_top,
                draw_width,
                draw_height,
            );
        }

        let padding_left = self.padding.left.value(box_width, scale);
        let padding_top = self.padding.top.value(box_height, scale);
        let border = self.box_decoration.border;
        let get_stroke = |dimension: Dimension, parent: f32| -> f32 {
            match dimension {
                Dimension::Px(width) => width * scale,
                Dimension::Percent(percent) => parent * (percent / 100.0),
                Dimension::Auto => 0.0,
            }
        };
        let border_left = get_stroke(border.left.stroke, box_width).max(0.0);
        let border_top = get_stroke(border.top.stroke, box_height).max(0.0);
        let content_width = (box_width
            - padding_left
            - border_left
            - self.padding.right.value(box_width, scale)
            - get_stroke(border.right.stroke, box_width).max(0.0))
            .max(0.0);
        let content_height = (box_height
            - padding_top
            - border_top
            - self.padding.bottom.value(box_height, scale)
            - get_stroke(border.bottom.stroke, box_height).max(0.0))
            .max(0.0);
        let inset_x = m_left + padding_left + border_left;
        let inset_y = m_top + padding_top + border_top;

        let mut child_ctx = ctx.clone();
        child_ctx.box_constraint.max_width = content_width;
        child_ctx.box_constraint.max_height = content_height;
        child_ctx.parent_size = ResolvedSize {
            width: content_width,
            height: content_height,
        };
        child_ctx.visible_rect = ctx
            .visible_rect
            .map(|(x, y, width, height)| (x - inset_x, y - inset_y, width, height));

        ctx.canvas.save();
        ctx.canvas.translate(Vec2d {
            x: inset_x,
            y: inset_y,
        });
        self.child.sync_paint_geometry(&child_ctx);
        ctx.canvas.restore();
    }

    #[inline]
    fn is_paint_stable(&self) -> bool {
        self.color.is_none()
            && self.box_decoration.box_shadow.is_empty()
            && self.box_decoration.outline == Default::default()
            && self.child.is_paint_stable()
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
        if !self.can_delegate_paint_islands() {
            return false;
        }

        let child_size = self.content_size(live_ctx);
        if !child_size.width.is_finite() || !child_size.height.is_finite() {
            return false;
        }

        // This transparent wrapper is not painted by the delegated path, but
        // its live screen bounds still participate in routed input and culling.
        let (start_x, start_y) = live_ctx.canvas.get_transform_translation();
        let scale = live_ctx.scale;
        self.bounds.save(
            scale,
            start_x,
            start_y,
            child_size.width,
            child_size.height,
        );

        // Match the ordinary draw path's child context. A scrollable has an
        // unbounded constraint on its main axis, so replacing it with the
        // measured content size would change flex justification and wrapping.
        let child_parent_size = ResolvedSize {
            width: live_ctx.box_constraint.max_width,
            height: live_ctx.box_constraint.max_height,
        };
        let mut retained_child_ctx = retained_ctx.clone();
        retained_child_ctx.parent_size = child_parent_size;
        retained_child_ctx.visible_rect = None;

        let mut live_child_ctx = live_ctx.clone();
        live_child_ctx.parent_size = child_parent_size;
        // The ordinary container path clips the child to its content box. The
        // island callbacks express clips at their parent's origin, so a plain
        // wrapper can preserve that contract by intersecting the child's clip
        // with this box before forwarding it.
        let wrapper_clip = child_parent_size;
        let clip = |child_clip: Option<ResolvedSize>| {
            Some(match child_clip {
                Some(child_clip) => ResolvedSize {
                    width: child_clip.width.min(wrapper_clip.width),
                    height: child_clip.height.min(wrapper_clip.height),
                },
                None => wrapper_clip,
            })
        };

        let mut forward_stable =
            |element: &dyn Element,
             child_ctx: &BuildContext,
             offset: Vec2d,
             child_clip: Option<ResolvedSize>| {
                draw_stable(element, child_ctx, offset, clip(child_clip));
            };
        let mut forward_dynamic =
            |element: &dyn Element,
             child_ctx: &BuildContext,
             offset: Vec2d,
             child_clip: Option<ResolvedSize>| {
                draw_dynamic(element, child_ctx, offset, clip(child_clip));
            };

        self.child.draw_paint_islands(
            &retained_child_ctx,
            &live_child_ctx,
            &mut forward_stable,
            &mut forward_dynamic,
        )
    }
}

impl<T: Element> VisitorElement for RawContainer<T> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}

impl<T: Element> EventElement for RawContainer<T> {
    fn event_tree_role(&self) -> EventTreeRole {
        if self.is_opaque() {
            EventTreeRole::IndexedTarget
        } else {
            EventTreeRole::Transparent
        }
    }

    /// The opaque container has one child in both structural views.
    #[inline]
    fn structural_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    fn on_event(&self, event: &ElementEvent) -> EventResult {
        // An opaque container occludes lower `Stack` layers: a wheel / trackpad
        // `Scroll` that lands on it must not fall through to a `Scrollable`
        // behind it. `dispatch_event` only calls `on_event` once the event
        // position is already inside our bounds and after our children had a
        // chance to consume it (deepest-first), so returning `true` here simply
        // absorbs a scroll that nothing in front of us wanted — this is exactly
        // why the website's scrollable used to scroll while the pointer was on
        // the opaque header. Non-scroll events keep falling through so existing
        // click/drag routing is unchanged.
        if matches!(event, ElementEvent::Scroll { .. }) && self.is_opaque() {
            return EventResult::consumed();
        }
        EventResult::ignored()
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }
}

impl<T: Element> LayoutElement for RawContainer<T> {
    fn event_tree_bounds(&self) -> Option<(Vec2d, Vec2d)> {
        // These bounds follow paint transforms such as scroll offsets, which
        // can change without a layout-generation update. Keep this target
        // unbounded in the cached event index; dispatch checks the live bounds.
        None
    }

    #[inline]
    fn is_layout_stable(&self) -> bool {
        self.can_delegate_paint_islands() && self.child.is_layout_stable()
    }

    fn size(&self) -> Option<Size> {
        Some(Size {
            width: self.width,
            height: self.height,
        })
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_computed(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let scale = ctx.scale;
        let p_w = ctx.box_constraint.max_width;
        let p_h = ctx.box_constraint.max_height;
        let threshold = 1_000_000.0f32;

        let m_left = self.margin.left.value(p_w, scale);
        let m_right = self.margin.right.value(p_w, scale);
        let m_top = self.margin.top.value(p_h, scale);
        let m_bottom = self.margin.bottom.value(p_h, scale);

        let box_width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => p_w * (p / 100.0) - (m_left + m_right),
            Dimension::Auto => p_w - (m_left + m_right),
        };

        let box_height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => p_h * (p / 100.0) - (m_top + m_bottom),
            Dimension::Auto => p_h - (m_top + m_bottom),
        };

        // When Auto dimension is unbounded (e.g. inside scrollable), derive size from
        // child
        let width_unbounded = matches!(self.width, Dimension::Auto) && box_width > threshold;
        let height_unbounded = matches!(self.height, Dimension::Auto) && box_height > threshold;

        let result = if width_unbounded || height_unbounded {
            let capped_w = box_width.min(threshold);
            let capped_h = box_height.min(threshold);

            let p_left = self.padding.left.value(capped_w, scale);
            let p_right = self.padding.right.value(capped_w, scale);
            let p_top = self.padding.top.value(capped_h, scale);
            let p_bottom = self.padding.bottom.value(capped_h, scale);

            let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
                match dim {
                    Dimension::Px(w) => w * scale,
                    Dimension::Percent(p) => parent_val * (p / 100.0),
                    Dimension::Auto => 0.0,
                }
            };
            let bl = get_stroke(self.box_decoration.border.left.stroke, capped_w).max(0.0);
            let br = get_stroke(self.box_decoration.border.right.stroke, capped_w).max(0.0);
            let bt = get_stroke(self.box_decoration.border.top.stroke, capped_h).max(0.0);
            let bb = get_stroke(self.box_decoration.border.bottom.stroke, capped_h).max(0.0);

            let mut child_ctx = ctx.clone();
            child_ctx.box_constraint.max_width = if width_unbounded {
                f32::MAX
            } else {
                (box_width - p_left - bl - p_right - br).max(0.0)
            };
            child_ctx.box_constraint.max_height = if height_unbounded {
                f32::MAX
            } else {
                (box_height - p_top - bt - p_bottom - bb).max(0.0)
            };
            let child_size = self.child.computed_size(&child_ctx);

            let final_w = if width_unbounded {
                child_size.width + p_left + p_right + bl + br + m_left + m_right
            } else {
                box_width + m_left + m_right
            };
            let final_h = if height_unbounded {
                child_size.height + p_top + p_bottom + bt + bb + m_top + m_bottom
            } else {
                box_height + m_top + m_bottom
            };

            ResolvedSize {
                width: final_w.max(0.0),
                height: final_h.max(0.0),
            }
        } else {
            ResolvedSize {
                width: (box_width + m_left + m_right).max(0.0),
                height: (box_height + m_top + m_bottom).max(0.0),
            }
        };
        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);
        result
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_content(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let scale = ctx.scale;
        let p_w = ctx.box_constraint.max_width;
        let p_h = ctx.box_constraint.max_height;
        let threshold = 1_000_000.0f32;

        let m_left = self.margin.left.value(p_w, scale);
        let m_right = self.margin.right.value(p_w, scale);
        let m_top = self.margin.top.value(p_h, scale);
        let m_bottom = self.margin.bottom.value(p_h, scale);

        let box_width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => p_w * (p / 100.0) - (m_left + m_right),
            Dimension::Auto => p_w - (m_left + m_right),
        };
        let box_height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => p_h * (p / 100.0) - (m_top + m_bottom),
            Dimension::Auto => p_h - (m_top + m_bottom),
        };

        let width_unbounded = matches!(self.width, Dimension::Auto) && box_width > threshold;
        let height_unbounded = matches!(self.height, Dimension::Auto) && box_height > threshold;

        let b_w = box_width.max(0.0);
        let b_h = box_height.max(0.0);
        let capped_w = b_w.min(threshold);
        let capped_h = b_h.min(threshold);

        let p_left = self.padding.left.value(capped_w, scale);
        let p_right = self.padding.right.value(capped_w, scale);
        let p_top = self.padding.top.value(capped_h, scale);
        let p_bottom = self.padding.bottom.value(capped_h, scale);

        let get_stroke = |dim: Dimension, parent_val: f32| -> f32 {
            match dim {
                Dimension::Px(w) => w * scale,
                Dimension::Percent(p) => parent_val * (p / 100.0),
                Dimension::Auto => 0.0,
            }
        };

        let border = self.box_decoration.border;

        let b_left = get_stroke(border.left.stroke, capped_w).max(0.0);
        let b_right = get_stroke(border.right.stroke, capped_w).max(0.0);
        let b_top = get_stroke(border.top.stroke, capped_h).max(0.0);
        let b_bottom = get_stroke(border.bottom.stroke, capped_h).max(0.0);

        let result = if width_unbounded || height_unbounded {
            let mut child_ctx = ctx.clone();
            child_ctx.box_constraint.max_width = if width_unbounded {
                f32::MAX
            } else {
                (b_w - p_left - b_left - p_right - b_right).max(0.0)
            };
            child_ctx.box_constraint.max_height = if height_unbounded {
                f32::MAX
            } else {
                (b_h - p_top - b_top - p_bottom - b_bottom).max(0.0)
            };
            let child_size = self.child.computed_size(&child_ctx);

            ResolvedSize {
                width: if width_unbounded {
                    child_size.width
                } else {
                    (b_w - p_left - p_right - b_left - b_right).max(0.0)
                },
                height: if height_unbounded {
                    child_size.height
                } else {
                    (b_h - p_top - p_bottom - b_top - b_bottom).max(0.0)
                },
            }
        } else {
            ResolvedSize {
                width: (b_w - p_left - p_right - b_left - b_right).max(0.0),
                height: (b_h - p_top - p_bottom - b_top - b_bottom).max(0.0),
            }
        };
        self.cache
            .set_content(ctx.box_constraint, scale_bits, result);
        result
    }

    fn get_size_from_child(&self) -> Option<Size> {
        let mut size = self.child.get_size_from_child().unwrap_or_default();

        let m_w: f32 = 0.0;
        let m_h: f32 = 0.0;
        let mut p_w: f32 = 0.0;
        let mut p_h: f32 = 0.0;
        let mut b_w: f32 = 0.0;
        let mut b_h: f32 = 0.0;

        // Note: For get_size_from_child, we don't have a parent size to resolve
        // percentages, so we can only accurately add Px values. Percentages
        // will be ignored or should be handled by the layout system during
        // actual resolution.

        if let Spacing::Px(v) = self.padding.left {
            p_w += v as f32;
        }
        if let Spacing::Px(v) = self.padding.right {
            p_w += v as f32;
        }
        if let Spacing::Px(v) = self.padding.top {
            p_h += v as f32;
        }
        if let Spacing::Px(v) = self.padding.bottom {
            p_h += v as f32;
        }

        if let Dimension::Px(v) = self.box_decoration.border.left.stroke {
            b_w += v;
        }
        if let Dimension::Px(v) = self.box_decoration.border.right.stroke {
            b_w += v;
        }
        if let Dimension::Px(v) = self.box_decoration.border.top.stroke {
            b_h += v;
        }
        if let Dimension::Px(v) = self.box_decoration.border.bottom.stroke {
            b_h += v;
        }

        if let Dimension::Px(w) = self.width {
            size.width = Dimension::Px(w + m_w);
        } else {
            size.width = match size.width {
                Dimension::Px(v) => Dimension::Px(v + m_w + p_w + b_w),
                other => other,
            };
        }

        if let Dimension::Px(h) = self.height {
            size.height = Dimension::Px(h + m_h);
        } else {
            size.height = match size.height {
                Dimension::Px(v) => Dimension::Px(v + m_h + p_h + b_h),
                other => other,
            };
        }

        Some(size)
    }

    fn invalidate_layout(&self) {
        self.cache.invalidate();
        self.child.invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bounds.pos_start_end()
    }
}

#[cfg(test)]
mod tests {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::rc::Rc;

    use aimer_cupid::draw_cmd::DrawCommand;
    use super::*;
    use crate::SizedBox;
    use aimer_widget::base::WindowHandle;
    use aimer_widget::{Drawable, EventElement, LayoutElement, Rebuildable, VisitorElement};

    struct DrawProbe {
        draws: Rc<Cell<usize>>,
    }

    impl Drawable for DrawProbe {
        fn update(&self, _ctx: &BuildContext) {
            self.draws.set(self.draws.get() + 1);
        }
    }

    impl EventElement for DrawProbe {}
    impl LayoutElement for DrawProbe {}
    impl Rebuildable for DrawProbe {}

    impl VisitorElement for DrawProbe {
        fn debug_name(&self) -> &'static str {
            "DrawProbe"
        }
    }

    struct PaintStableProbe;

    impl Drawable for PaintStableProbe {
        fn update(&self, _ctx: &BuildContext) {}

        fn is_paint_stable(&self) -> bool {
            true
        }
    }

    impl EventElement for PaintStableProbe {}
    impl LayoutElement for PaintStableProbe {}
    impl Rebuildable for PaintStableProbe {}

    impl VisitorElement for PaintStableProbe {
        fn debug_name(&self) -> &'static str {
            "PaintStableProbe"
        }
    }

    #[test]
    fn a_decorated_container_can_retain_its_stable_child_paint() {
        let mut container = RawContainer::new(PaintStableProbe);
        container.box_decoration = BoxDecoration::new()
            .background_color(Color::BLACK)
            .border_radius(8.0);

        assert!(container.is_paint_stable());
    }

    #[tokio::test]
    async fn retained_container_paint_includes_its_decoration() {
        let (ctx, inner) = recording_context();
        let mut element = RawContainer::new(PaintStableProbe);
        element.width = Dimension::Px(80.0);
        element.height = Dimension::Px(40.0);
        element.box_decoration = BoxDecoration::new()
            .background_color(Color::BLACK)
            .border_radius(8.0);

        let commands = record_local_v2(&element, &ctx);

        assert!(commands
            .iter()
            .any(|command| matches!(command, aimer_cupid::draw_cmd_v2::DrawCommand::FillRect { .. })));
        assert!(
            inner.draw_list().commands().is_empty(),
            "the canvas itself is not painted"
        );
    }

    #[test]
    fn containers_are_transparent_unless_they_occlude_scroll_and_never_cache_draw_bounds() {
        let child = DrawProbe {
            draws: Rc::new(Cell::new(0)),
        }
        .boxed();
        let mut container = RawContainer::new(child);

        assert_eq!(
            container.event_tree_role(),
            aimer_widget::EventTreeRole::Transparent
        );
        assert_eq!(container.event_tree_bounds(), None);

        container.color = Some(Color::BLACK);
        assert_eq!(
            container.event_tree_role(),
            aimer_widget::EventTreeRole::IndexedTarget
        );
    }

    /// Counts every call that reaches the system allocator on this thread.
    ///
    /// The count is what makes the migration's claim checkable: a widget hands
    /// its fields to its element instead of copying them, so a warm rebuild of a
    /// decorated tree must not reach the allocator at all.
    struct RecordingAllocator;

    thread_local! {
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }

    // SAFETY: Every method forwards to `System` unchanged; the counter only
    // observes the call.
    unsafe impl GlobalAlloc for RecordingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record();
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) };
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record();
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record();
            unsafe { System.realloc(pointer, layout, new_size) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: RecordingAllocator = RecordingAllocator;

    fn record() {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    }

    fn allocations() -> usize {
        ALLOCATIONS.with(Cell::get)
    }

    fn context() -> BuildContext<'static> {
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::FrameCanvas::new(inner)
        };
        BuildContext::new(
            canvas,
            ResolvedSize::default(),
            1.0,
            Default::default(),
            Default::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        )
    }

    fn recording_context() -> (BuildContext<'static>, &'static aimer_canvas::InnerCanvas) {
        let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
        let ctx = BuildContext::new(
            aimer_canvas::FrameCanvas::new(inner),
            ResolvedSize {
                width: 200.0,
                height: 160.0,
            },
            1.0,
            Default::default(),
            Default::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        );
        (ctx, inner)
    }

    /// Records `element`'s own retained list, as the frame loop does for a node.
    fn record_local_v2(
        element: &impl Drawable,
        ctx: &BuildContext<'_>,
    ) -> std::sync::Arc<[aimer_cupid::draw_cmd_v2::DrawCommand]> {
        let tree = aimer_cupid::draw_cmd_v2::RenderTree::new();
        let root = tree
            .add_root(aimer_cupid::draw_cmd_v2::Rect::new(0.0, 0.0, 400.0, 300.0))
            .unwrap();
        let node_context = tree.context(root).unwrap();
        ctx.with_local_v2_paint_context(node_context, |ctx| {
            assert!(element.can_paint_local_v2(ctx));
            element.paint_local_v2(ctx);
        });
        tree.draw_list_snapshot(root).unwrap().commands
    }

    #[tokio::test]
    async fn margin_keeps_outline_and_border_inside_the_decorated_box() {
        let (mut ctx, inner) = recording_context();
        ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 200.0,
            max_height: 160.0,
        };
        let border = BorderSlice::new()
            .style(BorderStyle::Solid)
            .stroke(4.0);
        let outline = BorderSlice::new()
            .style(BorderStyle::Solid)
            .stroke(6.0);

        let element = Container::new()
            .margin(LayoutSpacing::all(30))
            .padding(LayoutSpacing::all(8))
            .box_decoration(
                BoxDecoration::new()
                    .border(BoxBorder::all(border))
                    .outline(BoxOutline::all(outline)),
            )
            .child(crate::ZeroSizedBox)
            .to_element(&ctx);

        assert_eq!(
            element.content_size(&ctx),
            ResolvedSize {
                width: 116.0,
                height: 76.0,
            }
        );
        element.update(&ctx);

        let commands = inner.draw_list();
        assert!(matches!(commands.commands().first(), Some(DrawCommand::PushTransform { .. })));
        let margin_transform = commands.commands().get(1).and_then(|command| match command {
            DrawCommand::SetTransform { matrix } => Some(*matrix),
            _ => None,
        });
        assert_eq!(margin_transform.map(|matrix| matrix.cols[2]), Some([30.0, 30.0, 1.0]));

        // The decorated box is recorded in the node's own list, offset by the
        // margin, so the margin itself is never filled.
        let recorded = record_local_v2(&element, &ctx);
        let fill = recorded.iter().find_map(|command| match command {
            aimer_cupid::draw_cmd_v2::DrawCommand::FillRect {
                rect,
                border_width,
                outline_width,
                ..
            } => Some((*rect, *border_width, *outline_width)),
            _ => None,
        });
        let (fill, border_width, outline_width) =
            fill.expect("the decorated container should record one fill rectangle");
        assert_eq!((fill.x, fill.y), (30.0, 30.0), "the box starts after the margin");
        assert_eq!((fill.width, fill.height), (140.0, 100.0));
        assert_eq!(border_width, [4.0; 4]);
        assert_eq!(outline_width, [6.0; 4]);

        let transforms = commands
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::SetTransform { matrix } => Some(matrix.cols[2]),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(transforms, vec![[30.0, 30.0, 1.0], [42.0, 42.0, 1.0]]);
    }

    /// A decoration owning a shadow list, the field this migration is about.
    fn shadowed() -> BoxDecoration {
        BoxDecoration {
            box_shadow: vec![BoxShadow::default(), BoxShadow::default()],
            ..BoxDecoration::default()
        }
    }

    /// A 100x60 container with `decoration`, plus the radii it reports for its
    /// child's clip and whether it can paint locally.
    fn rounded_child_clip(decoration: BoxDecoration, scale: f32) -> (bool, [f32; 4]) {
        let (mut ctx, _inner) = recording_context();
        ctx.scale = scale;
        ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 400.0,
            max_height: 300.0,
        };
        let element = Container::new()
            .width(Dimension::Px(100.0))
            .height(Dimension::Px(60.0))
            .box_decoration(decoration)
            .child(crate::ZeroSizedBox)
            .to_element(&ctx);
        // The render tree walker asks the inner node, not the `AnyElement`
        // wrapper, which does not forward `can_paint_local_v2`.
        let node: &dyn Element = element.as_ref();
        let mut child = None;
        node.visit_children(&mut |element| child = Some(element));
        let child = child.expect("the container exposes its child");
        (
            node.can_paint_local_v2(&ctx),
            node.retained_v2_child_clip_radius(&ctx, child),
        )
    }

    #[tokio::test]
    async fn a_rounded_container_paints_locally_and_rounds_its_childs_clip() {
        let (local, radii) = rounded_child_clip(BoxDecoration::new().border_radius(12), 1.0);
        assert!(local, "a border radius must not force the legacy path");
        assert_eq!(radii, [12.0; 4]);
    }

    #[tokio::test]
    async fn a_square_container_reports_a_square_child_clip() {
        let (local, radii) = rounded_child_clip(BoxDecoration::new(), 1.0);
        assert!(local);
        assert_eq!(radii, [0.0; 4]);
    }

    #[tokio::test]
    async fn the_child_clip_radius_shrinks_by_the_border_like_the_live_path() {
        let border = BorderSlice::new().style(BorderStyle::Solid).stroke(4.0);
        let decoration = BoxDecoration::new()
            .border(BoxBorder::all(border))
            .border_radius(12);
        let (local, radii) = rounded_child_clip(decoration, 1.0);
        assert!(local);
        assert_eq!(radii, [8.0; 4]);
    }

    #[tokio::test]
    async fn a_radius_smaller_than_the_border_clamps_the_child_clip_to_square() {
        let border = BorderSlice::new().style(BorderStyle::Solid).stroke(4.0);
        let decoration = BoxDecoration::new()
            .border(BoxBorder::all(border))
            .border_radius(3);
        let (_, radii) = rounded_child_clip(decoration, 1.0);
        assert_eq!(radii, [0.0; 4]);
    }

    #[tokio::test]
    async fn the_child_clip_radius_is_reported_in_logical_pixels_at_any_scale() {
        let border = BorderSlice::new().style(BorderStyle::Solid).stroke(4.0);
        let decoration = BoxDecoration::new()
            .border(BoxBorder::all(border))
            .border_radius(12);
        let (_, radii) = rounded_child_clip(decoration, 2.0);
        assert_eq!(radii, [8.0; 4]);
    }

    /// A container with `decoration` whose parent leaves both axes unbounded in
    /// the given way, plus what its v2 geometry reports for its child.
    struct UnboundedProbe {
        can_paint: bool,
        clip: Option<aimer_cupid::draw_cmd_v2::Rect>,
        child_max: Option<(f32, f32)>,
        outsets: Option<[f32; 4]>,
    }

    fn unbounded_container(max_width: f32, max_height: f32) -> UnboundedProbe {
        let (mut ctx, _inner) = recording_context();
        ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width,
            max_height,
        };
        let element = Container::new()
            .box_decoration(BoxDecoration::new().background_color(Color::BLUE))
            .child(crate::ZeroSizedBox)
            .to_element(&ctx);
        let node: &dyn Element = element.as_ref();
        let mut child = None;
        node.visit_children(&mut |element| child = Some(element));
        let child = child.expect("the container exposes its child");
        UnboundedProbe {
            can_paint: node.can_paint_local_v2(&ctx),
            clip: node
                .retained_v2_child_geometry(&ctx, child)
                .and_then(|(_, clip)| clip),
            child_max: node
                .retained_v2_child_context(&ctx, child)
                .map(|ctx| (ctx.box_constraint.max_width, ctx.box_constraint.max_height)),
            outsets: node.retained_v2_paint_outsets(&ctx),
        }
    }

    #[tokio::test]
    async fn an_auto_height_container_in_a_scroll_view_paints_locally() {
        // A scrollable offers an unbounded height as `f32::MAX`.
        let probe = unbounded_container(400.0, f32::MAX);
        assert!(probe.can_paint, "an unbounded axis must not force the legacy path");

        // The render tree gets finite, sane rectangles: `f32::MAX` doubles to
        // infinity when scaled to device pixels.
        let clip = probe.clip.expect("the child is clipped");
        assert!(clip.width.is_finite() && clip.height.is_finite());
        assert!(clip.height <= LOCAL_V2_CONTAINER_EXTENT_LIMIT, "{clip:?}");
        assert!(probe.outsets.is_some_and(|outsets| outsets.iter().all(|v| v.is_finite())));
    }

    #[tokio::test]
    async fn the_child_still_sees_an_unbounded_axis_as_unbounded() {
        // Clamping the *render tree's* rectangles must not change layout: a
        // child sizes itself to its content only while its constraint stays
        // above the unbounded threshold.
        let probe = unbounded_container(400.0, f32::MAX);
        let (_, max_height) = probe.child_max.expect("child context");
        assert!(
            max_height > LOCAL_V2_CONTAINER_EXTENT_LIMIT,
            "child max height was clamped to {max_height}"
        );
    }

    #[tokio::test]
    async fn an_auto_width_container_under_an_unbounded_width_paints_locally() {
        // Row-like parents offer a quarter of `f32::MAX` per child.
        let probe = unbounded_container(f32::MAX / 4.0, 120.0);
        assert!(probe.can_paint);
        let clip = probe.clip.expect("the child is clipped");
        assert!(clip.width.is_finite() && clip.width <= LOCAL_V2_CONTAINER_EXTENT_LIMIT);
        let (max_width, _) = probe.child_max.expect("child context");
        assert!(max_width > LOCAL_V2_CONTAINER_EXTENT_LIMIT);
    }

    #[tokio::test]
    async fn a_bounded_container_is_unchanged_by_the_unbounded_handling() {
        let probe = unbounded_container(400.0, 300.0);
        assert!(probe.can_paint);
        let clip = probe.clip.expect("the child is clipped");
        assert_eq!((clip.width, clip.height), (400.0, 300.0));
        assert_eq!(probe.child_max, Some((400.0, 300.0)));
    }

    #[tokio::test]
    async fn offscreen_container_skips_unknown_child_draw_but_keeps_traversal_views() {
        let draws = Rc::new(Cell::new(0));
        let container = RawContainer::new(DrawProbe {
            draws: draws.clone(),
        });
        let mut ctx = context();
        ctx.parent_size = ResolvedSize {
            width: 100.0,
            height: 100.0,
        };
        ctx.box_constraint = aimer_attribute::BoxConstraint {
            min_width: 0.0,
            min_height: 0.0,
            max_width: 100.0,
            max_height: 100.0,
        };
        ctx.visible_rect = Some((0.0, 101.0, 100.0, 20.0));

        container.update(&ctx);

        assert_eq!(
            draws.get(),
            0,
            "a clipped container must not dynamically dispatch an offscreen child"
        );

        let mut structural = 0;
        container.structural_children(&mut |_| structural += 1);
        assert_eq!(structural, 1);

        let mut events = 0;
        container.event_children(&mut |_| events += 1);
        assert_eq!(events, 1);

        let mut hit_test = 0;
        container.hit_test_children(&mut |_| hit_test += 1);
        assert_eq!(hit_test, 1);

        ctx.visible_rect = Some((0.0, 0.0, 100.0, 100.0));
        container.update(&ctx);
        assert_eq!(
            draws.get(),
            1,
            "an unknown-bounds child remains drawable when its clipping parent is visible"
        );
    }

    #[test]
    fn the_counter_counts() {
        let before = allocations();
        let allocated = vec![0u8; 64];

        assert!(
            allocations() > before,
            "a test asserting zero allocations is worthless if nothing is counted"
        );
        drop(allocated);
    }

    #[tokio::test]
    async fn converting_a_decorated_container_adds_no_allocation() {
        let ctx = context();

        // Warm the pooled element storage the way a running application does:
        // the first frames pay for their blocks, the steady state reuses them.
        for _ in 0..4 {
            drop(
                Container::new()
                    .box_decoration(shadowed())
                    .child(SizedBox::new())
                    .to_element(&ctx),
            );
        }

        let before_decoration = allocations();
        let decoration = shadowed();
        let describing = allocations() - before_decoration;

        let before_build = allocations();
        drop(
            Container::new()
                .box_decoration(decoration)
                .child(SizedBox::new())
                .to_element(&ctx),
        );
        let building = allocations() - before_build;

        assert_eq!(
            describing, 1,
            "the shadow list is the one allocation a decorated container costs, \
             and it is paid when the decoration is described"
        );
        assert_eq!(
            building, 0,
            "the conversion must hand the shadow list to the element rather than \
             copy it, and must find its element block in the pool: a warm build \
             of a decorated container reaches the allocator zero times"
        );
    }

    /// A container's element has to fit the largest class the `aimer_rubick`
    /// pool serves — 512 bytes — or every build allocates it from the global
    /// allocator and every drop frees it again, because an oversized payload is
    /// unpooled and can never be recycled.
    ///
    /// The element is dominated by its [`BoxDecoration`], which is dominated by
    /// its eight border and outline colors. That is why a color is stored as one
    /// packed word: with a color wide enough to carry HSLA components this
    /// element measured 576 bytes and spilled out of the pool.
    #[test]
    fn a_container_element_fits_a_pooled_block() {
        eprintln!("Container Size : {}", size_of::<Container>());

        assert!(
            size_of::<RawContainer<AnyElement>>() <= 512,
            "a container element grew past the largest pooled class: {} bytes",
            size_of::<RawContainer<AnyElement>>()
        );
    }
}
