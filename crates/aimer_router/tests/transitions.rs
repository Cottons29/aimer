use std::time::Duration;

use aimer_router::{CrossFadeTransition, NavigationAction, NavigationContext, NavigationTransition, Route};
use aimer_widget::base::{BuildContext, ResolvedSize, WindowHandle};

#[derive(Clone)]
struct Page(u32);

impl Route for Page {
    fn parse(_: &str) -> Option<Self> { None }
    fn format(&self) -> String { format!("/{}", self.0) }
    fn view_level(&self) -> u32 { self.0 }
}

fn context() -> BuildContext<'static> {
    use std::sync::OnceLock;
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| tokio::runtime::Builder::new_current_thread()
        .enable_all().build().unwrap());
    let canvas = aimer_canvas::FrameCanvas::new(Box::leak(Box::new(aimer_canvas::InnerCanvas::new())));
    BuildContext::new(canvas, ResolvedSize { width: 200.0, height: 100.0 }, 1.0,
        Default::default(), Default::default(), WindowHandle::headless(Default::default(), 1.0),
        runtime.handle().clone())
}

#[test]
fn cross_fade_shows_both_pages_halfway_and_finishes_at_destination() {
    let ctx = context();
    let navigation = NavigationContext {
        from: &Page(0), to: &Page(1), action: NavigationAction::Push, build_context: &ctx,
    };
    let effect = CrossFadeTransition::new().duration(Duration::from_millis(100))
        .resolve(&navigation).unwrap();
    let frame = effect.sample(0.5, ctx.parent_size).unwrap();
    assert_eq!(frame.incoming.opacity, 0.5);
    assert_eq!(frame.outgoing.opacity, 0.5);
    assert_eq!(frame.incoming.transform, aimer_canvas::Mat3::identity());
    let frame = effect.sample(1.0, ctx.parent_size).unwrap();
    assert_eq!(frame.incoming.opacity, 1.0);
    assert_eq!(frame.outgoing.opacity, 0.0);
}

#[test]
fn zoom_is_centered_and_pop_reverses_the_departing_effect() {
    use aimer_router::ZoomTransition;
    let ctx = context();
    let mut navigation = NavigationContext {
        from: &Page(0), to: &Page(1), action: NavigationAction::Push, build_context: &ctx,
    };
    let forward = ZoomTransition::new().resolve(&navigation).unwrap();
    let start = forward.sample(0.0, ctx.parent_size).unwrap();
    assert!((start.incoming.transform.cols[0][0] - 0.92).abs() < 0.00001);
    let center = start.incoming.transform.transform_point(100.0, 50.0);
    assert!((center.0 - 100.0).abs() < 0.00001);
    assert!((center.1 - 50.0).abs() < 0.00001);
    navigation.action = NavigationAction::Pop;
    let backward = ZoomTransition::new().resolve(&navigation).unwrap();
    assert_eq!(backward.sample(0.0, ctx.parent_size).unwrap().outgoing,
        forward.sample(1.0, ctx.parent_size).unwrap().incoming);
    assert_eq!(backward.sample(1.0, ctx.parent_size).unwrap().outgoing,
        forward.sample(0.0, ctx.parent_size).unwrap().incoming);
}

#[test]
fn view_levels_use_direction_and_current_viewport_width() {
    use aimer_router::ViewLevelTransitions;
    let ctx = context();
    for (from, to, offset) in [(0, 2, 200.0), (2, 0, -200.0), (2, 2, 0.0)] {
        let navigation = NavigationContext {
            from: &Page(from), to: &Page(to), action: NavigationAction::Pop, build_context: &ctx,
        };
        let effect = ViewLevelTransitions::new().resolve(&navigation).unwrap();
        let frame = effect.sample(0.0, ctx.parent_size).unwrap();
        assert_eq!(frame.incoming.transform.cols[2][0], offset);
        let resized = effect.sample(0.0, ResolvedSize { width: 400.0, height: 100.0 }).unwrap();
        assert_eq!(resized.incoming.transform.cols[2][0], offset * 2.0);
        if from == to { assert_eq!(frame.incoming.opacity, 0.0); }
    }
}

#[test]
fn dynamic_selector_can_inspect_routes_and_disable_navigation() {
    use aimer_router::DynamicTransitions;
    let ctx = context();
    let transition = DynamicTransitions::new(|navigation: &NavigationContext<'_, '_, Page>| {
        assert_eq!(navigation.from.0, 0);
        if navigation.to.0 == 2 { None } else { CrossFadeTransition::new().resolve(navigation) }
    });
    for (to, animated) in [(1, true), (2, false)] {
        let navigation = NavigationContext {
            from: &Page(0), to: &Page(to), action: NavigationAction::Push, build_context: &ctx,
        };
        assert_eq!(transition.resolve(&navigation).is_some(), animated);
    }
}

#[test]
fn custom_effects_reject_non_finite_values_and_clamp_opacity() {
    use aimer_router::{Curve, NavigationTransitionFrame, PaintPresentation, ResolvedNavigationTransition};
    let effect = ResolvedNavigationTransition::new(Duration::ZERO, Curve::Linear, |p, _| {
        NavigationTransitionFrame {
            incoming: PaintPresentation { transform: aimer_canvas::Mat3::identity(), opacity: p * 4.0 },
            outgoing: PaintPresentation { transform: aimer_canvas::Mat3::identity(), opacity: -1.0 },
        }
    }).unwrap();
    let size = ResolvedSize { width: 200.0, height: 100.0 };
    assert_eq!(effect.duration(), Duration::ZERO);
    let frame = effect.sample(0.5, size).unwrap();
    assert_eq!(frame.incoming.opacity, 1.0);
    assert_eq!(frame.outgoing.opacity, 0.0);
    assert!(effect.sample(f32::NAN, size).is_none());
    assert!(effect.sample(0.0, ResolvedSize { width: f32::INFINITY, height: 1.0 }).is_none());
    assert!(ResolvedNavigationTransition::new(Duration::ZERO,
        Curve::CubicBezier(f32::NAN, 0.0, 1.0, 1.0), move |_, _| frame).is_none());
    let invalid = ResolvedNavigationTransition::new(Duration::ZERO, Curve::Linear, |_, _| {
        NavigationTransitionFrame {
            incoming: PaintPresentation { transform: aimer_canvas::Mat3::translate(f32::NAN, 0.0), opacity: 1.0 },
            outgoing: PaintPresentation::IDENTITY_OPAQUE,
        }
    }).unwrap();
    assert!(invalid.sample(0.0, size).is_none());
}
