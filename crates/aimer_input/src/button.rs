mod decoration;

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
use crate::gesture::gesture_detector::{GestureDetector, GestureDetectorBehavior};
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
/// Hover and press select retained decoration without rebuilding the input
/// wrappers or the child.
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
    is_hover: Rc<Cell<bool>>,
    /// Whether a pointer is currently held on the button.
    ///
    /// Distinct from `is_hover`: hovering is where the cursor is, pressing is
    /// what the button is doing. Driven by the recognizer's press lifecycle, so
    /// the highlight is dropped whether the press became a tap or was abandoned
    /// by sliding away.
    is_pressed: Rc<Cell<bool>>,
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
            is_hover: Rc::new(Cell::new(false)),
            is_pressed: Rc::new(Cell::new(false)),
            on_press: self.on_press,
            on_long_press: self.on_long_press,
            on_double_press: self.on_double_press,
            on_right_press: self.on_right_press,
            decoration: self.decoration,
            hover_decoration: self.hover_decoration,
            press_decoration: self.press_decoration,
            disable_decoration: self.disable_decoration,
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
    fn init_state(&mut self, _updater: StateUpdater<Self>) {}

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
        if self.is_disabled {
            return Container::new().box_decoration(self.decoration_for(false, false))
                .child(child).boxed();
        }
        let decoration = Rc::new(decoration::ButtonDecoration::new(self));
        let child = Container::new().retained_box_decoration(decoration.clone()).child(child);

        MouseRegion::new()
            .on_hover_enter({
                let decoration = decoration.clone();
                move || decoration.set_hover(true)
            })
            .on_hover_exit({
                let decoration = decoration.clone();
                move || decoration.set_hover(false)
            })
            .current_state(self.current_state.clone())
            .child(
                GestureDetector::new()
                    .behavior(GestureDetectorBehavior::BlockChild)
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
                        move |event: GestureEvent| match event {
                            GestureEvent::TapDown { .. } => {
                                decoration.set_pressed(true)
                            }
                            // Either terminator ends the press: the recognizer
                            // guarantees one of them arrives, so the highlight
                            // cannot be left stuck on.
                            GestureEvent::TapUp { .. } | GestureEvent::TapCancel => {
                                decoration.set_pressed(false)
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
    #[cfg(test)]
    fn active_decoration(&self) -> BoxDecoration {
        self.decoration_for(self.is_hover.get(), self.is_pressed.get())
    }

    fn decoration_for(&self, is_hover: bool, is_pressed: bool) -> BoxDecoration {
        if self.is_disabled {
            if let Some(decoration) = &self.disable_decoration {
                return decoration.clone();
            }

            let decoration = self.decoration.clone();
            decoration.background_color.set(Some(Color::BLACK.with_opacity(120)));
            return decoration;
        }

        if is_pressed
            && let Some(decoration) = &self.press_decoration {
                return decoration.clone();
            }

        let decoration = if is_hover {
            self.hover_decoration
                .clone()
                .unwrap_or_else(|| self.decoration.clone())
        } else {
            self.decoration.clone()
        };

        if is_hover
            && self.hover_decoration.is_none()
            && let Some(color) = decoration.background_color.get()
        {
            decoration.background_color.set(Some(color.lighten(0.2)));
        }

        // After the hover lightening, so a hovered button still visibly reacts to
        // being pressed rather than the two cancelling out.
        if is_pressed
            && self.press_decoration.is_none()
            && let Some(color) = decoration.background_color.get()
        {
            decoration.background_color.set(Some(color.darken(PRESSED_DARKEN)));
        }

        decoration
    }
}

#[cfg(test)]
mod tests;
