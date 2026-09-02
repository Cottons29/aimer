use std::rc::Rc;

use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, PortableWidget, State, StateUpdater, StatefulElement, StatefulWidget, Widget,
};

use crate::events::{emit_change, is_activation_key};
use crate::{
    Checkbox, CheckboxValue, CheckboxVisualBuilder, CheckboxVisualState, ChangeCallback, Key, Radio,
    Switch,
};

use super::chrome::{control_shell, indicator, labeled_row, tokens, wrap_interactive};

/// Retained state for a [`Checkbox`] widget.
pub struct CheckboxState {
    value: CheckboxValue,
    label: Option<String>,
    disabled: bool,
    error: Option<String>,
    on_change: Option<ChangeCallback<CheckboxValue>>,
    visual_builder: Option<CheckboxVisualBuilder>,
    hovered: bool,
    pressed: bool,
    updater: StateUpdater<Self>,
}

impl CheckboxState {
    /// Returns the controlled value last adopted from the widget.
    #[inline]
    pub fn current_value(&self) -> CheckboxValue {
        self.value
    }

    /// Proposes the next tri-state value without mutating the controlled value.
    pub fn propose_activation(&self) {
        if self.disabled {
            return;
        }
        let _ = emit_change(&self.on_change, self.value.toggled());
    }
}

impl StatefulWidget for Checkbox {
    type State = CheckboxState;

    fn create_state(self) -> Self::State {
        CheckboxState {
            value: self.value,
            label: self.label,
            disabled: self.state.disabled(),
            error: self.state.error().map(str::to_owned),
            on_change: self.on_change,
            visual_builder: self.visual_builder,
            hovered: self.state.hovered(),
            pressed: false,
            updater: StateUpdater::empty(),
        }
    }
}

impl State<Checkbox> for CheckboxState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: Self) {
        self.value = new.value;
        self.label = new.label;
        self.disabled = new.disabled;
        self.error = new.error;
        self.on_change = new.on_change;
        self.visual_builder = new.visual_builder;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let tokens = tokens(ctx);
        let filled = !matches!(self.value, CheckboxValue::Unchecked);
        let visual_state = CheckboxVisualState::new(
            self.value,
            self.label.as_deref(),
            self.disabled,
            self.hovered,
            self.pressed,
        );
        let content = self.visual_builder.as_ref().map_or_else(
            || {
                labeled_row(
                    &tokens,
                    indicator(
                        &tokens,
                        filled,
                        tokens.shape.small,
                        self.hovered,
                        self.pressed,
                        self.disabled,
                    ),
                    self.label.as_deref(),
                    self.disabled,
                )
            },
            |builder| builder(visual_state),
        );
        let visual = control_shell(
            &tokens,
            self.hovered,
            self.pressed,
            self.disabled,
            self.error.as_deref(),
            content,
        );
        let updater = self.updater;
        wrap_interactive(
            self.disabled,
            Rc::new({
                let updater = updater;
                move || updater.set_state(|state| state.propose_activation())
            }),
            Rc::new({
                let updater = updater;
                move |pressed| updater.set_state(move |state| state.pressed = pressed)
            }),
            Rc::new({
                let updater = updater;
                move |hovered| updater.set_state(move |state| state.hovered = hovered)
            }),
            Rc::new({
                let updater = updater;
                move |key| {
                    if is_activation_key(key) {
                        updater.set_state(|state| state.propose_activation());
                        true
                    } else {
                        matches!(key, Key::Tab)
                    }
                }
            }),
            visual,
        )
    }
}

impl Widget for Checkbox {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "Checkbox", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "Checkbox"
    }
}

impl PortableWidget for Checkbox {}

/// Retained state for a [`Switch`] widget.
pub struct SwitchState {
    value: bool,
    label: Option<String>,
    disabled: bool,
    error: Option<String>,
    on_change: Option<ChangeCallback<bool>>,
    hovered: bool,
    pressed: bool,
    updater: StateUpdater<Self>,
}

impl SwitchState {
    fn propose_activation(&self) {
        if self.disabled {
            return;
        }
        let _ = emit_change(&self.on_change, !self.value);
    }
}

impl StatefulWidget for Switch {
    type State = SwitchState;

    fn create_state(self) -> Self::State {
        SwitchState {
            value: self.value,
            label: self.label,
            disabled: self.state.disabled(),
            error: self.state.error().map(str::to_owned),
            on_change: self.on_change,
            hovered: self.state.hovered(),
            pressed: false,
            updater: StateUpdater::empty(),
        }
    }
}

impl State<Switch> for SwitchState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: Self) {
        self.value = new.value;
        self.label = new.label;
        self.disabled = new.disabled;
        self.error = new.error;
        self.on_change = new.on_change;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let tokens = tokens(ctx);
        let visual = control_shell(
            &tokens,
            self.hovered,
            self.pressed,
            self.disabled,
            self.error.as_deref(),
            labeled_row(
                &tokens,
                indicator(
                    &tokens,
                    self.value,
                    tokens.shape.pill,
                    self.hovered,
                    self.pressed,
                    self.disabled,
                ),
                self.label.as_deref(),
                self.disabled,
            ),
        );
        let updater = self.updater;
        wrap_interactive(
            self.disabled,
            Rc::new({
                let updater = updater;
                move || updater.set_state(|state| state.propose_activation())
            }),
            Rc::new({
                let updater = updater;
                move |pressed| updater.set_state(move |state| state.pressed = pressed)
            }),
            Rc::new({
                let updater = updater;
                move |hovered| updater.set_state(move |state| state.hovered = hovered)
            }),
            Rc::new({
                let updater = updater;
                move |key| {
                    if is_activation_key(key) {
                        updater.set_state(|state| state.propose_activation());
                        true
                    } else {
                        matches!(key, Key::Tab)
                    }
                }
            }),
            visual,
        )
    }
}

impl Widget for Switch {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "Switch", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "Switch"
    }
}

impl PortableWidget for Switch {}

/// Retained state for a [`Radio`] widget.
pub struct RadioState<T> {
    value: T,
    selected: bool,
    label: Option<String>,
    disabled: bool,
    error: Option<String>,
    on_change: Option<ChangeCallback<T>>,
    hovered: bool,
    pressed: bool,
    updater: StateUpdater<Self>,
}

impl<T: Clone + 'static> RadioState<T> {
    fn propose_activation(&self) {
        if self.disabled || self.selected {
            return;
        }
        let _ = emit_change(&self.on_change, self.value.clone());
    }
}

impl<T: Clone + 'static> StatefulWidget for Radio<T> {
    type State = RadioState<T>;

    fn create_state(self) -> Self::State {
        RadioState {
            value: self.value,
            selected: self.selected,
            label: self.label,
            disabled: self.state.disabled(),
            error: self.state.error().map(str::to_owned),
            on_change: self.on_change,
            hovered: self.state.hovered(),
            pressed: false,
            updater: StateUpdater::empty(),
        }
    }
}

impl<T: Clone + 'static> State<Radio<T>> for RadioState<T> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: Self) {
        self.value = new.value;
        self.selected = new.selected;
        self.label = new.label;
        self.disabled = new.disabled;
        self.error = new.error;
        self.on_change = new.on_change;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let tokens = tokens(ctx);
        let visual = control_shell(
            &tokens,
            self.hovered,
            self.pressed,
            self.disabled,
            self.error.as_deref(),
            labeled_row(
                &tokens,
                indicator(
                    &tokens,
                    self.selected,
                    tokens.shape.pill,
                    self.hovered,
                    self.pressed,
                    self.disabled,
                ),
                self.label.as_deref(),
                self.disabled,
            ),
        );
        let updater = self.updater;
        wrap_interactive(
            self.disabled,
            Rc::new({
                let updater = updater;
                move || updater.set_state(|state| state.propose_activation())
            }),
            Rc::new({
                let updater = updater;
                move |pressed| updater.set_state(move |state| state.pressed = pressed)
            }),
            Rc::new({
                let updater = updater;
                move |hovered| updater.set_state(move |state| state.hovered = hovered)
            }),
            Rc::new({
                let updater = updater;
                move |key| {
                    if is_activation_key(key) {
                        updater.set_state(|state| state.propose_activation());
                        true
                    } else {
                        matches!(key, Key::Tab)
                    }
                }
            }),
            visual,
        )
    }
}

impl<T: Clone + 'static> Widget for Radio<T> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "Radio", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "Radio"
    }
}

impl<T: 'static> PortableWidget for Radio<T> {}
