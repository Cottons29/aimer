use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use aimer_router::*;
use aimer_widget::base::{BuildContext, ResolvedSize, WindowHandle};
use aimer_widget::{AnyElement, AnyWidget, Drawable, Element, EventElement, LayoutElement,
    PortableWidget, Rebuildable, VisitorElement, Widget};

thread_local! {
    static BUILDS: RefCell<Vec<Page>> = const { RefCell::new(Vec::new()) };
    static DROPS: RefCell<Vec<Page>> = const { RefCell::new(Vec::new()) };
    static FRAMES: Cell<usize> = const { Cell::new(0) };
    static SHELL_ACTIONS: RefCell<Vec<NavigationAction>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Page { Home, Detail, Disabled, Redirect, Explicit }

impl Route for Page {
    fn parse(_: &str) -> Option<Self> { None }
    fn format(&self) -> String { format!("/{self:?}") }
    fn navigation_transition(&self) -> TransitionChoice<Self> {
        match self {
            Self::Disabled => TransitionChoice::Disabled,
            Self::Explicit => TransitionChoice::custom(CrossFadeTransition::new().duration(Duration::ZERO)),
            _ => TransitionChoice::Inherit,
        }
    }
    fn redirect(&self, _: &BuildContext) -> Option<Self> {
        (*self == Self::Redirect).then_some(Self::Detail)
    }
}

impl Router for Page {
    fn build(&self, _: &BuildContext) -> AnyWidget { Widget::boxed(Leaf(*self)) }
}

struct Leaf(Page);
impl PortableWidget for Leaf {}
impl Widget for Leaf {
    fn to_element(self, _: &BuildContext) -> AnyElement {
        BUILDS.with(|log| log.borrow_mut().push(self.0));
        self.boxed_element()
    }
}
impl Leaf { fn boxed_element(self) -> AnyElement { Element::boxed(self) } }
impl Drop for Leaf { fn drop(&mut self) { DROPS.with(|log| log.borrow_mut().push(self.0)); } }
impl VisitorElement for Leaf {
    fn debug_name(&self) -> &'static str {
        match self.0 { Page::Home => "HomePage", Page::Detail => "DetailPage", Page::Disabled => "DisabledPage", Page::Redirect => "RedirectPage", Page::Explicit => "ExplicitPage" }
    }
}
impl Drawable for Leaf { fn can_paint_local_v2(&self, _: &BuildContext) -> bool { true } }
impl LayoutElement for Leaf { fn flex(&self) -> Option<f32> { Some(2.0) } }
impl Rebuildable for Leaf {}
impl EventElement for Leaf {}

fn routes(page: Page) -> AnyWidget { Widget::boxed(Leaf(page)) }

fn context() -> BuildContext<'static> {
    use std::sync::OnceLock;
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap());
    let canvas = aimer_canvas::FrameCanvas::new(Box::leak(Box::new(aimer_canvas::InnerCanvas::new())));
    BuildContext::new(canvas, ResolvedSize { width: 200.0, height: 100.0 }, 1.0,
        Default::default(), Default::default(), WindowHandle::headless(Default::default(), 1.0), runtime.handle().clone())
}

fn pages(element: &dyn Element, interactive: bool) -> Vec<&'static str> {
    let mut result = Vec::new();
    fn walk(element: &dyn Element, interactive: bool, result: &mut Vec<&'static str>) {
        if element.debug_name().ends_with("Page") { result.push(element.debug_name()); }
        let mut visit = |child: &dyn Element| walk(child, interactive, result);
        if interactive { element.event_children(&mut visit); }
        else { element.visit_children(&mut visit); }
    }
    walk(element, interactive, &mut result);
    result.sort();
    result
}

#[test]
fn navigation_retains_two_pages_without_rebuilding_them_on_animation_frames() {
    let ctx = context();
    let selected = Rc::new(RefCell::new(Vec::new()));
    let log = selected.clone();
    let finish = Rc::new(Cell::new(false));
    let finish_sampler = finish.clone();
    let transition = DynamicTransitions::new(move |navigation: &NavigationContext<'_, '_, Page>| {
        log.borrow_mut().push((*navigation.from, *navigation.to, navigation.action));
        let finish = finish_sampler.clone();
        ResolvedNavigationTransition::new(Duration::from_secs(60), Curve::Linear, move |p, _| {
            NavigationTransitionFrame {
                incoming: PaintPresentation { transform: aimer_canvas::Mat3::identity(), opacity: if finish.get() { f32::NAN } else { p } },
                outgoing: PaintPresentation { transform: aimer_canvas::Mat3::identity(), opacity: 1.0 - p },
            }
        })
    });
    let navigator = Navigator::new(Page::Home, routes).transition(transition).to_element(&ctx);
    let controller = NavigatorController::<Page>::of(&ctx);
    assert!(selected.borrow().is_empty(), "initial mounting must not animate");
    controller.push(Page::Redirect);
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(*selected.borrow(), [(Page::Home, Page::Detail, NavigationAction::Push)]);
    assert_eq!(pages(navigator.as_ref(), false), ["DetailPage", "HomePage"]);
    assert_eq!(pages(navigator.as_ref(), true), ["DetailPage"]);
    for _ in 0..8 { navigator.update(&ctx); }
    BUILDS.with(|log| assert_eq!(*log.borrow(), [Page::Home, Page::Detail]));
    finish.set(true);
    navigator.update(&ctx);
    assert_eq!(pages(navigator.as_ref(), false), ["DetailPage"]);
    assert_eq!(find_host(navigator.as_ref()).unwrap().retained_clip(&ctx), None,
        "the transition must stop clipping ordinary page overflow when it finishes");
    DROPS.with(|log| assert!(log.borrow().contains(&Page::Home)));
}

#[test]
fn rapid_navigation_is_bounded_and_disabled_routes_override_fallback() {
    let ctx = context();
    let navigator = Navigator::new(Page::Home, routes)
        .transition(CrossFadeTransition::new().duration(Duration::from_secs(60))).to_element(&ctx);
    let controller = NavigatorController::<Page>::of(&ctx);
    controller.push(Page::Detail);
    navigator.rebuild_if_dirty(&ctx);
    controller.push(Page::Home);
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(pages(navigator.as_ref(), false), ["DetailPage", "HomePage"]);
    controller.push(Page::Disabled);
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(pages(navigator.as_ref(), false), ["DisabledPage"]);
    controller.pop();
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(pages(navigator.as_ref(), false), ["HomePage"], "the departing route disables pop animation");
}

#[test]
fn zero_duration_and_duplicate_urls_do_not_start_animations() {
    let ctx = context();
    let selected = Rc::new(Cell::new(0));
    let count = selected.clone();
    let navigator = Navigator::new(Page::Home, routes).transition(DynamicTransitions::new(
        move |n: &NavigationContext<'_, '_, Page>| {
            count.set(count.get() + 1);
            CrossFadeTransition::new().duration(Duration::ZERO).resolve(n)
        })).to_element(&ctx);
    let controller = NavigatorController::<Page>::of(&ctx);
    controller.push(Page::Home);
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(selected.get(), 0);
    assert_eq!(controller.history_len(), 2);
    controller.set_route(Page::Detail);
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(pages(navigator.as_ref(), false), ["DetailPage"]);
    assert_eq!(selected.get(), 1);
    assert_eq!(controller.history_len(), 1);
}

struct Frame;
impl PortableWidget for Frame {}
impl Widget for Frame {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        FRAMES.set(FRAMES.get() + 1);
        Outlet.to_element(ctx)
    }
}

#[derive(Clone)]
struct Dashboard(Page);
impl Route for Dashboard {
    fn parse(_: &str) -> Option<Self> { None }
    fn format(&self) -> String { format!("/dashboard{}", self.0.format()) }
    fn shell_key(&self) -> Option<&'static str> { Some("dashboard") }
}
impl Router for Dashboard {
    fn build(&self, _: &BuildContext) -> AnyWidget { dashboard(self.clone()) }
}
fn dashboard(route: Dashboard) -> AnyWidget {
    let child = route.0;
    Shell::new(Frame, move |_| routes(child)).route(child)
        .transition(DynamicTransitions::new(|n: &NavigationContext<'_, '_, Page>| {
            SHELL_ACTIONS.with(|log| log.borrow_mut().push(n.action));
            CrossFadeTransition::new().duration(Duration::from_secs(60)).resolve(n)
        })).boxed()
}

#[test]
fn shell_child_transitions_keep_the_frame_mounted_and_reverse_on_pop() {
    let ctx = context();
    let navigator = Navigator::new(Dashboard(Page::Home), dashboard)
        .transition(ZoomTransition::new()).to_element(&ctx);
    let controller = NavigatorController::<Dashboard>::of(&ctx);
    controller.push(Dashboard(Page::Detail));
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(FRAMES.get(), 1, "the shell frame must remain mounted");
    assert_eq!(pages(navigator.as_ref(), false), ["DetailPage", "HomePage"]);
    assert_eq!(pages(navigator.as_ref(), true), ["DetailPage"]);
    SHELL_ACTIONS.with(|log| assert_eq!(*log.borrow(), [NavigationAction::Push]));
    controller.pop();
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(FRAMES.get(), 1);
    SHELL_ACTIONS.with(|log| assert_eq!(*log.borrow(), [NavigationAction::Push, NavigationAction::Pop]));
    drop(navigator);
    BUILDS.with(|builds| DROPS.with(|drops| assert_eq!(builds.borrow().len(), drops.borrow().len(), "retained shell pages must be released")));
}

fn frame(_: &BuildContext) -> AnyWidget { Frame.boxed() }

#[test]
fn stateful_shell_animates_only_visible_changes_and_keeps_the_frame_mounted() {
    let ctx = context();
    let shell = StatefulShell::new(vec![Page::Home, Page::Detail], frame, routes)
        .transition(CrossFadeTransition::new().duration(Duration::from_secs(60))).to_element(&ctx);
    let controller = StatefulShellController::<Page>::of(&ctx);
    controller.push_in_branch(1, Page::Disabled);
    shell.rebuild_if_dirty(&ctx);
    assert_eq!(pages(shell.as_ref(), false), ["HomePage"]);
    controller.go_branch(1);
    shell.rebuild_if_dirty(&ctx);
    assert_eq!(pages(shell.as_ref(), false), ["DisabledPage"]);
    controller.go_branch(0);
    shell.rebuild_if_dirty(&ctx);
    assert_eq!(pages(shell.as_ref(), false), ["DisabledPage", "HomePage"]);
    assert_eq!(FRAMES.get(), 1);
    controller.go_branch(9);
    shell.rebuild_if_dirty(&ctx);
    assert_eq!(controller.active_branch(), 0);
}

thread_local! {
    static DEPARTING_FOCUS: RefCell<Option<aimer_widget::focus::FocusNode>> = const { RefCell::new(None) };
    static FOCUS_LOST: Cell<usize> = const { Cell::new(0) };
}
struct FocusedPage(aimer_widget::focus::FocusNode);
impl VisitorElement for FocusedPage { fn debug_name(&self) -> &'static str { "FocusedPage" } }
impl Drawable for FocusedPage {}
impl LayoutElement for FocusedPage {}
impl Rebuildable for FocusedPage {}
impl EventElement for FocusedPage {
    fn focus_node(&self) -> Option<&aimer_widget::focus::FocusNode> { Some(&self.0) }
    fn on_event(&self, event: &aimer_events::element::ElementEvent) -> aimer_widget::EventResult {
        if matches!(event, aimer_events::element::ElementEvent::FocusLost) { FOCUS_LOST.set(FOCUS_LOST.get() + 1); }
        aimer_widget::EventResult::ignored()
    }
}
struct FocusedWidget(Page);
impl PortableWidget for FocusedWidget {}
impl Widget for FocusedWidget {
    fn to_element(self, _: &BuildContext) -> AnyElement {
        let focus = aimer_widget::focus::FocusNode::new();
        if self.0 == Page::Home {
            focus.set_focused(true);
            DEPARTING_FOCUS.with(|slot| *slot.borrow_mut() = Some(focus.clone()));
        }
        FocusedPage(focus).boxed()
    }
}
fn focused_routes(page: Page) -> AnyWidget { FocusedWidget(page).boxed() }

#[test]
fn outgoing_pages_lose_keyboard_focus_when_the_transition_starts() {
    let ctx = context();
    let navigator = Navigator::new(Page::Home, focused_routes)
        .transition(CrossFadeTransition::new().duration(Duration::from_secs(60))).to_element(&ctx);
    NavigatorController::<Page>::of(&ctx).push(Page::Detail);
    navigator.rebuild_if_dirty(&ctx);
    DEPARTING_FOCUS.with(|slot| assert!(!slot.borrow().as_ref().unwrap().has_focus()));
    assert_eq!(FOCUS_LOST.get(), 1);
}

fn find_host(element: &dyn Element) -> Option<&dyn Element> {
    if element.debug_name() == "NavigationTransitionHost" { return Some(element); }
    let mut found = None;
    element.visit_children(&mut |child| { if found.is_none() { found = find_host(child); } });
    found
}

#[test]
fn custom_presentations_reach_retained_nodes_without_rerecording_page_lists() {
    use aimer_cupid::draw_cmd_v2::{Rect, RenderTree};
    let ctx = context();
    let navigator = Navigator::new(Page::Home, routes).transition(DynamicTransitions::new(
        |_: &NavigationContext<'_, '_, Page>| ResolvedNavigationTransition::new(
            Duration::from_secs(60), Curve::Linear, |_, _| NavigationTransitionFrame {
                incoming: PaintPresentation { transform: aimer_canvas::Mat3::translate(25.0, 0.0), opacity: 0.25 },
                outgoing: PaintPresentation { transform: aimer_canvas::Mat3::identity(), opacity: 0.75 },
            }))).to_element(&ctx);
    NavigatorController::<Page>::of(&ctx).push(Page::Detail);
    navigator.rebuild_if_dirty(&ctx);
    let host = find_host(navigator.as_ref()).unwrap();
    assert_eq!(host.retained_clip(&ctx), Some(Rect::new(0.0, 0.0, 200.0, 100.0)));
    let tree = RenderTree::new();
    let root = tree.add_root(Rect::new(0.0, 0.0, 200.0, 100.0)).unwrap();
    let outgoing = tree.add_child(root, Rect::new(0.0, 0.0, 200.0, 100.0)).unwrap();
    let incoming = tree.add_child(root, Rect::new(0.0, 0.0, 200.0, 100.0)).unwrap();
    for node in [incoming, outgoing] { tree.context(node).unwrap().begin_recording().unwrap().commit(vec![]).unwrap(); }
    let retained = tree.context(root).unwrap();
    for _ in 0..8 {
        assert!(ctx.with_local_v2_paint_context(retained.clone(), |ctx| host.sync_local_v2_state(ctx)));
    }
    assert_eq!(tree.presentation(incoming).unwrap(), (aimer_canvas::Mat3::translate(25.0, 0.0), 0.25));
    assert_eq!(tree.presentation(outgoing).unwrap(), (aimer_canvas::Mat3::identity(), 0.75));
    assert_eq!(tree.draw_list_revision(incoming).unwrap(), 1);
    assert_eq!(tree.draw_list_revision(outgoing).unwrap(), 1);
    BUILDS.with(|log| assert_eq!(*log.borrow(), [Page::Home, Page::Detail]));
}

#[test]
fn route_overrides_and_navigation_actions_use_the_correct_owner() {
    let ctx = context();
    let actions = Rc::new(RefCell::new(Vec::new()));
    let log = actions.clone();
    let navigator = Navigator::new(Page::Home, routes).transition(DynamicTransitions::new(
        move |n: &NavigationContext<'_, '_, Page>| {
            log.borrow_mut().push(n.action);
            CrossFadeTransition::new().duration(Duration::from_secs(60)).resolve(n)
        })).to_element(&ctx);
    let controller = NavigatorController::<Page>::of(&ctx);
    controller.pop();
    navigator.rebuild_if_dirty(&ctx);
    controller.push(Page::Explicit);
    navigator.rebuild_if_dirty(&ctx);
    assert!(actions.borrow().is_empty());
    assert_eq!(pages(navigator.as_ref(), false), ["ExplicitPage"]);
    controller.push(Page::Detail);
    navigator.rebuild_if_dirty(&ctx);
    controller.pop();
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(*actions.borrow(), [NavigationAction::Push, NavigationAction::Pop]);
    assert_eq!(pages(navigator.as_ref(), false), ["DetailPage", "ExplicitPage"]);
    controller.clear();
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(actions.borrow().last(), Some(&NavigationAction::Clear));
    controller.set_route(Page::Detail);
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(actions.borrow().last(), Some(&NavigationAction::Replace));
}

thread_local! { static ROOT_VERSION: Cell<bool> = const { Cell::new(false) }; }
fn versioned_routes(_: Page) -> AnyWidget {
    routes(if ROOT_VERSION.get() { Page::Detail } else { Page::Home })
}

#[test]
fn rebuilding_a_navigator_refreshes_route_configuration_without_starting_a_transition() {
    let ctx = context();
    let navigator = Navigator::new(Page::Home, versioned_routes).transition(ZoomTransition::new()).to_element(&ctx);
    assert_eq!(pages(navigator.as_ref(), false), ["HomePage"]);
    ROOT_VERSION.set(true);
    navigator.mark_needs_rebuild();
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(pages(navigator.as_ref(), false), ["DetailPage"]);
}

#[test]
fn a_route_keeps_its_flex_factor_through_the_transition_host() {
    let ctx = context();
    let navigator = Navigator::new(Page::Home, routes).transition(ZoomTransition::new()).to_element(&ctx);
    assert_eq!(navigator.flex(), Some(2.0));
    NavigatorController::<Page>::of(&ctx).push(Page::Detail);
    navigator.rebuild_if_dirty(&ctx);
    assert_eq!(navigator.flex(), Some(2.0));
}

#[test]
fn high_dpi_transitions_sample_and_clip_the_logical_viewport() {
    use aimer_cupid::draw_cmd_v2::{Rect, RenderTree};
    let mut ctx = context();
    ctx.scale = 2.0;
    ctx.parent_size = ResolvedSize { width: 400.0, height: 200.0 };
    let sampled_size = Rc::new(Cell::new(ResolvedSize::default()));
    let observed = sampled_size.clone();
    let navigator = Navigator::new(Page::Home, routes).transition(DynamicTransitions::new(
        move |_: &NavigationContext<'_, '_, Page>| {
            let observed = observed.clone();
            ResolvedNavigationTransition::new(Duration::from_secs(60), Curve::Linear, move |_, size| {
                observed.set(size);
                NavigationTransitionFrame {
                    outgoing: PaintPresentation::IDENTITY_OPAQUE,
                    incoming: PaintPresentation { transform: aimer_canvas::Mat3::translate(size.width * 0.5, size.height * 0.5), opacity: 1.0 },
                }
            })
        })).to_element(&ctx);
    NavigatorController::<Page>::of(&ctx).push(Page::Detail);
    navigator.rebuild_if_dirty(&ctx);
    let host = find_host(navigator.as_ref()).unwrap();
    let viewport = Rect::new(0.0, 0.0, 200.0, 100.0);
    assert_eq!(host.retained_clip(&ctx), Some(viewport));
    let tree = RenderTree::new();
    let root = tree.add_root(viewport).unwrap();
    tree.add_child(root, viewport).unwrap();
    let incoming = tree.add_child(root, viewport).unwrap();
    assert!(ctx.with_local_v2_paint_context(tree.context(root).unwrap(), |ctx| host.sync_local_v2_state(ctx)));
    assert_eq!(sampled_size.get(), ResolvedSize { width: 200.0, height: 100.0 });
    assert_eq!(tree.presentation(incoming).unwrap().0, aimer_canvas::Mat3::translate(100.0, 50.0));
}

#[test]
fn both_transition_children_receive_the_original_viewport_constraints() {
    let mut ctx = context();
    ctx.box_constraint.max_width = 240.0;
    let navigator = Navigator::new(Page::Home, routes)
        .transition(CrossFadeTransition::new().duration(Duration::from_secs(60))).to_element(&ctx);
    NavigatorController::<Page>::of(&ctx).push(Page::Detail);
    navigator.rebuild_if_dirty(&ctx);
    let host = find_host(navigator.as_ref()).unwrap();
    let mut children = 0;
    host.visit_retained_v2_children(&mut |_, child| {
        let child_ctx = host.retained_v2_child_context(&ctx, child)
            .expect("an overlay must preserve transparent-wrapper constraints for both pages");
        assert_eq!(child_ctx.box_constraint.max_width, 240.0);
        assert_eq!(child_ctx.parent_size, ctx.parent_size);
        children += 1;
    });
    assert_eq!(children, 2);
}
