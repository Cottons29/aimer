use std::cell::{Cell, UnsafeCell};
use std::rc::Rc;

use aimer_attribute::position::Vec2d;
use aimer_attribute::size::{ResolvedSize, Size};
use aimer_events::element::ElementEvent;
use aimer_widget::base::*;
use aimer_widget::{
    AnyElement, AnyWidget, ChildBuilder, Drawable, Element, EventElement, EventResult,
    LayoutElement, Rebuildable, VisitorElement, Widget, carry_element_state,
};
use aimer_widget::portable::__anteros::{
    BUILTIN_WIDGET_SCHEMA_VERSION, ChildCardinality, PortableWidgetSchemaMetadata,
    PropertyId, PropertySchemaMetadata, PropertyValueKind, WidgetDocumentView, WidgetNodeView,
    WidgetSchemaMetadata,
};
use aimer_widget::portable::{
    PortableMaterializeError, PortableNativeWidget, PortableWidgetSchema,
    optional_materialized_property, required_materialized_property,
};
#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "illumos",
))]
use aimer_widget::portable::PortableNativeWidgetRegistration;

#[cfg(feature = "portable-guest")]
use aimer_widget::portable::{
    PortableBuildContext, PortableBuildError, PortableNodeId, SourceFingerprint,
};
#[cfg(feature = "portable-guest")]
use aimer_widget::portable::__anteros::PropertyValue;

use crate::control::controller::{AnimationController, AnimationStatus};
use crate::primitives::time::AnimInstant;

type AnimatedElementBuilder = dyn Fn(f32, &BuildContext) -> AnyElement;

#[cfg(feature = "portable-guest")]
type AnimatedPortableBuilder = dyn Fn(
    f32,
    &mut PortableBuildContext,
    SourceFingerprint,
) -> Result<PortableNodeId, PortableBuildError>;

const ANIMATED_BUILDER_WIDGET_CANONICAL_NAME: &str =
    "aimer.widget:aimer_animation::AnimatedBuilder";
const ANIMATED_BUILDER_DURATION_CANONICAL_NAME: &str =
    "aimer.property:aimer_animation::AnimatedBuilder:duration_millis";
const ANIMATED_BUILDER_CURVE_CANONICAL_NAME: &str =
    "aimer.property:aimer_animation::AnimatedBuilder:curve";
const ANIMATED_BUILDER_CURVE_X1_CANONICAL_NAME: &str =
    "aimer.property:aimer_animation::AnimatedBuilder:curve_x1";
const ANIMATED_BUILDER_CURVE_Y1_CANONICAL_NAME: &str =
    "aimer.property:aimer_animation::AnimatedBuilder:curve_y1";
const ANIMATED_BUILDER_CURVE_X2_CANONICAL_NAME: &str =
    "aimer.property:aimer_animation::AnimatedBuilder:curve_x2";
const ANIMATED_BUILDER_CURVE_Y2_CANONICAL_NAME: &str =
    "aimer.property:aimer_animation::AnimatedBuilder:curve_y2";
const ANIMATED_BUILDER_VALUE_CANONICAL_NAME: &str =
    "aimer.property:aimer_animation::AnimatedBuilder:value";
const ANIMATED_BUILDER_STATUS_CANONICAL_NAME: &str =
    "aimer.property:aimer_animation::AnimatedBuilder:status";
const ANIMATED_BUILDER_REPEAT_CANONICAL_NAME: &str =
    "aimer.property:aimer_animation::AnimatedBuilder:repeat";
const ANIMATED_BUILDER_AUTO_REVERSE_CANONICAL_NAME: &str =
    "aimer.property:aimer_animation::AnimatedBuilder:auto_reverse";

const PROPERTY_ANIMATED_BUILDER_DURATION: PropertyId =
    PropertyId::from_canonical_name(ANIMATED_BUILDER_DURATION_CANONICAL_NAME);
const PROPERTY_ANIMATED_BUILDER_CURVE: PropertyId =
    PropertyId::from_canonical_name(ANIMATED_BUILDER_CURVE_CANONICAL_NAME);
const PROPERTY_ANIMATED_BUILDER_CURVE_X1: PropertyId =
    PropertyId::from_canonical_name(ANIMATED_BUILDER_CURVE_X1_CANONICAL_NAME);
const PROPERTY_ANIMATED_BUILDER_CURVE_Y1: PropertyId =
    PropertyId::from_canonical_name(ANIMATED_BUILDER_CURVE_Y1_CANONICAL_NAME);
const PROPERTY_ANIMATED_BUILDER_CURVE_X2: PropertyId =
    PropertyId::from_canonical_name(ANIMATED_BUILDER_CURVE_X2_CANONICAL_NAME);
const PROPERTY_ANIMATED_BUILDER_CURVE_Y2: PropertyId =
    PropertyId::from_canonical_name(ANIMATED_BUILDER_CURVE_Y2_CANONICAL_NAME);
const PROPERTY_ANIMATED_BUILDER_VALUE: PropertyId =
    PropertyId::from_canonical_name(ANIMATED_BUILDER_VALUE_CANONICAL_NAME);
const PROPERTY_ANIMATED_BUILDER_STATUS: PropertyId =
    PropertyId::from_canonical_name(ANIMATED_BUILDER_STATUS_CANONICAL_NAME);
const PROPERTY_ANIMATED_BUILDER_REPEAT: PropertyId =
    PropertyId::from_canonical_name(ANIMATED_BUILDER_REPEAT_CANONICAL_NAME);
const PROPERTY_ANIMATED_BUILDER_AUTO_REVERSE: PropertyId =
    PropertyId::from_canonical_name(ANIMATED_BUILDER_AUTO_REVERSE_CANONICAL_NAME);

const ANIMATED_BUILDER_PROPERTIES: &[PropertySchemaMetadata<'static>] = &[
    PropertySchemaMetadata::from_canonical_name(
        ANIMATED_BUILDER_DURATION_CANONICAL_NAME,
        PropertyValueKind::I64,
    ),
    PropertySchemaMetadata::from_canonical_name(
        ANIMATED_BUILDER_CURVE_CANONICAL_NAME,
        PropertyValueKind::I64,
    ),
    PropertySchemaMetadata::from_canonical_name(
        ANIMATED_BUILDER_CURVE_X1_CANONICAL_NAME,
        PropertyValueKind::F64,
    )
    .optional(),
    PropertySchemaMetadata::from_canonical_name(
        ANIMATED_BUILDER_CURVE_Y1_CANONICAL_NAME,
        PropertyValueKind::F64,
    )
    .optional(),
    PropertySchemaMetadata::from_canonical_name(
        ANIMATED_BUILDER_CURVE_X2_CANONICAL_NAME,
        PropertyValueKind::F64,
    )
    .optional(),
    PropertySchemaMetadata::from_canonical_name(
        ANIMATED_BUILDER_CURVE_Y2_CANONICAL_NAME,
        PropertyValueKind::F64,
    )
    .optional(),
    PropertySchemaMetadata::from_canonical_name(
        ANIMATED_BUILDER_VALUE_CANONICAL_NAME,
        PropertyValueKind::F64,
    ),
    PropertySchemaMetadata::from_canonical_name(
        ANIMATED_BUILDER_STATUS_CANONICAL_NAME,
        PropertyValueKind::I64,
    ),
    PropertySchemaMetadata::from_canonical_name(
        ANIMATED_BUILDER_REPEAT_CANONICAL_NAME,
        PropertyValueKind::Bool,
    ),
    PropertySchemaMetadata::from_canonical_name(
        ANIMATED_BUILDER_AUTO_REVERSE_CANONICAL_NAME,
        PropertyValueKind::Bool,
    ),
];

pub(crate) const ANIMATED_BUILDER_SCHEMA: PortableWidgetSchemaMetadata<'static> =
    PortableWidgetSchemaMetadata::new(
        WidgetSchemaMetadata::from_canonical_name(
            ANIMATED_BUILDER_WIDGET_CANONICAL_NAME,
            BUILTIN_WIDGET_SCHEMA_VERSION,
            BUILTIN_WIDGET_SCHEMA_VERSION,
        ),
        ANIMATED_BUILDER_PROPERTIES,
        &[],
        ChildCardinality::exactly(1),
    );

/// A widget that rebuilds its child on every animation tick.
///
/// Unlike [`crate::widgets::Animated`], which applies a fixed visual effect,
/// `AnimatedBuilder` gives you the current animation value each frame so you
/// can build any widget based on it. The builder receives the controller's
/// curved value. It runs once when the element is created and again whenever
/// that value changes; redraws continue while the controller is animating.
///
/// # Example
/// ```rust
/// use std::time::Duration;
///
/// use aimer_animation::{AnimatedBuilder, AnimationController, Curve};
/// use aimer_widget::ErrorWidget;
///
/// let controller = AnimationController::new(Duration::from_millis(250), Curve::Linear);
/// let animated = AnimatedBuilder::new(controller, |value| {
///     ErrorWidget::new(format!("Progress: {:.0}%", value * 100.0))
/// });
/// ```
pub struct AnimatedBuilder {
    pub controller: AnimationController,
    builder: Rc<AnimatedElementBuilder>,
    #[cfg(feature = "portable-guest")]
    portable_builder: Rc<AnimatedPortableBuilder>,
}

impl AnimatedBuilder {
    /// Creates a builder driven by `controller`.
    ///
    /// The closure must be `'static` because the resulting element retains it.
    /// Construction does not start or reset the controller.
    pub fn new<F, W>(controller: AnimationController, builder: F) -> Self
    where
        F: Fn(f32) -> W + 'static,
        W: Widget,
    {
        let builder = Rc::new(builder);
        let native_builder = Rc::clone(&builder);
        #[cfg(feature = "portable-guest")]
        let portable_source = Rc::clone(&builder);
        let builder: Rc<AnimatedElementBuilder> =
            Rc::new(move |value: f32, ctx: &BuildContext| (native_builder)(value).to_element(ctx));
        #[cfg(feature = "portable-guest")]
        let portable_builder: Rc<AnimatedPortableBuilder> = Rc::new(move |value, ctx, source| {
            (portable_source)(value).to_portable_node(ctx, source)
        });
        Self {
            controller,
            builder,
            #[cfg(feature = "portable-guest")]
            portable_builder,
        }
    }
}

impl PortableWidgetSchema for AnimatedBuilder {
    const SCHEMA: PortableWidgetSchemaMetadata<'static> = ANIMATED_BUILDER_SCHEMA;
}

#[cfg(not(feature = "portable-guest"))]
impl aimer_widget::portable::PortableWidget for AnimatedBuilder {}

#[cfg(feature = "portable-guest")]
impl aimer_widget::portable::PortableWidget for AnimatedBuilder {
    fn to_portable_node(
        self,
        ctx: &mut PortableBuildContext,
        source: SourceFingerprint,
    ) -> Result<PortableNodeId, PortableBuildError> {
        let duration_millis = i64::try_from(self.controller.duration().as_millis()).map_err(|_| {
            property_encoding_error(
                PROPERTY_ANIMATED_BUILDER_DURATION,
                ANIMATED_BUILDER_DURATION_CANONICAL_NAME,
                source,
                PortableBuildError::InvalidPropertyValue {
                    rust_type: "Duration",
                },
            )
        })?;
        let value = self.controller.value();
        if !value.is_finite() {
            return Err(property_encoding_error(
                PROPERTY_ANIMATED_BUILDER_VALUE,
                ANIMATED_BUILDER_VALUE_CANONICAL_NAME,
                source,
                PortableBuildError::NonFiniteFloat,
            ));
        }
        let (curve_tag, control_points) = portable_curve(self.controller.curve());
        let mut properties = vec![
            aimer_widget::portable::__anteros::WidgetProperty::new(
                PROPERTY_ANIMATED_BUILDER_DURATION,
                PropertyValue::I64(duration_millis),
            ),
            aimer_widget::portable::__anteros::WidgetProperty::new(
                PROPERTY_ANIMATED_BUILDER_CURVE,
                PropertyValue::I64(curve_tag),
            ),
            aimer_widget::portable::__anteros::WidgetProperty::new(
                PROPERTY_ANIMATED_BUILDER_VALUE,
                PropertyValue::F64(value as f64),
            ),
            aimer_widget::portable::__anteros::WidgetProperty::new(
                PROPERTY_ANIMATED_BUILDER_STATUS,
                PropertyValue::I64(portable_status(self.controller.status())),
            ),
            aimer_widget::portable::__anteros::WidgetProperty::new(
                PROPERTY_ANIMATED_BUILDER_REPEAT,
                PropertyValue::Bool(self.controller.repeat()),
            ),
            aimer_widget::portable::__anteros::WidgetProperty::new(
                PROPERTY_ANIMATED_BUILDER_AUTO_REVERSE,
                PropertyValue::Bool(self.controller.auto_reverse()),
            ),
        ];
        if let Some([x1, y1, x2, y2]) = control_points {
            let points = [
                (PROPERTY_ANIMATED_BUILDER_CURVE_X1, ANIMATED_BUILDER_CURVE_X1_CANONICAL_NAME, x1),
                (PROPERTY_ANIMATED_BUILDER_CURVE_Y1, ANIMATED_BUILDER_CURVE_Y1_CANONICAL_NAME, y1),
                (PROPERTY_ANIMATED_BUILDER_CURVE_X2, ANIMATED_BUILDER_CURVE_X2_CANONICAL_NAME, x2),
                (PROPERTY_ANIMATED_BUILDER_CURVE_Y2, ANIMATED_BUILDER_CURVE_Y2_CANONICAL_NAME, y2),
            ];
            for (property, property_name, value) in points {
                if !value.is_finite() {
                    return Err(property_encoding_error(
                        property,
                        property_name,
                        source,
                        PortableBuildError::NonFiniteFloat,
                    ));
                }
                properties.push(
                    aimer_widget::portable::__anteros::WidgetProperty::new(
                        property,
                        PropertyValue::F64(value),
                    )
                    .optional(),
                );
            }
        }
        let curved_value = self.controller.curve().transform(value);
        let child = (self.portable_builder)(curved_value, ctx, source.child(0))?;
        ctx.push_node(
            ANIMATED_BUILDER_SCHEMA.widget().id(),
            BUILTIN_WIDGET_SCHEMA_VERSION,
            None,
            source,
            &properties,
            &[child],
        )
    }
}

#[cfg(feature = "portable-guest")]
fn property_encoding_error(
    property: PropertyId,
    property_name: &'static str,
    source: SourceFingerprint,
    cause: PortableBuildError,
) -> PortableBuildError {
    PortableBuildError::PropertyEncoding {
        property,
        property_name,
        source,
        cause: Box::new(cause),
    }
}

#[cfg(feature = "portable-guest")]
fn portable_curve(curve: crate::primitives::curve::Curve) -> (i64, Option<[f64; 4]>) {
    use crate::primitives::curve::Curve;

    match curve {
        Curve::Linear => (0, None),
        Curve::EaseIn => (1, None),
        Curve::EaseOut => (2, None),
        Curve::EaseInOut => (3, None),
        Curve::CubicBezier(x1, y1, x2, y2) => {
            (4, Some([x1 as f64, y1 as f64, x2 as f64, y2 as f64]))
        }
        Curve::Decelerate => (5, None),
        Curve::BounceOut => (6, None),
        Curve::BounceIn => (7, None),
        Curve::BounceInOut => (8, None),
        Curve::ElasticIn => (9, None),
        Curve::ElasticOut => (10, None),
        Curve::ElasticInOut => (11, None),
        Curve::FastOutSlowIn => (12, None),
        Curve::LinearOutSlowIn => (13, None),
        Curve::FastOutLinearIn => (14, None),
    }
}

#[cfg(feature = "portable-guest")]
fn portable_status(status: AnimationStatus) -> i64 {
    match status {
        AnimationStatus::Dismissed => 0,
        AnimationStatus::Forward => 1,
        AnimationStatus::Reverse => 2,
        AnimationStatus::Completed => 3,
    }
}

pub(crate) fn materialize_animated_builder(
    document: &WidgetDocumentView<'_>,
    node: WidgetNodeView<'_>,
    mut children: Vec<AnyWidget>,
) -> Result<AnyWidget, PortableMaterializeError> {
    if children.len() != 1 {
        return Err(PortableMaterializeError::InvalidChildCount {
            expected: 1,
            actual: children.len(),
        });
    }

    let duration_millis: i64 = required_materialized_property(
        document,
        &node,
        PROPERTY_ANIMATED_BUILDER_DURATION,
    )?;
    let duration_millis = u64::try_from(duration_millis).map_err(|_| {
        PortableMaterializeError::InvalidPropertyValue {
            property: PROPERTY_ANIMATED_BUILDER_DURATION,
        }
    })?;
    let curve = materialize_curve(document, &node)?;
    let value: f32 = required_materialized_property(
        document,
        &node,
        PROPERTY_ANIMATED_BUILDER_VALUE,
    )?;
    if !(0.0..=1.0).contains(&value) {
        return Err(PortableMaterializeError::InvalidPropertyValue {
            property: PROPERTY_ANIMATED_BUILDER_VALUE,
        });
    }
    let status: i64 = required_materialized_property(
        document,
        &node,
        PROPERTY_ANIMATED_BUILDER_STATUS,
    )?;
    let status = match status {
        0 => AnimationStatus::Dismissed,
        1 => AnimationStatus::Forward,
        2 => AnimationStatus::Reverse,
        3 => AnimationStatus::Completed,
        _ => {
            return Err(PortableMaterializeError::InvalidPropertyValue {
                property: PROPERTY_ANIMATED_BUILDER_STATUS,
            });
        }
    };
    let repeat: bool = required_materialized_property(
        document,
        &node,
        PROPERTY_ANIMATED_BUILDER_REPEAT,
    )?;
    let auto_reverse: bool = required_materialized_property(
        document,
        &node,
        PROPERTY_ANIMATED_BUILDER_AUTO_REVERSE,
    )?;

    let controller = AnimationController::with_millis(duration_millis, curve);
    controller.set_value(value);
    controller.set_repeat(repeat);
    controller.set_auto_reverse(auto_reverse);
    match status {
        AnimationStatus::Forward => controller.forward_from_first_tick(),
        AnimationStatus::Reverse => controller.reverse(),
        AnimationStatus::Dismissed | AnimationStatus::Completed => {}
    }

    let child = ChildBuilder::from_widget(
        children
            .pop()
            .ok_or(PortableMaterializeError::InvalidChildCount {
                expected: 1,
                actual: 0,
            })?,
    );
    Ok(AnimatedBuilder::new(controller, move |_| child.clone()).boxed())
}

fn materialize_curve(
    document: &WidgetDocumentView<'_>,
    node: &WidgetNodeView<'_>,
) -> Result<crate::primitives::curve::Curve, PortableMaterializeError> {
    use crate::primitives::curve::Curve;

    let tag: i64 = required_materialized_property(document, node, PROPERTY_ANIMATED_BUILDER_CURVE)?;
    let x1: Option<f64> = optional_materialized_property(
        document,
        node,
        PROPERTY_ANIMATED_BUILDER_CURVE_X1,
    )?;
    let y1: Option<f64> = optional_materialized_property(
        document,
        node,
        PROPERTY_ANIMATED_BUILDER_CURVE_Y1,
    )?;
    let x2: Option<f64> = optional_materialized_property(
        document,
        node,
        PROPERTY_ANIMATED_BUILDER_CURVE_X2,
    )?;
    let y2: Option<f64> = optional_materialized_property(
        document,
        node,
        PROPERTY_ANIMATED_BUILDER_CURVE_Y2,
    )?;
    let points = [x1, y1, x2, y2];
    match tag {
        0..=3 | 5..=14 => {
            if points.iter().any(Option::is_some) {
                return Err(PortableMaterializeError::InvalidPropertyValue {
                    property: PROPERTY_ANIMATED_BUILDER_CURVE,
                });
            }
            Ok(match tag {
                0 => Curve::Linear,
                1 => Curve::EaseIn,
                2 => Curve::EaseOut,
                3 => Curve::EaseInOut,
                5 => Curve::Decelerate,
                6 => Curve::BounceOut,
                7 => Curve::BounceIn,
                8 => Curve::BounceInOut,
                9 => Curve::ElasticIn,
                10 => Curve::ElasticOut,
                11 => Curve::ElasticInOut,
                12 => Curve::FastOutSlowIn,
                13 => Curve::LinearOutSlowIn,
                14 => Curve::FastOutLinearIn,
                _ => unreachable!("the range above excludes cubic bezier"),
            })
        }
        4 => {
            let [Some(x1), Some(y1), Some(x2), Some(y2)] = points else {
                return Err(PortableMaterializeError::InvalidPropertyValue {
                    property: PROPERTY_ANIMATED_BUILDER_CURVE,
                });
            };
            let x1 = finite_curve_coordinate(x1, PROPERTY_ANIMATED_BUILDER_CURVE_X1)?;
            let y1 = finite_curve_coordinate(y1, PROPERTY_ANIMATED_BUILDER_CURVE_Y1)?;
            let x2 = finite_curve_coordinate(x2, PROPERTY_ANIMATED_BUILDER_CURVE_X2)?;
            let y2 = finite_curve_coordinate(y2, PROPERTY_ANIMATED_BUILDER_CURVE_Y2)?;
            Ok(Curve::CubicBezier(x1, y1, x2, y2))
        }
        _ => Err(PortableMaterializeError::InvalidPropertyValue {
            property: PROPERTY_ANIMATED_BUILDER_CURVE,
        }),
    }
}

fn finite_curve_coordinate(
    value: f64,
    property: PropertyId,
) -> Result<f32, PortableMaterializeError> {
    let value = value as f32;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(PortableMaterializeError::InvalidPropertyValue { property })
    }
}

impl PortableNativeWidget for AnimatedBuilder {
    fn materialize_widget(
        document: &WidgetDocumentView<'_>,
        node: WidgetNodeView<'_>,
        children: Vec<AnyWidget>,
    ) -> Result<AnyWidget, PortableMaterializeError> {
        materialize_animated_builder(document, node, children)
    }
}

impl Widget for AnimatedBuilder {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let curved_value = self.controller.curve().transform(self.controller.value());
        let child = (self.builder)(curved_value, ctx);
        let window = ctx.window.clone();

        AnimatedBuilderElement {
            child: UnsafeCell::new(child),
            controller: self.controller.clone(),
            builder: self.builder.clone(),
            last_value: Cell::new(curved_value),
            window,
        }
        .boxed()
    }
}

/// The element produced by `AnimatedBuilder`.
///
/// On each draw, it ticks the controller, rebuilds the child from the
/// builder closure (which was captured at construction), and draws the result.
/// This approach means the child is rebuilt every frame while animating,
/// which is the intended behavior for responsive animations.
struct AnimatedBuilderElement {
    child: UnsafeCell<AnyElement>,
    controller: AnimationController,
    builder: Rc<AnimatedElementBuilder>,
    last_value: Cell<f32>,
    window: WindowHandle,
}

// Safety: rendering pipeline is single-threaded
unsafe impl Send for AnimatedBuilderElement {}
unsafe impl Sync for AnimatedBuilderElement {}

impl Drawable for AnimatedBuilderElement {
    fn draw(&self, ctx: &BuildContext) {
        let curved_value = self.controller.tick(AnimInstant::now());
        if curved_value != self.last_value.get() {
            let child = (self.builder)(curved_value, ctx);
            // The builder is called fresh whenever the curved value changes,
            // but its element still owns runtime state an ordinary rebuild
            // would hand over — carry it across so a new value does not also
            // wipe everything nested below it.
            carry_element_state(unsafe { &*self.child.get() }.as_ref(), child.as_ref(), ctx);
            unsafe { *self.child.get() = child };
            self.last_value.set(curved_value);
        }

        unsafe { &*self.child.get() }.draw(ctx);

        if self.controller.is_animating() {
            self.window.request_redraw();
        }
    }
}

impl VisitorElement for AnimatedBuilderElement {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(unsafe { &*self.child.get() }.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedBuilderElement"
    }
}

impl EventElement for AnimatedBuilderElement {
    fn on_event(&self, event: &ElementEvent) -> EventResult {
        unsafe { &*self.child.get() }.on_event(event)
    }

    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(unsafe { &*self.child.get() }.as_ref());
    }
}

impl Rebuildable for AnimatedBuilderElement {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        unsafe { &*self.child.get() }.rebuild_if_dirty(ctx);
    }
}

impl LayoutElement for AnimatedBuilderElement {
    fn pos(&self) -> Option<Vec2d> {
        unsafe { &*self.child.get() }.pos()
    }

    fn size(&self) -> Option<Size> {
        unsafe { &*self.child.get() }.size()
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe { &*self.child.get() }.computed_size(ctx)
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe { &*self.child.get() }.content_size(ctx)
    }

    fn get_size_from_child(&self) -> Option<Size> {
        unsafe { &*self.child.get() }.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        unsafe { &*self.child.get() }.invalidate_layout();
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "illumos",
))]
#[aimer_widget::portable::__linkme::distributed_slice(
    aimer_widget::portable::materializer::PORTABLE_NATIVE_WIDGET_SCHEMAS
)]
#[linkme(crate = aimer_widget::portable::__linkme)]
#[allow(non_upper_case_globals)]
pub(crate) static ANIMATED_BUILDER_NATIVE_SCHEMA:
    PortableWidgetSchemaMetadata<'static> = ANIMATED_BUILDER_SCHEMA;

#[cfg(any(
    target_os = "linux",
    target_os = "macos",
    target_os = "windows",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "illumos",
))]
#[aimer_widget::portable::__linkme::distributed_slice(
    aimer_widget::portable::materializer::PORTABLE_NATIVE_WIDGET_REGISTRATIONS
)]
#[linkme(crate = aimer_widget::portable::__linkme)]
#[allow(non_upper_case_globals)]
pub(crate) static ANIMATED_BUILDER_NATIVE_REGISTRATION: PortableNativeWidgetRegistration =
    PortableNativeWidgetRegistration::new(
        ANIMATED_BUILDER_SCHEMA,
        materialize_animated_builder,
    );

/// Ensures this crate's native portable-widget registration is retained when
/// the crate is linked from an archive.
///
/// Linker-section registrations are discovered only from object files that
/// make it into the final link. Hosts that use animation types only at the
/// type level can call this anchor before reading the portable registry.
#[doc(hidden)]
#[inline(never)]
pub fn ensure_portable_native_registrations() {
    #[cfg(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "illumos",
    ))]
    {
        std::hint::black_box(&ANIMATED_BUILDER_NATIVE_SCHEMA);
        std::hint::black_box(&ANIMATED_BUILDER_NATIVE_REGISTRATION);
    }
}

#[cfg(test)]
mod tests {
    use aimer_widget::portable::__anteros::{
        ChildCardinality, ModelLimits, PropertyId, PropertyPresence,
        PropertyValue, Version, WidgetDocument, WidgetDocumentView, WidgetNode, WidgetProperty,
        WidgetSchemaId,
    };
    use aimer_widget::portable::{
        linked_portable_native_widget_registrations, linked_portable_native_widget_schemas,
        PortableNativeWidgetRegistration, PortableWidgetSchema,
    };
    use aimer_widget::{AnyElement, PortableWidget, Widget};

    use crate::Curve;

    use super::*;

    const WIDGET_CANONICAL_NAME: &str = "aimer.widget:aimer_animation::AnimatedBuilder";
    const PROPERTY_DURATION_CANONICAL_NAME: &str =
        "aimer.property:aimer_animation::AnimatedBuilder:duration_millis";
    const PROPERTY_CURVE_CANONICAL_NAME: &str =
        "aimer.property:aimer_animation::AnimatedBuilder:curve";
    const PROPERTY_CURVE_X1_CANONICAL_NAME: &str =
        "aimer.property:aimer_animation::AnimatedBuilder:curve_x1";
    const PROPERTY_CURVE_Y1_CANONICAL_NAME: &str =
        "aimer.property:aimer_animation::AnimatedBuilder:curve_y1";
    const PROPERTY_CURVE_X2_CANONICAL_NAME: &str =
        "aimer.property:aimer_animation::AnimatedBuilder:curve_x2";
    const PROPERTY_CURVE_Y2_CANONICAL_NAME: &str =
        "aimer.property:aimer_animation::AnimatedBuilder:curve_y2";
    const PROPERTY_VALUE_CANONICAL_NAME: &str =
        "aimer.property:aimer_animation::AnimatedBuilder:value";
    const PROPERTY_STATUS_CANONICAL_NAME: &str =
        "aimer.property:aimer_animation::AnimatedBuilder:status";
    const PROPERTY_REPEAT_CANONICAL_NAME: &str =
        "aimer.property:aimer_animation::AnimatedBuilder:repeat";
    const PROPERTY_AUTO_REVERSE_CANONICAL_NAME: &str =
        "aimer.property:aimer_animation::AnimatedBuilder:auto_reverse";

    fn property_id(canonical_name: &str) -> PropertyId {
        PropertyId::from_canonical_name(canonical_name)
    }

    struct PortableLeaf;

    impl Widget for PortableLeaf {
        fn to_element(self, _ctx: &aimer_widget::base::BuildContext) -> AnyElement {
            panic!("portable test leaf must not build natively");
        }
    }

    impl PortableWidget for PortableLeaf {
        #[cfg(feature = "portable-guest")]
        fn to_portable_node(
            self,
            ctx: &mut aimer_widget::portable::PortableBuildContext,
            source: aimer_widget::portable::SourceFingerprint,
        ) -> Result<
            aimer_widget::portable::PortableNodeId,
            aimer_widget::portable::PortableBuildError,
        > {
            ctx.push_node(
                WidgetSchemaId::from_canonical_name("aimer.widget:test::PortableLeaf"),
                Version::new(1, 0),
                None,
                source,
                &[],
                &[],
            )
        }
    }

    fn valid_properties() -> Vec<WidgetProperty> {
        vec![
            WidgetProperty::new(property_id(PROPERTY_DURATION_CANONICAL_NAME), PropertyValue::I64(321)),
            WidgetProperty::new(property_id(PROPERTY_CURVE_CANONICAL_NAME), PropertyValue::I64(4)),
            WidgetProperty::new(property_id(PROPERTY_CURVE_X1_CANONICAL_NAME), PropertyValue::F64(0.1))
                .optional(),
            WidgetProperty::new(property_id(PROPERTY_CURVE_Y1_CANONICAL_NAME), PropertyValue::F64(0.2))
                .optional(),
            WidgetProperty::new(property_id(PROPERTY_CURVE_X2_CANONICAL_NAME), PropertyValue::F64(0.8))
                .optional(),
            WidgetProperty::new(property_id(PROPERTY_CURVE_Y2_CANONICAL_NAME), PropertyValue::F64(0.9))
                .optional(),
            WidgetProperty::new(property_id(PROPERTY_VALUE_CANONICAL_NAME), PropertyValue::F64(0.25)),
            WidgetProperty::new(property_id(PROPERTY_STATUS_CANONICAL_NAME), PropertyValue::I64(1)),
            WidgetProperty::new(property_id(PROPERTY_REPEAT_CANONICAL_NAME), PropertyValue::Bool(true)),
            WidgetProperty::new(
                property_id(PROPERTY_AUTO_REVERSE_CANONICAL_NAME),
                PropertyValue::Bool(true),
            ),
        ]
    }

    fn host_document<R>(properties: &[WidgetProperty], child_count: usize, inspect: impl FnOnce(
        &WidgetDocumentView<'_>,
        aimer_widget::portable::__anteros::WidgetNodeView<'_>,
    ) -> R) -> R {
        let child_indices = [1_u32, 2_u32];
        let mut nodes = Vec::with_capacity(child_count + 1);
        nodes.push(
            WidgetNode::new(
                WidgetSchemaId::from_canonical_name(WIDGET_CANONICAL_NAME),
                Version::new(1, 0),
            )
            .properties(properties)
            .children(&child_indices[..child_count]),
        );
        for index in 0..child_count {
            nodes.push(WidgetNode::new(
                WidgetSchemaId::from_canonical_name(if index == 0 {
                    "aimer.widget:test::Child1"
                } else {
                    "aimer.widget:test::Child2"
                }),
                Version::new(1, 0),
            ));
        }
        let image = WidgetDocument::new(1, 0, 0, &nodes, &[], &[])
            .encode(ModelLimits::new(4_096, 32, 64, 128))
            .unwrap();
        let document = WidgetDocumentView::decode(
            &image,
            ModelLimits::new(4_096, 32, 64, 128),
        )
        .unwrap();
        inspect(&document, document.node(0).unwrap())
    }

    #[test]
    fn schema_declares_controller_properties_and_one_structural_child() {
        let schema = <AnimatedBuilder as PortableWidgetSchema>::SCHEMA;

        assert_eq!(schema.widget().id(), WidgetSchemaId::from_canonical_name(WIDGET_CANONICAL_NAME));
        assert_eq!(schema.children(), ChildCardinality::exactly(1));
        assert_eq!(schema.properties().len(), 10);
        assert_eq!(
            schema.properties()[0].id(),
            property_id(PROPERTY_DURATION_CANONICAL_NAME)
        );
        assert_eq!(schema.properties()[2].presence(), PropertyPresence::Optional);
        assert_eq!(schema.properties()[5].presence(), PropertyPresence::Optional);
    }

    #[cfg(feature = "portable-guest")]
    fn portable_context() -> aimer_widget::portable::PortableBuildContext {
        aimer_widget::portable::PortableBuildContext::new(
            1,
            0,
            aimer_widget::portable::PortableWidgetLimits::new(8, 32, 8, 8, 64, 4_096),
            aimer_widget::portable::PortableLimits::new(8, 16, 64, 128, 4_096),
        )
        .unwrap()
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn guest_lowering_round_trips_controller_configuration_and_child() {
        let mut context = portable_context();
        let source = aimer_widget::portable::SourceFingerprint::new(
            aimer_widget::portable::StableId128::from_bytes([9; 16]),
        );
        let root = AnimatedBuilder::new(
            AnimationController::with_millis(
                321,
                Curve::CubicBezier(0.1, 0.2, 0.8, 0.9),
            ),
            |_| PortableLeaf,
        )
        .to_portable_node(&mut context, source)
        .unwrap();
        let document = context.finish_document(root).unwrap();
        let limits = document.model_limits();
        let image = document.encode().unwrap();
        let view = WidgetDocumentView::decode(&image, limits).unwrap();
        let node = view.node(view.root_node()).unwrap();

        assert_eq!(node.widget_type(), WidgetSchemaId::from_canonical_name(WIDGET_CANONICAL_NAME));
        assert_eq!(node.children().count(), 1);
        assert_eq!(
            node.properties()
                .find(|property| property.property_id() == property_id(PROPERTY_DURATION_CANONICAL_NAME))
                .unwrap()
                .value(),
            PropertyValue::I64(321)
        );
        assert_eq!(
            node.properties()
                .find(|property| property.property_id() == property_id(PROPERTY_CURVE_CANONICAL_NAME))
                .unwrap()
                .value(),
            PropertyValue::I64(4)
        );
        assert_eq!(
            node.properties()
                .find(|property| property.property_id() == property_id(PROPERTY_VALUE_CANONICAL_NAME))
                .unwrap()
                .value(),
            PropertyValue::F64(0.0)
        );

        let child = vec![PortableLeaf.boxed()];
        let materialized = materialize_animated_builder(&view, node, child).unwrap();
        assert!(materialized.is_inline() || materialized.is_heap());
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn guest_lowering_rejects_non_finite_curve_control_points() {
        let mut context = portable_context();
        let source = aimer_widget::portable::SourceFingerprint::new(
            aimer_widget::portable::StableId128::from_bytes([7; 16]),
        );
        let error = AnimatedBuilder::new(
            AnimationController::with_millis(
                321,
                Curve::CubicBezier(f32::NAN, 0.2, 0.8, 0.9),
            ),
            |_| PortableLeaf,
        )
        .to_portable_node(&mut context, source)
        .unwrap_err();

        assert!(matches!(
            error,
            aimer_widget::portable::PortableBuildError::PropertyEncoding {
                property,
                cause,
                ..
            } if property == property_id(PROPERTY_CURVE_X1_CANONICAL_NAME)
                && matches!(*cause, aimer_widget::portable::PortableBuildError::NonFiniteFloat)
        ));
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "illumos",
    ))]
    #[test]
    fn linker_registration_exposes_animated_builder_constructor() {
        let widget_type = WidgetSchemaId::from_canonical_name(WIDGET_CANONICAL_NAME);
        let schema = linked_portable_native_widget_schemas()
            .iter()
            .copied()
            .find(|schema| schema.widget().id() == widget_type)
            .expect("AnimatedBuilder must publish a host schema");
        assert_eq!(schema.children(), ChildCardinality::exactly(1));
        assert_eq!(schema.properties().len(), 10);

        let registration = linked_portable_native_widget_registrations()
            .iter()
            .copied()
            .find(|registration| registration.widget_type() == widget_type)
            .expect("AnimatedBuilder must publish a host registration");

        assert_eq!(registration.schema().children(), ChildCardinality::exactly(1));
        assert_eq!(registration.schema().properties().len(), 10);
        let _: PortableNativeWidgetRegistration = registration;
    }

    #[test]
    fn host_materializer_rejects_invalid_controller_properties_and_children() {
        let mut properties = valid_properties();
        properties[0] = WidgetProperty::new(
            property_id(PROPERTY_DURATION_CANONICAL_NAME),
            PropertyValue::I64(-1),
        );
        host_document(&properties, 1, |document, node| {
            assert!(matches!(
                materialize_animated_builder(document, node, vec![PortableLeaf.boxed()]),
                Err(aimer_widget::portable::PortableMaterializeError::InvalidPropertyValue {
                    property
                }) if property == property_id(PROPERTY_DURATION_CANONICAL_NAME)
            ));
        });

        let mut properties = valid_properties();
        properties[1] = WidgetProperty::new(
            property_id(PROPERTY_CURVE_CANONICAL_NAME),
            PropertyValue::I64(99),
        );
        host_document(&properties, 1, |document, node| {
            assert!(matches!(
                materialize_animated_builder(document, node, vec![PortableLeaf.boxed()]),
                Err(aimer_widget::portable::PortableMaterializeError::InvalidPropertyValue {
                    property
                }) if property == property_id(PROPERTY_CURVE_CANONICAL_NAME)
            ));
        });

        let properties = valid_properties();
        host_document(&properties, 0, |document, node| {
            assert!(matches!(
                materialize_animated_builder(document, node, Vec::new()),
                Err(aimer_widget::portable::PortableMaterializeError::InvalidChildCount {
                    expected: 1,
                    actual: 0,
                })
            ));
        });
    }
}
