use std::any::Any;
use std::rc::Rc;
use aimer_widget::base::BuildContext;
use aimer_widget::{Element, State, StatefulElement, StateUpdater, StatefulWidget, Widget};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use crate::Route;

#[cfg(target_arch = "wasm32")]
fn browser_push_state(path: &str) {
    if let Some(window) = web_sys::window() {
        let history = window.history().expect("no history");
        let _ = history.push_state_with_url(
            &wasm_bindgen::JsValue::NULL,
            "",
            Some(path),
        );
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_replace_state(path: &str) {
    if let Some(window) = web_sys::window() {
        let history = window.history().expect("no history");
        let _ = history.replace_state_with_url(
            &wasm_bindgen::JsValue::NULL,
            "",
            Some(path),
        );
    }
}

#[cfg(target_arch = "wasm32")]
fn browser_current_path() -> Option<String> {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
}

pub struct Navigator<R>
where
    R: Route,
{
    pub initial_route: R,
    pub routes: fn(R) -> Box<dyn Widget>,
}

impl<R: Route> Navigator<R> {
    pub fn new(initial_route: R, routes: fn(R) -> Box<dyn Widget>) -> Self {
        // On WASM, try to restore the initial route from the browser URL
        #[cfg(target_arch = "wasm32")]
        let initial_route = {
            browser_current_path()
                .and_then(|path| R::parse(&path))
                .unwrap_or(initial_route)
        };
        Self { initial_route, routes }
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
        self.updater.set_state(|state| {
            state.history.push(route);
        });
    }

    pub fn pop(&self) {
        self.updater.set_state(|state| {
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
                if let Some(path) = web_sys::window()
                    .and_then(|w| w.location().pathname().ok())
                {
                    if let Some(route) = R::parse(&path) {
                        updater_clone.set_state(|state| {
                            // Replace the history stack with just this route
                            // (browser already manages the real history)
                            *state.history.last_mut().expect("History should not be empty") = route;
                        });
                    }
                }
            }) as Box<dyn FnMut(web_sys::PopStateEvent)>);

            if let Some(window) = web_sys::window() {
                let _ = window.add_event_listener_with_callback(
                    "popstate",
                    closure.as_ref().unchecked_ref(),
                );
            }

            // Leak the closure so it stays alive for the lifetime of the app
            closure.forget();
        }
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        ctx.insert_state(Rc::new(NavigatorController {
            push_fn: {
                let updater = self.updater.clone();
                Rc::new(move |route: R| {
                    #[cfg(target_arch = "wasm32")]
                    browser_push_state(&route.format());
                    updater.set_state(|state| {
                        state.history.push(route);
                    });
                })
            },
            pop_fn: {
                let updater = self.updater.clone();
                Rc::new(move || {
                    updater.set_state(|state| {
                        if state.history.len() > 1 {
                            state.history.pop();
                            #[cfg(target_arch = "wasm32")]
                            if let Some(prev) = state.history.last() {
                                browser_replace_state(&prev.format());
                            }
                        }
                    });
                })
            },
            can_pop_fn: {
                let history = self.history.clone();
                Rc::new(move || history.len() > 1)
            },
            history_len_fn: {
                let history = self.history.clone();
                Rc::new(move || history.len())
            },
        }));

        (self.routes)(self.history.last().expect("History should not be empty").clone())
    }
}

pub struct NavigatorController<R> {
    push_fn: Rc<dyn Fn(R) >,
    pop_fn: Rc<dyn Fn()>,
    can_pop_fn: Rc<dyn Fn() -> bool>,
    history_len_fn: Rc<dyn Fn() -> usize>,
}


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


pub type NavigatorInstance<R: 'static > = Rc<NavigatorController<R>>;

impl<R: 'static> NavigatorController<R> {
    /// Flutter-style: `Navigator::of(ctx).push(route)`
    #[track_caller]
    pub fn of(ctx: &BuildContext) -> NavigatorInstance<R> {
        ctx.get_state::<NavigatorController<R>>()
            .expect("No Navigator found in context. Make sure a Navigator widget is an ancestor.")
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
        let (el, _) = StatefulElement::new(self, ctx);
        Box::new(el)
    }
}