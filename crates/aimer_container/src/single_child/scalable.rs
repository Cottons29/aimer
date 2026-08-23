use aimer_attribute::size::ResolvedSize;
use aimer_macro::{EventElement, Rebuildable};
use aimer_widget::base::BuildContext;
#[cfg(feature = "portable-guest")]
use aimer_widget::portable::{PortableBuildContext, PortableBuildError, SourceFingerprint};
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, LayoutElement, RequiredChild, VisitorElement,
    Widget,
};

/// Applies a positive scale multiplier to a single child's build context.
///
/// The multiplier affects the child's drawing and resolved layout dimensions.
/// Finish the builder with [`Scalable::child`] or [`Scalable::box_child`].
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(
    id = "aimer_container::single_child::Scalable",
    validate = validate_portable_scalable
)]
pub struct Scalable<W = RequiredChild> {
    #[portable_optional]
    scale: f32,
    #[portable_child]
    child: W,
}

impl Scalable {
    /// Creates a builder at the identity scale.
    ///
    /// Finish the builder with [`Scalable::child`] or [`Scalable::box_child`].
    #[inline]
    pub fn new() -> Self {
        Self {
            child: RequiredChild,
            scale: 1.0,
        }
    }

    /// Sets the positive scale multiplier applied to the child.
    ///
    /// Native construction normalizes non-finite and non-positive values to
    /// `1.0`; portable lowering rejects them explicitly.
    #[inline]
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    /// Attaches the required child and completes this builder.
    #[inline]
    pub fn child<W: Widget>(self, child: W) -> Scalable<W> {
        Scalable {
            child,
            scale: self.scale,
        }
    }

    /// Attaches `child` and erases the completed widget's concrete type.
    #[inline]
    pub fn box_child<W: Widget + 'static>(self, child: W) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl Default for Scalable {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<W: Widget + 'static> Widget for Scalable<W> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        RawScalable {
            scale: normalized_scale(self.scale),
            child: self.child.to_element(ctx),
        }
        .boxed()
    }
}

#[cfg(feature = "portable-guest")]
fn validate_portable_scalable<W>(
    scalable: &Scalable<W>,
    _ctx: &PortableBuildContext,
    source: SourceFingerprint,
) -> Result<(), PortableBuildError> {
    if !scalable.scale.is_finite() || scalable.scale <= 0.0 {
        return Err(PortableBuildError::UnsupportedProperty {
            widget: "Scalable",
            property: "scale",
            source,
        });
    }
    Ok(())
}

#[inline]
fn normalized_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

#[derive(EventElement, Rebuildable)]
struct RawScalable {
    scale: f32,
    child: AnyElement,
}

impl RawScalable {
    #[inline]
    fn child_context<'a>(&self, ctx: &BuildContext<'a>) -> BuildContext<'a> {
        let mut child_ctx = ctx.clone();
        child_ctx.scale *= self.scale;
        child_ctx
    }
}

impl Drawable for RawScalable {
    fn draw(&self, ctx: &BuildContext) {
        self.child.draw(&self.child_context(ctx));
    }
}

impl LayoutElement for RawScalable {
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.computed_size(&self.child_context(ctx))
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.child.content_size(&self.child_context(ctx))
    }

    fn layer(&self) -> u32 {
        self.child.layer()
    }
}

impl VisitorElement for RawScalable {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "Scalable"
    }
}

#[cfg(all(test, feature = "portable-guest"))]
mod portable_tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use aimer_anteros::{PropertyValue, WidgetDocumentView, WidgetProperty};
    use aimer_widget::base::BuildContext;
    use aimer_widget::portable::{
        PortableBuildContext, PortableBuildError, PortableLimits, PortableWidgetLimits,
        PortableWidgetResource, PortableWidgetSchema, SourceFingerprint, StableId128,
    };
    use aimer_widget::{
        AnyElement, Drawable, Element, EventElement, LayoutElement, PortableWidget, Rebuildable,
        RequiredChild, VisitorElement, Widget,
    };

    use super::Scalable;
    use crate::ZeroSizedBox;

    fn source(value: u8) -> SourceFingerprint {
        SourceFingerprint::new(StableId128::from_bytes([value; 16]))
    }

    fn limits() -> PortableWidgetLimits {
        PortableWidgetLimits::new(4, 4, 4, 4, 64, 2_048)
    }

    fn context(limits: PortableWidgetLimits) -> PortableBuildContext {
        PortableBuildContext::new(
            3,
            1,
            limits,
            PortableLimits::new(8, 16, 64, 128, 1_024),
        )
        .unwrap()
    }

    struct ScaleProbe(Rc<Cell<f32>>);

    impl Drawable for ScaleProbe {
        fn draw(&self, ctx: &BuildContext) {
            self.0.set(ctx.scale);
        }
    }

    impl EventElement for ScaleProbe {}
    impl LayoutElement for ScaleProbe {}
    impl Rebuildable for ScaleProbe {}
    impl VisitorElement for ScaleProbe {
        fn debug_name(&self) -> &'static str {
            "ScaleProbe"
        }
    }

    struct ScaleProbeWidget(Rc<Cell<f32>>);

    impl PortableWidget for ScaleProbeWidget {}

    impl Widget for ScaleProbeWidget {
        fn to_element(self, _ctx: &BuildContext) -> AnyElement {
            ScaleProbe(self.0).boxed()
        }
    }

    #[test]
    fn scalable_lowering_preserves_scale_and_required_child() {
        let schema = <Scalable<RequiredChild> as PortableWidgetSchema>::SCHEMA;
        let scale_property = schema.properties()[0].id();
        let mut context = context(limits());
        let root = Scalable::new()
            .scale(1.75)
            .child(ZeroSizedBox::new())
            .to_portable_node(&mut context, source(1))
            .unwrap();
        let document = context.finish_document(root).unwrap();
        let bytes = document.encode().unwrap();
        let view = WidgetDocumentView::decode(&bytes, document.model_limits()).unwrap();
        let node = view.node(root.index()).unwrap();

        assert_eq!(
            schema.widget().canonical_name(),
            "aimer.widget:aimer_container::single_child::Scalable"
        );
        assert_eq!(
            schema.children(),
            aimer_anteros::ChildCardinality::exactly(1)
        );
        assert_eq!(node.children().collect::<Vec<_>>(), vec![0]);
        assert_eq!(
            node.properties().collect::<Vec<_>>(),
            vec![
                WidgetProperty::new(scale_property, PropertyValue::F64(1.75)).optional()
            ],
        );
    }

    #[test]
    fn scalable_lowering_is_bounded_and_rejects_non_positive_scale() {
        let mut bounded = context(limits().with_max_properties(0));
        assert!(matches!(
            Scalable::new()
                .scale(2.0)
                .child(ZeroSizedBox::new())
                .to_portable_node(&mut bounded, source(2)),
            Err(PortableBuildError::LimitExceeded {
                resource: PortableWidgetResource::Properties,
                max: 0,
                actual: 1,
            })
        ));

        let mut invalid = context(limits());
        assert!(matches!(
            Scalable::new()
                .scale(0.0)
                .child(ZeroSizedBox::new())
                .to_portable_node(&mut invalid, source(3)),
            Err(PortableBuildError::UnsupportedProperty {
                widget: "Scalable",
                property: "scale",
                ..
            })
        ));
    }

    #[test]
    fn scalable_native_materialization_applies_the_scale() {
        let observed = Rc::new(Cell::new(0.0));
        let context = BuildContext::portable();
        let element = Scalable::new()
            .scale(1.5)
            .child(ScaleProbeWidget(observed.clone()))
            .to_element(&context);

        element.draw(&context);

        assert_eq!(observed.get(), 1.5);
    }
}
