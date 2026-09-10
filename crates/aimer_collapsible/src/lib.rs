#![deny(missing_docs)]

//! Stateful, reusable disclosure-list widgets for Aimer applications.
//!
//! [`CollapsibleList`] keeps its expanded state in the framework's ordinary
//! [`aimer_widget::StatefulWidget`] lifecycle. [`ListHeader`] and [`ListBody`]
//! are semantic child slots, so callers can use the same list behavior with
//! any header and body widgets they already have.

mod key_relay;
mod widgets;

pub use widgets::{CollapsibleList, CollapsibleListState, ListBody, ListHeader};

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use super::widgets::collapsed_height;
    use super::{CollapsibleList, ListBody, ListHeader};
    use aimer_animation::Curve;
    use aimer_canvas::{Canvas, InnerCanvas};
    use aimer_text::Text;
    use aimer_widget::base::{BuildContext, ResolvedSize, Vec2d, WindowHandle};
    use aimer_widget::{AnyElement, PortableWidget, Rebuildable, State, StatefulWidget, Widget};

    fn context() -> BuildContext<'static> {
        let inner = Box::leak(Box::new(InnerCanvas::new()));
        BuildContext::new(
            Canvas::new(inner),
            ResolvedSize::default(),
            1.0,
            Vec2d::ZERO,
            Vec2d::ZERO,
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        )
    }

    struct Probe {
        builds: Rc<Cell<usize>>,
    }

    impl PortableWidget for Probe {}

    impl Widget for Probe {
        fn to_element(self, ctx: &BuildContext) -> AnyElement {
            self.builds.set(self.builds.get() + 1);
            aimer_widget::ErrorWidget::new("probe").to_element(ctx)
        }

        fn debug_name(&self) -> &'static str {
            "CollapsibleListProbe"
        }
    }

    #[test]
    fn expanded_is_initial_state_and_live_state_survives_configuration_adoption() {
        let widget = CollapsibleList::new()
            .expanded(false)
            .header(ListHeader::new().child(Text::new("Tags")))
            .body(ListBody::new().child(Text::new("Red")));
        let mut state = widget.create_state();

        assert!(!state.is_expanded());
        assert_eq!(state.chevron_progress(), 1.0);
        assert_eq!(state.body_progress(), 0.0);

        state.toggle();
        assert!(state.is_expanded());
        assert_eq!(state.chevron_progress(), 1.0);
        assert_eq!(state.body_progress(), 0.0);

        let replacement = CollapsibleList::new()
            .expanded(false)
            .animation_duration(Duration::from_millis(900))
            .animation_curve(Curve::Linear)
            .header(ListHeader::new().child(Text::new("Updated tags")))
            .body(ListBody::new().child(Text::new("Updated red")));
        state.adopt_config_from(replacement.create_state());

        assert!(state.is_expanded());
        assert_eq!(state.chevron_duration(), Duration::from_millis(900));
        assert_eq!(state.body_duration(), Duration::from_millis(900));
        assert_eq!(state.chevron_curve(), Curve::Linear);
        assert_eq!(state.body_curve(), Curve::Linear);
    }

    #[test]
    fn custom_animation_timing_is_applied_to_both_transitions() {
        let widget = CollapsibleList::new()
            .animation_millis(420)
            .animation_curve(Curve::EaseOut)
            .header(ListHeader::new().child(Text::new("Tags")))
            .body(ListBody::new().child(Text::new("Red")));
        let state = widget.create_state();

        assert_eq!(state.chevron_duration(), Duration::from_millis(420));
        assert_eq!(state.body_duration(), Duration::from_millis(420));
        assert_eq!(state.chevron_curve(), Curve::EaseOut);
        assert_eq!(state.body_curve(), Curve::EaseOut);
    }

    #[test]
    fn body_height_follows_animation_progress() {
        assert_eq!(collapsed_height(120.0, 0.0), 0.0);
        assert_eq!(collapsed_height(120.0, 0.5), 60.0);
        assert_eq!(collapsed_height(120.0, 1.0), 120.0);
    }

    #[test]
    fn expanded_lists_start_full_and_reverse_from_the_full_height() {
        let widget = CollapsibleList::new()
            .header(ListHeader::new().child(Text::new("Tags")))
            .body(ListBody::new().child(Text::new("Red")));
        let mut state = widget.create_state();

        assert!(state.is_expanded());
        assert_eq!(state.body_progress(), 1.0);

        state.toggle();

        assert!(!state.is_expanded());
        assert_eq!(state.body_progress(), 1.0);
    }

    #[tokio::test]
    async fn collapsed_body_is_not_built_until_expanded_and_is_then_retained() {
        let builds = Rc::new(Cell::new(0));
        let widget = CollapsibleList::new()
            .expanded(false)
            .header(ListHeader::new().child(Text::new("Tags")))
            .body(ListBody::new().child(Probe {
                builds: builds.clone(),
            }));
        let mut state = widget.create_state();
        let ctx = context();

        let _collapsed = state.build(&ctx).to_element(&ctx);
        assert_eq!(builds.get(), 0);

        state.toggle();
        let _first_expanded = state.build(&ctx).to_element(&ctx);
        let _second_expanded = state.build(&ctx).to_element(&ctx);

        assert_eq!(builds.get(), 1);
    }

    #[tokio::test]
    async fn collapsed_layout_contains_only_the_always_visible_header() {
        let widget = CollapsibleList::new()
            .expanded(false)
            .header(ListHeader::new().child(Text::new("Tags")))
            .body(ListBody::new().child(Text::new("Red")));
        let state = widget.create_state();
        let ctx = context();
        let element = state.build(&ctx).to_element(&ctx);
        let mut child_count = 0;

        element.visit_children(&mut |_| child_count += 1);

        assert_eq!(child_count, 1);
    }

    #[tokio::test]
    async fn mounted_state_updates_from_a_focused_activation_key() {
        use aimer_events::element::{ElementEvent, KeyAction, Modifiers, NamedKey};
        use aimer_widget::{dispatch_focused_event, StatefulElement};

        let widget = CollapsibleList::new()
            .expanded(false)
            .header(ListHeader::new().child(Text::new("Tags")))
            .body(ListBody::new().child(Text::new("Red")));
        let ctx = context();
        let (element, updater) = StatefulElement::new_with_name(widget, &ctx, "TestList", None);
        let root = element.boxed();

        let _ = dispatch_focused_event(
            root.as_ref(),
            &ElementEvent::KeyInput {
                key: NamedKey::Enter,
                action: KeyAction::Pressed,
                modifiers: Modifiers::default(),
            },
        );
        root.rebuild_if_dirty(&ctx);

        assert_eq!(updater.try_read(|state| state.is_expanded()), Some(true));
    }

    #[tokio::test]
    async fn mounted_state_updates_from_a_primary_pointer_tap() {
        use aimer_events::pointer::{PointerButton, PointerInfo};
        use aimer_widget::{broadcast_event, Drawable, LayoutElement, StatefulElement};

        let widget = CollapsibleList::new()
            .expanded(false)
            .header(ListHeader::new().child(Text::new("Tags")))
            .body(ListBody::new().child(Text::new("Red")));
        let mut ctx = context();
        ctx.parent_size = ResolvedSize {
            width: 300.0,
            height: 300.0,
        };
        ctx.box_constraint.max_width = 300.0;
        ctx.box_constraint.max_height = 300.0;
        let (element, updater) = StatefulElement::new_with_name(widget, &ctx, "TestList", None);
        let root = element.boxed();
        root.layout(&ctx);
        ctx.canvas.begin_frame();
        root.draw(&ctx);

        let pointer = PointerInfo::mouse(Vec2d { x: 16.0, y: 16.0 }, PointerButton::Primary);
        let _ = broadcast_event(
            root.as_ref(),
            &aimer_events::element::ElementEvent::PointerDown(pointer),
        );
        let _ = broadcast_event(
            root.as_ref(),
            &aimer_events::element::ElementEvent::PointerUp(pointer),
        );
        root.rebuild_if_dirty(&ctx);

        assert_eq!(updater.try_read(|state| state.is_expanded()), Some(true));
    }

}
