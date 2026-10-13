use std::time::Duration;

use aimer_animation::Curve;
use aimer_canvas::Mat3;
use aimer_widget::ErrorWidget;

use super::*;

#[test]
fn playback_starts_on_first_tick_and_releases_outgoing_content_at_completion() {
    let controller = AnimationController::new(Duration::from_millis(100), Curve::Linear);
    controller.forward_from_first_tick();
    let effect = ResolvedNavigationTransition::new(Duration::from_millis(100), Curve::Linear, |p, _| {
        NavigationTransitionFrame {
            incoming: crate::PaintPresentation { transform: Mat3::identity(), opacity: p },
            outgoing: crate::PaintPresentation { transform: Mat3::identity(), opacity: 1.0 - p },
        }
    }).unwrap();
    let playback = Playback { controller, effect, finished: Cell::new(false),
        outgoing: RefCell::new(Some(ChildBuilder::from_widget(ErrorWidget::new("outgoing")))) };
    let size = ResolvedSize { width: 100.0, height: 100.0 };
    let now = AnimInstant::now();
    assert_eq!(playback.sample(now, size).incoming.opacity, 0.0);
    assert_eq!(playback.sample(now + Duration::from_millis(50), size).incoming.opacity, 0.5);
    assert!(!playback.finished.get());
    assert_eq!(playback.sample(now + Duration::from_millis(100), size), NavigationTransitionFrame::FINISHED);
    assert!(playback.finished.get());
    assert!(!playback.controller.is_animating());
    assert!(playback.outgoing.borrow().is_none());
}
