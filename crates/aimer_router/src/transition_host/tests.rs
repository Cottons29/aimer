use std::time::Duration;

use aimer_animation::curves;
use aimer_canvas::Mat3;
use aimer_widget::ErrorWidget;

use super::*;

#[test]
fn playback_starts_on_first_tick_and_releases_outgoing_content_at_completion() {
    let controller = AnimationController::new(Duration::from_millis(100), curves::LINEAR);
    controller.forward_from_first_tick();
    let effect = ResolvedNavigationTransition::new(Duration::from_millis(100), curves::LINEAR, |p, _| {
        NavigationTransitionFrame {
            incoming: crate::PaintPresentation { transform: Mat3::identity(), opacity: p, corner_radius: 0.0 },
            outgoing: crate::PaintPresentation { transform: Mat3::identity(), opacity: 1.0 - p, corner_radius: 0.0 },
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

mod presentation {
    use std::sync::OnceLock;

    use aimer_attribute::BoxConstraint;
    use aimer_cupid::draw_cmd_v2::{Rect, RenderNodeId, RenderTree};
    use aimer_widget::base::WindowHandle;

    use super::*;
    use crate::{AnchorContent, AnchoredFrame, PaintPresentation};

    fn context() -> BuildContext<'static> {
        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
        });
        let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
        let mut context = BuildContext::new(
            aimer_canvas::FrameCanvas::new(inner),
            ResolvedSize { width: 100.0, height: 100.0 },
            1.0,
            Vec2d::default(),
            Vec2d::default(),
            WindowHandle::headless(Default::default(), 1.0),
            runtime.handle().clone(),
        );
        context.box_constraint = BoxConstraint { min_width: 0.0, min_height: 0.0, max_width: 100.0, max_height: 100.0 };
        context
    }

    fn opacity(value: f32) -> PaintPresentation {
        PaintPresentation { opacity: value, ..PaintPresentation::IDENTITY_OPAQUE }
    }

    /// A host mid-transition with an outgoing page whose effect was built by `effect`.
    fn host(effect: ResolvedNavigationTransition, context: &BuildContext) -> AnyElement {
        host_with_playback(effect, context).0
    }

    /// Like [`host`], also returning the playback so a test can end it.
    fn host_with_playback(effect: ResolvedNavigationTransition, context: &BuildContext) -> (AnyElement, Rc<Playback>) {
        let controller = AnimationController::new(Duration::from_millis(100), curves::LINEAR);
        controller.forward_from_first_tick();
        let playback = Rc::new(Playback {
            controller,
            effect,
            finished: Cell::new(false),
            outgoing: RefCell::new(Some(ChildBuilder::from_widget(ErrorWidget::new("outgoing")))),
        });
        let element = NavigationFrame {
            child: ChildBuilder::from_widget(ErrorWidget::new("incoming")),
            playback: Some(Rc::clone(&playback)),
        }
        .to_element(context);
        (element, playback)
    }

    fn fixed_effect(on_top: bool) -> ResolvedNavigationTransition {
        ResolvedNavigationTransition::new(Duration::from_millis(100), curves::LINEAR, |_, _| {
            NavigationTransitionFrame { outgoing: opacity(0.25), incoming: opacity(0.75) }
        })
        .unwrap()
        .outgoing_on_top(on_top)
    }

    /// A host node with the two page nodes under it, as retained sync builds it.
    fn tree(host_bounds: Rect) -> (RenderTree, RenderNodeId, [RenderNodeId; 2]) {
        let tree = RenderTree::new();
        let root = tree.add_root(host_bounds).unwrap();
        let first = tree.add_child(root, Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
        let second = tree.add_child(root, Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
        (tree, root, [first, second])
    }

    fn sync(element: &AnyElement, tree: &RenderTree, root: RenderNodeId, context: &BuildContext) -> bool {
        let retained = tree.context(root).unwrap();
        context.with_local_v2_paint_context(retained, |context| element.sync_local_v2_state(context))
    }

    #[test]
    fn the_departing_page_is_painted_below_by_default_and_above_on_request() {
        let context = context();
        for (on_top, expected) in [(false, [1, 0]), (true, [0, 1])] {
            let element = host(fixed_effect(on_top), &context);
            let mut visited = Vec::new();

            element.visit_retained_v2_children(&mut |index, _| visited.push(index));

            // Structural slot 0 is the arriving page, 1 the departing one;
            // later in the order is painted on top.
            assert_eq!(visited, expected, "outgoing_on_top = {on_top}");
        }
    }

    #[test]
    fn each_page_gets_its_own_presentation_in_either_paint_order() {
        let context = context();
        // (outgoing_on_top, opacity of the first painted node, of the second)
        for (on_top, first, second) in [(false, 0.25, 0.75), (true, 0.75, 0.25)] {
            let element = host(fixed_effect(on_top), &context);
            let (tree, root, [a, b]) = tree(Rect::new(0.0, 0.0, 100.0, 100.0));

            assert!(sync(&element, &tree, root, &context));

            assert_eq!(tree.presentation(a).unwrap().1, first, "outgoing_on_top = {on_top}");
            assert_eq!(tree.presentation(b).unwrap().1, second, "outgoing_on_top = {on_top}");
        }
    }

    #[test]
    fn the_sampler_is_told_where_the_host_is_in_the_window() {
        let context = context();
        let seen = Rc::new(Cell::new((f32::NAN, f32::NAN)));
        let recorded = seen.clone();
        let effect = ResolvedNavigationTransition::positioned(
            Duration::from_millis(100),
            curves::LINEAR,
            move |_, _, origin| {
                recorded.set(origin);
                NavigationTransitionFrame::FINISHED
            },
        )
        .unwrap();
        let element = host(effect, &context);
        let (tree, root, _) = tree(Rect::new(30.0, 40.0, 100.0, 100.0));

        sync(&element, &tree, root, &context);

        assert_eq!(seen.get(), (30.0, 40.0));
    }

    fn anchored_effect(on_top: bool) -> ResolvedNavigationTransition {
        let content = aimer_animation::SharedContent::new(|| ErrorWidget::new("copy").boxed());
        ResolvedNavigationTransition::anchored(
            Duration::from_millis(100),
            curves::LINEAR,
            AnchorContent::new(content, ResolvedSize { width: 40.0, height: 30.0 }),
            |_, _, _| AnchoredFrame {
                frame: NavigationTransitionFrame { outgoing: opacity(0.25), incoming: opacity(0.75) },
                anchor: opacity(0.5),
            },
        )
        .unwrap()
        .outgoing_on_top(on_top)
    }

    #[test]
    fn the_anchor_is_painted_above_both_pages_either_way() {
        let context = context();
        for (on_top, expected) in [(false, [1, 0, 2]), (true, [0, 1, 2])] {
            let element = host(anchored_effect(on_top), &context);
            let mut visited = Vec::new();

            element.visit_retained_v2_children(&mut |index, _| visited.push(index));

            // Structural slot 2 is the anchor copy, always painted last.
            assert_eq!(visited, expected, "outgoing_on_top = {on_top}");
        }
    }

    #[test]
    fn the_anchor_gets_its_own_presentation() {
        let context = context();
        for (on_top, expected) in [(false, [0.25, 0.75, 0.5]), (true, [0.75, 0.25, 0.5])] {
            let element = host(anchored_effect(on_top), &context);
            let tree = RenderTree::new();
            let root = tree.add_root(Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap();
            let nodes: Vec<_> = (0..3)
                .map(|_| tree.add_child(root, Rect::new(0.0, 0.0, 100.0, 100.0)).unwrap())
                .collect();

            assert!(sync(&element, &tree, root, &context));

            let opacities: Vec<_> = nodes.iter().map(|node| tree.presentation(*node).unwrap().1).collect();
            assert_eq!(opacities, expected, "outgoing_on_top = {on_top}");
        }
    }

    #[test]
    fn the_anchor_takes_no_input_and_is_dropped_when_the_transition_ends() {
        let context = context();
        let (element, playback) = host_with_playback(anchored_effect(false), &context);
        let mut events = 0;
        element.event_children(&mut |_| events += 1);
        assert_eq!(events, 1, "only the arriving page takes input");
        let mut focusable = 0;
        element.focus_children(&mut |_| focusable += 1);
        assert_eq!(focusable, 1, "and only the arriving page takes focus");
        let mut structural = 0;
        element.structural_children(&mut |_| structural += 1);
        assert_eq!(structural, 3, "all three still take part in reconciliation");
        let mut visited = 0;
        element.visit_retained_v2_children(&mut |_, _| visited += 1);
        assert_eq!(visited, 3);

        // The transition ends: the next frame removes both the page that left and the copy.
        playback.finish();
        element.update(&context);

        let mut remaining = Vec::new();
        element.visit_retained_v2_children(&mut |index, _| remaining.push(index));
        assert_eq!(remaining, [0]);
    }
}
