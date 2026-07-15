use std::rc::Rc;

use aimer_widget::base::BuildContext;
use aimer_widget::{Element, State, StateUpdater, StatefulElement, StatefulWidget, Widget};

use crate::Route;
use crate::outlet::{OutletChildBuilder, OutletSlot};

/// A persistent layout frame (nav bar, drawer, header, ...) that stays mounted
/// while only its inner [`crate::outlet::Outlet`] swaps between child routes.
///
/// Construct a `Shell` with the frame widget — which must contain an `Outlet`
/// somewhere in its tree — and a builder for the currently active child route.
/// When built, the shell injects an [`OutletSlot`] into the context so the
/// descendant outlet can render the child.
pub struct Shell {
    frame: Box<dyn Widget>,
    child_builder: OutletChildBuilder,
}

impl Shell {
    /// Create a shell from a `frame` widget (containing an `Outlet`) and a
    /// closure that builds the active child widget.
    pub fn new(
        frame: impl Widget + 'static,
        child_builder: impl Fn(&BuildContext) -> Box<dyn Widget> + 'static,
    ) -> Self {
        Self { frame: Box::new(frame), child_builder: Rc::new(child_builder) }
    }

    /// The Heap-allocated version of this `Shell::new`.
    /// Create a shell from a `frame` widget (containing an `Outlet`) and a
    /// closure that builds the active child widget.
    pub fn boxing(
        frame: impl Widget + 'static,
        child_builder: impl Fn(&BuildContext) -> Box<dyn Widget> + 'static,
    ) -> Box<dyn Widget> {
        Self::new(frame, child_builder).boxed()
    }

    /// Create a shell whose active child is a fixed widget value.
    pub fn with_child(frame: impl Widget + 'static, child: impl Widget + Clone + 'static) -> Self {
        Self::new(frame, move |_| Box::new(child.clone()))
    }
}

impl Widget for Shell {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        // Make the active child available to the descendant `Outlet`.
        ctx.insert_state(OutletSlot::new(self.child_builder.clone()));
        self.frame.to_element(ctx)
    }

    fn debug_name(&self) -> &'static str {
        "Shell"
    }
}

// ---------------------------------------------------------------------------
// StatefulShell: per-branch history stacks (go_router's StatefulShellRoute).
// ---------------------------------------------------------------------------

/// Push `route` onto branch `index`'s stack (no-op if the index is out of
/// range).
pub fn branch_push<R>(branches: &mut [Vec<R>], index: usize, route: R) {
    if let Some(branch) = branches.get_mut(index) {
        branch.push(route);
    }
}

/// Pop branch `index`'s stack, guarded so a branch stack is never emptied.
pub fn branch_pop<R>(branches: &mut [Vec<R>], index: usize) {
    if let Some(branch) = branches.get_mut(index)
        && branch.len() > 1
    {
        branch.pop();
    }
}

/// The top (active) route of branch `active`, if any.
pub fn active_top<R: Clone>(branches: &[Vec<R>], active: usize) -> Option<R> {
    branches
        .get(active)
        .and_then(|b| b.last().cloned())
}

/// A tabbed shell that keeps an independent navigation stack per branch, so
/// switching branches preserves each branch's history (StatefulShellRoute).
///
/// Only the active branch's top route is rendered into the shell's `Outlet`.
pub struct StatefulShell<R: Route> {
    pub branches: Vec<Vec<R>>,
    pub active: usize,
    pub frame: fn(&BuildContext) -> Box<dyn Widget>,
    pub routes: fn(R) -> Box<dyn Widget>,
}

impl<R: Route> StatefulShell<R> {
    /// Create a stateful shell from one initial route per branch.
    ///
    /// `frame` builds the persistent layout (which must contain an `Outlet`)
    /// and `routes` builds the widget for a given child route.
    pub fn new(
        initial_routes: Vec<R>,
        frame: fn(&BuildContext) -> Box<dyn Widget>,
        routes: fn(R) -> Box<dyn Widget>,
    ) -> Self {
        let branches = initial_routes
            .into_iter()
            .map(|r| vec![r])
            .collect();
        Self { branches, active: 0, frame, routes }
    }
}

pub struct StatefulShellState<R: Route> {
    pub branches: Vec<Vec<R>>,
    pub active: usize,
    pub updater: StateUpdater<Self>,
    pub frame: fn(&BuildContext) -> Box<dyn Widget>,
    pub routes: fn(R) -> Box<dyn Widget>,
}

impl<R: Route> State<StatefulShell<R>> for StatefulShellState<R> {
    fn init_state(&mut self, updater: StateUpdater<Self>) {
        self.updater = updater;
    }

    fn build(&self, ctx: &BuildContext) -> impl Widget {
        // Inject the imperative controller for descendants.
        ctx.insert_state(StatefulShellController::<R> {
            go_branch_fn: {
                let updater = self.updater.clone();
                Rc::new(move |index: usize| {
                    updater.set_state(move |state| state.active = index);
                })
            },
            push_in_branch_fn: {
                let updater = self.updater.clone();
                Rc::new(move |index: usize, route: R| {
                    updater.set_state(move |state| branch_push(&mut state.branches, index, route));
                })
            },
            pop_in_branch_fn: {
                let updater = self.updater.clone();
                Rc::new(move |index: usize| {
                    updater.set_state(move |state| branch_pop(&mut state.branches, index));
                })
            },
            active_branch_fn: {
                let active = self.active;
                Rc::new(move || active)
            },
            branch_len_fn: {
                let branches = self.branches.clone();
                Rc::new(move |index: usize| {
                    branches
                        .get(index)
                        .map(|b| b.len())
                        .unwrap_or(0)
                })
            },
        });

        // Feed only the active branch's top route into the `Outlet`.
        let top = active_top(&self.branches, self.active);
        let routes = self.routes;
        ctx.insert_state(OutletSlot::new(Rc::new(move |_ctx: &BuildContext| {
            let route = top
                .clone()
                .expect("StatefulShell branch stack must not be empty");
            routes(route)
        })));

        // Reflect the active branch's top route in the browser address bar.
        #[cfg(target_arch = "wasm32")]
        if let Some(route) = active_top(&self.branches, self.active) {
            crate::navigator::browser_replace_state(&route.format());
        }

        (self.frame)(ctx)
    }
}

pub struct StatefulShellController<R> {
    go_branch_fn: Rc<dyn Fn(usize)>,
    push_in_branch_fn: Rc<dyn Fn(usize, R)>,
    pop_in_branch_fn: Rc<dyn Fn(usize)>,
    active_branch_fn: Rc<dyn Fn() -> usize>,
    branch_len_fn: Rc<dyn Fn(usize) -> usize>,
}

impl<R> Clone for StatefulShellController<R> {
    fn clone(&self) -> Self {
        Self {
            go_branch_fn: self.go_branch_fn.clone(),
            push_in_branch_fn: self.push_in_branch_fn.clone(),
            pop_in_branch_fn: self.pop_in_branch_fn.clone(),
            active_branch_fn: self.active_branch_fn.clone(),
            branch_len_fn: self.branch_len_fn.clone(),
        }
    }
}

pub type StatefulShellInstance<R> = Rc<StatefulShellController<R>>;

impl<R: 'static> StatefulShellController<R> {
    /// Obtain the controller from the context:
    /// `StatefulShellController::<R>::of(ctx)`.
    #[track_caller]
    pub fn of(ctx: &BuildContext) -> StatefulShellInstance<R> {
        ctx.get_state::<StatefulShellController<R>>()
            .expect("No StatefulShell found in context. Make sure a StatefulShell widget is an ancestor.")
            .clone()
    }

    /// Switch the active branch.
    pub fn go_branch(&self, index: usize) {
        (self.go_branch_fn)(index);
    }

    /// Push a route onto a specific branch's stack.
    pub fn push_in_branch(&self, index: usize, route: R) {
        (self.push_in_branch_fn)(index, route);
    }

    /// Pop a specific branch's stack (guarded so it never empties).
    pub fn pop_in_branch(&self, index: usize) {
        (self.pop_in_branch_fn)(index);
    }

    /// The currently active branch index.
    pub fn active_branch(&self) -> usize {
        (self.active_branch_fn)()
    }

    /// The stack depth of a given branch.
    pub fn branch_len(&self, index: usize) -> usize {
        (self.branch_len_fn)(index)
    }
}

impl<R: Route> StatefulWidget for StatefulShell<R> {
    type State = StatefulShellState<R>;
    fn create_state(&self) -> Self::State {
        StatefulShellState::<R> {
            branches: self.branches.clone(),
            active: self.active,
            updater: StateUpdater::empty(),
            frame: self.frame,
            routes: self.routes,
        }
    }
}

impl<R: Route> Widget for StatefulShell<R> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        let (el, _) = StatefulElement::new(self, ctx);
        Box::new(el)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_push_updates_active_top_and_depth() {
        let mut branches = vec![vec!["home"], vec!["reports"]];
        branch_push(&mut branches, 0, "detail");
        assert_eq!(active_top(&branches, 0), Some("detail"));
        assert_eq!(branches[0].len(), 2);
    }

    #[test]
    fn switching_branch_preserves_each_stack() {
        let mut branches = vec![vec!["a_home"], vec!["b_home"]];
        // Push into branch A.
        branch_push(&mut branches, 0, "a_detail");
        // Switch to branch B (only the active index changes): B keeps its own top.
        assert_eq!(active_top(&branches, 1), Some("b_home"));
        // Switch back to branch A: its top route and depth are restored.
        assert_eq!(active_top(&branches, 0), Some("a_detail"));
        assert_eq!(branches[0].len(), 2);
        assert_eq!(branches[1].len(), 1);
    }

    #[test]
    fn pop_is_guarded_and_never_empties_a_branch() {
        let mut branches = vec![vec!["only"]];
        branch_pop(&mut branches, 0);
        assert_eq!(branches[0].len(), 1);

        branch_push(&mut branches, 0, "second");
        branch_pop(&mut branches, 0);
        assert_eq!(active_top(&branches, 0), Some("only"));
    }

    #[test]
    fn out_of_range_branch_ops_are_noops() {
        let mut branches: Vec<Vec<&str>> = vec![vec!["x"]];
        branch_push(&mut branches, 5, "y");
        branch_pop(&mut branches, 5);
        assert_eq!(active_top(&branches, 5), None);
        assert_eq!(branches[0].len(), 1);
    }
}
