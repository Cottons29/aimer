use std::cell::Cell;
use std::future::Future;
use std::marker::PhantomData;
use std::panic::Location;
use std::rc::Rc;

use aimer_container::Container;
use aimer_style::BoxDecoration;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    AnyElement, AnyWidget, ChildBuilder, Key, RequiredChild, State, StateUpdater,
    StatefulElement, StatefulWidget, Widget,
};

#[cfg(feature = "portable-guest")]
use aimer_widget::portable::{
    PortableBuildContext, PortableBuildError, SourceFingerprint,
};

use crate::callback::VoidCallback;
use crate::gesture::GestureEvent;
use crate::gesture::gesture_detector::GestureDetector;
use crate::mouse_region::{MouseRegion, PointerState};

/// How much the background is darkened while the button is held.
///
/// A press has to be visible *before* the gesture resolves, or the button feels
/// unresponsive on a slow tap; the depth is chosen to read as pushed-in next to
/// the hover lightening rather than to compete with it.
const PRESSED_DARKEN: f32 = 0.15;

/// A clickable button widget with visual feedback.
///
/// `Button` renders a decorated container (background, border, outline) and
/// provides callbacks for primary tap, double-tap, long-press, and
/// secondary-button tap. It supports optional decorations for hover, press,
/// and disabled states, and suppresses all pointer callbacks when disabled.
///
/// The default button is enabled, has an empty [`BoxDecoration`], and has no-op
/// callbacks. Finish construction with [`Button::child`] or
/// [`Button::box_child`].
///
/// # Example
///
/// ```
/// use aimer_input::button::Button;
/// use aimer_text::Text;
///
/// let button = Button::new().on_press(|| println!("pressed"))
///                           .child(Text::new("Save"));
/// ```
#[allow(dead_code)]
#[derive(aimer_macro::PortableWidget)]
#[portable_widget(
    id = "aimer_input::Button",
    version = "1.0",
    schema_only,
    validate = validate_portable_button
)]
pub struct Button<W = RequiredChild> {
    #[portable_callback(async)]
    pub on_press: VoidCallback,
    #[portable_callback(async)]
    pub on_long_press: VoidCallback,
    #[portable_callback(async)]
    pub on_double_press: VoidCallback,
    #[portable_callback(async)]
    pub on_right_press: VoidCallback,
    #[portable_optional]
    pub decoration: BoxDecoration,
    #[portable_skip]
    pub hover_decoration: Option<BoxDecoration>,
    #[portable_skip]
    pub press_decoration: Option<BoxDecoration>,
    #[portable_skip]
    pub disable_decoration: Option<BoxDecoration>,
    #[portable_skip]
    pub is_disabled: bool,
    /// The subtree consumed by either native state creation or portable lowering.
    // Keep the legacy child slot stable: the handwritten lowering used zero
    // rather than the field-name discriminator used by new derived widgets.
    #[portable_child(discriminator = 0)]
    child: Option<AnyWidget>,
    #[portable_skip]
    widget_key: Option<Key>,
    /// Records which child type completed the builder without storing it.
    ///
    /// The child itself is erased into [`AnyWidget`], but the parameter has to
    /// survive so that a button without a child stays
    /// `Button<RequiredChild>` — a type that is deliberately not a [`Widget`].
    #[portable_skip]
    marker: PhantomData<W>,
}

/// Mounted state used internally by [`Button`].
pub struct ButtonState<W: Widget + 'static> {
    is_hover: bool,
    /// Whether a pointer is currently held on the button.
    ///
    /// Distinct from `is_hover`: hovering is where the cursor is, pressing is
    /// what the button is doing. Driven by the recognizer's press lifecycle, so
    /// the highlight is dropped whether the press became a tap or was abandoned
    /// by sliding away.
    is_pressed: bool,
    pub on_press: VoidCallback,
    pub is_disabled: bool,
    pub on_long_press: VoidCallback,
    pub on_double_press: VoidCallback,
    pub on_right_press: VoidCallback,
    pub decoration: BoxDecoration,
    pub hover_decoration: Option<BoxDecoration>,
    pub press_decoration: Option<BoxDecoration>,
    pub disable_decoration: Option<BoxDecoration>,
    current_state: Rc<Cell<PointerState>>,
    state_updater: StateUpdater<Self>,
    child: ChildBuilder,
    /// Keeps one state type per child type, exactly as the previous typed
    /// child field did, so a button whose child type changes is rebuilt from
    /// scratch rather than adopting the state of a different button.
    marker: PhantomData<W>,
}

impl Default for Button {
    fn default() -> Self {
        Self::new()
    }
}

impl Button {
    /// Creates an enabled button with default decoration and no-op callbacks.
    pub fn new() -> Self {
        Self {
            on_press: VoidCallback::default(),
            on_long_press: VoidCallback::default(),
            on_double_press: VoidCallback::default(),
            on_right_press: VoidCallback::default(),
            decoration: BoxDecoration::default(),
            hover_decoration: None,
            press_decoration: None,
            disable_decoration: None,
            is_disabled: false,
            child: None,
            widget_key: None,
            marker: PhantomData,
        }
    }
}

impl<W> Button<W> {
    /// Sets the callback invoked for a completed primary tap.
    ///
    /// The callback is not invoked while the button is disabled.
    pub fn on_press(mut self, on_press: impl Into<VoidCallback>) -> Self {
        self.on_press = on_press.into();
        self
    }

    /// Registers an asynchronous callback for a completed primary tap.
    ///
    /// The closure must return a `Future` (e.g. an `async` block). The future
    /// runs on Aimer's UI-thread runtime, before the next build phase, so
    /// neither it nor its captures have to be [`Send`] — a handler may `await`
    /// while holding a `StateUpdater`, a controller, or any other `Rc` the tree
    /// handed it. Work that *blocks* rather than awaits belongs on
    /// `Venus::offload`, which runs it on a worker thread.
    ///
    /// **Note**: Since async closures capture state, they implement `FnOnce`.
    /// The closure is taken on first invocation — subsequent presses produce
    /// no action. If you need repeated invocations, clone your captured data
    /// before the async block or use `Rc<RefCell<...>>`.
    pub fn on_press_async<F, Fut>(mut self, on_press: F) -> Self
    where
        F: FnOnce() -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        self.on_press = VoidCallback::from_async(on_press);
        self
    }

    /// Sets the callback invoked once a held pointer is recognized as a
    /// long-press.
    ///
    /// The callback is not invoked while the button is disabled.
    pub fn on_long_press(mut self, on_long_press: impl Into<VoidCallback>) -> Self {
        self.on_long_press = on_long_press.into();
        self
    }

    /// Registers an asynchronous long-press callback.
    ///
    /// Like [`Button::on_press_async`], this one-shot closure is taken on its
    /// first invocation.
    pub fn on_long_press_async<F, Fut>(mut self, on_long_press: F) -> Self
    where
        F: FnOnce() -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        self.on_long_press = VoidCallback::from_async(on_long_press);
        self
    }

    /// Sets the callback invoked when a second primary tap completes within the
    /// double-tap timeout.
    ///
    /// The callback is not invoked while the button is disabled.
    pub fn on_double_press(mut self, on_double_press: impl Into<VoidCallback>) -> Self {
        self.on_double_press = on_double_press.into();
        self
    }

    /// Registers an asynchronous double-press callback.
    ///
    /// Like [`Button::on_press_async`], this one-shot closure is taken on its
    /// first invocation.
    pub fn on_double_press_async<F, Fut>(mut self, on_double_press: F) -> Self
    where
        F: FnOnce() -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        self.on_double_press = VoidCallback::from_async(on_double_press);
        self
    }

    /// Sets the callback invoked for a completed secondary-button tap.
    ///
    /// The callback is not invoked while the button is disabled.
    pub fn on_right_press(mut self, on_right_press: impl Into<VoidCallback>) -> Self {
        self.on_right_press = on_right_press.into();
        self
    }

    /// Registers an asynchronous secondary-button tap callback.
    ///
    /// Like [`Button::on_press_async`], this one-shot closure is taken on its
    /// first invocation.
    pub fn on_right_press_async<F, Fut>(mut self, on_right_press: F) -> Self
    where
        F: FnOnce() -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        self.on_right_press = VoidCallback::from_async(on_right_press);
        self
    }

    /// Replaces the decoration drawn behind the child.
    ///
    /// Hovering lightens an existing background color. Disabled buttons replace
    /// that background with translucent black.
    pub fn decoration(mut self, decoration: BoxDecoration) -> Self {
        self.decoration = decoration;
        self
    }

    /// Sets the decoration drawn while the enabled button is hovered.
    ///
    /// When unset, the normal decoration is lightened automatically.
    #[inline]
    pub fn hover_decoration(mut self, hover_decoration: BoxDecoration) -> Self {
        self.hover_decoration = Some(hover_decoration);
        self
    }

    /// Sets the decoration drawn while the button is pressed.
    ///
    /// When unset, the active decoration is darkened automatically.
    #[inline]
    pub fn press_decoration(mut self, press_decoration: BoxDecoration) -> Self {
        self.press_decoration = Some(press_decoration);
        self
    }

    /// Sets the decoration drawn while the button is disabled.
    ///
    /// When unset, the background is replaced with translucent black.
    #[inline]
    pub fn disable_decoration(mut self, disable_decoration: BoxDecoration) -> Self {
        self.disable_decoration = Some(disable_decoration);
        self
    }

    /// Enables or disables primary, double, and long-press interaction.
    ///
    /// A disabled button omits its hover and gesture wrappers and draws its
    /// disabled background.
    pub fn disabled(mut self, is_disabled: bool) -> Self {
        self.is_disabled = is_disabled;
        self
    }

    /// Sets the identity of this button for widget reconciliation.
    #[track_caller]
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        let caller = Location::caller();
        self.widget_key = Some(key.into().with_location(caller));
        self
    }

    /// Supplies the terminal child and returns a statically typed [`Button`].
    ///
    /// Builder settings made before this call are preserved. A button without a
    /// child is only an intermediate builder and does not implement
    /// [`Widget`].
    pub fn child<C: Widget + 'static>(self, child: C) -> Button<C> {
        Button {
            on_press: self.on_press,
            on_long_press: self.on_long_press,
            on_double_press: self.on_double_press,
            on_right_press: self.on_right_press,
            decoration: self.decoration,
            hover_decoration: self.hover_decoration,
            press_decoration: self.press_decoration,
            disable_decoration: self.disable_decoration,
            is_disabled: self.is_disabled,
            child: Some(child.boxed()),
            widget_key: self.widget_key,
            marker: PhantomData,
        }
    }

    /// Supplies the terminal child and erases the completed button's concrete
    /// type.
    ///
    /// This is exactly equivalent to `self.child(child).boxed()`, combining
    /// [`Button::child`] with [`Widget::boxed`]. Use it when branching APIs
    /// need to return one [`AnyWidget`] despite using different concrete
    /// child types.
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl<W: Widget + 'static> StatefulWidget for Button<W> {
    type State = ButtonState<W>;

    fn create_state(self) -> Self::State {
        ButtonState {
            is_hover: false,
            is_pressed: false,
            on_press: self.on_press,
            on_long_press: self.on_long_press,
            on_double_press: self.on_double_press,
            on_right_press: self.on_right_press,
            decoration: self.decoration,
            hover_decoration: self.hover_decoration,
            press_decoration: self.press_decoration,
            disable_decoration: self.disable_decoration,
            state_updater: StateUpdater::empty(),
            current_state: Rc::new(Cell::new(PointerState::Outside)),
            child: ChildBuilder::from_widget(
                self.child
                    .expect("a completed Button always owns its required child"),
            ),
            is_disabled: self.is_disabled,
            marker: PhantomData,
        }
    }
}

impl<W: Widget + 'static> Widget for Button<W> {
    fn key(&self) -> Option<Key> {
        self.widget_key.clone()
    }

    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let __key = Widget::key(&self);
        StatefulElement::new_with_name(self, ctx, "Button", __key)
            .0
            .boxed()
    }
}

#[cfg(feature = "portable-guest")]
fn validate_portable_button<W>(
    button: &Button<W>,
    _ctx: &PortableBuildContext,
    source: SourceFingerprint,
) -> Result<(), PortableBuildError> {
    if button.is_disabled {
        return Err(PortableBuildError::UnsupportedProperty {
            widget: "Button",
            property: "disabled",
            source,
        });
    }
    if button.hover_decoration.is_some() {
        return Err(PortableBuildError::UnsupportedProperty {
            widget: "Button",
            property: "hover_decoration",
            source,
        });
    }
    if button.press_decoration.is_some() {
        return Err(PortableBuildError::UnsupportedProperty {
            widget: "Button",
            property: "press_decoration",
            source,
        });
    }
    if button.disable_decoration.is_some() {
        return Err(PortableBuildError::UnsupportedProperty {
            widget: "Button",
            property: "disable_decoration",
            source,
        });
    }

    Ok(())
}

impl<W: Widget + 'static> State<Button<W>> for ButtonState<W> {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.state_updater = updater;
    }

    fn adopt_config_from(&mut self, new: Self) {
        // `is_hover` and `is_pressed` are deliberately not adopted: they describe
        // what the pointer is doing right now, which a rebuild does not change.
        self.on_press = new.on_press;
        self.is_disabled = new.is_disabled;
        self.on_long_press = new.on_long_press;
        self.on_double_press = new.on_double_press;
        self.on_right_press = new.on_right_press;
        self.decoration = new.decoration;
        self.hover_decoration = new.hover_decoration;
        self.press_decoration = new.press_decoration;
        self.disable_decoration = new.disable_decoration;
        self.child = new.child;
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        let child = self.child.clone();
        let decor = self.active_decoration();
        let child = Container::new().box_decoration(decor).child(child);

        if self.is_disabled {
            return child.boxed();
        }

        MouseRegion::new()
            .on_hover_enter({
                let updater = self.state_updater.clone();
                move || {
                    updater.set_state(|s| {
                        s.is_hover = true;
                    })
                }
            })
            .on_hover_exit({
                let updater = self.state_updater.clone();
                move || {
                    updater.set_state(|s| {
                        s.is_hover = false;
                    })
                }
            })
            .current_state(self.current_state.clone())
            .child(
                GestureDetector::new()
                    .on_tap(if self.is_disabled {
                        VoidCallback::default()
                    } else {
                        self.on_press.clone()
                    })
                    .on_double_press(if self.is_disabled {
                        VoidCallback::default()
                    } else {
                        self.on_double_press.clone()
                    })
                    .on_long_press(if self.is_disabled {
                        VoidCallback::default()
                    } else {
                        self.on_long_press.clone()
                    })
                    .on_right_tap(self.on_right_press.clone())
                    .on_gesture({
                        let updater = self.state_updater.clone();
                        move |event: GestureEvent| match event {
                            GestureEvent::TapDown { .. } => {
                                updater.set_state(|state| state.is_pressed = true)
                            }
                            // Either terminator ends the press: the recognizer
                            // guarantees one of them arrives, so the highlight
                            // cannot be left stuck on.
                            GestureEvent::TapUp { .. } | GestureEvent::TapCancel => {
                                updater.set_state(|state| state.is_pressed = false)
                            }
                            _ => {}
                        }
                    })
                    .child(child),
            )
            .boxed()
    }
}

impl<W: Widget + 'static> ButtonState<W> {
    #[inline]
    fn active_decoration(&self) -> BoxDecoration {
        if self.is_disabled {
            if let Some(decoration) = &self.disable_decoration {
                return decoration.clone();
            }

            let decoration = self.decoration.clone();
            decoration.background_color.set(Some(Color::BLACK.with_opacity(120)));
            return decoration;
        }

        if self.is_pressed {
            if let Some(decoration) = &self.press_decoration {
                return decoration.clone();
            }
        }

        let decoration = if self.is_hover {
            self.hover_decoration
                .clone()
                .unwrap_or_else(|| self.decoration.clone())
        } else {
            self.decoration.clone()
        };

        if self.is_hover
            && self.hover_decoration.is_none()
            && let Some(color) = decoration.background_color.get()
        {
            decoration.background_color.set(Some(color.lighten(0.2)));
        }

        // After the hover lightening, so a hovered button still visibly reacts to
        // being pressed rather than the two cancelling out.
        if self.is_pressed
            && self.press_decoration.is_none()
            && let Some(color) = decoration.background_color.get()
        {
            decoration.background_color.set(Some(color.darken(PRESSED_DARKEN)));
        }

        decoration
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use aimer_attribute::size::ResolvedSize;
    use aimer_style::BoxDecoration;
    use aimer_widget::base::{Color, WindowHandle};
    use aimer_widget::{AnyElement, ErrorWidget, Key, PortableWidget, State, StatefulWidget, Widget};

    use super::{Button, BuildContext};

    #[cfg(feature = "portable-guest")]
    use aimer_anteros::{
        EVENT_BUTTON_DOUBLE_PRESS, EVENT_BUTTON_LONG_PRESS, EVENT_BUTTON_PRESS,
        EVENT_BUTTON_RIGHT_PRESS, PROPERTY_BUTTON_DECORATION, Version, WIDGET_BUTTON,
        WidgetDocumentView, WidgetSchemaId,
    };
    #[cfg(feature = "portable-guest")]
    use aimer_widget::portable::{
        PortableBuildContext, PortableBuildError, PortableLimits, PortableNodeId,
        PortableWidgetLimits, SourceFingerprint, StableId128,
    };
    #[cfg(feature = "portable-guest")]
    use aimer_anteros::PropertyValue;

    /// A child that reports how often it was asked for an element.
    struct Probe {
        builds: Rc<Cell<usize>>,
    }

    impl Widget for Probe {
        fn to_element(self, ctx: &BuildContext) -> AnyElement {
            self.builds.set(self.builds.get() + 1);
            ErrorWidget::new("probe").to_element(ctx)
        }

        fn debug_name(&self) -> &'static str {
            "Probe"
        }
    }

    impl PortableWidget for Probe {}

    #[cfg(feature = "portable-guest")]
    struct PortableChild;

    #[cfg(feature = "portable-guest")]
    impl Widget for PortableChild {
        fn to_element(self, _ctx: &BuildContext) -> AnyElement {
            panic!("portable child must not use native construction")
        }
    }

    #[cfg(feature = "portable-guest")]
    impl PortableWidget for PortableChild {
        fn to_portable_node(
            self,
            ctx: &mut PortableBuildContext,
            source: SourceFingerprint,
        ) -> Result<PortableNodeId, PortableBuildError> {
            ctx.push_node(
                WidgetSchemaId::new(77),
                Version::new(1, 0),
                None,
                source,
                &[],
                &[],
            )
        }
    }

    #[cfg(feature = "portable-guest")]
    fn portable_context() -> PortableBuildContext {
        PortableBuildContext::new(
            3,
            5,
            PortableWidgetLimits::new(8, 8, 8, 8, 64, 4_096)
                .with_max_blob_bytes(1_024)
                .with_max_callbacks(8),
            PortableLimits::new(8, 16, 64, 128, 1_024),
        )
        .unwrap()
    }

    #[cfg(feature = "portable-guest")]
    fn portable_source(value: u8) -> SourceFingerprint {
        SourceFingerprint::new(StableId128::from_bytes([value; 16]))
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn portable_button_emits_exact_schema_child_and_four_distinct_routes() {
        let calls = Rc::new(Cell::new(0_u32));
        let mut context = portable_context();
        let source = portable_source(1);
        let button = Button::new()
            .on_press({ let calls = calls.clone(); move || calls.set(calls.get() | 1) })
            .on_long_press({ let calls = calls.clone(); move || calls.set(calls.get() | 2) })
            .on_double_press({ let calls = calls.clone(); move || calls.set(calls.get() | 4) })
            .on_right_press({ let calls = calls.clone(); move || calls.set(calls.get() | 8) })
            .child(PortableChild);
        let root = button.to_portable_node(&mut context, source).unwrap();
        let document = context.finish_document(root).unwrap();
        let bytes = document.encode().unwrap();
        let view = WidgetDocumentView::decode(&bytes, document.model_limits()).unwrap();
        let node = view.node(root.index()).unwrap();
        let bindings = node.callbacks().collect::<Vec<_>>();

        assert_eq!(node.widget_type(), WIDGET_BUTTON);
        assert_eq!(node.widget_schema(), Version::new(1, 0));
        assert!(node.properties().next().is_none());
        assert_eq!(node.children().collect::<Vec<_>>(), vec![0]);
        assert_eq!(
            bindings.iter().map(|binding| binding.event_kind()).collect::<Vec<_>>(),
            vec![
                EVENT_BUTTON_PRESS,
                EVENT_BUTTON_LONG_PRESS,
                EVENT_BUTTON_DOUBLE_PRESS,
                EVENT_BUTTON_RIGHT_PRESS,
            ]
        );
        let mut ids = bindings.iter().map(|binding| binding.callback_id()).collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 4);

        let registry = context.callback_registry();
        for binding in bindings {
            registry
                .dispatch(
                    StableId128::from_bytes(*binding.callback_id().as_bytes()),
                    &mut context,
                )
                .unwrap();
        }
        assert_eq!(calls.get(), 15);
        assert!(context.take_rebuild_request());
        assert!(!context.take_rebuild_request());
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn portable_button_identity_uses_key_or_source_and_rejects_lossy_properties() {
        let source = portable_source(2);
        let mut first = portable_context();
        let root = Button::new()
            .key("save")
            .child(PortableChild)
            .to_portable_node(&mut first, source)
            .unwrap();
        let keyed = first.finish_document(root).unwrap();
        let keyed_bytes = keyed.encode().unwrap();
        let keyed_view = WidgetDocumentView::decode(&keyed_bytes, keyed.model_limits()).unwrap();
        let keyed_id = keyed_view.node(root.index()).unwrap().key();

        let mut second = portable_context();
        let root = Button::new()
            .key("save")
            .child(PortableChild)
            .to_portable_node(&mut second, portable_source(99))
            .unwrap();
        let same_key = second.finish_document(root).unwrap();
        let same_key_bytes = same_key.encode().unwrap();
        let same_key_view = WidgetDocumentView::decode(&same_key_bytes, same_key.model_limits()).unwrap();
        assert_eq!(same_key_view.node(root.index()).unwrap().key(), keyed_id);

        let mut disabled = portable_context();
        let error = Button::new()
            .disabled(true)
            .child(PortableChild)
            .to_portable_node(&mut disabled, source)
            .unwrap_err();
        assert_eq!(error.to_string(), "button property `disabled` has no portable lowering");

        let mut decorated = portable_context();
        let root = Button::new()
            .decoration(BoxDecoration::new().background_color(Color::BLACK))
            .child(PortableChild)
            .to_portable_node(&mut decorated, source)
            .unwrap();
        let document = decorated.finish_document(root).unwrap();
        let bytes = document.encode().unwrap();
        let view = WidgetDocumentView::decode(&bytes, document.model_limits()).unwrap();
        let property = view
            .node(root.index())
            .unwrap()
            .properties()
            .find(|property| property.property_id() == PROPERTY_BUTTON_DECORATION)
            .expect("non-default Button decoration must be lowered");
        assert!(matches!(property.value(), PropertyValue::BlobRef(_)));

        let mut asynchronous = portable_context();
        let node = Button::new()
            .on_press_async(|| async {})
            .child(PortableChild)
            .to_portable_node(&mut asynchronous, source)
            .unwrap();
        asynchronous.finish_document(node).unwrap();
        let callback_id = asynchronous
            .callback_id_for(None, source, EVENT_BUTTON_PRESS);
        let start = asynchronous
            .callback_registry()
            .dispatch_start(callback_id, &mut asynchronous)
            .unwrap();
        assert!(matches!(
            start,
            aimer_widget::portable::PortableCallbackStart::Started { .. }
        ));
        asynchronous.run_async_microtasks();
    }

    fn context() -> BuildContext<'static> {
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
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

    /// A button rebuilds itself whenever the pointer enters, leaves, presses or
    /// releases it, and every one of those rebuilds has to reach the same child
    /// again — reusing its element rather than building another one, so the
    /// child's own state survives a hover.
    #[tokio::test]
    async fn every_rebuild_reaches_the_same_child() {
        let builds = Rc::new(Cell::new(0));
        let state = Button::new()
            .child(Probe {
                builds: Rc::clone(&builds),
            })
            .create_state();
        let ctx = context();

        state.build(&ctx).to_element(&ctx);
        state.build(&ctx).to_element(&ctx);

        assert_eq!(
            builds.get(),
            1,
            "a hover or a press must reuse the child, not rebuild it"
        );
    }

    /// An asynchronous press handler may keep an [`Rc`] from the element tree
    /// across an `await`, and its effect lands on the runtime that owns the
    /// frame.
    ///
    /// This is the whole reason the async path moved off a thread pool: a
    /// handler that cannot capture a `StateUpdater` cannot change anything a
    /// user would see. The bound is easy to re-tighten by accident, so it is
    /// pinned here rather than left to a compile that happens to still work.
    #[tokio::test]
    async fn an_async_press_handler_may_hold_tree_state_across_an_await() {
        use aimer_utils::callback::CallbackExecutor;
        use aimer_venus::Venus;

        let venus = Venus::new();
        venus.install();

        let presses = Rc::new(Cell::new(0));
        let counted = Rc::clone(&presses);
        let button = Button::new()
            .child(aimer_widget::ErrorWidget::new("button"))
            .on_press_async(move || async move {
                aimer_venus::yield_now().await;
                counted.set(counted.get() + 1);
            });

        button.on_press.execute(());
        while venus.task_count() > 0 {
            venus.run_microtasks();
        }

        assert_eq!(presses.get(), 1);
    }

    #[test]
    fn explicit_key_sets_reconciliation_identity() {
        let button = Button::new()
            .child(aimer_widget::ErrorWidget::new("button"))
            .key("platform-button");

        assert_eq!(
            Widget::key(&button),
            Some(Key::Value("platform-button".to_owned()))
        );
    }

    #[test]
    fn state_specific_decorations_are_transferred_to_button_state() {
        let normal = BoxDecoration::new().background_color(Color::WHITE);
        let hover = BoxDecoration::new().background_color(Color::BLUE);
        let press = BoxDecoration::new().background_color(Color::RED);
        let disabled = BoxDecoration::new().background_color(Color::GRAY);

        let state = Button::new()
            .decoration(normal.clone())
            .hover_decoration(hover.clone())
            .press_decoration(press.clone())
            .disable_decoration(disabled.clone())
            .child(aimer_widget::ErrorWidget::new("button"))
            .create_state();

        assert_eq!(state.decoration, normal);
        assert_eq!(state.hover_decoration, Some(hover));
        assert_eq!(state.press_decoration, Some(press));
        assert_eq!(state.disable_decoration, Some(disabled));
    }

    #[test]
    fn state_specific_decorations_follow_disabled_pressed_hover_precedence() {
        let normal = BoxDecoration::new().background_color(Color::WHITE);
        let hover = BoxDecoration::new().background_color(Color::BLUE);
        let press = BoxDecoration::new().background_color(Color::RED);
        let disabled = BoxDecoration::new().background_color(Color::GRAY);
        let mut state = Button::new()
            .decoration(normal.clone())
            .hover_decoration(hover.clone())
            .press_decoration(press.clone())
            .disable_decoration(disabled.clone())
            .child(aimer_widget::ErrorWidget::new("button"))
            .create_state();

        state.is_disabled = true;
        state.is_hover = true;
        state.is_pressed = true;
        assert_eq!(state.active_decoration(), disabled);

        state.is_disabled = false;
        assert_eq!(state.active_decoration(), press);

        state.is_pressed = false;
        assert_eq!(state.active_decoration(), hover);

        state.is_hover = false;
        assert_eq!(state.active_decoration(), normal);
    }

    #[cfg(feature = "portable-guest")]
    mod portable {
        use aimer_anteros::{
            CallbackBinding, EVENT_BUTTON_DOUBLE_PRESS, EVENT_BUTTON_LONG_PRESS,
            EVENT_BUTTON_PRESS, EVENT_BUTTON_RIGHT_PRESS, Version, WidgetDocument, WidgetNode,
            WIDGET_BUTTON, WidgetSchemaId,
        };
        use aimer_widget::portable::{
            PortableBuildContext, PortableBuildError, PortableLimits, PortableNodeId,
            PortableWidgetLimits, SourceFingerprint, StableId128,
        };

        use super::*;

        struct PortableLeaf;

        impl Widget for PortableLeaf {
            fn to_element(self, _ctx: &BuildContext) -> AnyElement {
                panic!("portable test child must not build natively")
            }
        }

        impl PortableWidget for PortableLeaf {
            fn to_portable_node(
                self,
                ctx: &mut PortableBuildContext,
                source: SourceFingerprint,
            ) -> Result<PortableNodeId, PortableBuildError> {
                ctx.push_node(
                    WidgetSchemaId::new(99),
                    Version::new(1, 0),
                    None,
                    source,
                    &[],
                    &[],
                )
            }
        }

        fn source(value: u128) -> SourceFingerprint {
            SourceFingerprint::new(StableId128::from_u128(value))
        }

        fn context() -> PortableBuildContext {
            PortableBuildContext::new(
                8,
                3,
                PortableWidgetLimits::new(8, 8, 8, 8, 64, 4_096)
                    .with_max_blob_bytes(1_024)
                    .with_max_callbacks(8),
                PortableLimits::new(8, 16, 64, 128, 1_024),
            )
            .unwrap()
        }

        #[test]
        fn generated_button_lowering_matches_the_legacy_wire_shape() {
            let source = source(2);

            let mut context = context();
            let button = Button::new().child(PortableLeaf);
            let expected_ids = [
                context.callback_id_for(None, source, EVENT_BUTTON_PRESS),
                context.callback_id_for(None, source, EVENT_BUTTON_LONG_PRESS),
                context.callback_id_for(None, source, EVENT_BUTTON_DOUBLE_PRESS),
                context.callback_id_for(None, source, EVENT_BUTTON_RIGHT_PRESS),
            ];
            let child_key = aimer_anteros::StableId128::from_bytes(
                context.slot_for(None, source.child(0)).to_bytes(),
            );
            let button_key = aimer_anteros::StableId128::from_bytes(
                context.slot_for(None, source).to_bytes(),
            );
            let root = button
                .to_portable_node(&mut context, source)
                .unwrap();
            let document = context.finish_document(root).unwrap();

            let callbacks = [
                CallbackBinding::new(
                    EVENT_BUTTON_PRESS,
                    Version::new(1, 0),
                    aimer_anteros::StableId128::from_bytes(expected_ids[0].to_bytes()),
                ),
                CallbackBinding::new(
                    EVENT_BUTTON_LONG_PRESS,
                    Version::new(1, 0),
                    aimer_anteros::StableId128::from_bytes(expected_ids[1].to_bytes()),
                ),
                CallbackBinding::new(
                    EVENT_BUTTON_DOUBLE_PRESS,
                    Version::new(1, 0),
                    aimer_anteros::StableId128::from_bytes(expected_ids[2].to_bytes()),
                ),
                CallbackBinding::new(
                    EVENT_BUTTON_RIGHT_PRESS,
                    Version::new(1, 0),
                    aimer_anteros::StableId128::from_bytes(expected_ids[3].to_bytes()),
                ),
            ];
            let children = [0];
            let nodes = [
                WidgetNode::new(WidgetSchemaId::new(99), Version::new(1, 0)).key(child_key),
                WidgetNode::new(WIDGET_BUTTON, Version::new(1, 0))
                    .key(button_key)
                    .callbacks(&callbacks)
                    .children(&children),
            ];
            let expected = WidgetDocument::new(8, 3, 1, &nodes, &[], &[])
                .encode(document.model_limits())
                .unwrap();

            assert_eq!(document.encode().unwrap(), expected);
            let registry = context.callback_registry();
            for callback_id in expected_ids {
                registry.dispatch(callback_id, &mut context).unwrap();
            }
        }

        #[test]
        fn button_lowers_exact_schema_child_and_four_distinct_callback_routes() {
            let counts = Rc::new([Cell::new(0), Cell::new(0), Cell::new(0), Cell::new(0)]);
            let mut button = Button::new().key("stable-button");
            let press = counts.clone();
            button = button.on_press(move || press[0].set(press[0].get() + 1));
            let long = counts.clone();
            button = button.on_long_press(move || long[1].set(long[1].get() + 1));
            let double = counts.clone();
            button = button.on_double_press(move || double[2].set(double[2].get() + 1));
            let right = counts.clone();
            let button = button
                .on_right_press(move || right[3].set(right[3].get() + 1))
                .child(PortableLeaf);
            let widget_source = source(1);
            let mut ctx = context();
            let expected_ids = [
                ctx.callback_id_for(Widget::key(&button).as_ref(), widget_source, EVENT_BUTTON_PRESS),
                ctx.callback_id_for(
                    Widget::key(&button).as_ref(),
                    widget_source,
                    EVENT_BUTTON_LONG_PRESS,
                ),
                ctx.callback_id_for(
                    Widget::key(&button).as_ref(),
                    widget_source,
                    EVENT_BUTTON_DOUBLE_PRESS,
                ),
                ctx.callback_id_for(
                    Widget::key(&button).as_ref(),
                    widget_source,
                    EVENT_BUTTON_RIGHT_PRESS,
                ),
            ];

            let root = button.to_portable_node(&mut ctx, widget_source).unwrap();
            let document = ctx.finish_document(root).unwrap();
            let bytes = document.encode().unwrap();
            let view = aimer_anteros::WidgetDocumentView::decode(
                &bytes,
                document.model_limits(),
            )
            .unwrap();
            {
                assert_eq!(view.root_node(), 1);
                let node = view.node(1).unwrap();
                assert_eq!(node.widget_type(), WIDGET_BUTTON);
                assert_eq!(node.widget_schema(), Version::new(1, 0));
                assert_eq!(node.properties().len(), 0);
                assert_eq!(node.children().collect::<Vec<_>>(), vec![0]);
                let callbacks: Vec<_> = node.callbacks().collect();
                assert_eq!(callbacks.len(), 4);
                assert_eq!(
                    callbacks.iter().map(|binding| binding.callback_id()).collect::<Vec<_>>(),
                    expected_ids
                        .map(|id| aimer_anteros::StableId128::from_bytes(id.to_bytes())),
                );
            }
            let registry = ctx.take_callback_registry();
            for id in expected_ids {
                registry.dispatch(id, &mut ctx).unwrap();
            }
            assert_eq!(counts.iter().map(Cell::get).collect::<Vec<_>>(), vec![1, 1, 1, 1]);
        }

        #[test]
        fn callback_identity_prefers_explicit_key_and_otherwise_uses_source() {
            let ctx = context();
            let key = Key::Value("button".into());
            assert_eq!(
                ctx.callback_id_for(Some(&key), source(1), EVENT_BUTTON_PRESS),
                ctx.callback_id_for(Some(&key), source(2), EVENT_BUTTON_PRESS),
            );
            assert_ne!(
                ctx.callback_id_for(None, source(1), EVENT_BUTTON_PRESS),
                ctx.callback_id_for(None, source(2), EVENT_BUTTON_PRESS),
            );
            assert_ne!(
                ctx.callback_id_for(Some(&key), source(1), EVENT_BUTTON_PRESS),
                ctx.callback_id_for(Some(&key), source(1), EVENT_BUTTON_LONG_PRESS),
            );
        }

        #[test]
        fn disabled_and_state_specific_decorations_are_diagnosed_while_async_lowering_is_retained() {
            let mut ctx = context();
            let disabled = Button::new()
                .disabled(true)
                .child(PortableLeaf)
                .to_portable_node(&mut ctx, source(3))
                .unwrap_err();
            assert!(matches!(
                disabled,
                PortableBuildError::UnsupportedProperty {
                    widget: "Button",
                    property: "disabled",
                    ..
                }
            ));

            let mut ctx = context();
            let decorated = Button::new()
                .decoration(BoxDecoration::new().background_color(Color::WHITE))
                .child(PortableLeaf)
                .to_portable_node(&mut ctx, source(4))
                .unwrap();
            ctx.finish_document(decorated).unwrap();

            let mut ctx = context();
            let asynchronous = Button::new()
                .on_press_async(|| async {})
                .child(PortableLeaf)
                .to_portable_node(&mut ctx, source(5))
                .unwrap();
            ctx.finish_document(asynchronous).unwrap();
            let callback_id = ctx.callback_id_for(None, source(5), EVENT_BUTTON_PRESS);
            assert!(matches!(
                ctx.callback_registry()
                    .dispatch_start(callback_id, &mut ctx),
                Ok(aimer_widget::portable::PortableCallbackStart::Started { .. })
            ));
            ctx.run_async_microtasks();
        }
    }
}
