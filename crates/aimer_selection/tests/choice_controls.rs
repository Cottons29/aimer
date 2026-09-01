use std::cell::RefCell;
use std::rc::Rc;

use aimer_selection::{
    Autocomplete, Checkbox, CheckboxValue, ChoiceOption, ControlAction, InputEvent, Key,
    RadioGroup, Select, Switch,
};

#[test]
fn checkbox_is_controlled_and_activates_from_pointer_and_space() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&changes);
    let mut checkbox = Checkbox::new()
        .with_value(CheckboxValue::Unchecked)
        .on_change(move |value| observed.borrow_mut().push(value));

    assert_eq!(checkbox.handle_event(InputEvent::PointerDown), ControlAction::Pressed);
    assert!(checkbox.interaction_state().pressed());
    assert_eq!(
        checkbox.handle_event(InputEvent::PointerUp { inside: true }),
        ControlAction::Activated(CheckboxValue::Checked)
    );
    assert_eq!(checkbox.current_value(), CheckboxValue::Unchecked);

    checkbox = Checkbox::new()
        .with_value(CheckboxValue::Checked)
        .on_change({
            let changes = Rc::clone(&changes);
            move |value| changes.borrow_mut().push(value)
        });
    assert_eq!(
        checkbox.handle_event(InputEvent::KeyDown(Key::Space)),
        ControlAction::Activated(CheckboxValue::Unchecked)
    );
    assert_eq!(
        changes.borrow().as_slice(),
        &[CheckboxValue::Checked, CheckboxValue::Unchecked]
    );
}

#[test]
fn tri_state_disabled_and_semantics_are_observable_without_mutating_value() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&changes);
    let mut checkbox = Checkbox::new()
        .with_value(CheckboxValue::Indeterminate)
        .with_label("Accept terms")
        .focused(true)
        .hovered(true)
        .error("A decision is required")
        .on_change(move |value| observed.borrow_mut().push(value));

    assert_eq!(checkbox.handle_event(InputEvent::KeyDown(Key::Enter)), ControlAction::Activated(CheckboxValue::Checked));
    assert_eq!(checkbox.current_value(), CheckboxValue::Indeterminate);
    assert_eq!(changes.borrow().as_slice(), &[CheckboxValue::Checked]);

    assert_eq!(checkbox.handle_event(InputEvent::PointerDown), ControlAction::Pressed);
    assert_eq!(checkbox.handle_event(InputEvent::PointerUp { inside: false }), ControlAction::Released);
    assert!(!checkbox.interaction_state().pressed());

    let semantics = checkbox.semantics();
    assert_eq!(semantics.role(), aimer_selection::SemanticRole::Checkbox);
    assert_eq!(semantics.label(), Some("Accept terms"));
    assert!(semantics.enabled());
    assert!(semantics.focused());
    assert!(semantics.hovered());
    assert_eq!(semantics.checked(), Some(CheckboxValue::Indeterminate));
    assert_eq!(semantics.error(), Some("A decision is required"));

    let mut disabled = Switch::new().with_value(true).disabled(true);
    assert_eq!(disabled.handle_event(InputEvent::PointerDown), ControlAction::Ignored);
    assert_eq!(disabled.handle_event(InputEvent::KeyDown(Key::Space)), ControlAction::Ignored);
    assert_eq!(disabled.value(), true);
    assert!(!disabled.semantics().enabled());
}

#[test]
fn radio_group_is_exclusive_skips_disabled_options_and_retains_controlled_selection() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&changes);
    let options = vec![
        ChoiceOption::new("first", "Same label", 1),
        ChoiceOption::new("disabled", "Same label", 2).with_disabled(true),
        ChoiceOption::new("last", "Same label", 3),
    ];
    let mut group = RadioGroup::new()
        .try_options(options)
        .expect("unique keys are valid")
        .with_selected(Some(1))
        .on_change(move |value| observed.borrow_mut().push(value));

    assert_eq!(
        group.handle_event(InputEvent::KeyDown(Key::ArrowDown)),
        ControlAction::Activated(3)
    );
    assert_eq!(group.focused_index(), Some(2));
    assert_eq!(group.selected(), Some(&1));
    assert_eq!(changes.borrow().as_slice(), &[3]);

    let mut rebuilt = RadioGroup::new()
        .try_options(group.options().to_vec())
        .expect("the same option keys remain valid")
        .with_selected(Some(3))
        .with_focus_index(Some(2));
    assert_eq!(rebuilt.handle_event(InputEvent::KeyDown(Key::Space)), ControlAction::Ignored);
    assert_eq!(rebuilt.activate_key("disabled"), ControlAction::Ignored);
    assert_eq!(rebuilt.activate_key("missing"), ControlAction::Ignored);

    let radio = aimer_selection::Radio::new("free")
        .with_selected(true)
        .with_label("Free plan");
    assert_eq!(radio.activate(), ControlAction::Ignored);
    assert_eq!(radio.semantics().selected(), Some(true));
}

#[test]
fn duplicate_option_keys_are_rejected_but_duplicate_labels_are_allowed() {
    let result = Select::new().try_options([
        ChoiceOption::new("same", "Repeated", 1),
        ChoiceOption::new("same", "Repeated", 2),
    ]);
    assert!(matches!(
        result,
        Err(aimer_selection::OptionError::DuplicateKey(key)) if key == "same"
    ));

    let select = Select::new()
        .try_options([
            ChoiceOption::new("one", "Repeated", 1),
            ChoiceOption::new("two", "Repeated", 2),
        ])
        .expect("duplicate labels do not identify options");
    assert_eq!(select.options()[0].label(), "Repeated");
    assert_eq!(select.options()[1].key(), "two");
}

#[test]
fn select_opens_navigates_selects_and_cancels_without_changing_controlled_value() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let observed = Rc::clone(&changes);
    let mut select = Select::new()
        .try_options([
            ChoiceOption::new("one", "One", 1),
            ChoiceOption::new("disabled", "Disabled", 2).with_disabled(true),
            ChoiceOption::new("three", "Three", 3),
        ])
        .expect("unique keys are valid")
        .with_selected(Some(1))
        .on_change(move |value| observed.borrow_mut().push(value));

    assert_eq!(select.handle_event(InputEvent::KeyDown(Key::Enter)), ControlAction::Opened);
    assert!(select.is_open());
    assert_eq!(select.focused_key(), Some("one"));
    assert_eq!(
        select.handle_event(InputEvent::KeyDown(Key::ArrowDown)),
        ControlAction::FocusMoved(2)
    );
    assert_eq!(select.focused_key(), Some("three"));
    assert_eq!(select.select_key("disabled"), ControlAction::Ignored);
    assert_eq!(select.select_key("three"), ControlAction::Activated(3));
    assert!(!select.is_open());
    assert_eq!(select.selected(), Some(&1));
    assert_eq!(changes.borrow().as_slice(), &[3]);

    assert_eq!(select.handle_event(InputEvent::KeyDown(Key::Space)), ControlAction::Opened);
    assert_eq!(select.handle_event(InputEvent::KeyDown(Key::Escape)), ControlAction::Cancelled);
    assert!(!select.is_open());
    assert_eq!(select.selected(), Some(&1));
    assert_eq!(changes.borrow().as_slice(), &[3]);
}

#[test]
fn autocomplete_filters_by_query_and_reports_loading_error_and_query_callbacks() {
    let queries = Rc::new(RefCell::new(Vec::new()));
    let observed_queries = Rc::clone(&queries);
    let mut autocomplete = Autocomplete::new()
        .try_options([
            ChoiceOption::new("first", "Alpha", 1),
            ChoiceOption::new("second", "Alpha", 2),
            ChoiceOption::new("third", "Beta", 3),
        ])
        .expect("unique keys are valid")
        .with_query("al")
        .on_query_change(move |query| observed_queries.borrow_mut().push(query))
        .loading(true)
        .error("Suggestions unavailable");

    let visible_keys: Vec<_> = autocomplete.visible_options().map(|option| option.key()).collect();
    assert_eq!(visible_keys, ["first", "second"]);
    assert_eq!(autocomplete.change_query("be"), Some("be".to_owned()));
    assert_eq!(autocomplete.query(), "al");
    assert_eq!(queries.borrow().as_slice(), &["be"]);

    assert_eq!(autocomplete.handle_event(InputEvent::KeyDown(Key::ArrowDown)), ControlAction::Opened);
    assert_eq!(autocomplete.select_key("first"), ControlAction::Ignored);
    let semantics = autocomplete.semantics();
    assert_eq!(semantics.role(), aimer_selection::SemanticRole::Autocomplete);
    assert!(semantics.busy());
    assert_eq!(semantics.expanded(), Some(true));
    assert_eq!(semantics.error(), Some("Suggestions unavailable"));

    autocomplete = Autocomplete::new()
        .try_options([
            ChoiceOption::new("first", "Alpha", 1),
            ChoiceOption::new("second", "Alpha", 2),
        ])
        .expect("unique keys are valid")
        .with_query("al");
    assert_eq!(autocomplete.handle_event(InputEvent::KeyDown(Key::Enter)), ControlAction::Opened);
    assert_eq!(autocomplete.select_key("second"), ControlAction::Activated(2));
    assert_eq!(autocomplete.selected(), None);
}
