//! Behaviour tests for [`super::RichText`] and [`super::RawRichText`].

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use aimer_attribute::{Bounds, Vec2d};
use aimer_events::element::{ElementEvent, KeyAction, Modifiers, NamedKey};
use aimer_events::pointer::{PointerButton, PointerInfo, PointerSource};
use aimer_style::{TextAlign, TextOverflow, TextStyle};
use aimer_widget::base::{Color, WindowHandle};
use aimer_widget::{EventElement, PointerKey};

use super::{
    DEFAULT_SELECTION_COLOR, LinkCallback, LinkRegion, RawRichText, SelectionBinding,
};
use crate::paragraph::Paragraph;
use crate::selection::TextHitRegion;
use crate::selection::selectable::{SelectionCoordinator, TextGeometry};
use crate::selection::session::SelectionSession;
use crate::text_span::{ResolvedTextSpan, layout_resolved_spans};

/// Builds a standalone registration: its own session with a single slot,
/// which is what a `RichText` outside a selection region gets.
fn standalone_binding(
    window: &WindowHandle,
    coordinator: Rc<SelectionCoordinator>,
    text: Rc<str>,
) -> RefCell<SelectionBinding> {
    let session = SelectionSession::new(window.clone(), coordinator, DEFAULT_SELECTION_COLOR);
    let geometry = Rc::new(TextGeometry::new(window.clone()));
    let slot = session.register(text, Rc::downgrade(&geometry) as _);
    slot.stamp();
    RefCell::new(SelectionBinding {
        geometry,
        session,
        slot,
        owns_session: true,
    })
}

fn selectable_raw_text(on_link: LinkCallback) -> RawRichText {
    selectable_raw_text_with_coordinator(on_link, Rc::new(SelectionCoordinator::default()))
}

fn selectable_raw_text_with_coordinator(
    on_link: LinkCallback,
    selection_coordinator: Rc<SelectionCoordinator>,
) -> RawRichText {
    raw_text_with(
        on_link,
        selection_coordinator,
        Rc::from("élink"),
        vec![TextHitRegion::new(0..6, Bounds::new(0.0, 0.0, 20.0, 10.0))],
        Bounds::new(0.0, 0.0, 20.0, 10.0),
    )
}

/// Builds an element that behaves as if it had already painted `regions`
/// inside `bounds`, which is what pointer handling and the session's
/// geometric hit test read.
fn raw_text_with(
    on_link: LinkCallback,
    selection_coordinator: Rc<SelectionCoordinator>,
    plain_text: Rc<str>,
    regions: Vec<TextHitRegion>,
    bounds: Bounds,
) -> RawRichText {
    let window = WindowHandle::headless(winit::dpi::PhysicalSize::new(100, 100), 1.0);
    let text = RawRichText {
        paragraph: Paragraph::new(
            vec![ResolvedTextSpan::plain(
                Rc::clone(&plain_text),
                TextStyle::default(),
            )],
            TextAlign::TopLeft,
            TextOverflow::Clip,
        ),
        plain_text: Rc::clone(&plain_text),
        on_link,
        link_hover_color: Some(Color::Hex(0x388BFD)),
        selectable: true,
        selection_color: DEFAULT_SELECTION_COLOR,
        binding: standalone_binding(&window, selection_coordinator, plain_text),
        link_regions: RefCell::new(vec![LinkRegion {
            target: Rc::from("https://aimer.dev"),
            bounds: Bounds::new(0.0, 0.0, 20.0, 10.0),
        }]),
        pressed_link: RefCell::new(None),
        hovered_link: RefCell::new(None),
        hover_cursor: crate::selection::cursor::HoverCursor::new(),
        touch_hold: crate::selection::touch_hold::TouchHoldGate::new(),
    };
    let geometry = text.geometry();
    *geometry.regions.borrow_mut() = regions;
    geometry
        .bounds
        .save(1.0, bounds.x, bounds.y, bounds.width, bounds.height);
    text
}

/// The range of the element's own text that is selected.
fn selected(text: &RawRichText) -> Option<std::ops::Range<usize>> {
    text.slot().selected_range()
}

#[test]
fn rich_text_selection_is_opt_in() {
    let plain = super::RichText::new(crate::TextSpan::new("plain"));
    let selectable = super::RichText::new(crate::TextSpan::new("selectable")).selectable();

    assert!(!plain.selectable);
    assert!(selectable.selectable);
}

#[test]
fn rich_text_selection_color_is_customizable() {
    let color = Color::Rgba(255, 0, 128, 64);
    let text = super::RichText::new(crate::TextSpan::new("selectable"))
        .selectable()
        .selection_color(color);

    assert_eq!(text.selection_color, Some(color));
}

#[test]
fn explicit_overflow_override_is_independent_of_builder_order() {
    let before_style = super::RichText::new(crate::TextSpan::new("before"))
        .text_overflow(TextOverflow::Wrap)
        .text_style(TextStyle::new().font_size(20));
    let after_style = super::RichText::new(crate::TextSpan::new("after"))
        .text_style(TextStyle::new().font_size(20))
        .text_overflow(TextOverflow::Wrap);

    assert!(matches!(
        before_style.resolved_overflow(),
        TextOverflow::Wrap
    ));
    assert!(matches!(
        after_style.resolved_overflow(),
        TextOverflow::Wrap
    ));
}

// #[test]
// fn hovering_interactive_text_claims_the_cursor_event() {
//     let text = selectable_raw_text(LinkCallback::default());
//
//     assert!(text.on_event(&ElementEvent::PointerMove(
//         Vec2d { x: 1.0, y: 5.0 },
//         PointerSource::Mouse,
//         0,
//     )).is_consumed());
// }

#[test]
fn moving_into_and_out_of_a_link_updates_hover_and_requests_redraw() {
    let text = selectable_raw_text(LinkCallback::default());

    let _ = text.on_event(&ElementEvent::PointerMove(PointerInfo::mouse(
        Vec2d { x: 1.0, y: 5.0 },
        PointerButton::Primary,
    )));
    assert_eq!(
        text.hovered_link.borrow().as_deref(),
        Some("https://aimer.dev")
    );
    assert!(text.geometry().window().take_redraw_request());

    let _ = text.on_event(&ElementEvent::PointerMove(PointerInfo::mouse(
        Vec2d { x: 50.0, y: 50.0 },
        PointerButton::Primary,
    )));
    assert!(text.hovered_link.borrow().is_none());
    assert!(text.geometry().window().take_redraw_request());
}

#[test]
fn moving_within_a_link_keeps_the_link_hovered() {
    let text = selectable_raw_text(LinkCallback::default());

    for x in 1..20 {
        let _ = text.on_event(&ElementEvent::PointerMove(PointerInfo::mouse(
            Vec2d { x: x as f32, y: 5.0 },
            PointerButton::Primary,
        )));

        assert_eq!(
            text.hovered_link.borrow().as_deref(),
            Some("https://aimer.dev")
        );
    }
}

#[test]
fn select_all_shortcut_selects_the_visible_text_after_focus() {
    let text = selectable_raw_text(LinkCallback::default());
    let _ = text.on_event(&ElementEvent::PointerDown(PointerInfo::mouse(
        Vec2d { x: 1.0, y: 5.0 },
        PointerButton::Primary,
    )));

    let handled = text.on_event(&ElementEvent::KeyInput {
        key: NamedKey::Other("a".into()),
        action: KeyAction::Pressed,
        modifiers: Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
    });

    assert!(handled.is_consumed());
    assert_eq!(selected(&text), Some(0..6));
}

#[test]
fn a_press_outside_a_standalone_text_clears_its_selection() {
    let text = selectable_raw_text(LinkCallback::default());
    let _ = text.on_event(&ElementEvent::PointerDown(PointerInfo::mouse(
        Vec2d { x: 1.0, y: 5.0 },
        PointerButton::Primary,
    )));
    let _ = text.on_event(&ElementEvent::PointerUp(PointerInfo::mouse(
        Vec2d { x: 1.0, y: 5.0 },
        PointerButton::Primary,
    )));
    text.session().select_all();
    assert_eq!(selected(&text), Some(0..6));

    // Broadcast to every element is how the tree reports a press nobody took.
    let _ = text.on_event(&ElementEvent::PointerDown(PointerInfo::mouse(
        Vec2d { x: 500.0, y: 900.0 },
        PointerButton::Primary,
    )));

    assert_eq!(selected(&text), None);
    assert!(!text.session().is_focused());
}

#[test]
fn selecting_second_text_clears_first_selection_focus_and_capture() {
    let coordinator = Rc::new(SelectionCoordinator::default());
    let first =
        selectable_raw_text_with_coordinator(LinkCallback::default(), coordinator.clone());
    let second = selectable_raw_text_with_coordinator(LinkCallback::default(), coordinator);

    let _ = first.on_event(&ElementEvent::PointerDown(PointerInfo::new(
        Vec2d { x: 1.0, y: 5.0 },
        PointerSource::Mouse,
        7,
        PointerButton::Primary,
    )));
    first.session().select_all();
    assert!(first.session().is_focused());
    assert_eq!(selected(&first), Some(0..6));
    let _ = first.geometry().window().take_redraw_request();

    let second_result = second.on_event(&ElementEvent::PointerDown(PointerInfo::new(
        Vec2d { x: 1.0, y: 5.0 },
        PointerSource::Mouse,
        8,
        PointerButton::Primary,
    )));

    assert_eq!(selected(&first), None);
    assert!(!first.session().is_focused());
    assert_eq!(first.session().active_pointer(), None);
    assert!(first.geometry().window().take_redraw_request());
    assert!(second.session().is_focused());
    let second_pointer = PointerKey::new(PointerSource::Mouse, 8);
    assert_eq!(second.session().active_pointer(), Some(second_pointer));
    assert_eq!(
        second_result.capture_request(),
        aimer_widget::CaptureRequest::Capture(second_pointer)
    );
}

#[test]
fn coordinator_does_not_retain_a_dropped_session() {
    let coordinator = Rc::new(SelectionCoordinator::default());
    let session = SelectionSession::new(
        WindowHandle::headless(winit::dpi::PhysicalSize::new(100, 100), 1.0),
        coordinator.clone(),
        DEFAULT_SELECTION_COLOR,
    );
    let weak_session = Rc::downgrade(&session);
    session.claim();

    drop(session);

    assert!(weak_session.upgrade().is_none());
    assert!(coordinator.current().is_none());
}

#[test]
fn dragging_a_link_selects_text_without_activating_the_link() {
    let activations = Rc::new(Cell::new(0));
    let text = selectable_raw_text(LinkCallback::from({
        let activations = activations.clone();
        move |_| activations.set(activations.get() + 1)
    }));

    let down_result = text.on_event(&ElementEvent::PointerDown(PointerInfo::mouse(
        Vec2d { x: 1.0, y: 5.0 },
        PointerButton::Primary,
    )));
    let pointer = PointerKey::new(PointerSource::Mouse, 0);
    assert_eq!(
        down_result.capture_request(),
        aimer_widget::CaptureRequest::Capture(pointer)
    );
    assert_eq!(selected(&text), Some(0..0));
    let _ = text.on_event(&ElementEvent::PointerMove(PointerInfo::mouse(
        Vec2d { x: 19.0, y: 5.0 },
        PointerButton::Primary,
    )));
    let up_result = text.on_event(&ElementEvent::PointerUp(PointerInfo::mouse(
        Vec2d { x: 19.0, y: 5.0 },
        PointerButton::Primary,
    )));

    assert_eq!(selected(&text), Some(0..6));
    assert_eq!(
        up_result.capture_request(),
        aimer_widget::CaptureRequest::Release(pointer)
    );
    assert_eq!(activations.get(), 0);
}

#[test]
fn dragging_below_short_final_line_selects_complete_text() {
    let text = raw_text_with(
        LinkCallback::default(),
        Rc::new(SelectionCoordinator::default()),
        Rc::from("long\n}"),
        vec![
            TextHitRegion::new(0..1, Bounds::new(10.0, 20.0, 100.0, 10.0)),
            TextHitRegion::new(5..6, Bounds::new(10.0, 30.0, 10.0, 10.0)),
        ],
        Bounds::new(10.0, 20.0, 100.0, 20.0),
    );

    let _ = text.on_event(&ElementEvent::PointerDown(PointerInfo::mouse(
        Vec2d { x: 10.0, y: 25.0 },
        PointerButton::Primary,
    )));
    let _ = text.on_event(&ElementEvent::PointerMove(PointerInfo::mouse(
        Vec2d { x: 200.0, y: 50.0 },
        PointerButton::Primary,
    )));
    let _ = text.on_event(&ElementEvent::PointerUp(PointerInfo::mouse(
        Vec2d { x: 200.0, y: 50.0 },
        PointerButton::Primary,
    )));

    let selection = selected(&text).expect("the drag selects the whole text");
    assert_eq!(selection, 0..text.plain_text.len());
    assert_eq!(text.session().selected_text(), text.plain_text.as_ref());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn selection_highlight_starts_at_the_text_line_top() {
    use aimer_attribute::ResolvedSize;
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_cupid::draw_cmd::DrawCommand;
    use aimer_widget::Drawable;
    use aimer_widget::base::BuildContext;

    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let context = BuildContext::new(
        canvas,
        ResolvedSize {
            width: 200.0,
            height: 100.0,
        },
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 100), 1.0),
        runtime.handle().clone(),
    );
    let selection_color = Color::Rgba(255, 0, 128, 64);
    let text = RawRichText {
        paragraph: Paragraph::new(vec![ResolvedTextSpan::plain(
            Rc::from("selected"),
            TextStyle::new().font_size(24),
        )], TextAlign::TopLeft, TextOverflow::Wrap),
        plain_text: Rc::from("selected"),
        on_link: LinkCallback::default(),
        link_hover_color: None,
        selectable: true,
        selection_color,
        binding: standalone_binding(
            &context.window,
            Rc::new(SelectionCoordinator::default()),
            Rc::from("selected"),
        ),
        link_regions: RefCell::new(Vec::new()),
        pressed_link: RefCell::new(None),
        hovered_link: RefCell::new(None),
        hover_cursor: crate::selection::cursor::HoverCursor::new(),
        touch_hold: crate::selection::touch_hold::TouchHoldGate::new(),
    };
    text.session().select_all();
    let layout = text.paragraph.prepare(&context);
    let expected_top = layout.fragments[0].baseline - layout.fragments[0].ascent;

    text.draw(&context);

    let (selection_top, rendered_color) = inner
        .draw_list()
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::FillRect { rect, color, .. } => Some((rect.y, *color)),
            _ => None,
        })
        .unwrap();
    let expected_color: aimer_cupid::utilities::Color = selection_color.into();
    assert_eq!(selection_top, expected_top);
    assert_eq!(rendered_color.to_array(), expected_color.to_array());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn selection_highlight_connects_across_adjacent_spans() {
    use aimer_attribute::ResolvedSize;
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_cupid::draw_cmd::DrawCommand;
    use aimer_widget::Drawable;
    use aimer_widget::base::BuildContext;

    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let context = BuildContext::new(
        canvas,
        ResolvedSize {
            width: 200.0,
            height: 100.0,
        },
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 100), 1.0),
        runtime.handle().clone(),
    );
    let text = RawRichText {
        paragraph: Paragraph::new(vec![
            ResolvedTextSpan::plain(Rc::from("normal "), TextStyle::new().font_size(20)),
            ResolvedTextSpan::plain(
                Rc::from("italic"),
                TextStyle::new()
                    .font_size(20)
                    .font_style(aimer_style::FontStyle::Italic),
            ),
        ], TextAlign::TopLeft, TextOverflow::Wrap),
        plain_text: Rc::from("normal italic"),
        on_link: LinkCallback::default(),
        link_hover_color: None,
        selectable: true,
        selection_color: DEFAULT_SELECTION_COLOR,
        binding: standalone_binding(
            &context.window,
            Rc::new(SelectionCoordinator::default()),
            Rc::from("normal italic"),
        ),
        link_regions: RefCell::new(Vec::new()),
        pressed_link: RefCell::new(None),
        hovered_link: RefCell::new(None),
        hover_cursor: crate::selection::cursor::HoverCursor::new(),
        touch_hold: crate::selection::touch_hold::TouchHoldGate::new(),
    };
    text.session().select_all();

    text.draw(&context);

    let highlight_count = inner
        .draw_list()
        .commands()
        .iter()
        .filter(|command| matches!(command, DrawCommand::FillRect { .. }))
        .count();
    assert_eq!(highlight_count, 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn selection_highlights_touch_between_wrapped_lines() {
    use aimer_attribute::{BoxConstraint, ResolvedSize};
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_cupid::draw_cmd::DrawCommand;
    use aimer_widget::Drawable;
    use aimer_widget::base::BuildContext;

    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut context = BuildContext::new(
        canvas,
        ResolvedSize {
            width: 70.0,
            height: 200.0,
        },
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(winit::dpi::PhysicalSize::new(70, 200), 1.0),
        runtime.handle().clone(),
    );
    context.box_constraint = BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 70.0,
        max_height: 200.0,
    };
    let text = RawRichText {
        paragraph: Paragraph::new(vec![ResolvedTextSpan::plain(
            Rc::from("first second third"),
            TextStyle::new().font_size(24),
        )], TextAlign::TopLeft, TextOverflow::Wrap),
        plain_text: Rc::from("first second third"),
        on_link: LinkCallback::default(),
        link_hover_color: None,
        selectable: true,
        selection_color: DEFAULT_SELECTION_COLOR,
        binding: standalone_binding(
            &context.window,
            Rc::new(SelectionCoordinator::default()),
            Rc::from("first second third"),
        ),
        link_regions: RefCell::new(Vec::new()),
        pressed_link: RefCell::new(None),
        hovered_link: RefCell::new(None),
        hover_cursor: crate::selection::cursor::HoverCursor::new(),
        touch_hold: crate::selection::touch_hold::TouchHoldGate::new(),
    };
    text.session().select_all();

    text.draw(&context);

    let highlights = inner
        .draw_list()
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::FillRect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(highlights.len() > 1);
    for adjacent in highlights.windows(2) {
        assert!((adjacent[0].y + adjacent[0].height - adjacent[1].y).abs() < 0.01);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn explicit_newlines_have_stable_hit_targets_and_connected_highlights() {
    use aimer_attribute::{BoxConstraint, ResolvedSize};
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_cupid::draw_cmd::DrawCommand;
    use aimer_widget::Drawable;
    use aimer_widget::base::BuildContext;

    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut context = BuildContext::new(
        canvas,
        ResolvedSize {
            width: 200.0,
            height: 200.0,
        },
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 200), 1.0),
        runtime.handle().clone(),
    );
    context.box_constraint = BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: 200.0,
        max_height: 200.0,
    };
    let text = RawRichText {
        paragraph: Paragraph::new(vec![
            ResolvedTextSpan::plain(Rc::from("first\n"), TextStyle::new().font_size(20)),
            ResolvedTextSpan::plain(Rc::from("\n"), TextStyle::new().font_size(20)),
            ResolvedTextSpan::plain(Rc::from("third"), TextStyle::new().font_size(20)),
        ], TextAlign::TopLeft, TextOverflow::Wrap),
        plain_text: Rc::from("first\n\nthird"),
        on_link: LinkCallback::default(),
        link_hover_color: None,
        selectable: true,
        selection_color: DEFAULT_SELECTION_COLOR,
        binding: standalone_binding(
            &context.window,
            Rc::new(SelectionCoordinator::default()),
            Rc::from("first\n\nthird"),
        ),
        link_regions: RefCell::new(Vec::new()),
        pressed_link: RefCell::new(None),
        hovered_link: RefCell::new(None),
        hover_cursor: crate::selection::cursor::HoverCursor::new(),
        touch_hold: crate::selection::touch_hold::TouchHoldGate::new(),
    };
    text.session().select_all();

    let layout = text.paragraph.prepare(&context);
    assert_eq!(layout.line_breaks.len(), 2);
    assert_eq!(layout.line_breaks[0].source_range, 5..6);
    assert_eq!(layout.line_breaks[1].source_range, 6..7);
    assert_eq!(
        layout.line_breaks[0].x + layout.line_breaks[0].hit_width,
        layout.size.width
    );
    assert_eq!(layout.line_breaks[1].hit_width, layout.size.width);
    assert_eq!(layout.line_breaks[0].selection_width, 1.0);
    assert_eq!(layout.line_breaks[1].selection_width, 1.0);
    assert!(layout.line_breaks[1].height > 0.0);
    assert!(
        (layout.line_breaks[0].y + layout.line_breaks[0].height - layout.line_breaks[1].y)
            .abs()
            < 0.01
    );

    text.draw(&context);

    let geometry = text.geometry();
    let regions = geometry.regions.borrow();
    assert!(regions.iter().any(|region| region.source_range == (5..5)));
    assert!(regions.iter().any(|region| region.source_range == (6..6)));
    assert_eq!(
        crate::selection::text_offset_at(
            &regions,
            199.0,
            layout.line_breaks[0].y + layout.line_breaks[0].height / 2.0,
        ),
        Some(5),
    );
    assert_eq!(
        crate::selection::text_offset_at(
            &regions,
            199.0,
            layout.line_breaks[1].y + layout.line_breaks[1].height / 2.0,
        ),
        Some(6),
    );
    let highlights = inner
        .draw_list()
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::FillRect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(highlights.len(), 3);
    assert_eq!(highlights[0].width, layout.fragments[0].width + 1.0);
    assert_eq!(highlights[1].width, 1.0);
    for adjacent in highlights.windows(2) {
        assert!((adjacent[0].y + adjacent[0].height - adjacent[1].y).abs() < 0.01);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn italic_span_enables_synthetic_italic_for_its_draw() {
    use aimer_attribute::ResolvedSize;
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_cupid::draw_cmd::DrawCommand;
    use aimer_widget::Drawable;
    use aimer_widget::base::BuildContext;

    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let context = BuildContext::new(
        canvas,
        ResolvedSize {
            width: 200.0,
            height: 100.0,
        },
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 100), 1.0),
        runtime.handle().clone(),
    );
    let text = RawRichText {
        paragraph: Paragraph::new(vec![ResolvedTextSpan::plain(
            Rc::from("italic"),
            TextStyle::new()
                .font_size(20)
                .font_style(aimer_style::FontStyle::Italic),
        )], TextAlign::TopLeft, TextOverflow::Clip),
        plain_text: Rc::from("italic"),
        on_link: LinkCallback::default(),
        link_hover_color: None,
        selectable: false,
        selection_color: DEFAULT_SELECTION_COLOR,
        binding: standalone_binding(
            &context.window,
            Rc::new(SelectionCoordinator::default()),
            Rc::from("italic"),
        ),
        link_regions: RefCell::new(Vec::new()),
        pressed_link: RefCell::new(None),
        hovered_link: RefCell::new(None),
        hover_cursor: crate::selection::cursor::HoverCursor::new(),
        touch_hold: crate::selection::touch_hold::TouchHoldGate::new(),
    };

    text.draw(&context);

    let commands = inner.draw_list();
    let commands = commands.commands();
    let draw_index = commands
        .iter()
        .position(|command| matches!(command, DrawCommand::DrawText { .. }))
        .unwrap();
    assert!(matches!(
        commands[draw_index - 1],
        DrawCommand::SetItalic { italic: true }
    ));
    assert!(matches!(
        commands[draw_index + 1],
        DrawCommand::SetItalic { italic: false }
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn backgrounds_draw_before_text_without_changing_size_or_link_regions() {
    use std::cell::RefCell;

    use aimer_attribute::{ResolvedSize, Vec2d};
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_cupid::draw_cmd::DrawCommand;
    use aimer_style::{TextAlign, TextOverflow};
    use aimer_widget::Drawable;
    use aimer_widget::base::{BuildContext, WindowHandle};

    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let context = BuildContext::new(
        canvas,
        ResolvedSize {
            width: 200.0,
            height: 100.0,
        },
        1.0,
        Vec2d { x: 1.0, y: 5.0 },
        Vec2d::default(),
        WindowHandle::headless(winit::dpi::PhysicalSize::new(200, 100), 1.0),
        runtime.handle().clone(),
    );
    let highlighted_span = ResolvedTextSpan {
        text: Rc::from("linked"),
        style: TextStyle::new().background_color(aimer_widget::base::Color::RED),
        link: Some(Rc::from("https://aimer.dev")),
    };
    let highlighted = RawRichText {
        paragraph: Paragraph::new(vec![highlighted_span.clone()], TextAlign::TopLeft, TextOverflow::Clip),
        plain_text: Rc::from("linked"),
        on_link: LinkCallback::default(),
        link_hover_color: None,
        selectable: false,
        selection_color: DEFAULT_SELECTION_COLOR,
        binding: standalone_binding(
            &context.window,
            Rc::new(SelectionCoordinator::default()),
            Rc::from("linked"),
        ),
        link_regions: RefCell::new(Vec::new()),
        pressed_link: RefCell::new(None),
        hovered_link: RefCell::new(None),
        hover_cursor: crate::selection::cursor::HoverCursor::new(),
        touch_hold: crate::selection::touch_hold::TouchHoldGate::new(),
    };
    let plain = RawRichText {
        paragraph: Paragraph::new(vec![ResolvedTextSpan {
            style: TextStyle {
                background_color: None,
                ..highlighted_span.style
            },
            ..highlighted_span
        }], TextAlign::TopLeft, TextOverflow::Clip),
        plain_text: Rc::from("linked"),
        on_link: LinkCallback::default(),
        link_hover_color: None,
        selectable: false,
        selection_color: DEFAULT_SELECTION_COLOR,
        binding: standalone_binding(
            &context.window,
            Rc::new(SelectionCoordinator::default()),
            Rc::from("linked"),
        ),
        link_regions: RefCell::new(Vec::new()),
        pressed_link: RefCell::new(None),
        hovered_link: RefCell::new(None),
        hover_cursor: crate::selection::cursor::HoverCursor::new(),
        touch_hold: crate::selection::touch_hold::TouchHoldGate::new(),
    };

    assert_eq!(
        highlighted.paragraph.prepare(&context).size,
        plain.paragraph.prepare(&context).size
    );
    highlighted.draw(&context);
    assert_eq!(
        highlighted.hovered_link.borrow().as_deref(),
        Some("https://aimer.dev")
    );

    let commands = inner.draw_list();
    let background_index = commands
        .commands()
        .iter()
        .position(|command| matches!(command, DrawCommand::FillRect { .. }))
        .unwrap();
    let text_index = commands
        .commands()
        .iter()
        .position(|command| matches!(command, DrawCommand::DrawText { .. }))
        .unwrap();
    assert!(background_index < text_index);
    assert_eq!(highlighted.link_regions.borrow().len(), 1);
}

#[test]
fn wrapping_uses_one_cursor_across_span_boundaries() {
    let style = TextStyle::new().font_size(10);
    let spans = vec![
        ResolvedTextSpan::plain(Rc::from("abc"), style),
        ResolvedTextSpan::plain(Rc::from("def"), style),
    ];

    let layout =
        layout_resolved_spans(&spans, 20.0, |text, _| text.chars().count() as f32 * 5.0);

    assert_eq!(layout.line_count, 2);
    assert_eq!(layout.fragments[0].line, 0);
    assert_eq!(layout.fragments[1].line, 0);
    assert_eq!(layout.fragments[1].x, 15.0);
    assert_eq!(layout.fragments[2].line, 1);
    assert_eq!(layout.fragments[2].x, 0.0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn wrapping_uses_parent_width_when_constraint_is_unbounded() {
    use aimer_attribute::{BoxConstraint, ResolvedSize, Vec2d};
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_style::{TextAlign, TextOverflow};
    use aimer_widget::base::{BuildContext, WindowHandle};

    let inner = InnerCanvas::new();
    let canvas = Canvas::new(&inner);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let mut context = BuildContext::new(
        canvas,
        ResolvedSize {
            width: 20.0,
            height: 100.0,
        },
        1.0,
        Vec2d::default(),
        Vec2d::default(),
        WindowHandle::headless(winit::dpi::PhysicalSize::new(20, 100), 1.0),
        runtime.handle().clone(),
    );
    context.box_constraint = BoxConstraint {
        min_width: 0.0,
        min_height: 0.0,
        max_width: f32::MAX,
        max_height: f32::MAX,
    };
    let rich_text = RawRichText {
        paragraph: Paragraph::new(vec![ResolvedTextSpan::plain(
            Rc::from("abcdef"),
            TextStyle::new().font_size(10),
        )], TextAlign::TopLeft, TextOverflow::Wrap),
        plain_text: Rc::from("abcdef"),
        on_link: LinkCallback::default(),
        link_hover_color: None,
        selectable: false,
        selection_color: DEFAULT_SELECTION_COLOR,
        binding: standalone_binding(
            &context.window,
            Rc::new(SelectionCoordinator::default()),
            Rc::from("abcdef"),
        ),
        link_regions: RefCell::new(Vec::new()),
        pressed_link: RefCell::new(None),
        hovered_link: RefCell::new(None),
        hover_cursor: crate::selection::cursor::HoverCursor::new(),
        touch_hold: crate::selection::touch_hold::TouchHoldGate::new(),
    };

    assert_eq!(rich_text.paragraph.available_width(&context), 20.0);
    let first_layout = rich_text.paragraph.prepare(&context);
    let cached_layout = rich_text.paragraph.prepare(&context);
    assert_eq!(first_layout.size.width, 20.0);
    assert!(Rc::ptr_eq(&first_layout, &cached_layout));

    context.parent_size.width = 40.0;
    let resized_layout = rich_text.paragraph.prepare(&context);
    assert_eq!(resized_layout.size.width, 40.0);
    assert!(!Rc::ptr_eq(&first_layout, &resized_layout));
}

mod region;
