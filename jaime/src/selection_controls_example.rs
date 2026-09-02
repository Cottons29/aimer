//! A live, interactive showcase for Aimer's controlled choice controls.
//!
//! Every control below is the real widget from `aimer_selection`: clicking,
//! tapping, or using the keyboard drives its pointer/keyboard handling
//! directly, and each `on_change` callback feeds the proposed value back into
//! this page's own retained state — the same controlled-component loop an
//! application would use. Nothing here is a static snapshot.
//!
//! W17 exposes the choice-control package through the umbrella crate and
//! registers this page in the central showcase.

use aimer::macros::widget;
use aimer::style::{
    BorderSlice, BorderStyle, BoxBorder, BoxDecoration, FontWeight, LayoutSpacing, Spacing,
    TextAlign, TextStyle, Theme, ThemeData,
};
use aimer::{
    AimerApp, AnyWidget, BuildContext, Button, Column, Container, Dimension, Row, ScrollAxis,
    Scrollable, State, StateUpdater, StatefulWidget, Text, Widget,
};

pub use aimer::selection::CheckboxValue;
use aimer::selection::{Autocomplete, Checkbox, ChoiceOption, RadioGroup, Select, Switch};

use crate::theme;

/// Builds the choice-controls showcase without starting an application.
pub fn selection_controls_example() -> impl Widget {
    SelectionControlsExample::new()
}

/// Starts the standalone choice-controls showcase.
pub fn start_selection_controls_example() {
    AimerApp::start(theme::provide(selection_controls_example()));
}

/// A live page exercising checkbox, switch, radio, select, and autocomplete
/// interactions with real pointer/keyboard activation.
#[widget(Stateful)]
pub struct SelectionControlsExample {}

impl SelectionControlsExample {
    #[inline]
    pub const fn new() -> Self {
        Self {}
    }

    fn section(
        title: &'static str,
        status: String,
        control: AnyWidget,
        app_theme: ThemeData,
    ) -> AnyWidget {
        Container::new()
            .padding(LayoutSpacing::all(Spacing::Px(16)))
            .box_decoration(
                BoxDecoration::new()
                    .background_color(app_theme.surface_color)
                    .border(
                        BoxBorder::all(
                            BorderSlice::new()
                                .style(BorderStyle::Solid)
                                .stroke(1.0)
                                .color(theme::divider(&app_theme)),
                        ),
                    )
                    .border_radius(12),
            )
            .child(
                Column::new()
                    .gaps(LayoutSpacing::all(Spacing::Px(8)))
                    .children([
                        Text::new(title)
                            .text_style(
                                TextStyle::new()
                                    .font_size(16)
                                    .font_weight(FontWeight::Bold)
                                    .color(app_theme.on_surface_color),
                            )
                            .boxed(),
                        Text::new(status)
                            .text_style(
                                TextStyle::new()
                                    .font_size(13)
                                    .color(theme::muted_text(&app_theme)),
                            )
                            .boxed(),
                        control,
                    ]),
            )
            .boxed()
    }
}

impl Default for SelectionControlsExample {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SelectionControlsExampleState {
    checkbox_value: CheckboxValue,
    switch_value: bool,
    radio_selected: &'static str,
    select_selected: Option<&'static str>,
    autocomplete_query: String,
    autocomplete_selected: Option<&'static str>,
    updater: StateUpdater<Self>,
}

impl SelectionControlsExampleState {
    fn initial() -> Self {
        Self {
            checkbox_value: CheckboxValue::Unchecked,
            switch_value: false,
            radio_selected: "basic",
            select_selected: None,
            autocomplete_query: String::new(),
            autocomplete_selected: None,
            updater: StateUpdater::empty(),
        }
    }
}

impl StatefulWidget for SelectionControlsExample {
    type State = SelectionControlsExampleState;

    fn create_state(self) -> Self::State {
        SelectionControlsExampleState::initial()
    }
}

impl State<SelectionControlsExample> for SelectionControlsExampleState {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let app_theme = ThemeData::copied(ctx);

        let checkbox_updater = self.updater;
        let checkbox_theme = app_theme;
        let checkbox = Checkbox::new()
            .with_value(self.checkbox_value)
            .with_label("Accept terms")
            .on_change(move |value| {
                checkbox_updater.set_state(move |state| state.checkbox_value = value);
            })
            .builder(move |state| {
                let mark = if state.is_checked() { "✓" } else { "" };
                let indicator_fill = if state.is_checked() {
                    checkbox_theme.primary_color
                } else if state.is_hovered() || state.is_pressed() {
                    theme::raised_surface(&checkbox_theme)
                } else {
                    theme::recessed_surface(&checkbox_theme)
                };
                let indicator_border = if state.is_disabled() {
                    theme::muted_text(&checkbox_theme)
                } else {
                    checkbox_theme.primary_color
                };
                Row::new()
                    .gaps(LayoutSpacing::all(Spacing::Px(8)))
                    .children([
                        Container::new()
                            .width(Dimension::Px(28.0))
                            .height(Dimension::Px(28.0))
                            .box_decoration(
                                BoxDecoration::new()
                                    .background_color(indicator_fill)
                                    .border(BoxBorder::all(
                                        BorderSlice::new()
                                            .style(BorderStyle::Solid)
                                            .stroke(1.5)
                                            .color(indicator_border),
                                    ))
                                    .border_radius(7),
                            )
                            .child(
                                Text::new(mark)
                                    .text_align(TextAlign::MidCenter)
                                    .text_style(
                                        TextStyle::new()
                                            .font_size(18)
                                            .font_weight(FontWeight::Bold)
                                            .color(checkbox_theme.on_primary_color),
                                    ),
                            )
                            .boxed(),
                        Text::new(state.label().unwrap_or_default().to_owned())
                            .text_style(
                                TextStyle::new()
                                    .font_size(15)
                                    .color(if state.is_disabled() {
                                        theme::muted_text(&checkbox_theme)
                                    } else {
                                        checkbox_theme.on_surface_color
                                    }),
                            )
                            .boxed(),
                    ])
                    .boxed()
            });
        let disabled_checkbox = Checkbox::new()
            .with_value(CheckboxValue::Checked)
            .with_label("Locked by policy (disabled)")
            .disabled(true);

        let switch_updater = self.updater;
        let switch = Switch::new()
            .with_value(self.switch_value)
            .with_label("Email notifications")
            .on_change(move |value| {
                switch_updater.set_state(move |state| state.switch_value = value);
            });

        let radio_updater = self.updater;
        let radios = RadioGroup::new()
            .try_options([
                ChoiceOption::new("basic", "Basic", "basic"),
                ChoiceOption::new("pro", "Pro", "pro"),
                ChoiceOption::new("enterprise", "Enterprise (disabled)", "enterprise")
                    .with_disabled(true),
            ])
            .expect("the example uses unique radio keys")
            .with_label("Plan")
            .with_selected(Some(self.radio_selected))
            .on_change(move |value| {
                radio_updater.set_state(move |state| state.radio_selected = value);
            });

        let select_updater = self.updater;
        let select = Select::new()
            .try_options([
                ChoiceOption::new("small", "Small", "small"),
                ChoiceOption::new("medium", "Medium", "medium"),
                ChoiceOption::new("large", "Large", "large"),
            ])
            .expect("the example uses unique select keys")
            .with_label("Size")
            .with_selected(self.select_selected)
            .on_change(move |value| {
                select_updater.set_state(move |state| state.select_selected = Some(value));
            });

        let autocomplete_updater = self.updater;
        let autocomplete = Autocomplete::new()
            .try_options([
                ChoiceOption::new("apple", "Apple", "apple"),
                ChoiceOption::new("apricot", "Apricot", "apricot"),
                ChoiceOption::new("banana", "Banana", "banana"),
                ChoiceOption::new("blueberry", "Blueberry", "blueberry"),
                ChoiceOption::new("cherry", "Cherry", "cherry"),
            ])
            .expect("the example uses unique autocomplete keys")
            .with_label("Fruit")
            .with_query(self.autocomplete_query.clone())
            .with_selected(self.autocomplete_selected)
            .on_change(move |value| {
                autocomplete_updater
                    .set_state(move |state| state.autocomplete_selected = Some(value));
            });

        let autocomplete_filters = Row::new()
            .gaps(LayoutSpacing::all(Spacing::Px(8)))
            .children([
                filter_chip("Query: \"ap\"", &self.updater, "ap", app_theme),
                filter_chip("Query: \"b\"", &self.updater, "b", app_theme),
                filter_chip("Clear query", &self.updater, "", app_theme),
            ])
            .boxed();

        let reset_updater = self.updater;
        let reset = action_button("Reset all", app_theme, move || {
            reset_updater.set_state(|state| {
                *state = SelectionControlsExampleState {
                    updater: state
                        .updater
                        .clone(),
                    ..SelectionControlsExampleState::initial()
                }
            });
        });

        Scrollable::new()
            .axis(ScrollAxis::Vertical)
            .child(
                Container::new()
                    .color(app_theme.background_color)
                    .padding(LayoutSpacing::all(Spacing::Px(4)))
                    .child(
                        Column::new()
                            .gaps(LayoutSpacing::all(Spacing::Px(16)))
                            .children([
                                Text::new("Choice controls")
                                    .text_style(
                                        TextStyle::new()
                                            .font_size(26)
                                            .font_weight(FontWeight::Bold)
                                            .color(app_theme.on_background_color),
                                    )
                                    .boxed(),
                                Text::new(
                                    "Every control is live: click, tap, or use the keyboard. Each \
                             change flows through a real on_change callback back into this \
                             page's own state.",
                                )
                                .text_style(
                                    TextStyle::new()
                                        .font_size(15)
                                        .color(theme::muted_text(&app_theme)),
                                )
                                .wrapped()
                                .boxed(),
                                SelectionControlsExample::section(
                                    "Checkbox",
                                    format!("Proposed value: {:?}", self.checkbox_value),
                                    Column::new()
                                        .gaps(LayoutSpacing::all(Spacing::Px(4)))
                                        .children([checkbox.boxed(), disabled_checkbox.boxed()])
                                        .boxed(),
                                    app_theme,
                                ),
                                SelectionControlsExample::section(
                                    "Switch",
                                    format!("On: {}", self.switch_value),
                                    switch.boxed(),
                                    app_theme,
                                ),
                                SelectionControlsExample::section(
                                    "Radio group",
                                    format!("Selected plan: {}", self.radio_selected),
                                    radios.boxed(),
                                    app_theme,
                                ),
                                SelectionControlsExample::section(
                                    "Select",
                                    format!(
                                        "Selected size: {}",
                                        self.select_selected
                                            .unwrap_or("(none yet — click to open)")
                                    ),
                                    select.boxed(),
                                    app_theme,
                                ),
                                SelectionControlsExample::section(
                                    "Autocomplete",
                                    format!(
                                        "Query: {:?}; selected: {}",
                                        self.autocomplete_query,
                                        self.autocomplete_selected
                                            .unwrap_or("(none yet)")
                                    ),
                                    Column::new()
                                        .gaps(LayoutSpacing::all(Spacing::Px(8)))
                                        .children([autocomplete_filters, autocomplete.boxed()])
                                        .boxed(),
                                    app_theme,
                                ),
                                reset,
                            ]),
                    ),
            )
    }
}

fn filter_chip(
    label: &'static str,
    updater: &StateUpdater<SelectionControlsExampleState>,
    query: &'static str,
    app_theme: ThemeData,
) -> AnyWidget {
    let updater = *updater;
    action_button(label, app_theme, move || {
        updater.set_state(move |state| state.autocomplete_query = query.to_owned());
    })
}

fn action_button(
    label: &'static str,
    app_theme: ThemeData,
    on_press: impl Fn() + 'static,
) -> AnyWidget {
    Button::new()
        .on_press(on_press)
        .decoration(
            BoxDecoration::new()
                .background_color(app_theme.primary_color)
                .border_radius(8),
        )
        .hover_decoration(
            BoxDecoration::new()
                .background_color(app_theme.primary_color.lighten(0.08))
                .border_radius(8),
        )
        .press_decoration(
            BoxDecoration::new()
                .background_color(app_theme.primary_color.darken(0.08))
                .border_radius(8),
        )
        .child(
            Container::new()
                .height(Dimension::Px(32.0))
                .padding(
                    LayoutSpacing::new()
                        .left(10)
                        .right(10),
                )
                .child(
                    Text::new(label)
                        .text_align(TextAlign::MidCenter)
                        .text_style(
                            TextStyle::new()
                                .font_size(13)
                                .font_weight(FontWeight::Bold)
                                .color(app_theme.on_primary_color),
                        ),
                ),
        )
        .boxed()
}
