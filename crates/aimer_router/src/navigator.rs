#[cfg(feature = "portable-guest")]
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

use crate::{Route, Router};

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

  #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
fn browser_push_state(path: &str) {
    if let Some(window) = web_sys::window() {
        let history = window.history().expect("no history");
        let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
    }
}

  #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
pub(crate) fn browser_replace_state(path: &str) {
    if let Some(window) = web_sys::window() {
        let history = window.history().expect("no history");
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
    }
}

  #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
fn browser_current_path() -> Option<String> {
    web_sys::window().and_then(|w| w.location().pathname().ok())
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
        }
    }
}

impl<R: Route + Router> aimer_widget::PortableWidget for Navigator<R> {
    #[cfg(feature = "portable-guest")]
    fn to_portable_node(
        self,
        ctx: &mut aimer_widget::portable::PortableBuildContext,
        source: aimer_widget::portable::SourceFingerprint,
    ) -> Result<
        aimer_widget::portable::PortableNodeId,
        aimer_widget::portable::PortableBuildError,
    > {
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
}

impl<R: Route> NavigatorState<R> {
    pub fn push(&self, route: R) {
          #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
        browser_push_state(&route.format());
        self.updater.set_state(|state| {
            state.history.push(route);
        });
    }

    pub fn pop(&self) {
        self.updater.set_state(|state| {
            if state.history.len() > 1 {
                state.history.pop();
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
        self.history.iter().any(|candidate| candidate.format() == route)
    }

    pub fn clear(&self) {
        self.updater.set_state(|state| {
            state.history.clear();
            state.history.push(state.initial_route.clone());
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
        });
    }
}

impl<R: Route + Router> State<Navigator<R>> for NavigatorState<R> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater.clone();

          #[cfg(all(target_arch = "wasm32", not(aimer_portable_guest)))]
        {
            let updater_clone = updater;
            let closure = Closure::wrap(Box::new(move |_event: web_sys::PopStateEvent| {
                if let Some(path) = web_sys::window().and_then(|w| w.location().pathname().ok()) {
                    if let Some(route) = R::parse(&path) {
                        updater_clone.set_state(|state| {
                            // Replace the history stack with just
                            // this route
                            // (browser already manages the real
                            // history)
                            *state
                                .history
                                .last_mut()
                                .expect("History should not be empty") = route;
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
        let controller = navigator_controller(self.updater.clone());
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

        (self.routes)(effective)
    }
}

struct NavigatorElement<R> {
    controller: NavigatorController<R>,
    child: AnyElement,
}

impl<R: 'static> NavigatorElement<R> {
    fn scoped<T>(&self, ctx: &BuildContext, callback: impl FnOnce(&BuildContext) -> T) -> T {
        ctx.with_state(self.controller.clone(), callback)
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
    fn draw(&self, ctx: &BuildContext) {
        self.scoped(ctx, |ctx| self.child.draw(ctx));
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
        }
    }
}

impl<R: Route + Router> Widget for Navigator<R> {
    fn to_element(self, ctx: &BuildContext) -> AnyElement {
        let (child, updater) = StatefulElement::new(self, ctx);
        NavigatorElement {
            controller: navigator_controller(updater),
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
            let updater = updater.clone();
            Rc::new(move |route: R| updater.read(|state| state.push(route)))
        },
        pop_fn: {
            let updater = updater.clone();
            Rc::new(move || updater.read(|state| state.pop()))
        },
        can_pop_fn: {
            let updater = updater.clone();
            Rc::new(move || updater.read(|state| state.history.len() > 1))
        },
        history_len_fn: {
            let updater = updater.clone();
            Rc::new(move || updater.read(|state| state.history.len()))
        },
        routes_fn: {
            let updater = updater.clone();
            Rc::new(move || updater.read(|state| state.routes()))
        },
        contains_route_fn: {
            let updater = updater.clone();
            Rc::new(move |route| updater.read(|state| state.contains_route(route)))
        },
        current_route_fn: {
            let updater = updater.clone();
            Rc::new(move || updater.read(|state| state.current_route()))
        },
        clear_fn: {
            let updater = updater.clone();
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
mod tests {
    use std::cell::Cell;

    use aimer_widget::base::{ResolvedSize, WindowHandle};
    use aimer_widget::{Drawable, EventElement, LayoutElement, Rebuildable, VisitorElement};

    use crate::Router;

    use super::*;

    thread_local! {
        static NAVIGATOR_OBSERVED: Cell<bool> = const { Cell::new(false) };
        static CURRENT_ROUTE_OBSERVED: Cell<Option<TestRoute>> = const { Cell::new(None) };
        static HISTORY_LENGTH_OBSERVED: Cell<usize> = const { Cell::new(0) };
        static NAVIGATOR_OPERATION_STEP: Cell<u8> = const { Cell::new(0) };
        #[cfg(feature = "portable-guest")]
        static PORTABLE_ROUTE_BUILDS: RefCell<Vec<TestRoute>> = const { RefCell::new(Vec::new()) };
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TestRoute {
        Home,
        Settings,
        Profile,
    }

    impl Route for TestRoute {
        fn parse(path: &str) -> Option<Self> {
            (path == "/").then_some(Self::Home)
        }

        fn format(&self) -> String {
            match self {
                Self::Home => "/".to_owned(),
                Self::Settings => "/settings".to_owned(),
                Self::Profile => "/profile".to_owned(),
            }
        }
    }

    struct NavigatorLookupWidget;

    impl Widget for NavigatorLookupWidget {
        fn to_element(self, _ctx: &BuildContext) -> AnyElement {
            NavigatorLookupElement.boxed()
        }
    }

    impl aimer_widget::PortableWidget for NavigatorLookupWidget {}

    struct NavigatorLookupElement;

    impl VisitorElement for NavigatorLookupElement {
        fn debug_name(&self) -> &'static str {
            "NavigatorLookupElement"
        }
    }
    impl EventElement for NavigatorLookupElement {}
    impl LayoutElement for NavigatorLookupElement {}
    impl Rebuildable for NavigatorLookupElement {}
    impl Drawable for NavigatorLookupElement {
        fn draw(&self, ctx: &BuildContext) {
            let _ = NavigatorController::<TestRoute>::of(ctx);
            NAVIGATOR_OBSERVED.set(true);
        }
    }

    struct NavigatorControllerOperationsWidget;

    impl Widget for NavigatorControllerOperationsWidget {
        fn to_element(self, _ctx: &BuildContext) -> AnyElement {
            NavigatorControllerOperationsElement.boxed()
        }
    }

    impl aimer_widget::PortableWidget for NavigatorControllerOperationsWidget {}

    struct NavigatorControllerOperationsElement;

    impl VisitorElement for NavigatorControllerOperationsElement {
        fn debug_name(&self) -> &'static str {
            "NavigatorControllerOperationsElement"
        }
    }
    impl EventElement for NavigatorControllerOperationsElement {}
    impl LayoutElement for NavigatorControllerOperationsElement {}
    impl Rebuildable for NavigatorControllerOperationsElement {}
    impl Drawable for NavigatorControllerOperationsElement {
        fn draw(&self, ctx: &BuildContext) {
            let navigator = NavigatorController::<TestRoute>::of(ctx);

            match NAVIGATOR_OPERATION_STEP.get() {
                0 => {
                    assert_eq!(navigator.current_route(), TestRoute::Home);
                    navigator.push(TestRoute::Settings);
                }
                1 => {
                    assert_eq!(navigator.current_route(), TestRoute::Settings);
                    navigator.push(TestRoute::Profile);
                }
                2 => {
                    assert_eq!(navigator.current_route(), TestRoute::Profile);
                    navigator.set_route(TestRoute::Settings);
                }
                3 => {
                    assert_eq!(navigator.current_route(), TestRoute::Settings);
                    assert_eq!(navigator.history_len(), 1);
                    navigator.push(TestRoute::Profile);
                }
                4 => {
                    assert_eq!(navigator.current_route(), TestRoute::Profile);
                    navigator.clear();
                }
                5 => {
                    CURRENT_ROUTE_OBSERVED.set(Some(navigator.current_route()));
                    HISTORY_LENGTH_OBSERVED.set(navigator.history_len());
                    assert_eq!(navigator.routes(), vec![TestRoute::Home]);
                    assert!(navigator.contains_route(&TestRoute::Home));
                    let routes: Vec<_> = (&navigator).into_iter().collect();
                    assert_eq!(routes, vec![TestRoute::Home]);
                }
                _ => return,
            }
            NAVIGATOR_OPERATION_STEP.set(NAVIGATOR_OPERATION_STEP.get() + 1);
        }
    }

    fn lookup_route(_: TestRoute) -> AnyWidget {
        NavigatorLookupWidget.boxed()
    }

    fn lookup_controller_operations(_: TestRoute) -> AnyWidget {
        NavigatorControllerOperationsWidget.boxed()
    }

    #[cfg(not(feature = "portable-guest"))]
    impl Router for TestRoute {
        fn build(&self, _ctx: &BuildContext) -> AnyWidget {
            lookup_route(*self)
        }
    }

    #[cfg(feature = "portable-guest")]
    struct PortableRouteWidget;

    #[cfg(feature = "portable-guest")]
    impl aimer_widget::PortableWidget for PortableRouteWidget {
        fn to_portable_node(
            self,
            ctx: &mut aimer_widget::portable::PortableBuildContext,
            source: aimer_widget::portable::SourceFingerprint,
        ) -> Result<
            aimer_widget::portable::PortableNodeId,
            aimer_widget::portable::PortableBuildError,
        > {
            let navigator = NavigatorController::<TestRoute>::of(&ctx.build_context());
            let event_kind = aimer_widget::portable::__anteros::EventId::new(1);
            let callback_id = ctx.callback_id_for(None, source, event_kind);
            let callback = aimer_widget::portable::PortableCallback::new(
                event_kind,
                aimer_widget::portable::__anteros::Version::new(1, 0),
                callback_id,
                move || {
                    navigator.push(TestRoute::Settings);
                    Ok(())
                },
            );
            ctx.push_node_with_callbacks(
                aimer_widget::portable::__anteros::WIDGET_TEXT,
                aimer_widget::portable::__anteros::Version::new(1, 0),
                None,
                source,
                &[],
                vec![callback],
                &[],
            )
        }
    }

    #[cfg(feature = "portable-guest")]
    impl Widget for PortableRouteWidget {
        fn to_element(self, _ctx: &BuildContext) -> AnyElement {
            panic!("portable route widget must not build natively")
        }
    }

    #[cfg(feature = "portable-guest")]
    impl Router for TestRoute {
        fn build(&self, _ctx: &BuildContext) -> AnyWidget {
            PORTABLE_ROUTE_BUILDS.with(|routes| routes.borrow_mut().push(*self));
            PortableRouteWidget.boxed()
        }
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn navigator_portable_lowering_uses_the_active_router_tree() {
        use aimer_widget::PortableWidget as _;
        use aimer_widget::portable::{
            PortableBuildContext, PortableLimits, PortableWidgetLimits, SourceFingerprint,
            StableId128,
        };
        use aimer_widget::portable::__anteros::{WidgetDocumentView, WIDGET_TEXT};

        let mut context = PortableBuildContext::new(
            1,
            1,
            PortableWidgetLimits::new(8, 8, 8, 8, 64, 2_048),
            PortableLimits::new(8, 16, 64, 128, 1_024),
        )
        .unwrap();
        let root = Navigator::new(TestRoute::Home, lookup_route)
            .to_portable_node(
                &mut context,
                SourceFingerprint::new(StableId128::from_bytes([7; 16])),
            )
            .unwrap();
        let document = context.finish_document(root).unwrap();
        let bytes = document.encode().unwrap();
        let view = WidgetDocumentView::decode(&bytes, document.model_limits()).unwrap();

        assert_eq!(view.node(view.root_node()).unwrap().widget_type(), WIDGET_TEXT);
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn navigator_portable_push_survives_the_rebuild_triggered_by_a_callback() {
        use aimer_widget::PortableWidget as _;
        use aimer_widget::portable::{PortableBuildContext, PortableLimits, PortableWidgetLimits,
            SourceFingerprint, StableId128};

        PORTABLE_ROUTE_BUILDS.with(|routes| routes.borrow_mut().clear());

        let mut context = PortableBuildContext::new(
            1,
            1,
            PortableWidgetLimits::new(8, 8, 8, 8, 64, 2_048),
            PortableLimits::new(8, 16, 64, 128, 1_024),
        )
        .unwrap();
        let source = SourceFingerprint::new(StableId128::from_bytes([7; 16]));
        let root = Navigator::new(TestRoute::Home, lookup_route)
            .to_portable_node(&mut context, source)
            .unwrap();
        context.finish_document(root).unwrap();

        let event_kind = aimer_widget::portable::__anteros::EventId::new(1);
        let callback_id = context.callback_id_for(None, source, event_kind);
        let callbacks = context.take_callback_registry();
        callbacks.dispatch(callback_id, &mut context).unwrap();
        assert!(context.take_rebuild_request());

        let root = Navigator::new(TestRoute::Home, lookup_route)
            .to_portable_node(&mut context, source)
            .unwrap();
        context.finish_document(root).unwrap();

        assert_eq!(
            PORTABLE_ROUTE_BUILDS.with(|routes| routes.borrow().clone()),
            vec![TestRoute::Home, TestRoute::Settings],
            "a callback-triggered portable rebuild must render the pushed route"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn context() -> BuildContext<'static> {
        use std::sync::OnceLock;

        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        });
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        let _guard = runtime.enter();
        BuildContext::new(
            canvas,
            ResolvedSize::default(),
            1.0,
            Default::default(),
            Default::default(),
            WindowHandle::headless(Default::default(), 1.0),
            tokio::runtime::Handle::current(),
        )
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn navigator_controller_remains_scoped_on_a_fresh_frame_context() {
        NAVIGATOR_OBSERVED.set(false);
        let navigator = Navigator::new(TestRoute::Home, lookup_route);
        let initial_context = context();
        let element = navigator.to_element(&initial_context);

        element.draw(&context());

        assert!(NAVIGATOR_OBSERVED.get());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn navigator_controller_can_read_and_replace_route_history() {
        CURRENT_ROUTE_OBSERVED.set(None);
        HISTORY_LENGTH_OBSERVED.set(0);
        NAVIGATOR_OPERATION_STEP.set(0);
        let navigator = Navigator::new(TestRoute::Home, lookup_controller_operations);
        let initial_context = context();
        let element = navigator.to_element(&initial_context);

        for _ in 0..6 {
            element.draw(&context());
            element.rebuild_if_dirty(&context());
        }

        assert_eq!(CURRENT_ROUTE_OBSERVED.get(), Some(TestRoute::Home));
        assert_eq!(HISTORY_LENGTH_OBSERVED.get(), 1);
    }

    #[test]
    fn redirect_reroutes_once_then_settles() {
        // "guarded" redirects to "login"; "login" does not redirect.
        let result = resolve_redirect_chain(
            "guarded",
            |r| if *r == "guarded" { Some("login") } else { None },
            MAX_REDIRECT_HOPS,
        );
        assert_eq!(result, "login");
    }

    #[test]
    fn redirect_none_passes_through() {
        let result = resolve_redirect_chain("home", |_| None, MAX_REDIRECT_HOPS);
        assert_eq!(result, "home");
    }

    #[test]
    fn redirect_chain_follows_multiple_hops() {
        let result = resolve_redirect_chain(
            0,
            |n| if *n < 3 { Some(n + 1) } else { None },
            MAX_REDIRECT_HOPS,
        );
        assert_eq!(result, 3);
    }

    #[test]
    fn redirect_loop_is_bounded_and_terminates() {
        // Always redirects: must terminate at the hop limit without hanging/panicking.
        let result = resolve_redirect_chain(0u32, |n| Some(n.wrapping_add(1)), 4);
        assert_eq!(result, 4);
    }
}
