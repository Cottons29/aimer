use std::sync::Arc;
use widget::base::BuildContext;
use widget::{Element, State, StatefulElement, StateUpdater, StatefulWidget, Widget};
use crate::Route;

pub struct Navigator<R>
where
    R: Route,
{
    pub routes: fn(R) -> Box<dyn Widget>,
}

impl<R: Route> Navigator<R> {
    pub fn new(routes: fn(R) -> Box<dyn Widget>) -> Self {
        Self { routes }
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
        self.updater.set_state(|state| {
            state.history.push(route);
        });
    }

    pub fn pop(&self) {
        self.updater.set_state(|state| {
            if state.history.len() > 1 {
                state.history.pop();
            }
        });
    }
}

impl<R: Route> State<Navigator<R>> for NavigatorState<R> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        ctx.insert_state(Arc::new(NavigatorController {
            push_fn: {
                let updater = self.updater.clone();
                Arc::new(move |route| {
                    updater.set_state(|state| {
                        state.history.push(route);
                    });
                })
            },
            pop_fn: {
                let updater = self.updater.clone();
                Arc::new(move || {
                    updater.set_state(|state| {
                        if state.history.len() > 1 {
                            state.history.pop();
                        }
                    });
                })
            },
        }));

        (self.routes)(self.history.last().expect("History should not be empty").clone())
    }
}

pub struct NavigatorController<R> {
    push_fn: Arc<dyn Fn(R) + Send + Sync>,
    pop_fn: Arc<dyn Fn() + Send + Sync>,
}

impl<R: 'static + Send + Sync> NavigatorController<R> {
    /// Flutter-style: `Navigator::of(ctx).push(route)`
    pub fn of(ctx: &BuildContext) -> Arc<NavigatorController<R>> {
        ctx.get_state::<Arc<NavigatorController<R>>>()
            .expect("No Navigator found in context. Make sure a Navigator widget is an ancestor.")
            .as_ref()
            .clone()
    }

    pub fn push(&self, route: R) {
        (self.push_fn)(route);
    }

    pub fn pop(&self) {
        (self.pop_fn)();
    }
}

impl<R: Route> StatefulWidget for Navigator<R> {
    type State = NavigatorState<R>;
    fn create_state(&self) -> Self::State {
        NavigatorState::<R> {
            history: vec![R::parse("/").expect("Root route '/' not found")],
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