use std::marker::PhantomData;
use std::sync::Arc;
use widget::base::BuildContext;
use widget::{Element, State, StatefulElement, StateUpdater, StatefulWidget, Widget};
use crate::RouteParser;

pub struct Navigator<R, P>
where
    R: 'static + Send + Sync + Clone,
    P: RouteParser<R> + 'static + Send + Sync,
{
    pub routes: fn(R) -> Box<dyn Widget>,
    _p: PhantomData<P>,
}

impl<R, P> Navigator<R, P>
where
    R: 'static + Send + Sync + Clone,
    P: RouteParser<R> + 'static + Send + Sync,
{
    pub fn new(routes: fn(R) -> Box<dyn Widget>) -> Self {
        Self {
            routes,
            _p: PhantomData,
        }
    }
}

pub struct NavigatorState<R, P>
where
    R: 'static + Send + Sync + Clone,
    P: RouteParser<R> + 'static + Send + Sync,
{
    pub history: Vec<R>,
    pub updater: StateUpdater<Self>,
    pub routes: fn(R) -> Box<dyn Widget>,
    _p: PhantomData<P>,
}

impl<R, P> NavigatorState<R, P>
where
    R: 'static + Send + Sync + Clone,
    P: RouteParser<R> + 'static + Send + Sync,
{
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

impl<R, P> State<Navigator<R, P>> for NavigatorState<R, P>
where
    R: 'static + Send + Sync + Clone,
    P: RouteParser<R> + 'static + Send + Sync,
{
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> Box<dyn Widget> {
        let mut ctx = ctx.clone();
        ctx.insert_state(Arc::new(NavigatorController {
            push: {
                let updater = self.updater.clone();
                Arc::new(move |route| {
                    updater.set_state(|state| {
                        state.history.push(route);
                    });
                })
            },
            pop: {
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
    pub push: Arc<dyn Fn(R) + Send + Sync>,
    pub pop: Arc<dyn Fn() + Send + Sync>,
}

impl<R, P> StatefulWidget for Navigator<R, P>
where
    R: 'static + Send + Sync + Clone,
    P: RouteParser<R> + 'static + Send + Sync,
{
    type State = NavigatorState<R, P>;
    fn create_state(&self) -> Self::State {
        NavigatorState::<R, P> {
            history: vec![P::parse("/")],
            updater: StateUpdater::empty(),
            routes: self.routes,
            _p: PhantomData,
        }
    }
}

impl<R, P> Widget for Navigator<R, P>
where
    R: 'static + Send + Sync + Clone,
    P: RouteParser<R> + 'static + Send + Sync,
{
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let (el, _) = StatefulElement::new(self, ctx);
        Box::new(el)
    }
}