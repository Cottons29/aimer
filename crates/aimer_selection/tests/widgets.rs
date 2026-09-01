use std::cell::{Cell, RefCell};
use std::rc::Rc;

use aimer_attribute::BoxConstraint;
use aimer_container::{Container, SizedBox};
use aimer_events::element::ElementEvent;
use aimer_events::pointer::{PointerButton, PointerInfo};
use aimer_attribute::size::ResolvedSize;
use aimer_flex::{Column, Row};
use aimer_text::Text;
use aimer_selection::{
    Autocomplete, Checkbox, CheckboxValue, ChoiceOption, Radio, RadioGroup, Select, Switch,
};
use aimer_widget::base::{BuildContext, WindowHandle};
use aimer_widget::{
    Drawable, EventDispatcher, LayoutElement, PortableWidget, State, StatefulWidget,
    VisitorElement, Widget,
};

fn context() -> BuildContext<'static> {
    let canvas = {
        let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
        aimer_canvas::Canvas::new(inner)
    };
    let mut ctx = BuildContext::new(
        canvas,
        ResolvedSize {
            width: 400.0,
            height: 400.0,
        },
        1.0,
        Default::default(),
        Default::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    );
    // `BuildContext::new` leaves `box_constraint` at its zeroed default; a
    // real host seeds it from the window/parent size before the first
    // layout pass, which is what lets an `Auto`-width container like the
    // choice-control shell resolve to real space instead of zero.
    ctx.box_constraint = BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 400.0,
        max_height: 400.0,
    };
    ctx
}

#[tokio::test]
async fn checkbox_switch_and_radio_are_named_widgets() {
    let ctx = context();
    let checkbox = Checkbox::new()
        .with_value(CheckboxValue::Checked)
        .with_label("Accept");
    assert_eq!(Widget::debug_name(&checkbox), "Checkbox");
    assert_eq!(checkbox.to_element(&ctx).debug_name(), "Checkbox");

    let switch = Switch::new().with_value(true).with_label("Alerts");
    assert_eq!(Widget::debug_name(&switch), "Switch");
    assert_eq!(switch.to_element(&ctx).debug_name(), "Switch");

    let radio = Radio::new("pro").with_selected(true).with_label("Pro");
    assert_eq!(Widget::debug_name(&radio), "Radio");
    assert_eq!(radio.to_element(&ctx).debug_name(), "Radio");
}

#[tokio::test]
async fn disabled_choice_widgets_still_build() {
    let ctx = context();
    Checkbox::new()
        .disabled(true)
        .with_label("Locked")
        .to_element(&ctx);
    Switch::new().disabled(true).to_element(&ctx);
    Radio::new(1).disabled(true).to_element(&ctx);
}

#[tokio::test]
async fn checkbox_rebuild_keeps_the_child_label_and_adopts_the_controlled_value() {
    let ctx = context();
    let mut state = Checkbox::new()
        .with_value(CheckboxValue::Unchecked)
        .with_label("Terms")
        .create_state();
    state.build(&ctx).to_element(&ctx);

    state.adopt_config_from(
        Checkbox::new()
            .with_value(CheckboxValue::Checked)
            .with_label("Terms")
            .create_state(),
    );
    assert_eq!(state.current_value(), CheckboxValue::Checked);
    state.build(&ctx).to_element(&ctx);
}

#[tokio::test]
async fn checkbox_hit_target_meets_density_minimum() {
    let ctx = context();
    let element = Checkbox::new().with_label("Hit").to_element(&ctx);
    let size = element.layout(&ctx);
    assert!(
        size.width >= 44.0 && size.height >= 44.0,
        "choice controls must honor the style density minimum target, got {size:?}"
    );
}

#[tokio::test]
async fn radio_group_builds_exclusive_options_with_stable_keys() {
    let ctx = context();
    let group = RadioGroup::new()
        .try_options([
            ChoiceOption::new("basic", "Basic", "basic"),
            ChoiceOption::new("pro", "Pro", "pro"),
        ])
        .expect("unique keys")
        .with_selected(Some("basic"));
    assert_eq!(Widget::debug_name(&group), "RadioGroup");
    group.to_element(&ctx);
}

/// Lays a single widget out as the only child of a `Column`, which is how
/// every choice control is actually mounted in practice. A choice control
/// placed directly as the *root* element gets a fully bounded constraint
/// that a `Dimension::Auto` container fills regardless of its content, which
/// would hide a shrink-to-fit regression instead of catching it.
fn column_wrapped_height(ctx: &BuildContext, widget: impl Widget + 'static) -> f32 {
    Column::new().children([widget.boxed()]).to_element(ctx).layout(ctx).height
}

#[tokio::test]
async fn radio_group_lays_out_every_option_instead_of_clipping_to_one_row() {
    // Regression test: the shared control shell used to force every choice
    // control to one fixed single-row height, which silently clipped every
    // radio option beyond the first instead of growing to show them all.
    let ctx = context();
    let one_option = RadioGroup::new()
        .try_options([ChoiceOption::new("basic", "Basic", "basic")])
        .expect("unique keys")
        .with_selected(Some("basic"));
    let three_options = RadioGroup::new()
        .try_options([
            ChoiceOption::new("basic", "Basic", "basic"),
            ChoiceOption::new("pro", "Pro", "pro"),
            ChoiceOption::new("enterprise", "Enterprise", "enterprise"),
        ])
        .expect("unique keys")
        .with_selected(Some("basic"));

    let one_row_height = column_wrapped_height(&ctx, one_option);
    let three_row_height = column_wrapped_height(&ctx, three_options);

    assert!(
        three_row_height > one_row_height * 1.5,
        "a three-option radio group must lay out taller than a one-option group instead of \
         clipping to a single row's height: one={one_row_height}, three={three_row_height}"
    );
}

#[tokio::test]
async fn select_open_surface_grows_to_show_every_option() {
    // Regression test: an open select must grow past its closed trigger
    // height to actually show its option list, not clip to one row.
    let ctx = context();
    let closed = Select::new()
        .try_options([
            ChoiceOption::new("small", "Small", "small"),
            ChoiceOption::new("medium", "Medium", "medium"),
            ChoiceOption::new("large", "Large", "large"),
        ])
        .expect("unique keys");
    let mut opened = Select::new()
        .try_options([
            ChoiceOption::new("small", "Small", "small"),
            ChoiceOption::new("medium", "Medium", "medium"),
            ChoiceOption::new("large", "Large", "large"),
        ])
        .expect("unique keys");
    let _ = opened.open_menu();

    let closed_height = column_wrapped_height(&ctx, closed);
    let opened_height = column_wrapped_height(&ctx, opened);

    assert!(
        opened_height > closed_height * 1.5,
        "an open select must grow to show its option list instead of clipping to the closed \
         trigger's height: closed={closed_height}, opened={opened_height}"
    );
}

#[tokio::test]
async fn select_retains_open_surface_across_controlled_rebuilds() {
    let ctx = context();
    let mut state = Select::new()
        .try_options([
            ChoiceOption::new("one", "One", 1),
            ChoiceOption::new("two", "Two", 2),
        ])
        .expect("unique keys")
        .with_selected(Some(1))
        .create_state();
    state.open_menu();
    assert!(state.is_open());

    state.adopt_config_from(
        Select::new()
            .try_options([
                ChoiceOption::new("one", "One", 1),
                ChoiceOption::new("two", "Two", 2),
            ])
            .expect("unique keys")
            .with_selected(Some(2))
            .create_state(),
    );
    assert!(state.is_open());
    assert_eq!(state.selected(), Some(&2));
    state.build(&ctx).to_element(&ctx);
}

#[tokio::test]
async fn autocomplete_builds_loading_and_error_states() {
    let ctx = context();
    let widget = Autocomplete::new()
        .try_options([
            ChoiceOption::new("apple", "Apple", "apple"),
            ChoiceOption::new("apricot", "Apricot", "apricot"),
        ])
        .expect("unique keys")
        .with_query("ap")
        .loading(true)
        .error("offline");
    assert_eq!(Widget::debug_name(&widget), "Autocomplete");
    widget.to_element(&ctx);
}

#[tokio::test]
async fn choice_widgets_use_the_default_portable_contract() {
    fn assert_portable<T: PortableWidget>() {}
    assert_portable::<Checkbox>();
    assert_portable::<Switch>();
    assert_portable::<Radio<&'static str>>();
    assert_portable::<RadioGroup<i32>>();
    assert_portable::<Select<i32>>();
    assert_portable::<Autocomplete<&'static str>>();
}

#[tokio::test]
async fn checkbox_on_change_is_retained_by_state() {
    let calls = Rc::new(Cell::new(0_u32));
    let observed = Rc::clone(&calls);
    let state = Checkbox::new()
        .with_value(CheckboxValue::Unchecked)
        .on_change(move |_| observed.set(observed.get() + 1))
        .create_state();
    state.propose_activation();
    assert_eq!(calls.get(), 1);
    assert_eq!(state.current_value(), CheckboxValue::Unchecked);
}

#[tokio::test]
async fn checkbox_widget_dispatches_a_pointer_tap_to_on_change() {
    let ctx = context();
    let calls = Rc::new(Cell::new(0_u32));
    let observed = Rc::clone(&calls);
    let element = Checkbox::new()
        .with_label("Accept")
        .on_change(move |_| observed.set(observed.get() + 1))
        .to_element(&ctx);
    let size = element.layout(&ctx);
    assert!(size.width > 0.0 && size.height > 0.0);
    element.draw(&ctx);

    let pointer = PointerInfo::mouse(
        aimer_widget::base::Vec2d { x: 8.0, y: 8.0 },
        PointerButton::Primary,
    );
    let mut dispatcher = EventDispatcher::new();
    let down = dispatcher.dispatch(
        element.as_ref(),
        pointer.pos,
        &ElementEvent::PointerDown(pointer),
    );
    element.draw(&ctx);
    let up = dispatcher.dispatch(
        element.as_ref(),
        pointer.pos,
        &ElementEvent::PointerUp(pointer),
    );
    element.draw(&ctx);

    assert!(down.is_consumed());
    assert!(up.is_consumed());
    assert_eq!(calls.get(), 1);
}

#[tokio::test]
async fn checkbox_can_build_a_custom_box_tick_and_label_composition() {
    let ctx = context();
    let builds = Rc::new(Cell::new(0_u32));
    let observed = Rc::clone(&builds);
    let element = Checkbox::new()
        .with_value(CheckboxValue::Checked)
        .with_label("Accept")
        .builder(move |state| {
            observed.set(observed.get() + 1);
            let tick = if state.is_checked() {
                Text::new("✓").boxed()
            } else {
                SizedBox::new().width(0.0).height(0.0).boxed()
            };
            let box_widget = Container::new().child(tick).boxed();
            Row::new()
                .children([
                    box_widget,
                    Text::new(state.label().unwrap_or_default().to_owned()).boxed(),
                ])
                .boxed()
        })
        .to_element(&ctx);

    let size = element.layout(&ctx);
    assert!(size.width > 0.0 && size.height > 0.0);
    assert!(builds.get() > 0, "the custom composition builder must run");
}

#[tokio::test]
async fn checkbox_can_wrap_an_explicit_widget_child() {
    let ctx = context();
    let element = Checkbox::new()
        .child(
            Row::new()
                .children([Text::new("□").boxed(), Text::new("Accept").boxed()]),
        )
        .to_element(&ctx);

    let size = element.layout(&ctx);
    assert!(size.width > 0.0 && size.height > 0.0);
}

#[tokio::test]
async fn checkbox_custom_composition_receives_the_controlled_value_after_rebuild() {
    let ctx = context();
    let values = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&values);
    let mut state = Checkbox::new()
        .with_value(CheckboxValue::Unchecked)
        .builder(move |visual| {
            observed.borrow_mut().push(visual.value());
            SizedBox::new().width(24.0).height(24.0).boxed()
        })
        .create_state();

    state.build(&ctx).to_element(&ctx);
    let observed = Rc::clone(&values);
    state.adopt_config_from(
        Checkbox::new()
            .with_value(CheckboxValue::Checked)
            .builder(move |visual| {
                observed.borrow_mut().push(visual.value());
                SizedBox::new().width(24.0).height(24.0).boxed()
            })
            .create_state(),
    );
    state.build(&ctx).to_element(&ctx);

    assert_eq!(values.borrow().as_slice(), &[CheckboxValue::Unchecked, CheckboxValue::Checked]);
}
