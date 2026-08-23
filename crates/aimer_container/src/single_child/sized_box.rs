use aimer_attribute::dimension::Dimension;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_macro::{EventElement, PortableWidget, Rebuildable};
use aimer_widget::base::{Color, *};
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, LayoutCache, LayoutElement, VisitorElement, Widget,
};
#[cfg(feature = "portable-guest")]
use aimer_widget::portable::{
    PortableBuildContext, PortableBuildError, SourceFingerprint,
};

use crate::ZeroSizedBox;

/// A single-child box with optional explicit dimensions and background color.
///
/// Attach a child with [`SizedBox::child`] to retain its concrete type, or with
/// [`SizedBox::box_child`] when branches need a shared erased type.
#[derive(PortableWidget)]
#[portable_widget(
    id = "aimer_container::single_child::SizedBox",
    schema_only,
    validate = validate_portable_sized_box
)]
pub struct SizedBox<W: Widget + 'static = ZeroSizedBox> {
    #[portable_optional]
    width: Dimension,
    #[portable_optional]
    height: Dimension,
    #[portable_skip]
    color: Color,
    #[portable_skip]
    child: Option<W>,
}

impl Default for SizedBox {
    fn default() -> Self {
        Self::new()
    }
}

impl SizedBox {
    /// Creates a transparent, automatically sized box without a child.
    ///
    /// The box is already a valid widget; use [`SizedBox::child`] or
    /// [`SizedBox::box_child`] to attach content.
    #[inline]
    pub fn new() -> Self {
        Self {
            width: Dimension::Px(0.0),
            height: Dimension::Px(0.0),
            color: Color::Transparent,
            child: None,
        }
    }

    /// Sets the preferred width.
    ///
    /// The default is [`Dimension::Auto`], which derives width from the child
    /// or zero when no child exists. Pixel values are logical pixels,
    /// percentages resolve against the parent's maximum width, and
    /// constraints still apply.
    #[inline]
    pub fn width(mut self, width: impl Into<Dimension>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the preferred height.
    ///
    /// The default is [`Dimension::Auto`], which derives height from the child
    /// or zero when no child exists. Pixel values are logical pixels,
    /// percentages resolve against the parent's maximum height, and
    /// constraints still apply.
    #[inline]
    pub fn height(mut self, height: impl Into<Dimension>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the box's fill color.
    ///
    /// The default is [`Color::Transparent`]. The color fills the box's
    /// resolved bounds before its child is drawn.
    #[inline]
    pub fn color(mut self, color: impl Into<Color>) -> Self {
        self.color = color.into();
        self
    }

    /// Attaches or replaces the optional child.
    ///
    /// `SizedBox::new()` is already valid without content. This operation
    /// preserves the concrete child type; use [`SizedBox::box_child`] when
    /// branches need an erased type.
    #[inline]
    pub fn child<W: Widget>(self, child: W) -> SizedBox<W> {
        SizedBox {
            width: self.width,
            height: self.height,
            color: self.color,
            child: Some(child),
        }
    }

    /// Attaches `child` and erases the resulting widget's concrete type.
    ///
    /// This is equivalent to calling [`SizedBox::child`] followed by
    /// [`Widget::boxed`]. Use it when different branches must return one
    /// [`AnyWidget`] type.
    #[inline]
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl SizedBox {
    pub const PLACE_HOLDER: Option<ZeroSizedBox> = Some(ZeroSizedBox);
}

impl<W: Widget + 'static> Widget for SizedBox<W> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let child = self
            .child
            .map(|child| child.to_element(ctx))
            .unwrap_or_else(|| ZeroSizedBox.to_element(ctx));
        RawSizedBox {
            width: self.width,
            height: self.height,
            child,
            color: self.color,
            cache: LayoutCache::new(),
            debug_name: "SizedBox",
            bounds: std::cell::Cell::new(None),
        }
        .boxed()
    }
}

#[cfg(feature = "portable-guest")]
fn validate_portable_sized_box<W: Widget + 'static>(
    sized_box: &SizedBox<W>,
    ctx: &PortableBuildContext,
    source: SourceFingerprint,
) -> Result<(), PortableBuildError> {
    if sized_box.child.is_some() {
        return Err(ctx.unsupported_widget("SizedBox.child", source));
    }
    if sized_box.color != Color::Transparent {
        return Err(ctx.unsupported_widget("SizedBox.color", source));
    }
    for (dimension, property) in [
        (sized_box.width, "SizedBox.width"),
        (sized_box.height, "SizedBox.height"),
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
#[derive(Rebuildable, EventElement)]
pub struct RawSizedBox<E: Element> {
    pub(crate) width: Dimension,
    pub(crate) height: Dimension,
    pub(crate) color: Color,
    pub(crate) child: E,
    pub(crate) cache: LayoutCache,
    pub(crate) debug_name: &'static str,
    pub(crate) bounds: std::cell::Cell<Option<(Vec2d, Vec2d)>>,
}

impl<E: Element> Drawable for RawSizedBox<E> {
    fn draw(&self, ctx: &BuildContext) {
        let size = self.computed_size(ctx);
        let width = size.width;
        let height = size.height;

        #[cfg(debug_assertions)]
        {
            if aimer_widget::inspector_overlay::is_enabled() {
                let (start_x, start_y) = ctx.canvas.get_transform_translation();
                let end_x = start_x + width;
                let end_y = start_y + height;

                let scale = ctx.scale;
                let l_start = Vec2d {
                    x: start_x / scale,
                    y: start_y / scale,
                };
                let l_end = Vec2d {
                    x: end_x / scale,
                    y: end_y / scale,
                };
                self.bounds.set(Some((l_start, l_end)));

                let cp = ctx.cursor_pos;
                if cp.x >= l_start.x
                    && cp.x <= l_end.x
                    && cp.y >= l_start.y
                    && cp.y <= l_end.y
                    && let Ok(mut hovered) = aimer_widget::inspector_overlay::HOVERED_WIDGET.write()
                {
                    *hovered = Some((self.debug_name, l_start, l_end));
                }
            }
        }

        ctx.canvas.fill_color_rect(
            Vec2d { x: 0.0, y: 0.0 },
            ResolvedSize { width, height },
            self.color,
            [0.0; 4],
        );
    }
}

impl<E: Element> VisitorElement for RawSizedBox<E> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(&self.child);
    }

    fn debug_name(&self) -> &'static str {
        self.debug_name
    }
}

impl<E: Element> LayoutElement for RawSizedBox<E> {
    fn size(&self) -> Option<Size> {
        match (self.width, self.height) {
            (Dimension::Px(w), Dimension::Px(h)) => Some(Size {
                width: Dimension::Px(w),
                height: Dimension::Px(h),
            }),
            _ => None,
        }
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        let scale_bits = ctx.scale.to_bits();
        if let Some(cached) = self.cache.get_computed(ctx.box_constraint, scale_bits) {
            return cached;
        }

        let scale = ctx.scale;

        let mut child_ctx = BuildContext {
            parent_size: ctx.parent_size,
            canvas: ctx.canvas.clone(),
            scale: ctx.scale,
            parent_pos: ctx.parent_pos,
            cursor_pos: ctx.cursor_pos,
            box_constraint: ctx.box_constraint,
            visible_rect: ctx.visible_rect,
            window: ctx.window.clone(),
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: ctx.async_handle.clone(),
            inherited_states: ctx.inherited_states.clone(),
        };

        child_ctx.box_constraint.max_width =
            self.width.resolve(ctx.box_constraint.max_width, scale);
        child_ctx.box_constraint.max_height =
            self.height.resolve(ctx.box_constraint.max_height, scale);

        let width = match self.width {
            Dimension::Px(w) => w * scale,
            Dimension::Percent(p) => ctx.box_constraint.max_width * (p / 100.0),
            Dimension::Auto => self.child.computed_size(&child_ctx).width,
        };

        let height = match self.height {
            Dimension::Px(h) => h * scale,
            Dimension::Percent(p) => ctx.box_constraint.max_height * (p / 100.0),
            Dimension::Auto => self.child.computed_size(&child_ctx).height,
        };

        let result = ResolvedSize { width, height };
        self.cache
            .set_computed(ctx.box_constraint, scale_bits, result);
        result
    }

    fn get_size_from_child(&self) -> Option<Size> {
        let mut size = self.child.get_size_from_child().unwrap_or_default();
        if let Dimension::Px(_) = self.width {
            size.width = self.width;
        }
        if let Dimension::Px(_) = self.height {
            size.height = self.height;
        }
        Some(size)
    }

    fn invalidate_layout(&self) {
        self.cache.invalidate();
        self.child.invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.bounds.get()
    }
}

#[cfg(all(test, feature = "portable-guest"))]
mod portable_tests {
    use super::*;
    use aimer_anteros::{
        PropertyValue, Version, WidgetDocumentView, WidgetProperty, PROPERTY_SIZED_BOX_HEIGHT,
        PROPERTY_SIZED_BOX_WIDTH, WIDGET_SIZED_BOX,
    };
    use aimer_widget::portable::{
        PortableBuildContext, PortableBuildError, PortableLimits, PortableWidgetLimits,
        PortableWidgetResource, PortableWidgetSchema, SourceFingerprint, StableId128,
    };
    use aimer_widget::PortableWidget;

    fn source(value: u8) -> SourceFingerprint {
        SourceFingerprint::new(StableId128::from_bytes([value; 16]))
    }

    fn limits() -> PortableWidgetLimits {
        PortableWidgetLimits::new(8, 8, 8, 8, 64, 2_048)
    }

    fn context(limits: PortableWidgetLimits) -> PortableBuildContext {
        PortableBuildContext::new(
            7,
            11,
            limits,
            PortableLimits::new(8, 16, 64, 128, 1_024),
        )
        .unwrap()
    }

    #[test]
    fn sized_box_publishes_its_reflected_schema() {
        let schema = <SizedBox<ZeroSizedBox> as PortableWidgetSchema>::SCHEMA;

        assert_eq!(schema.widget().id(), WIDGET_SIZED_BOX);
        assert_eq!(schema.children().minimum(), 0);
        assert_eq!(schema.children().maximum(), 0);
        assert_eq!(schema.properties().len(), 2);
        assert_eq!(schema.properties()[0].id(), PROPERTY_SIZED_BOX_WIDTH);
        assert_eq!(schema.properties()[1].id(), PROPERTY_SIZED_BOX_HEIGHT);
    }

    #[test]
    fn sized_box_lowers_exact_bounded_widget_ir() {
        let source = source(5);
        let mut ctx = context(limits());
        let expected_key = ctx.slot_for(None, source).to_bytes();
        let root = SizedBox::new()
            .width(32.0)
            .height(16.0)
            .to_portable_node(&mut ctx, source)
            .unwrap();
        let document = ctx.finish_document(root).unwrap();
        let bytes = document.encode().unwrap();
        let view = WidgetDocumentView::decode(&bytes, document.model_limits()).unwrap();
        let node = view.node(0).unwrap();

        assert_eq!(view.node_count(), 1);
        assert_eq!(node.widget_type(), WIDGET_SIZED_BOX);
        assert_eq!(node.widget_schema(), Version::new(1, 0));
        assert_eq!(node.key().unwrap().as_bytes(), &expected_key);
        assert_eq!(node.children().count(), 0);
        assert_eq!(
            node.properties().collect::<Vec<_>>(),
            vec![
                WidgetProperty::new(PROPERTY_SIZED_BOX_WIDTH, PropertyValue::F64(32.0)).optional(),
                WidgetProperty::new(PROPERTY_SIZED_BOX_HEIGHT, PropertyValue::F64(16.0)).optional(),
            ]
        );
    }

    #[test]
    fn sized_box_portable_lowering_enforces_property_and_key_limits() {
        let source = source(6);
        let mut properties = context(limits().with_max_properties(1));
        assert!(matches!(
            SizedBox::new()
                .width(10.0)
                .height(20.0)
                .to_portable_node(&mut properties, source),
            Err(PortableBuildError::LimitExceeded {
                resource: PortableWidgetResource::Properties,
                max: 1,
                actual: 2,
            })
        ));

        let mut keys = context(limits().with_max_keys(0));
        assert!(matches!(
            SizedBox::new().to_portable_node(&mut keys, source),
            Err(PortableBuildError::LimitExceeded {
                resource: PortableWidgetResource::Keys,
                max: 0,
                actual: 1,
            })
        ));
    }

    #[test]
    fn sized_box_rejects_schema_unsupported_native_features() {
        let source = source(7);
        let mut child_context = context(limits());
        assert!(matches!(
            SizedBox::new()
                .child(SizedBox::new())
                .to_portable_node(&mut child_context, source),
            Err(PortableBuildError::UnsupportedWidget {
                widget: "SizedBox.child",
                source: actual,
            }) if actual == source
        ));

        let mut color_context = context(limits());
        assert!(matches!(
            SizedBox::new()
                .color(Color::HexA(0xFFFFFFFF))
                .to_portable_node(&mut color_context, source),
            Err(PortableBuildError::UnsupportedWidget {
                widget: "SizedBox.color",
                source: actual,
            }) if actual == source
        ));

        let mut dimension_context = context(limits());
        assert!(matches!(
            SizedBox::new()
                .width(Dimension::Percent(50.0))
                .to_portable_node(&mut dimension_context, source),
            Err(PortableBuildError::UnsupportedWidget {
                widget: "SizedBox.width",
                source: actual,
            }) if actual == source
        ));
    }
}
