use std::rc::Rc;

use aimer_widget::{AnyWidget, ChildBuilder, Widget};

use super::events::{ChangeCallback, ControlAction, InputEvent, emit_change, is_activation_key};
use super::semantics::{SelectionSemantics, SemanticRole};
use super::state::InteractionState;

/// The controlled value of a [`Checkbox`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckboxValue {
    /// The checkbox is not selected.
    Unchecked,
    /// The checkbox is selected.
    Checked,
    /// The checkbox represents a mixed value owned by its parent.
    Indeterminate,
}

impl CheckboxValue {
    #[inline]
    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::Unchecked | Self::Indeterminate => Self::Checked,
            Self::Checked => Self::Unchecked,
        }
    }
}

/// The visual state supplied to a custom [`Checkbox`] composition builder.
///
/// The state is a short-lived view of the controlled value and interaction
/// flags. It is passed by value so a builder can freely compose ordinary
/// widgets without retaining references into the checkbox's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckboxVisualState<'a> {
    value: CheckboxValue,
    label: Option<&'a str>,
    disabled: bool,
    hovered: bool,
    pressed: bool,
}

impl<'a> CheckboxVisualState<'a> {
    #[inline]
    pub(crate) fn new(
        value: CheckboxValue,
        label: Option<&'a str>,
        disabled: bool,
        hovered: bool,
        pressed: bool,
    ) -> Self {
        Self {
            value,
            label,
            disabled,
            hovered,
            pressed,
        }
    }

    /// Returns the controlled tri-state value.
    #[inline]
    pub fn value(self) -> CheckboxValue {
        self.value
    }

    /// Returns whether the checkbox is checked.
    #[inline]
    pub fn is_checked(self) -> bool {
        matches!(self.value, CheckboxValue::Checked)
    }

    /// Returns whether the checkbox is indeterminate.
    #[inline]
    pub fn is_indeterminate(self) -> bool {
        matches!(self.value, CheckboxValue::Indeterminate)
    }

    /// Returns the optional accessible label configured on the checkbox.
    #[inline]
    pub fn label(self) -> Option<&'a str> {
        self.label
    }

    /// Returns whether the checkbox is disabled.
    #[inline]
    pub fn is_disabled(self) -> bool {
        self.disabled
    }

    /// Returns whether the pointer is currently over the checkbox.
    #[inline]
    pub fn is_hovered(self) -> bool {
        self.hovered
    }

    /// Returns whether the pointer is currently pressing the checkbox.
    #[inline]
    pub fn is_pressed(self) -> bool {
        self.pressed
    }
}

/// A builder that creates the visual content of a checkbox for each state.
///
/// The interaction and accessibility behavior remains owned by [`Checkbox`];
/// the returned widget is only the visual content inside its interactive shell.
pub type CheckboxVisualBuilder = Rc<dyn for<'a> Fn(CheckboxVisualState<'a>) -> AnyWidget>;

/// A controlled checkbox model with an explicit tri-state transition policy.
///
/// `Checkbox` does not mutate its value when activated. It invokes the
/// registered callback and returns [`ControlAction::Activated`] with the
/// proposed value; the owner can then rebuild it with [`Self::with_value`].
/// `Indeterminate` resolves to `Checked`, while `Checked` resolves to
/// `Unchecked`. Use [`Checkbox::builder`] to replace the default visual with a
/// composition of ordinary widgets while keeping this interaction contract.
pub struct Checkbox {
    pub(crate) value: CheckboxValue,
    pub(crate) label: Option<String>,
    pub(crate) state: InteractionState,
    pub(crate) on_change: Option<ChangeCallback<CheckboxValue>>,
    pub(crate) visual_builder: Option<CheckboxVisualBuilder>,
}

impl Default for Checkbox {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Checkbox {
    /// Creates an unchecked checkbox with no callback or accessible label.
    #[inline]
    pub fn new() -> Self {
        Self {
            value: CheckboxValue::Unchecked,
            label: None,
            state: InteractionState::new(),
            on_change: None,
            visual_builder: None,
        }
    }

    /// Returns a checkbox configured with the caller-owned value.
    #[inline]
    pub fn with_value(mut self, value: CheckboxValue) -> Self {
        self.value = value;
        self
    }

    /// Returns the current controlled value.
    #[inline]
    pub fn current_value(&self) -> CheckboxValue {
        self.value
    }

    /// Returns a checkbox with an accessible label.
    #[inline]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Registers the callback that receives proposed controlled values.
    #[inline]
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(CheckboxValue) + 'static,
    {
        self.on_change = Some(std::rc::Rc::new(callback));
        self
    }

    /// Supplies a custom widget composition for the checkbox's visual content.
    ///
    /// The builder runs whenever the checkbox is rebuilt and receives the
    /// current value, label, and interaction flags. Compose any ordinary
    /// widgets you need — for example a box, a check-mark widget, and a label —
    /// and return them as one widget. The checkbox keeps pointer, keyboard,
    /// focus, disabled, and callback behavior around that composition.
    ///
    /// The default composition is used when this method is not called.
    ///
    /// # Example
    ///
    /// ```rust
    /// use aimer_container::Container;
    /// use aimer_flex::Row;
    /// use aimer_selection::Checkbox;
    /// use aimer_text::Text;
    /// use aimer_widget::Widget;
    ///
    /// let checkbox = Checkbox::new().with_label("Accept").builder(|state| {
    ///     let mark = if state.is_checked() {
    ///         Text::new("✓").boxed()
    ///     } else {
    ///         Text::new(" ").boxed()
    ///     };
    ///     Row::new()
    ///         .children([
    ///             Container::new().child(mark).boxed(),
    ///             Text::new(state.label().unwrap_or_default().to_owned()).boxed(),
    ///         ])
    ///         .boxed()
    /// });
    /// ```
    #[inline]
    pub fn builder<F, W>(mut self, builder: F) -> Self
    where
        F: for<'a> Fn(CheckboxVisualState<'a>) -> W + 'static,
        W: Widget + 'static,
    {
        self.visual_builder = Some(Rc::new(move |state| builder(state).boxed()));
        self
    }

    /// Replaces the generated visual content with a retained child widget.
    ///
    /// Use [`Checkbox::builder`] when the child must change with the checked
    /// or pressed state. This method is useful when the caller already owns a
    /// stateful composition of widgets and wants the checkbox interaction
    /// contract around it. Calling `child` or `builder` again replaces the
    /// previous visual composition.
    #[inline]
    pub fn child<W: Widget + 'static>(self, child: W) -> Self {
        let child = ChildBuilder::from_widget(child);
        self.builder(move |_| child.clone())
    }

    /// Replaces the generated visual content and erases the configured
    /// checkbox for use in a heterogeneous widget collection.
    #[inline]
    pub fn box_child<W: Widget + 'static>(self, child: W) -> AnyWidget {
        self.child(child).boxed()
    }

    /// Returns a checkbox with the supplied interaction state.
    #[inline]
    pub fn with_state(mut self, state: InteractionState) -> Self {
        self.state = state;
        self
    }

    /// Returns a checkbox with the disabled flag set.
    #[inline]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.state.set_disabled(disabled);
        self
    }

    /// Returns a checkbox with the focus flag set.
    #[inline]
    pub fn focused(mut self, focused: bool) -> Self {
        self.state.set_focused(focused);
        self
    }

    /// Returns a checkbox with the hover flag set.
    #[inline]
    pub fn hovered(mut self, hovered: bool) -> Self {
        self.state.set_hovered(hovered);
        self
    }

    /// Returns a checkbox with a validation error.
    #[inline]
    pub fn error(mut self, error: impl Into<String>) -> Self {
        self.state.set_error(Some(error.into()));
        self
    }

    /// Returns the live interaction state.
    #[inline]
    pub fn interaction_state(&self) -> &InteractionState {
        &self.state
    }

    /// Proposes the next tri-state value without changing the current value.
    pub fn activate(&self) -> ControlAction<CheckboxValue> {
        if self.state.disabled() {
            return ControlAction::Ignored;
        }
        emit_change(&self.on_change, self.value.toggled())
    }

    /// Applies a platform-neutral input event.
    pub fn handle_event(&mut self, event: InputEvent) -> ControlAction<CheckboxValue> {
        if self.state.disabled() {
            self.state.set_pressed(false);
            return ControlAction::Ignored;
        }

        match event {
            InputEvent::PointerDown => {
                self.state.set_pressed(true);
                ControlAction::Pressed
            }
            InputEvent::PointerUp { inside } => {
                let was_pressed = self.state.pressed();
                self.state.set_pressed(false);
                if was_pressed && inside {
                    self.activate()
                } else {
                    ControlAction::Released
                }
            }
            InputEvent::KeyDown(key) if is_activation_key(key) => self.activate(),
            InputEvent::Cancel => {
                self.state.set_pressed(false);
                ControlAction::Cancelled
            }
            InputEvent::KeyDown(_) | InputEvent::KeyUp(_) => ControlAction::Ignored,
        }
    }

    /// Builds a platform-neutral semantic snapshot.
    pub fn semantics(&self) -> SelectionSemantics {
        SelectionSemantics::from_state(SemanticRole::Checkbox, &self.label, &self.state)
            .with_checked(self.value)
    }
}
