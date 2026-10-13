use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use aimer_widget::base::{BuildContext, ResolvedSize, Size, Vec2d};
use aimer_widget::{
    AnyElement, AnyWidget, Drawable, Element, EventElement, LayoutElement, Rebuildable, State,
    StateUpdater, StatefulElement, StatefulWidget, VisitorElement, Widget,
};
#[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
use wasm_bindgen::prelude::*;

use crate::outlet::RouteChildContext;
use crate::{Route, Router};
use crate::{NavigationAction, NavigationTransition};
use crate::transition_host::{NavigationPages, NavigationRequest};

#[cfg(any(test, all(target_arch = "wasm32", not(aimer_portable_guest))))]
mod history;
#[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
use history::{browser_current_path, browser_current_position, browser_position, browser_push_state, history_action};
#[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
pub(crate) use history::browser_replace_state;

/// Maximum number of redirect hops resolved before the navigator bails out.
/// Prevents infinite redirect loops from hanging the app.
pub const MAX_REDIRECT_HOPS: usize = 16;

/// Follow a route's redirect chain until it settles on a route that does not
/// redirect, or until `max_hops` is exhausted (loop guard). `redirect` is the
/// per-route hook; extracted as a closure so the resolution logic is unit
/// testable without a live `BuildContext`.
pub fn resolve_redirect_chain<R, F>(start: R, mut redirect: F, max_hops: usize) -> R
where
    R: Clone,
    F: FnMut(&R) -> Option<R>,
{
    let mut current = start;
    for _ in 0..max_hops {
        match redirect(&current) {
            Some(next) => current = next,
            None => return current,
        }
    }
    // Bailed out after too many hops: return the last route rather than looping
    // forever.
    current
}

/// A stateful route stack that renders the widget for its current top route.
///
/// Descendants can retrieve a [`NavigatorController`] from the build context.
/// Pushing appends a route; popping never removes the initial route. On
/// WebAssembly, the initial browser path overrides `initial_route` when it
/// parses successfully, and later stack changes synchronize with browser
/// history.
pub struct Navigator<R>
where
    R: Route + Router,
{
    pub initial_route: R,
    pub routes: fn(R) -> AnyWidget,
    /// Optional fallback used by routes that inherit their transition.
    pub transition: Option<Rc<dyn NavigationTransition<R>>>,
}

impl<R: Route + Router> Navigator<R> {
    /// Creates a navigator with one initial route and a route-to-widget
    /// builder.
    ///
    /// `routes` is called for the active route after redirect resolution. The
    /// initial route remains the bottom of the in-memory stack.
    pub fn new(initial_route: R, routes: fn(R) -> AnyWidget) -> Self {
        // On WASM, try to restore the initial route from the browser URL
        #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
        let initial_route = {
            browser_current_path()
                .and_then(|path| R::parse(&path))
                .unwrap_or(initial_route)
        };
        Self {
            initial_route,
            routes,
            transition: None,
        }
    }

    /// Sets the fallback effect for routes without an explicit selection.
    /// Initial mounting never animates. A route may override or disable it.
    #[inline]
    pub fn transition(mut self, transition: impl NavigationTransition<R>) -> Self {
        self.transition = Some(Rc::new(transition));
        self
    }
}

impl<R: Route + Router> aimer_widget::PortableWidget for Navigator<R> {
    #[cfg(feature = "portable-guest")]
    fn to_portable_node(
        self,
        ctx: &mut aimer_widget::portable::PortableBuildContext,
        source: aimer_widget::portable::SourceFingerprint,
    ) -> Result<aimer_widget::portable::PortableNodeId, aimer_widget::portable::PortableBuildError>
    {
        let initial_route = self.initial_route;
        let slot = ctx.slot_for(None, source);
        // Callbacks run between portable builds. Keep the controller's route
        // stack in generation-local retained storage so a rebuild observes the
        // route pushed by the callback instead of recreating the initial route.
        ctx.with_animation_state(
            slot,
            || {
                Rc::new(RefCell::new(PortableNavigatorState {
                    initial_route: initial_route.clone(),
                    history: vec![initial_route.clone()],
                }))
            },
            |state, ctx| {
                let controller = portable_navigator_controller(state.clone());
                let current_route = controller.current_route();
                ctx.with_state(controller, |ctx| {
                    let build_context = ctx.build_context();
                    let effective = resolve_redirect_chain(
                        current_route,
                        |route| route.redirect(&build_context),
                        MAX_REDIRECT_HOPS,
                    );
                    let route_widget = Router::build(&effective, &build_context);
                    aimer_widget::AnyWidgetExt::into_portable_node(route_widget, ctx, source)
                })
            },
        )
    }
}

impl<R: Route + Router> Navigator<R> {
    /// Shorthand of [`NavigatorController::of`].
    pub fn of(ctx: &BuildContext) -> NavigatorInstance<R> {
        NavigatorController::of(ctx)
    }
}

pub struct NavigatorState<R>
where
    R: Route,
{
    pub initial_route: R,
    pub history: Vec<R>,
    pub updater: StateUpdater<Self>,
    pub routes: fn(R) -> AnyWidget,
    transition: Option<Rc<dyn NavigationTransition<R>>>,
    pages: RefCell<NavigationPages<R>>,
    action: NavigationAction,
    #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
    browser_position: Option<u64>,
}

impl<R: Route> NavigatorState<R> {
    pub fn push(&self, route: R) {
        self.updater.set_state(|state| {
            #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
            {
                let position = state.browser_position.unwrap_or(0).saturating_add(1);
                browser_push_state(&route.format(), position);
                state.browser_position = Some(position);
            }
            state.history.push(route);
            state.action = NavigationAction::Push;
        });
    }

    pub fn pop(&self) {
        self.updater.set_state(|state| {
            if state.history.len() > 1 {
                state.history.pop();
                state.action = NavigationAction::Pop;
                #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
                if let Some(prev) = state.history.last() {
                    browser_replace_state(&prev.format());
                }
            }
        });
    }

    pub fn current_route(&self) -> R {
        self.history
            .last()
            .expect("History should not be empty")
            .clone()
    }

    pub fn routes(&self) -> Vec<R> {
        self.history.clone()
    }

    pub fn contains_route(&self, route: &R) -> bool {
        let route = route.format();
        self.history
            .iter()
            .any(|candidate| candidate.format() == route)
    }

    pub fn clear(&self) {
        self.updater.set_state(|state| {
            state.history.clear();
            state.history.push(state.initial_route.clone());
            state.action = NavigationAction::Clear;
            #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
            browser_replace_state(&state.initial_route.format());
        });
    }

    pub fn set_route(&self, route: R) {
        #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
        browser_replace_state(&route.format());
        self.updater.set_state(|state| {
            state.history.clear();
            state.history.push(route);
            state.action = NavigationAction::Replace;
        });
    }
}

impl<R: Route + Router> State<Navigator<R>> for NavigatorState<R> {
    fn adopt_config_from(&mut self, new: Self) {
        self.routes = new.routes;
        self.transition = new.transition;
    }

    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;

        #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
        {
            let updater_clone = updater;
            let closure = Closure::wrap(Box::new(move |event: web_sys::PopStateEvent| {
                if let Some(path) = browser_current_path() {
                    if let Some(route) = R::parse(&path) {
                        let position = browser_position(&event.state());
                        updater_clone.set_state(move |state| {
                            // Replace the history stack with just
                            // this route
                            // (browser already manages the real
                            // history)
                            *state
                                .history
                                .last_mut()
                                .expect("History should not be empty") = route;
                            state.action = history_action(state.browser_position, position);
                            state.browser_position = position;
                        });
                    }
                }
            }) as Box<dyn FnMut(web_sys::PopStateEvent)>);

            if let Some(window) = web_sys::window() {
                let _ = window
                    .add_event_listener_with_callback("popstate", closure.as_ref().unchecked_ref());
            }

            // Leak the closure so it stays alive for the lifetime of the app
            closure.forget();
        }
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        let controller = navigator_controller(self.updater);
        ctx.insert_state(controller.clone());

        let top = self
            .history
            .last()
            .expect("History should not be empty")
            .clone();
        let effective = ctx.with_state(controller.clone(), |ctx| {
            resolve_redirect_chain(top.clone(), |route| route.redirect(ctx), MAX_REDIRECT_HOPS)
        });

        // Keep the browser address bar in sync with the final, post-redirect route.
        #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
        if effective.format() != top.format() {
            browser_replace_state(&effective.format());
        }

        ctx.with_state(NavigationRequest(self.action), |ctx| {
            self.pages.borrow_mut().build(effective, self.action, self.transition.as_deref(), ctx, self.routes)
        })
    }
}

struct NavigatorElement<R> {
    controller: Rc<NavigatorController<R>>,
    route_context: RouteChildContext,
    child: AnyElement,
}

impl<R: 'static> NavigatorElement<R> {
    fn scoped<T>(&self, ctx: &BuildContext, callback: impl FnOnce(&BuildContext) -> T) -> T {
        self.route_context
            .with_shared_state(ctx, self.controller.clone(), callback)
    }
}

impl<R: 'static> VisitorElement for NavigatorElement<R> {
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        visitor(self.child.as_ref());
    }

    fn debug_name(&self) -> &'static str {
        "NavigatorScope"
    }
}

impl<R: 'static> Drawable for NavigatorElement<R> {
    fn update(&self, ctx: &BuildContext) {
        self.scoped(ctx, |ctx| self.child.update(ctx));
    }

    fn can_paint_local_v2(&self, _ctx: &BuildContext) -> bool {
        true
    }

    fn paint_local_v2(&self, _ctx: &BuildContext) {}

    fn retained_v2_child_context<'a>(
        &self,
        ctx: &BuildContext<'a>,
        _child: &dyn Element,
    ) -> Option<BuildContext<'a>> {
        self.route_context.with(ctx, |ctx| {
            Some(ctx.fork_with_state(self.controller.as_ref().clone()))
        })
    }

    fn sync_paint_geometry(&self, ctx: &BuildContext) {
        self.scoped(ctx, |ctx| self.child.sync_paint_geometry(ctx));
    }
}

impl<R: 'static> LayoutElement for NavigatorElement<R> {
    fn pos(&self) -> Option<Vec2d> {
        self.child.pos()
    }

    fn size(&self) -> Option<Size> {
        self.child.size()
    }

    fn layout(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| self.child.layout(ctx))
    }

    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| self.child.computed_size(ctx))
    }

    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        self.scoped(ctx, |ctx| self.child.content_size(ctx))
    }

    fn layer(&self) -> u32 {
        self.child.layer()
    }

    fn flex(&self) -> Option<f32> {
        self.child.flex()
    }

    fn get_size_from_child(&self) -> Option<Size> {
        self.child.get_size_from_child()
    }

    fn invalidate_layout(&self) {
        self.child.invalidate_layout();
    }

    fn pos_start_end(&self) -> Option<(Vec2d, Vec2d)> {
        self.child.pos_start_end()
    }
}

impl<R: 'static> EventElement for NavigatorElement<R> {}

impl<R: 'static> Rebuildable for NavigatorElement<R> {
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        self.scoped(ctx, |ctx| self.child.rebuild_if_dirty(ctx));
    }

    fn is_carry_state(&self) -> bool {
        true
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.scoped(ctx, callback);
    }

    fn mark_needs_rebuild(&self) {
        self.child.mark_needs_rebuild();
    }
}

pub struct NavigatorController<R> {
    push_fn: Rc<dyn Fn(R)>,
    pop_fn: Rc<dyn Fn()>,
    can_pop_fn: Rc<dyn Fn() -> bool>,
    history_len_fn: Rc<dyn Fn() -> usize>,
    routes_fn: Rc<dyn Fn() -> Vec<R>>,
    contains_route_fn: Rc<dyn Fn(&R) -> bool>,
    current_route_fn: Rc<dyn Fn() -> R>,
    clear_fn: Rc<dyn Fn()>,
    set_route_fn: Rc<dyn Fn(R)>,
}

unsafe impl<R> Send for NavigatorController<R> {}
unsafe impl<R> Sync for NavigatorController<R> {}
impl<R> Clone for NavigatorController<R> {
    fn clone(&self) -> Self {
        NavigatorController {
            push_fn: self.push_fn.clone(),
            pop_fn: self.pop_fn.clone(),
            can_pop_fn: self.can_pop_fn.clone(),
            history_len_fn: self.history_len_fn.clone(),
            routes_fn: self.routes_fn.clone(),
            contains_route_fn: self.contains_route_fn.clone(),
            current_route_fn: self.current_route_fn.clone(),
            clear_fn: self.clear_fn.clone(),
            set_route_fn: self.set_route_fn.clone(),
        }
    }
}

pub type NavigatorInstance<R> = NavigatorController<R>;

impl<R: 'static> NavigatorController<R> {
    /// Flutter-style: `Navigator::of(ctx).push(route)`
    #[track_caller]
    pub fn of(ctx: &BuildContext) -> NavigatorInstance<R> {
        (*ctx
            .get_state::<NavigatorController<R>>()
            .expect("No Navigator found in context. Make sure a Navigator widget is an ancestor."))
        .clone()
    }

    pub fn push(&self, route: R) {
        (self.push_fn)(route);
    }

    pub fn pop(&self) {
        (self.pop_fn)();
    }

    pub fn can_pop(&self) -> bool {
        (self.can_pop_fn)()
    }

    pub fn history_len(&self) -> usize {
        (self.history_len_fn)()
    }

    /// Returns a snapshot of the navigator's route history, from the initial
    /// route to the currently displayed route.
    ///
    /// The returned vector is independent of the navigator. Pushing, popping,
    /// or replacing routes after this method returns does not change it.
    pub fn routes(&self) -> Vec<R> {
        (self.routes_fn)()
    }

    /// Returns whether `route` is present in the navigator's route history.
    ///
    /// Routes are compared by their formatted paths rather than by requiring
    /// every [`Route`] implementation to also implement [`PartialEq`]. This
    /// makes the query available to all existing route types while matching
    /// the identity used by browser navigation.
    pub fn contains_route(&self, route: &R) -> bool
    where
        R: Route,
    {
        (self.contains_route_fn)(route)
    }

    /// Iterates over a snapshot of the navigator's route history.
    ///
    /// The iterator does not keep the navigator borrowed and therefore remains
    /// valid if the navigator is used to change its history while iterating.
    pub fn iter(&self) -> std::vec::IntoIter<R>
    where
        R: Route,
    {
        self.routes().into_iter()
    }

    /// Returns the route currently displayed by the navigator.
    ///
    /// # Panics
    ///
    /// Panics if the navigator history is empty. A [`Navigator`] always keeps
    /// at least one route, so this indicates a broken internal invariant.
    pub fn current_route(&self) -> R {
        (self.current_route_fn)()
    }

    /// Removes all pushed routes and returns the navigator to its initial route.
    pub fn clear(&self) {
        (self.clear_fn)();
    }

    /// Replaces the complete navigation history with `route`.
    ///
    /// The supplied route becomes the only route in the history, so a later
    /// [`Self::pop`] does nothing until another route is pushed.
    pub fn set_route(&self, route: R) {
        (self.set_route_fn)(route);
    }
}

impl<R: Route> NavigatorController<R> {
    /// Navigate to a route resolved by its declared `name` and a set of
    /// path/query parameters (keyed by field name). Returns `true` when the
    /// name resolved to a route and was pushed, `false` otherwise.
    pub fn push_named(&self, name: &str, params: &HashMap<String, String>) -> bool {
        match R::resolve_named(name, params) {
            Some(route) => {
                (self.push_fn)(route);
                true
            }
            None => false,
        }
    }
}

impl<R: Route> IntoIterator for &NavigatorController<R> {
    type Item = R;
    type IntoIter = std::vec::IntoIter<R>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<R: Route> IntoIterator for NavigatorController<R> {
    type Item = R;
    type IntoIter = std::vec::IntoIter<R>;

    fn into_iter(self) -> Self::IntoIter {
        self.routes().into_iter()
    }
}

impl<R: Route + Router> StatefulWidget for Navigator<R> {
    type State = NavigatorState<R>;
    fn create_state(self) -> Self::State {
        NavigatorState::<R> {
            initial_route: self.initial_route.clone(),
            history: vec![self.initial_route.clone()],
            updater: StateUpdater::empty(),
            routes: self.routes,
            transition: self.transition,
            pages: RefCell::new(NavigationPages::default()),
            action: NavigationAction::Push,
            #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
            browser_position: browser_current_position(),
        }
    }
}

impl<R: Route + Router> Widget for Navigator<R> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let route_context = RouteChildContext::capture(ctx);
        let (child, updater) = route_context.with(ctx, |ctx| StatefulElement::new(self, ctx));
        NavigatorElement {
            controller: Rc::new(navigator_controller(updater)),
            route_context,
            child: child.boxed(),
        }
        .boxed()
    }
}

fn navigator_controller<R: Route>(
    updater: StateUpdater<NavigatorState<R>>,
) -> NavigatorController<R> {
    NavigatorController {
        push_fn: {
            let updater = updater;
            Rc::new(move |route: R| updater.read(|state| state.push(route)))
        },
        pop_fn: {
            let updater = updater;
            Rc::new(move || updater.read(|state| state.pop()))
        },
        can_pop_fn: {
            let updater = updater;
            Rc::new(move || updater.read(|state| state.history.len() > 1))
        },
        history_len_fn: {
            let updater = updater;
            Rc::new(move || updater.read(|state| state.history.len()))
        },
        routes_fn: {
            let updater = updater;
            Rc::new(move || updater.read(|state| state.routes()))
        },
        contains_route_fn: {
            let updater = updater;
            Rc::new(move |route| updater.read(|state| state.contains_route(route)))
        },
        current_route_fn: {
            let updater = updater;
            Rc::new(move || updater.read(|state| state.current_route()))
        },
        clear_fn: {
            let updater = updater;
            Rc::new(move || updater.read(|state| state.clear()))
        },
        set_route_fn: Rc::new(move |route| updater.read(|state| state.set_route(route))),
    }
}

#[cfg(feature = "portable-guest")]
struct PortableNavigatorState<R> {
    initial_route: R,
    history: Vec<R>,
}

#[cfg(feature = "portable-guest")]
fn portable_navigator_controller<R: Route>(
    state: Rc<RefCell<PortableNavigatorState<R>>>,
) -> NavigatorController<R> {
    NavigatorController {
        push_fn: {
            let state = state.clone();
            Rc::new(move |route| state.borrow_mut().history.push(route))
        },
        pop_fn: {
            let state = state.clone();
            Rc::new(move || {
                let mut state = state.borrow_mut();
                if state.history.len() > 1 {
                    state.history.pop();
                }
            })
        },
        can_pop_fn: {
            let state = state.clone();
            Rc::new(move || state.borrow().history.len() > 1)
        },
        history_len_fn: {
            let state = state.clone();
            Rc::new(move || state.borrow().history.len())
        },
        routes_fn: {
            let state = state.clone();
            Rc::new(move || state.borrow().history.clone())
        },
        contains_route_fn: {
            let state = state.clone();
            Rc::new(move |route| {
                let route = route.format();
                state
                    .borrow()
                    .history
                    .iter()
                    .any(|candidate| candidate.format() == route)
            })
        },
        current_route_fn: {
            let state = state.clone();
            Rc::new(move || {
                state
                    .borrow()
                    .history
                    .last()
                    .expect("History should not be empty")
                    .clone()
            })
        },
        clear_fn: {
            let state = state.clone();
            Rc::new(move || {
                let mut state = state.borrow_mut();
                let initial_route = state.initial_route.clone();
                state.history.clear();
                state.history.push(initial_route);
            })
        },
        set_route_fn: Rc::new(move |route| {
            let mut state = state.borrow_mut();
            state.history.clear();
            state.history.push(route);
        }),
    }
}

#[cfg(test)]
mod tests;
