use std::cell::RefCell;
use std::rc::Rc;

use aimer_attribute::BoxConstraint;
use aimer_attribute::size::ResolvedSize;
use aimer_container::Container;
use aimer_cupid::draw_cmd::DrawCommand;
use aimer_events::element::{ElementEvent, KeyAction, Modifiers, NamedKey};
use aimer_events::pointer::{PointerButton, PointerInfo};
use aimer_range::{RangeSlider, RangeThumb, Slider, SliderThumb, SliderTrail};
use aimer_style::{LayoutSpacing, Spacing};
use aimer_widget::base::{BuildContext, Vec2d, WindowHandle};
use aimer_widget::{
    Drawable, ErrorWidget, EventDispatcher, LayoutElement, State, StatefulWidget, Widget,
};

#[test]
fn slider_builder_constructs_a_standalone_value_model() {
    let slider = Slider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(20.0);

    assert_eq!(slider.current_value(), 20.0);
    assert_eq!(slider.min(), 0.0);
    assert_eq!(slider.max(), 100.0);
    assert_eq!(slider.step_value(), 10.0);
}

#[test]
fn range_slider_builder_accepts_integer_ranges() {
    let slider = RangeSlider::new()
        .range(0_u32..100_u32)
        .step(10_u32)
        .value(20_u32..70_u32);

    assert_eq!(slider.current_values(), 20_u32..70_u32);
    assert_eq!(slider.min(), 0_u32);
    assert_eq!(slider.max(), 100_u32);
}

#[tokio::test]
async fn range_controls_keep_composable_visual_slots_in_the_element_tree() {
    let ctx = context();
    let slider = Slider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(20.0)
        .track(ErrorWidget::new("track"))
        .trail(ErrorWidget::new("trail"))
        .thumb(ErrorWidget::new("thumb"))
        .to_element(&ctx);
    slider.layout(&ctx);

    let mut slot_names = Vec::new();
    slider.visit_children(&mut |surface| {
        surface.visit_children(&mut |slot| slot_names.push(slot.debug_name()));
    });
    assert_eq!(
        slot_names,
        ["ErrorWidget", "ErrorWidget", "ErrorWidget"]
    );

    let range_slider = RangeSlider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(20.0..70.0)
        .track(ErrorWidget::new("track"))
        .trail(ErrorWidget::new("trail"))
        .thumbs(ErrorWidget::new("lower"), ErrorWidget::new("upper"))
        .to_element(&ctx);
    range_slider.layout(&ctx);

    let mut range_slot_names = Vec::new();
    range_slider.visit_children(&mut |surface| {
        surface.visit_children(&mut |slot| range_slot_names.push(slot.debug_name()));
    });
    assert_eq!(
        range_slot_names,
        ["ErrorWidget", "ErrorWidget", "ErrorWidget", "ErrorWidget"]
    );
}

#[test]
fn slider_visual_parts_are_standalone_widgets() {
    fn assert_widget(_: impl Widget) {}

    assert_widget(SliderTrail::new());
    assert_widget(SliderThumb::new());
}

#[tokio::test]
async fn slider_materializes_default_trail_and_thumb_widgets() {
    let ctx = context();
    let element = Slider::<f64>::new().to_element(&ctx);
    element.layout(&ctx);

    let mut slot_names = Vec::new();
    element.visit_children(&mut |surface| {
        surface.visit_children(&mut |slot| slot_names.push(slot.debug_name()));
    });

    assert_eq!(slot_names, ["SliderTrail", "SliderThumb"]);

    let range_element = RangeSlider::<f64>::new().to_element(&ctx);
    range_element.layout(&ctx);
    let mut range_slot_names = Vec::new();
    range_element.visit_children(&mut |surface| {
        surface.visit_children(&mut |slot| range_slot_names.push(slot.debug_name()));
    });
    assert_eq!(
        range_slot_names,
        ["SliderTrail", "SliderThumb", "SliderThumb"]
    );
}

fn context() -> BuildContext<'static> {
    let canvas = {
        let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
        aimer_canvas::Canvas::new(inner)
    };
    let mut ctx = BuildContext::new(
        canvas,
        ResolvedSize {
            width: 320.0,
            height: 120.0,
        },
        1.0,
        Default::default(),
        Default::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    );
    ctx.box_constraint = BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 320.0,
        max_height: 120.0,
    };
    ctx
}

#[tokio::test]
async fn slider_widget_dispatches_pointer_drag_and_keyboard_increment() {
    let ctx = context();
    let values = Rc::new(RefCell::new(Vec::<f64>::new()));
    let observed = Rc::clone(&values);
    let element = Slider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(20.0)
        .width(200.0)
        .on_change(move |value| observed.borrow_mut().push(value))
        .to_element(&ctx);

    let size = element.layout(&ctx);
    assert_eq!(size.width, 200.0);
    assert!(size.height > 0.0);
    element.draw(&ctx);

    let mut dispatcher = EventDispatcher::new();
    let down = PointerInfo::mouse(Vec2d { x: 20.0, y: 20.0 }, PointerButton::Primary);
    assert!(dispatcher
        .dispatch(element.as_ref(), down.pos, &ElementEvent::PointerDown(down))
        .is_consumed());

    let moved = down.at(Vec2d { x: 140.0, y: 20.0 });
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &ElementEvent::PointerMove(moved))
        .is_consumed());
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &ElementEvent::PointerUp(moved))
        .is_consumed());

    let key = ElementEvent::KeyInput {
        key: NamedKey::ArrowRight,
        action: KeyAction::Pressed,
        modifiers: Modifiers::default(),
    };
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &key)
        .is_consumed());

    let values = values.borrow();
    assert!(values.iter().any(|value| (*value - 10.0).abs() < f64::EPSILON));
    assert!(values.iter().any(|value| (*value - 70.0).abs() < f64::EPSILON));
    assert!(values.iter().any(|value| (*value - 80.0).abs() < f64::EPSILON));
}

#[tokio::test]
async fn slider_widget_dispatches_pointer_drag_when_nested_in_a_padded_container() {
    let ctx = context();
    let values = Rc::new(RefCell::new(Vec::<f64>::new()));
    let observed = Rc::clone(&values);
    let slider = Slider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(20.0)
        .width(200.0)
        .on_change(move |value| observed.borrow_mut().push(value));
    let element = Container::new()
        .padding(LayoutSpacing::all(Spacing::Px(20)))
        .child(slider)
        .to_element(&ctx);

    element.layout(&ctx);
    element.draw(&ctx);

    let mut dispatcher = EventDispatcher::new();
    let down = PointerInfo::mouse(Vec2d { x: 40.0, y: 40.0 }, PointerButton::Primary);
    assert!(dispatcher
        .dispatch(element.as_ref(), down.pos, &ElementEvent::PointerDown(down))
        .is_consumed());
    let moved = down.at(Vec2d { x: 160.0, y: 40.0 });
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &ElementEvent::PointerMove(moved))
        .is_consumed());
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &ElementEvent::PointerUp(moved))
        .is_consumed());

    assert!(values.borrow().iter().any(|value| *value == 70.0));
}

#[tokio::test]
async fn range_slider_widget_dispatches_the_nearest_thumb_and_keyboard_increment() {
    let ctx = context();
    let values = Rc::new(RefCell::new(Vec::<(f64, f64)>::new()));
    let observed = Rc::clone(&values);
    let element = RangeSlider::new()
        .range(0.0..100.0)
        .step(10.0)
        .values(20.0..70.0)
        .width(200.0)
        .on_change(move |value| observed.borrow_mut().push(value))
        .to_element(&ctx);
    let size = element.layout(&ctx);
    assert_eq!(size.width, 200.0);
    element.draw(&ctx);

    let mut dispatcher = EventDispatcher::new();
    let down = PointerInfo::mouse(Vec2d { x: 40.0, y: 20.0 }, PointerButton::Primary);
    assert!(dispatcher
        .dispatch(element.as_ref(), down.pos, &ElementEvent::PointerDown(down))
        .is_consumed());
    let moved = down.at(Vec2d { x: 100.0, y: 20.0 });
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &ElementEvent::PointerMove(moved))
        .is_consumed());
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &ElementEvent::PointerUp(moved))
        .is_consumed());

    let key = ElementEvent::KeyInput {
        key: NamedKey::ArrowRight,
        action: KeyAction::Pressed,
        modifiers: Modifiers::default(),
    };
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &key)
        .is_consumed());

    let values = values.borrow();
    assert!(values
        .iter()
        .any(|(lower, upper)| (*lower - 50.0).abs() < f64::EPSILON
            && (*upper - 70.0).abs() < f64::EPSILON));
    assert!(values
        .iter()
        .any(|(lower, upper)| (*lower - 60.0).abs() < f64::EPSILON
            && (*upper - 70.0).abs() < f64::EPSILON));
}

#[tokio::test]
async fn slider_state_retains_runtime_press_while_adopting_a_controlled_value() {
    let ctx = context();
    let mut state = Slider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(20.0)
        .width(200.0)
        .create_state();
    let element = state.build(&ctx).to_element(&ctx);
    element.layout(&ctx);
    element.draw(&ctx);

    let mut dispatcher = EventDispatcher::new();
    let pointer = PointerInfo::mouse(Vec2d { x: 40.0, y: 20.0 }, PointerButton::Primary);
    assert!(dispatcher
        .dispatch(element.as_ref(), pointer.pos, &ElementEvent::PointerDown(pointer))
        .is_consumed());
    assert!(state.is_pressed());
    assert!(state.is_focused());

    let moved = pointer.at(Vec2d { x: 100.0, y: 20.0 });
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &ElementEvent::PointerMove(moved))
        .is_consumed());
    assert_eq!(state.current_value(), 50.0);

    state.adopt_config_from(
        Slider::new()
            .range(0.0..100.0)
            .step(10.0)
            .value(80.0)
            .width(200.0)
            .create_state(),
    );
    assert_eq!(state.current_value(), 80.0);
    assert!(state.is_pressed());
}

#[tokio::test]
async fn range_slider_state_retains_the_selected_thumb_while_adopting_values() {
    let ctx = context();
    let mut state = RangeSlider::new()
        .range(0.0..100.0)
        .step(10.0)
        .values(20.0..70.0)
        .width(200.0)
        .create_state();
    let element = state.build(&ctx).to_element(&ctx);
    element.layout(&ctx);
    element.draw(&ctx);

    let mut dispatcher = EventDispatcher::new();
    let pointer = PointerInfo::mouse(Vec2d { x: 40.0, y: 20.0 }, PointerButton::Primary);
    assert!(dispatcher
        .dispatch(element.as_ref(), pointer.pos, &ElementEvent::PointerDown(pointer))
        .is_consumed());
    assert_eq!(state.active_thumb(), Some(RangeThumb::Lower));

    let moved = pointer.at(Vec2d { x: 100.0, y: 20.0 });
    assert!(dispatcher
        .dispatch(element.as_ref(), moved.pos, &ElementEvent::PointerMove(moved))
        .is_consumed());
    assert_eq!(state.current_values(), 50.0..70.0);

    state.adopt_config_from(
        RangeSlider::new()
            .range(0.0..100.0)
            .step(10.0)
            .values(30.0..80.0)
            .width(200.0)
            .create_state(),
    );
    assert_eq!(state.current_values(), 30.0..80.0);
    assert_eq!(state.active_thumb(), Some(RangeThumb::Lower));
    assert!(state.is_pressed());
}

#[tokio::test]
async fn disabled_slider_does_not_capture_or_propose_input() {
    let ctx = context();
    let values = Rc::new(RefCell::new(Vec::<f64>::new()));
    let observed = Rc::clone(&values);
    let element = Slider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(20.0)
        .width(200.0)
        .disabled(true)
        .on_change(move |value| observed.borrow_mut().push(value))
        .to_element(&ctx);
    element.layout(&ctx);
    element.draw(&ctx);

    let mut dispatcher = EventDispatcher::new();
    let pointer = PointerInfo::mouse(Vec2d { x: 100.0, y: 20.0 }, PointerButton::Primary);
    assert!(!dispatcher
        .dispatch(element.as_ref(), pointer.pos, &ElementEvent::PointerDown(pointer))
        .is_consumed());
    let key = ElementEvent::KeyInput {
        key: NamedKey::ArrowRight,
        action: KeyAction::Pressed,
        modifiers: Modifiers::default(),
    };
    assert!(!dispatcher
        .dispatch(element.as_ref(), pointer.pos, &key)
        .is_consumed());
    assert!(values.borrow().is_empty());
}

#[tokio::test]
async fn slider_paints_track_active_segment_and_thumb() {
    let canvas = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
    let mut ctx = BuildContext::new(
        aimer_canvas::Canvas::new(canvas),
        ResolvedSize {
            width: 320.0,
            height: 120.0,
        },
        1.0,
        Default::default(),
        Default::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    );
    ctx.box_constraint = BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 320.0,
        max_height: 120.0,
    };

    let element = Slider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(50.0)
        .width(200.0)
        .to_element(&ctx);
    element.layout(&ctx);
    element.draw(&ctx);

    let draw_list = canvas.draw_list();
    let commands = draw_list.commands();
    assert_eq!(
        commands
            .iter()
            .filter(|command| matches!(command, DrawCommand::FillRect { .. }))
            .count(),
        3
    );
    assert!(commands
        .iter()
        .any(|command| matches!(command, DrawCommand::PushClip { .. })));
}

#[tokio::test]
async fn slider_minimum_thumb_stays_inside_the_visual_bounds() {
    let canvas = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
    let mut ctx = BuildContext::new(
        aimer_canvas::Canvas::new(canvas),
        ResolvedSize {
            width: 320.0,
            height: 120.0,
        },
        1.0,
        Default::default(),
        Default::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    );
    ctx.box_constraint = BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 320.0,
        max_height: 120.0,
    };

    let element = Slider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(0.0)
        .width(200.0)
        .to_element(&ctx);
    let size = element.layout(&ctx);
    element.draw(&ctx);

    {
        let draw_list = canvas.draw_list();
        let thumb = draw_list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::FillRect { rect, .. } => Some(rect),
                _ => None,
            })
            .nth(2)
            .expect("default slider thumb should be the third rectangle");
        assert!(thumb.x >= 0.0);
        assert!(thumb.x + thumb.width <= size.width);
    }

    canvas.begin_frame();
    let element = Slider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(100.0)
        .width(200.0)
        .to_element(&ctx);
    let size = element.layout(&ctx);
    element.draw(&ctx);
    let draw_list = canvas.draw_list();
    let thumb = draw_list
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::FillRect { rect, .. } => Some(rect),
            _ => None,
        })
        .nth(2)
        .expect("default slider thumb should be the third rectangle");
    assert!(thumb.x >= 0.0);
    assert!(thumb.x + thumb.width <= size.width);
}

#[tokio::test]
async fn range_slider_endpoint_thumbs_stay_inside_the_visual_bounds() {
    let canvas = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
    let mut ctx = BuildContext::new(
        aimer_canvas::Canvas::new(canvas),
        ResolvedSize {
            width: 320.0,
            height: 120.0,
        },
        1.0,
        Default::default(),
        Default::default(),
        WindowHandle::headless(Default::default(), 1.0),
        tokio::runtime::Handle::current(),
    );
    ctx.box_constraint = BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 320.0,
        max_height: 120.0,
    };

    let element = RangeSlider::new()
        .range(0.0..100.0)
        .step(10.0)
        .values(0.0..100.0)
        .width(200.0)
        .to_element(&ctx);
    let size = element.layout(&ctx);
    element.draw(&ctx);

    let draw_list = canvas.draw_list();
    let thumbs = draw_list
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::FillRect { rect, .. } => Some(rect),
            _ => None,
        })
        .skip(2)
        .take(2)
        .collect::<Vec<_>>();
    assert_eq!(thumbs.len(), 2);
    for thumb in thumbs {
        assert!(thumb.x >= 0.0);
        assert!(thumb.x + thumb.width <= size.width);
    }
}

#[tokio::test]
async fn zero_width_slider_is_layout_safe_and_not_hit_testable() {
    let ctx = context();
    let element = Slider::new()
        .range(0.0..100.0)
        .step(10.0)
        .value(50.0)
        .width(0.0)
        .height(0.0)
        .to_element(&ctx);
    assert_eq!(element.layout(&ctx), ResolvedSize::default());

    let pointer = PointerInfo::mouse(Vec2d::default(), PointerButton::Primary);
    let mut dispatcher = EventDispatcher::new();
    assert!(!dispatcher
        .dispatch(element.as_ref(), pointer.pos, &ElementEvent::PointerDown(pointer))
        .is_consumed());
}
