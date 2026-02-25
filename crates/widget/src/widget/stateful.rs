use std::cell::{Cell, RefCell, UnsafeCell};
use std::marker::PhantomPinned;
use std::ops::Deref;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use winit::window::Window;

use crate::{Element, Widget, base::*, components::element::ElementEvent};

/// A `Send + Sync` wrapper around `UnsafeCell<Box<dyn Element>>`.
/// Safety: the rendering pipeline is single-threaded, so concurrent access does not occur.
struct SyncChild(UnsafeCell<Box<dyn Element>>);
unsafe impl Send for SyncChild {}
unsafe impl Sync for SyncChild {}

/// A handle that allows StatefulWidgets to trigger state mutations and rebuilds.
/// This is the Rust equivalent of Flutter's `setState`.
pub struct StateUpdater<S> {
    state: Rc<RefCell<S>>,
    dirty: Rc<Cell<bool>>,
    window: &'static Window,
}

impl<S> Clone for StateUpdater<S> {
    fn clone(&self) -> Self {
        Self { state: self.state.clone(), dirty: self.dirty.clone(), window: self.window }
    }
}

impl<S> StateUpdater<S> {
    /// Create a new `StateUpdater` from shared state and a dirty flag.
    pub fn new(state: Rc<RefCell<S>>, dirty: Rc<Cell<bool>>, window: &'static Window) -> Self {
        Self { state, dirty, window }
    }

    /// Mutate the state and mark the widget as dirty for rebuild.
    /// Similar to Flutter's `setState(() { ... })`.
    pub fn set_state(&self, f: impl FnOnce(&mut S)) {
        let mut state = self.state.borrow_mut();
        f(&mut *state);
        self.dirty.set(true);
        self.window.request_redraw();
    }

    /// Read the current state without marking dirty.
    pub fn read<R>(&self, f: impl FnOnce(&S) -> R) -> R {
        let state = self.state.borrow();
        f(&*state)
    }
}

pub trait StatefulWidget: Sized {
    type State: State<Self>;
    fn create_state(&self) -> Self::State;
}

pub trait State<W: StatefulWidget> {
    /// Called once after the state is created, providing a [`StateUpdater`] handle.
    /// Store the updater in your state struct to later call `set_state()` from
    /// event handlers or callbacks — similar to Flutter's `setState`.
    fn init_state(&mut self, _updater: StateUpdater<Self>)
    where
        Self: Sized,
    {
    }

    fn build(&self) -> impl Widget;
}
pub type RebuildCallBack = dyn Fn(&BuildContext) -> Box<dyn Element>;
pub struct StatefulElement {
    child: SyncChild,
    pub dirty: Rc<Cell<bool>>,
    pub rebuild_fn: Rc<RebuildCallBack>,
}

impl StatefulElement {
    /// Create a new StatefulElement from a StatefulWidget.
    /// Returns the element and a StateUpdater that can be used in callbacks.
    pub fn new<W: StatefulWidget + 'static>(widget: &W, ctx: &BuildContext) -> (Self, StateUpdater<W::State>) {
        let state = widget.create_state();
        let state = Rc::new(RefCell::new(state));


        // Create the updater and pass it into init_state.
        let init_updater = StateUpdater::new(state, Rc::new(Cell::new(false)), ctx.window);
        let dirty = Rc::clone(&init_updater.dirty);
        let dirty_clone = Rc::clone(&init_updater.dirty);
        let state = Rc::clone(&init_updater.state);
        // unsafe{
            state.borrow_mut().init_state(init_updater);
        // }

        let state_for_build = state.clone();
        let rebuild_fn: Rc<RebuildCallBack> = Rc::new(move |ctx| {
            let s = state_for_build.borrow();
            let child_widget = s.build();
            Widget::to_element(&child_widget, ctx)
        });

        let child = {
            Widget::to_element(&state.borrow().build(), ctx)
        };

        let updater = StateUpdater::new(state, dirty, ctx.window);
        // updater.set_window(ctx.window);

        let element = StatefulElement { child: SyncChild(UnsafeCell::new(child)), dirty: dirty_clone, rebuild_fn };

        (element, updater)
    }

    /// Check if this element needs a rebuild and perform it if so.
    pub fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        if self.dirty.get() {
            let new_child = (self.rebuild_fn)(ctx);
            // Safety: single-threaded rendering pipeline
            unsafe {
                *self.child.0.get() = new_child;
            }
            self.dirty.set(false);
        }
    }

    /// Returns true if this element is marked dirty.
    pub fn is_dirty(&self) -> bool {
        self.dirty.get()
    }
}

impl Element for StatefulElement {
    fn draw(&self, ctx: &BuildContext) {
        self.rebuild_if_dirty(ctx);
        // Safety: single-threaded rendering pipeline
        let child = unsafe { &*self.child.0.get() };
        child.draw(ctx);
    }
    fn pos(&self) -> Option<Vec2d> {
        unsafe { &*self.child.0.get() }.pos()
    }
    fn size(&self) -> Option<Size> {
        unsafe { &*self.child.0.get() }.size()
    }
    fn on_event(&self, event: &ElementEvent) -> bool {
        let child = unsafe { &*self.child.0.get() };
        crate::components::element::dispatch_event(
            child.as_ref(),
            match event {
                ElementEvent::PointerDown(p) => *p,
                ElementEvent::PointerUp(p) => *p,
                ElementEvent::PointerMove(p) => *p,
            },
            event,
        )
    }
    fn visit_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Safety: single-threaded rendering pipeline
        let child = unsafe { &*self.child.0.get() };
        visitor(child.as_ref());
    }
    fn event_children<'a>(&'a self, visitor: &mut dyn FnMut(&'a dyn Element)) {
        // Safety: single-threaded rendering pipeline
        let child = unsafe { &*self.child.0.get() };
        visitor(child.as_ref());
    }
    fn computed_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe { &*self.child.0.get() }.computed_size(ctx)
    }
    fn content_size(&self, ctx: &BuildContext) -> ResolvedSize {
        unsafe { &*self.child.0.get() }.content_size(ctx)
    }
    fn get_size_from_child(&self) -> Option<Size> {
        unsafe { &*self.child.0.get() }.get_size_from_child()
    }
    fn invalidate_layout(&self) {
        unsafe { &*self.child.0.get() }.invalidate_layout();
    }
}
