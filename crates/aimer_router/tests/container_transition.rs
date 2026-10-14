//! A page growing out of the item that opened it, and shrinking back into it.

use std::time::Duration;

use aimer_cupid::draw_cmd_v2::{Rect, RenderNodeId, RenderTree};
use aimer_router::{
    Curve, ContainerTransition, NavigationAction, NavigationContext, NavigationTransition, Route,
    NavigationTransitionFrame, ResolvedNavigationTransition,
};
use aimer_widget::base::{BuildContext, ResolvedSize, WindowHandle};
use aimer_widget::{AnyElement, ErrorWidget, Key, Widget};

#[derive(Clone)]
struct Page(u32);

impl Route for Page {
    fn parse(_: &str) -> Option<Self> { None }
    fn format(&self) -> String { format!("/{}", self.0) }
}

const VIEWPORT: ResolvedSize = ResolvedSize { width: 400.0, height: 800.0 };

fn context() -> BuildContext<'static> {
    use std::sync::OnceLock;
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| tokio::runtime::Builder::new_current_thread()
        .enable_all().build().unwrap());
    let canvas = aimer_canvas::FrameCanvas::new(Box::leak(Box::new(aimer_canvas::InnerCanvas::new())));
    BuildContext::new(canvas, VIEWPORT, 1.0,
        Default::default(), Default::default(), WindowHandle::headless(Default::default(), 1.0),
        runtime.handle().clone())
}

/// Page `n` is opened from the item tagged `album-n`; page 0 has no item.
fn transition() -> ContainerTransition<Page> {
    ContainerTransition::new().tag(|page: &Page| (page.0 != 0).then(|| Key::from(format!("album-{}", page.0))))
}

/// An item on screen: its tag is registered against a node at `bounds`.
struct Item {
    element: AnyElement,
    tree: RenderTree,
    node: RenderNodeId,
}

fn item(tag: &str, bounds: Rect, ctx: &BuildContext) -> Item {
    let element = aimer_animation::SharedElement::new().tag(tag).child(ErrorWidget::new("cover")).to_element(ctx);
    registered(element, bounds, ctx)
}

/// Like [`item`], but the tile can be drawn again, so the transition can lead
/// with a copy of it.
fn repeatable_item(tag: &str, bounds: Rect, ctx: &BuildContext) -> Item {
    let element = aimer_animation::SharedElement::new().tag(tag)
        .child_with(|| ErrorWidget::new("cover")).to_element(ctx);
    registered(element, bounds, ctx)
}

fn registered(element: AnyElement, bounds: Rect, ctx: &BuildContext) -> Item {
    let tree = RenderTree::new();
    let node = tree.add_root(bounds).unwrap();
    let retained = tree.context(node).unwrap();
    assert!(ctx.with_local_v2_paint_context(retained, |ctx| element.sync_local_v2_state(ctx)));
    Item { element, tree, node }
}

fn resolve(
    transition: &ContainerTransition<Page>,
    from: u32,
    to: u32,
    action: NavigationAction,
    ctx: &BuildContext,
) -> Option<ResolvedNavigationTransition> {
    transition.resolve(&NavigationContext { from: &Page(from), to: &Page(to), action, build_context: ctx })
}

fn corner(frame: &NavigationTransitionFrame, x: f32, y: f32) -> (f32, f32) {
    frame.incoming.transform.transform_point(x, y)
}

fn near(actual: (f32, f32), expected: (f32, f32)) {
    assert!((actual.0 - expected.0).abs() < 1e-3 && (actual.1 - expected.1).abs() < 1e-3,
        "{actual:?} != {expected:?}");
}

#[test]
fn a_push_grows_the_destination_out_of_the_tagged_item() {
    let ctx = context();
    let _item = item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);

    let effect = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();

    let start = effect.sample(0.0, VIEWPORT).unwrap();
    near(corner(&start, 0.0, 0.0), (20.0, 120.0));
    near(corner(&start, VIEWPORT.width, 0.0), (180.0, 120.0));
    assert_eq!(start.incoming.corner_radius, 12.0);
    let end = effect.sample(1.0, VIEWPORT).unwrap();
    assert_eq!(end.incoming.transform, aimer_canvas::Mat3::identity());
    assert_eq!(end.incoming.corner_radius, 0.0);
    // The page behind stays underneath and dims.
    assert!(!effect.is_outgoing_on_top());
    assert_eq!(start.outgoing.opacity, 1.0);
    assert!(end.outgoing.opacity < 1.0);
}

#[test]
fn the_item_is_measured_from_the_hosts_corner_not_the_windows() {
    let ctx = context();
    let _item = item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);
    let effect = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();

    // A router beside a 10x50 sidebar: the item is that much closer to its corner.
    let start = effect.sample_at(0.0, VIEWPORT, (10.0, 50.0)).unwrap();

    near(corner(&start, 0.0, 0.0), (10.0, 70.0));
}

#[test]
fn a_pop_shrinks_the_departing_page_into_the_item_and_keeps_it_visible_on_top() {
    let ctx = context();
    let _item = item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);

    let effect = resolve(&transition(), 7, 0, NavigationAction::Pop, &ctx).unwrap();

    // Playback runs 0 -> 1; a pop begins with the page filling the viewport...
    let start = effect.sample(0.0, VIEWPORT).unwrap();
    assert_eq!(start.outgoing.transform, aimer_canvas::Mat3::identity());
    assert_eq!(start.outgoing.corner_radius, 0.0);
    // ...and ends with it back on the item, painted above the returning page.
    let end = effect.sample(1.0, VIEWPORT).unwrap();
    near(end.outgoing.transform.transform_point(0.0, 0.0), (20.0, 120.0));
    assert_eq!(end.outgoing.corner_radius, 12.0);
    assert!(effect.is_outgoing_on_top());
    assert_eq!(end.incoming.opacity, 1.0);
}

#[test]
fn a_pop_finds_the_item_after_its_page_was_dropped() {
    let ctx = context();
    drop(item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx));

    // The page holding the item is rebuilt only after the pop has resolved.
    let effect = resolve(&transition(), 7, 0, NavigationAction::Pop, &ctx).unwrap();

    near(effect.sample(1.0, VIEWPORT).unwrap().outgoing.transform.transform_point(0.0, 0.0), (20.0, 120.0));
}

#[test]
fn the_item_is_looked_up_again_every_frame() {
    let ctx = context();
    let item = item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);
    let effect = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();

    // The grid scrolls while the page is growing.
    item.tree.set_bounds(item.node, Rect::new(20.0, 60.0, 160.0, 160.0)).unwrap();

    near(corner(&effect.sample(0.0, VIEWPORT).unwrap(), 0.0, 0.0), (20.0, 60.0));
}

#[test]
fn an_item_that_disappears_mid_flight_leaves_the_last_known_position() {
    let ctx = context();
    let live = item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);
    let effect = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();
    let before = effect.sample(0.0, VIEWPORT).unwrap();

    drop(live);

    assert_eq!(effect.sample(0.0, VIEWPORT).unwrap(), before);
}

#[test]
fn without_a_tagged_item_it_falls_back_to_a_plain_zoom() {
    let ctx = context();
    for (to, case) in [(8, "tag registered nowhere"), (0, "the route has no tag")] {
        let effect = resolve(&transition(), 1, to, NavigationAction::Push, &ctx)
            .unwrap_or_else(|| panic!("{case}: should still animate"));

        let start = effect.sample(0.0, VIEWPORT).unwrap();
        assert_eq!(start.incoming.corner_radius, 0.0, "{case}");
        assert!(!effect.is_outgoing_on_top(), "{case}");
        // Zoom scales about the viewport centre, so the centre does not move.
        near(corner(&start, 200.0, 400.0), (200.0, 400.0));
    }
}

#[test]
fn the_effect_is_configurable() {
    let ctx = context();
    let _item = item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);
    let configured = transition()
        .duration(Duration::from_millis(250))
        .curve(Curve::Linear)
        .source_radius(24.0)
        .page_radius(6.0)
        .scrim(0.5);

    let effect = resolve(&configured, 0, 7, NavigationAction::Push, &ctx).unwrap();

    assert_eq!(effect.duration(), Duration::from_millis(250));
    assert_eq!(effect.curve(), Curve::Linear);
    assert_eq!(effect.sample(0.0, VIEWPORT).unwrap().incoming.corner_radius, 24.0);
    assert_eq!(effect.sample(1.0, VIEWPORT).unwrap().incoming.corner_radius, 6.0);
    assert_eq!(effect.sample(1.0, VIEWPORT).unwrap().outgoing.opacity, 0.5);
}

#[test]
fn an_untagged_transition_always_falls_back() {
    let ctx = context();
    let _item = item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);

    let effect = resolve(&ContainerTransition::new(), 0, 7, NavigationAction::Push, &ctx).unwrap();

    assert_eq!(effect.sample(0.0, VIEWPORT).unwrap().incoming.corner_radius, 0.0);
}

#[test]
fn a_zero_duration_swaps_immediately() {
    let ctx = context();
    let _item = item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);

    let effect = resolve(&transition().duration(Duration::ZERO), 0, 7, NavigationAction::Push, &ctx).unwrap();

    assert!(effect.duration().is_zero());
}

#[test]
fn a_repeatable_item_leads_the_motion_with_a_copy_of_itself() {
    let ctx = context();
    let _item = repeatable_item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);

    let effect = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();

    let anchor = effect.anchor().expect("the item can be drawn again");
    assert_eq!((anchor.size().width, anchor.size().height), (160.0, 160.0));
    let start = effect.sample_anchored_at(0.0, VIEWPORT, (0.0, 0.0)).unwrap();
    // The copy lies exactly on the item and the page is not there yet.
    near(start.anchor.transform.transform_point(0.0, 0.0), (20.0, 120.0));
    near(start.anchor.transform.transform_point(160.0, 160.0), (180.0, 280.0));
    assert_eq!(start.anchor.opacity, 1.0);
    assert_eq!(start.frame.incoming.opacity, 0.0);
    // By the end the copy is gone and the page fills the host.
    let end = effect.sample_anchored_at(1.0, VIEWPORT, (0.0, 0.0)).unwrap();
    assert_eq!(end.anchor.opacity, 0.0);
    assert_eq!(end.frame.incoming.opacity, 1.0);
}

#[test]
fn the_page_takes_over_from_the_item_a_quarter_of_the_way_in() {
    let ctx = context();
    let _item = repeatable_item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);
    let effect = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();

    let before = effect.sample_anchored_at(0.15, VIEWPORT, (0.0, 0.0)).unwrap();
    let after = effect.sample_anchored_at(0.40, VIEWPORT, (0.0, 0.0)).unwrap();

    assert_eq!((before.anchor.opacity, before.frame.incoming.opacity), (1.0, 0.0));
    assert!(after.anchor.opacity.abs() < 1e-3 && (after.frame.incoming.opacity - 1.0).abs() < 1e-3);
}

#[test]
fn a_pop_plays_the_same_thing_in_reverse_and_ends_on_the_item() {
    let ctx = context();
    let _item = repeatable_item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);

    let effect = resolve(&transition(), 7, 0, NavigationAction::Pop, &ctx).unwrap();

    // The pop starts with the page alone, filling the host...
    let start = effect.sample_anchored_at(0.0, VIEWPORT, (0.0, 0.0)).unwrap();
    assert_eq!((start.anchor.opacity, start.frame.outgoing.opacity), (0.0, 1.0));
    // ...hands over to the item's copy three quarters of the way through...
    let late = effect.sample_anchored_at(0.90, VIEWPORT, (0.0, 0.0)).unwrap();
    assert_eq!((late.anchor.opacity, late.frame.outgoing.opacity), (1.0, 0.0));
    // ...which finishes on the item itself.
    let end = effect.sample_anchored_at(1.0, VIEWPORT, (0.0, 0.0)).unwrap();
    near(end.anchor.transform.transform_point(0.0, 0.0), (20.0, 120.0));
    near(end.anchor.transform.transform_point(160.0, 160.0), (180.0, 280.0));
    assert!(effect.is_outgoing_on_top());
}

#[test]
fn a_pop_can_lead_with_the_copy_after_the_items_page_was_dropped() {
    let ctx = context();
    drop(repeatable_item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx));

    let effect = resolve(&transition(), 7, 0, NavigationAction::Pop, &ctx).unwrap();

    assert!(effect.anchor().is_some(), "the remembered item can still be drawn again");
}

#[test]
fn an_item_that_cannot_be_repeated_keeps_the_page_only_motion() {
    let ctx = context();
    let _item = item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);

    let effect = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();

    assert!(effect.anchor().is_none());
    // The page fades itself in over the first quarter.
    assert_eq!(effect.sample(0.125, VIEWPORT).unwrap().incoming.opacity, 0.5);
}

#[test]
fn the_copy_is_measured_from_the_hosts_corner_too() {
    let ctx = context();
    let _item = repeatable_item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);
    let effect = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();

    let start = effect.sample_anchored_at(0.0, VIEWPORT, (10.0, 50.0)).unwrap();

    near(start.anchor.transform.transform_point(0.0, 0.0), (10.0, 70.0));
}

#[test]
fn the_page_curve_reshapes_the_pages_growth_in_both_directions() {
    let ctx = context();
    let _item = item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);
    let plain = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();
    let eased = resolve(&transition().page_curve(Curve::EaseIn), 0, 7, NavigationAction::Push, &ctx).unwrap();

    // Halfway through, the eased page is where the plain one is at EaseIn(0.5).
    let got = eased.sample(0.5, VIEWPORT).unwrap().incoming.transform;
    let expected = plain.sample(Curve::EaseIn.transform(0.5), VIEWPORT).unwrap().incoming.transform;
    assert_eq!(got, expected);
    assert_ne!(got, plain.sample(0.5, VIEWPORT).unwrap().incoming.transform);

    // Going back runs the same curve over reversed time.
    let pop = resolve(&transition().page_curve(Curve::EaseIn), 7, 0, NavigationAction::Pop, &ctx).unwrap();
    assert_eq!(pop.sample(0.25, VIEWPORT).unwrap().outgoing.transform,
        eased.sample(0.75, VIEWPORT).unwrap().incoming.transform);
}

#[test]
fn the_anchor_curve_reshapes_the_copys_travel_and_leaves_its_fade_alone() {
    let ctx = context();
    let _item = repeatable_item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);
    let plain = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();
    let eased = resolve(&transition().anchor_curve(Curve::EaseIn), 0, 7, NavigationAction::Push, &ctx).unwrap();
    let origin = (0.0, 0.0);

    let plain_frame = plain.sample_anchored_at(0.125, VIEWPORT, origin).unwrap();
    let eased_frame = eased.sample_anchored_at(0.125, VIEWPORT, origin).unwrap();

    assert_ne!(plain_frame.anchor.transform, eased_frame.anchor.transform);
    assert_eq!(plain_frame.anchor.opacity, eased_frame.anchor.opacity);
    // It still arrives on the card at the handoff.
    let landed = eased.sample_anchored_at(0.25, VIEWPORT, origin).unwrap();
    near(landed.anchor.transform.transform_point(0.0, 0.0), landed.frame.incoming.transform.transform_point(0.0, 0.0));
}

#[test]
fn an_invalid_element_curve_is_ignored() {
    let ctx = context();
    let _item = repeatable_item("album-7", Rect::new(20.0, 120.0, 160.0, 160.0), &ctx);
    let bad = Curve::CubicBezier(f32::NAN, 0.0, 1.0, 1.0);
    let plain = resolve(&transition(), 0, 7, NavigationAction::Push, &ctx).unwrap();
    let ignored = resolve(&transition().anchor_curve(bad).page_curve(bad), 0, 7, NavigationAction::Push, &ctx).unwrap();

    for step in 0..=10 {
        let p = step as f32 / 10.0;
        assert_eq!(plain.sample_anchored_at(p, VIEWPORT, (0.0, 0.0)), ignored.sample_anchored_at(p, VIEWPORT, (0.0, 0.0)));
    }
}
