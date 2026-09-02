use std::rc::Rc;

use aimer_container::Container;
use aimer_flex::{BoxAlignment, Column};
use aimer_style::{LayoutSpacing, Spacing, TextStyle};
use aimer_text::Text;
use aimer_widget::base::BuildContext;
use aimer_widget::{
    AnyElement, AnyWidget, PortableWidget, State, StateUpdater, StatefulElement, StatefulWidget,
    Widget,
};

use crate::events::{ControlAction, InputEvent, is_activation_key};
use crate::{Autocomplete, ChoiceOption, Key, RadioGroup, Select};

use super::chrome::{
    control_shell, error_text, indicator, labeled_row, tokens, wrap_interactive,
};

/// Retained state for a [`RadioGroup`] widget.
pub struct RadioGroupState<T> {
    control: RadioGroup<T>,
    hovered: bool,
    updater: StateUpdater<Self>,
}

impl<T: Clone + PartialEq + 'static> RadioGroupState<T> {
    fn handle(&mut self, event: InputEvent) -> ControlAction<T> {
        self.control.handle_event(event)
    }
}

impl<T: Clone + PartialEq + 'static> StatefulWidget for RadioGroup<T> {
    type State = RadioGroupState<T>;

    fn create_state(self) -> Self::State {
        RadioGroupState {
            control: self,
            hovered: false,
            updater: StateUpdater::empty(),
        }
    }
}

impl<T: Clone + PartialEq + 'static> State<RadioGroup<T>> for RadioGroupState<T> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: Self) {
        let focused = self.control.focused_index();
        let mut next = new.control;
        next = next.with_focus_index(focused);
        self.control = next;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let tokens = tokens(ctx);
        let selected = self.control.selected().cloned();
        let options: Vec<AnyWidget> = self
            .control
            .options()
            .iter()
            .map(|option| {
                let key = option.key().to_owned();
                let selected = selected.as_ref() == Some(option.value());
                let disabled = option.disabled() || self.control.interaction_state().disabled();
                let visual = labeled_row(
                    &tokens,
                    indicator(
                        &tokens,
                        selected,
                        tokens.shape.pill,
                        false,
                        false,
                        disabled,
                    ),
                    Some(option.label()),
                    disabled,
                );
                let updater = self.updater;
                wrap_interactive(
                    disabled,
                    Rc::new(move || {
                        let key = key.clone();
                        updater.set_state(move |state| {
                            let _ = state.control.activate_key(&key);
                        });
                    }),
                    Rc::new(|_| {}),
                    Rc::new(|_| {}),
                    Rc::new(|_| false),
                    visual,
                )
            })
            .collect();
        let body = Column::new()
            .horizontal_alignment(BoxAlignment::Start)
            .gaps(LayoutSpacing::all(Spacing::Px(tokens.spacing.small as u32)))
            .children(options);
        let visual = control_shell(
            &tokens,
            self.hovered,
            self.control.interaction_state().pressed(),
            self.control.interaction_state().disabled(),
            self.control.interaction_state().error(),
            body.boxed(),
        );
        let updater = self.updater;
        wrap_interactive(
            self.control.interaction_state().disabled(),
            Rc::new({
                let updater = updater;
                move || {
                    updater.set_state(|state| {
                        let _ = state.handle(InputEvent::PointerDown);
                        let _ = state.handle(InputEvent::PointerUp { inside: true });
                    });
                }
            }),
            Rc::new({
                let updater = updater;
                move |pressed| {
                    updater.set_state(move |state| {
                        let event = if pressed {
                            InputEvent::PointerDown
                        } else {
                            InputEvent::Cancel
                        };
                        let _ = state.handle(event);
                    });
                }
            }),
            Rc::new({
                let updater = updater;
                move |hovered| updater.set_state(move |state| state.hovered = hovered)
            }),
            Rc::new({
                let updater = updater;
                move |key| {
                    updater.set_state(move |state| {
                        let _ = state.handle(InputEvent::KeyDown(key));
                    });
                    !matches!(key, Key::Tab)
                }
            }),
            visual,
        )
    }
}

impl<T: Clone + PartialEq + 'static> Widget for RadioGroup<T> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "RadioGroup", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "RadioGroup"
    }
}

impl<T: 'static> PortableWidget for RadioGroup<T> {}

/// Retained state for a [`Select`] widget.
pub struct SelectState<T> {
    control: Select<T>,
    hovered: bool,
    updater: StateUpdater<Self>,
}

impl<T> SelectState<T> {
    /// Returns whether the option surface is currently open.
    #[inline]
    pub fn is_open(&self) -> bool {
        self.control.is_open()
    }

    /// Returns the controlled selected value.
    #[inline]
    pub fn selected(&self) -> Option<&T> {
        self.control.selected()
    }
}

impl<T: PartialEq> SelectState<T> {
    /// Opens the option surface if the control is enabled.
    #[inline]
    pub fn open_menu(&mut self) {
        let _ = self.control.open_menu();
    }
}

impl<T: Clone + PartialEq + 'static> SelectState<T> {
    fn handle(&mut self, event: InputEvent) -> ControlAction<T> {
        self.control.handle_event(event)
    }
}

impl<T: Clone + PartialEq + 'static> StatefulWidget for Select<T> {
    type State = SelectState<T>;

    fn create_state(self) -> Self::State {
        SelectState {
            hovered: self.interaction_state().hovered(),
            control: self,
            updater: StateUpdater::empty(),
        }
    }
}

impl<T: Clone + PartialEq + 'static> State<Select<T>> for SelectState<T> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: Self) {
        let open = self.control.is_open();
        let focused = self.control.focused_index();
        let mut next = new.control;
        if open {
            let _ = next.open_menu();
        }
        next = next.with_focus_index(focused);
        self.control = next;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let tokens = tokens(ctx);
        let selected_label = self
            .control
            .selected()
            .and_then(|selected| {
                self.control
                    .options()
                    .iter()
                    .find(|option| option.value() == selected)
                    .map(ChoiceOption::label)
            })
            .unwrap_or("Select…");
        let trigger = labeled_row(
            &tokens,
            indicator(
                &tokens,
                self.control.is_open(),
                tokens.shape.small,
                self.hovered,
                self.control.interaction_state().pressed(),
                self.control.interaction_state().disabled(),
            ),
            Some(selected_label),
            self.control.interaction_state().disabled(),
        );
        let mut children = vec![trigger];
        if self.control.is_open() {
            let options: Vec<AnyWidget> = self
                .control
                .options()
                .iter()
                .map(|option| option_row(self, &tokens, option))
                .collect();
            children.push(
                Column::new()
                    .horizontal_alignment(BoxAlignment::Start)
                    .gaps(LayoutSpacing::all(Spacing::Px(tokens.spacing.x_small as u32)))
                    .children(options)
                    .boxed(),
            );
        }
        let visual = control_shell(
            &tokens,
            self.hovered,
            self.control.interaction_state().pressed(),
            self.control.interaction_state().disabled(),
            self.control.interaction_state().error(),
            Column::new()
                .horizontal_alignment(BoxAlignment::Start)
                .gaps(LayoutSpacing::all(Spacing::Px(tokens.spacing.x_small as u32)))
                .children(children)
                .boxed(),
        );
        let updater = self.updater;
        wrap_interactive(
            self.control.interaction_state().disabled(),
            Rc::new({
                let updater = updater;
                move || {
                    updater.set_state(|state| {
                        let _ = state.handle(InputEvent::PointerDown);
                        let _ = state.handle(InputEvent::PointerUp { inside: true });
                    });
                }
            }),
            Rc::new({
                let updater = updater;
                move |pressed| {
                    updater.set_state(move |state| {
                        let event = if pressed {
                            InputEvent::PointerDown
                        } else {
                            InputEvent::Cancel
                        };
                        let _ = state.handle(event);
                    });
                }
            }),
            Rc::new({
                let updater = updater;
                move |hovered| updater.set_state(move |state| state.hovered = hovered)
            }),
            Rc::new({
                let updater = updater;
                move |key| {
                    updater.set_state(move |state| {
                        let _ = state.handle(InputEvent::KeyDown(key));
                    });
                    !matches!(key, Key::Tab)
                }
            }),
            visual,
        )
    }
}

fn option_row<T: Clone + PartialEq + 'static>(
    state: &SelectState<T>,
    tokens: &aimer_style::ThemeTokens,
    option: &ChoiceOption<T>,
) -> AnyWidget {
    let key = option.key().to_owned();
    let selected = state.control.selected() == Some(option.value());
    let disabled = option.disabled();
    let visual = labeled_row(
        tokens,
        indicator(tokens, selected, tokens.shape.small, false, false, disabled),
        Some(option.label()),
        disabled,
    );
    let updater = state.updater;
    wrap_interactive(
        disabled,
        Rc::new(move || {
            let key = key.clone();
            updater.set_state(move |state| {
                let _ = state.control.select_key(&key);
            });
        }),
        Rc::new(|_| {}),
        Rc::new(|_| {}),
        Rc::new(|_| false),
        visual,
    )
}

impl<T: Clone + PartialEq + 'static> Widget for Select<T> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "Select", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "Select"
    }
}

impl<T: 'static> PortableWidget for Select<T> {}

/// Retained state for an [`Autocomplete`] widget.
pub struct AutocompleteState<T> {
    control: Autocomplete<T>,
    hovered: bool,
    updater: StateUpdater<Self>,
}

impl<T: Clone + PartialEq + 'static> AutocompleteState<T> {
    fn handle(&mut self, event: InputEvent) -> ControlAction<T> {
        self.control.handle_event(event)
    }
}

impl<T: Clone + PartialEq + 'static> StatefulWidget for Autocomplete<T> {
    type State = AutocompleteState<T>;

    fn create_state(self) -> Self::State {
        AutocompleteState {
            hovered: self.interaction_state().hovered(),
            control: self,
            updater: StateUpdater::empty(),
        }
    }
}

impl<T: Clone + PartialEq + 'static> State<Autocomplete<T>> for AutocompleteState<T> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn adopt_config_from(&mut self, new: Self) {
        let open = self.control.is_open();
        let focused = self.control.focused_index();
        let mut next = new.control;
        if open {
            let _ = next.open_menu();
        }
        if let Some(index) = focused {
            // Focus is transient; reconstruct after opening.
            let _ = index;
        }
        self.control = next;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let tokens = tokens(ctx);
        let query = if self.control.query().is_empty() {
            "Search…".to_owned()
        } else {
            self.control.query().to_owned()
        };
        let mut children: Vec<AnyWidget> = vec![
            Text::new(query)
                .text_style(
                    TextStyle::new()
                        .font_size(tokens.typography.body.font_size as u32)
                        .color(tokens.colors.on_surface),
                )
                .boxed(),
        ];
        if self.control.is_loading() {
            children.push(
                Text::new("Loading…")
                    .text_style(
                        TextStyle::new()
                            .font_size(tokens.typography.label.font_size as u32)
                            .color(tokens.colors.outline),
                    )
                    .boxed(),
            );
        }
        if let Some(error) = error_text(&tokens, self.control.interaction_state().error()) {
            children.push(error);
        }
        if self.control.is_open() && !self.control.is_loading() {
            let options: Vec<AnyWidget> = self
                .control
                .visible_options()
                .map(|option| {
                    let key = option.key().to_owned();
                    let disabled = option.disabled();
                    let visual = labeled_row(
                        &tokens,
                        indicator(
                            &tokens,
                            self.control.selected() == Some(option.value()),
                            tokens.shape.small,
                            false,
                            false,
                            disabled,
                        ),
                        Some(option.label()),
                        disabled,
                    );
                    let updater = self.updater;
                    wrap_interactive(
                        disabled,
                        Rc::new(move || {
                            let key = key.clone();
                            updater.set_state(move |state| {
                                let _ = state.control.select_key(&key);
                            });
                        }),
                        Rc::new(|_| {}),
                        Rc::new(|_| {}),
                        Rc::new(|_| false),
                        visual,
                    )
                })
                .collect();
            children.push(
                Column::new()
                    .horizontal_alignment(BoxAlignment::Start)
                    .children(options)
                    .boxed(),
            );
        }
        let visual = control_shell(
            &tokens,
            self.hovered,
            self.control.interaction_state().pressed(),
            self.control.interaction_state().disabled(),
            None,
            Container::new().child(
                Column::new()
                    .horizontal_alignment(BoxAlignment::Start)
                    .gaps(LayoutSpacing::all(Spacing::Px(tokens.spacing.x_small as u32)))
                    .children(children),
            )
            .boxed(),
        );
        let updater = self.updater;
        wrap_interactive(
            self.control.interaction_state().disabled(),
            Rc::new({
                let updater = updater;
                move || {
                    updater.set_state(|state| {
                        let _ = state.handle(InputEvent::PointerDown);
                        let _ = state.handle(InputEvent::PointerUp { inside: true });
                    });
                }
            }),
            Rc::new({
                let updater = updater;
                move |pressed| {
                    updater.set_state(move |state| {
                        let event = if pressed {
                            InputEvent::PointerDown
                        } else {
                            InputEvent::Cancel
                        };
                        let _ = state.handle(event);
                    });
                }
            }),
            Rc::new({
                let updater = updater;
                move |hovered| updater.set_state(move |state| state.hovered = hovered)
            }),
            Rc::new({
                let updater = updater;
                move |key| {
                    if is_activation_key(key)
                        || matches!(
                            key,
                            Key::ArrowDown
                                | Key::ArrowUp
                                | Key::Escape
                                | Key::Home
                                | Key::End
                        )
                    {
                        updater.set_state(move |state| {
                            let _ = state.handle(InputEvent::KeyDown(key));
                        });
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

impl<T: Clone + PartialEq + 'static> Widget for Autocomplete<T> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        StatefulElement::new_with_name(self, ctx, "Autocomplete", None)
            .0
            .boxed()
    }

    fn debug_name(&self) -> &'static str {
        "Autocomplete"
    }
}

impl<T: 'static> PortableWidget for Autocomplete<T> {}
