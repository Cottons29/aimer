use std::collections::HashMap;
use std::rc::Rc;

use aimer_widget::base::{BuildContext, ResolvedSize, Size, Vec2d};
use aimer_widget::{
    Drawable, Element, EventElement, LayoutElement, Rebuildable, State, StateUpdater,
    StatefulElement, StatefulWidget, VisitorElement, Widget,
};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::Route;

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

#[cfg(target_arch = "wasm32")]
fn browser_push_state(path: &str) {
    if let Some(window) = web_sys::window() {
        let history = window
            .history()
            .expect("no history");
        let _ = history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn browser_replace_state(path: &str) {
    if let Some(window) = web_sys::window() {
        let history = window
            .history()
            .expect("no history");
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path));
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_current_path() -> Option<String> {
    web_sys::window().and_then(|w| w.location().pathname().ok())
}

/// A stateful route stack that renders the widget for its current top route.
///
/// Descendants can retrieve a [`NavigatorController`] from the build context.
/// Pushing appends a route; popping never removes the initial route. On WebAssembly,
/// the initial browser path overrides `initial_route` when it parses successfully,
/// and later stack changes synchronize with browser history.
pub struct Navigator<R>
where
    R: Route,
{
    pub initial_route: R,
    pub routes: fn(R) -> Box<dyn Widget>,
}

impl<R: Route> Navigator<R> {
    /// Creates a navigator with one initial route and a route-to-widget builder.
    ///
    /// `routes` is called for the active route after redirect resolution. The
    /// initial route remains the bottom of the in-memory stack.
    pub fn new(initial_route: R, routes: fn(R) -> Box<dyn Widget>) -> Self {
        // On WASM, try to restore the initial route from the browser URL
        #[cfg(target_arch = "wasm32")]
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

pub struct NavigatorState<R>
where
    R: Route,
{
    pub history: Vec<R>,
    pub updater: StateUpdater<Self>,
    pub routes: fn(R) -> Box<dyn Widget>,
}

impl<R: Route> NavigatorState<R> {
    pub fn push(&self, route: R) {
        #[cfg(target_arch = "wasm32")]
        browser_push_state(&route.format());
        self.updater
            .set_state(|state| {
                state.history.push(route);
            });
    }

    pub fn pop(&self) {
        self.updater
            .set_state(|state| {
                if state.history.len() > 1 {
                    state.history.pop();
                    #[cfg(target_arch = "wasm32")]
                    if let Some(prev) = state.history.last() {
                        browser_replace_state(&prev.format());
                    }
                }
            });
    }
}

impl<R: Route> State<Navigator<R>> for NavigatorState<R> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater.clone();

        #[cfg(target_arch = "wasm32")]
        {
            let updater_clone = updater;
            let closure = Closure::wrap(Box::new(move |_event: web_sys::PopStateEvent| {
                if let Some(path) = web_sys::window().and_then(|w| w.location().pathname().ok()) {
                    if let Some(route) = R::parse(&path) {
                        updater_clone.set_state(|state| {
                            // Replace the history stack with just this route
                            // (browser already manages the real history)
                            *state
                                .history
                                .last_mut()
                                .expect("History should not be empty") = route;
                        });
                    }
                }
            }) as Box<dyn FnMut(web_sys::PopStateEvent)>);

            if let Some(window) = web_sys::window() {
                let _ = window.add_event_listener_with_callback(
                    "popstate",
                    closure
                        .as_ref()
                        .unchecked_ref(),
                );
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
        #[cfg(target_arch = "wasm32")]
        if effective.format() != top.format() {
            browser_replace_state(&effective.format());
        }

        (self.routes)(effective)
    }
}

struct NavigatorElement<R> {
    controller: NavigatorController<R>,
    child: Box<dyn Element>,
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
        self.child
            .get_size_from_child()
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
        self.scoped(ctx, |ctx| {
            self.child
                .rebuild_if_dirty(ctx)
        });
    }

    fn with_rebuild_context(&self, ctx: &BuildContext, callback: &mut dyn FnMut(&BuildContext)) {
        self.scoped(ctx, callback);
    }

    fn is_carry_state(&self) -> bool {
        true
    }

    fn mark_needs_rebuild(&self) {
        self.child
            .mark_needs_rebuild();
    }
}

pub struct NavigatorController<R> {
    push_fn: Rc<dyn Fn(R)>,
    pop_fn: Rc<dyn Fn()>,
    can_pop_fn: Rc<dyn Fn() -> bool>,
    history_len_fn: Rc<dyn Fn() -> usize>,
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

impl<R: Route> StatefulWidget for Navigator<R> {
    type State = NavigatorState<R>;
    fn create_state(&self) -> Self::State {
        NavigatorState::<R> {
            history: vec![self.initial_route.clone()],
            updater: StateUpdater::empty(),
            routes: self.routes,
        }
    }
}

impl<R: Route> Widget for Navigator<R> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let (child, updater) = StatefulElement::new(self, ctx);
        Box::new(NavigatorElement {
            controller: navigator_controller(updater),
            child: Box::new(child),
        })
    }
}

fn navigator_controller<R: Route>(
    updater: StateUpdater<NavigatorState<R>>,
) -> NavigatorController<R> {
    NavigatorController {
        push_fn: {
            let updater = updater.clone();
            Rc::new(move |route: R| {
                #[cfg(target_arch = "wasm32")]
                browser_push_state(&route.format());
                updater.set_state(|state| {
                    state.history.push(route);
                });
            })
        },
        pop_fn: {
            let updater = updater.clone();
            Rc::new(move || {
                updater.set_state(|state| {
                    if state.history.len() > 1 {
                        state.history.pop();
                        #[cfg(target_arch = "wasm32")]
                        if let Some(previous) = state.history.last() {
                            browser_replace_state(&previous.format());
                        }
                    }
                });
            })
        },
        can_pop_fn: {
            let updater = updater.clone();
            Rc::new(move || updater.read(|state| state.history.len() > 1))
        },
        history_len_fn: Rc::new(move || updater.read(|state| state.history.len())),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use aimer_widget::base::{ResolvedSize, WindowHandle};
    use aimer_widget::{Drawable, EventElement, LayoutElement, Rebuildable, VisitorElement};

    use super::*;

    thread_local! {
        static NAVIGATOR_OBSERVED: Cell<bool> = const { Cell::new(false) };
    }

    #[derive(Clone)]
    enum TestRoute {
        Home,
    }

    impl Route for TestRoute {
        fn parse(path: &str) -> Option<Self> {
            (path == "/").then_some(Self::Home)
        }

        fn format(&self) -> String {
            "/".to_owned()
        }
    }

    struct NavigatorLookupWidget;

    impl Widget for NavigatorLookupWidget {
        fn to_element(&self, _ctx: &BuildContext) -> Box<dyn Element> {
            Box::new(NavigatorLookupElement)
        }
    }

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

    fn lookup_route(_: TestRoute) -> Box<dyn Widget> {
        Box::new(NavigatorLookupWidget)
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
