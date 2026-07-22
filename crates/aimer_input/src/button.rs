use std::cell::Cell;
use std::future::Future;
use std::rc::Rc;

use aimer_container::Container;
use aimer_style::BoxDecoration;
use aimer_widget::base::{BuildContext, Color};
use aimer_widget::{
    AnyElement, AnyWidget, Key, RequiredChild, State, StateUpdater, StatefulElement,
    StatefulWidget, Widget,
};

use crate::callback::VoidCallback;
use crate::gesture::gesture_detector::GestureDetector;
use crate::mouse_region::{MouseRegion, PointerState};

/// A clickable button widget with visual feedback.
///
/// `Button` renders a decorated container (background, border, outline) and provides callbacks for
/// primary tap, double-tap, long-press, and secondary-button tap. It substitutes a disabled
/// background and suppresses all pointer callbacks when disabled.
///
/// The default button is enabled, has an empty [`BoxDecoration`], and has no-op callbacks. Finish
/// construction with [`Button::child`] or [`Button::box_child`].
///
/// # Example
///
/// ```
/// use aimer_input::button::Button;
/// use aimer_text::Text;
///
/// let button = Button::new()
///     .on_press(|| println!("pressed"))
///     .child(Text::new("Save"));
/// ```
#[allow(dead_code)]
pub struct Button<W = RequiredChild> {
    pub on_press: VoidCallback,
    pub on_long_press: VoidCallback,
    pub on_double_press: VoidCallback,
    pub on_right_press: VoidCallback,
    pub decoration: BoxDecoration,
    pub is_disabled: bool,
    child: Rc<W>,
    widget_key: Option<Key>,
}

/// Mounted state used internally by [`Button`].
pub struct ButtonState<W: Widget + 'static> {
    is_hover: bool,
    pub on_press: VoidCallback,
    pub is_disabled: bool,
    pub on_long_press: VoidCallback,
    pub on_double_press: VoidCallback,
    pub on_right_press: VoidCallback,
    pub decoration: BoxDecoration,
    current_state: Rc<Cell<PointerState>>,
    state_updater: StateUpdater<Self>,
    child: Rc<W>,
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
            is_disabled: false,
            child: Rc::new(RequiredChild),
            widget_key: None,
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
    /// The closure must return a `Future` (e.g. an `async` block).
    /// The future is spawned by the framework's executor.
    ///
    /// **Note**: Since async closures capture state, they implement `FnOnce`.
    /// The closure is taken on first invocation — subsequent presses produce
    /// no action. If you need repeated invocations, clone your captured data
    /// before the async block or use `Rc<RefCell<...>>`.
    pub fn on_press_async<F, Fut>(mut self, on_press: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_press = VoidCallback::from_async(on_press);
        self
    }

    /// Sets the callback invoked once a held pointer is recognized as a long-press.
    ///
    /// The callback is not invoked while the button is disabled.
    pub fn on_long_press(mut self, on_long_press: impl Into<VoidCallback>) -> Self {
        self.on_long_press = on_long_press.into();
        self
    }

    /// Registers an asynchronous long-press callback.
    ///
    /// Like [`Button::on_press_async`], this one-shot closure is taken on its first invocation.
    pub fn on_long_press_async<F, Fut>(mut self, on_long_press: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_long_press = VoidCallback::from_async(on_long_press);
        self
    }

    /// Sets the callback invoked when a second primary tap completes within the double-tap timeout.
    ///
    /// The callback is not invoked while the button is disabled.
    pub fn on_double_press(mut self, on_double_press: impl Into<VoidCallback>) -> Self {
        self.on_double_press = on_double_press.into();
        self
    }

    /// Registers an asynchronous double-press callback.
    ///
    /// Like [`Button::on_press_async`], this one-shot closure is taken on its first invocation.
    pub fn on_double_press_async<F, Fut>(mut self, on_double_press: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
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
    /// Like [`Button::on_press_async`], this one-shot closure is taken on its first invocation.
    pub fn on_right_press_async<F, Fut>(mut self, on_right_press: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.on_right_press = VoidCallback::from_async(on_right_press);
        self
    }

    /// Replaces the decoration drawn behind the child.
    ///
    /// Hovering lightens an existing background color. Disabled buttons replace that background
    /// with translucent black.
    pub fn decoration(mut self, decoration: BoxDecoration) -> Self {
        self.decoration = decoration;
        self
    }

    /// Enables or disables primary, double, and long-press interaction.
    ///
    /// A disabled button omits its hover and gesture wrappers and draws its disabled background.
    pub fn disabled(mut self, is_disabled: bool) -> Self {
        self.is_disabled = is_disabled;
        self
    }

    /// Sets the identity of this button for widget reconciliation.
    pub fn key(mut self, key: impl Into<Key>) -> Self {
        self.widget_key = Some(key.into());
        self
    }

    /// Supplies the terminal child and returns a statically typed [`Button`].
    ///
    /// Builder settings made before this call are preserved. A button without a child is only an
    /// intermediate builder and does not implement [`Widget`].
    pub fn child<C: Widget>(self, child: C) -> Button<C> {
        Button {
            on_press: self.on_press,
            on_long_press: self.on_long_press,
            on_double_press: self.on_double_press,
            on_right_press: self.on_right_press,
            decoration: self.decoration,
            is_disabled: self.is_disabled,
            child: Rc::new(child),
            widget_key: self.widget_key,
        }
    }

    /// Supplies the terminal child and erases the completed button's concrete type.
    ///
    /// This is exactly equivalent to `self.child(child).boxed()`, combining [`Button::child`] with
    /// [`Widget::boxed`]. Use it when branching APIs need to return one [`AnyWidget`] despite using
    /// different concrete child types.
    pub fn box_child<C: Widget + 'static>(self, child: C) -> AnyWidget {
        self.child(child).boxed()
    }
}

impl<W: Widget + 'static> StatefulWidget for Button<W> {
    type State = ButtonState<W>;

    fn create_state(&self) -> Self::State {
        ButtonState {
            is_hover: false,
            on_press: self.on_press.clone(),
            on_long_press: self.on_long_press.clone(),
            on_double_press: self.on_double_press.clone(),
            on_right_press: self.on_right_press.clone(),
            decoration: self.decoration.clone(),
            state_updater: StateUpdater::empty(),
            current_state: Rc::new(Cell::new(PointerState::Outside)),
            child: self.child.clone(),
            is_disabled: self.is_disabled,
        }
    }
}

impl<W: Widget + 'static> Widget for Button<W> {
    fn key(&self) -> Option<Key> {
        self.widget_key.clone()
    }

    fn to_element(&self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "Button", self.key())
            .0
            .boxed()
    }
}

impl<W: Widget + 'static> State<Button<W>> for ButtonState<W> {
    fn init_state(&mut self, updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
        self.state_updater = updater;
    }

    fn adopt_config_from(&mut self, new: &Self) {
        self.on_press = new.on_press.clone();
        self.is_disabled = new.is_disabled;
        self.on_long_press = new.on_long_press.clone();
        self.on_double_press = new.on_double_press.clone();
        self.on_right_press = new.on_right_press.clone();
        self.decoration = new.decoration.clone();
        self.child = new.child.clone();
    }

    fn build(&self, _: &BuildContext) -> impl Widget {
        let child = self.child.clone();

        let mut decor = self.decoration.clone();

        if self.is_hover
            && let Some(color) = decor.background_color
        {
            decor.background_color = Some(color.lighten(0.2));
        }

        if self.is_disabled {
            decor.background_color = Option::from(Color::BLACK.with_opacity(120));
        }
        let child = Container::new()
            .box_decoration(decor)
            .child(child as Rc<dyn Widget>);

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
                    .child(child),
            )
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use aimer_widget::{Key, Widget};

    use super::Button;

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
}
