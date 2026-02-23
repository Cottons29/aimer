use std::cell::UnsafeCell;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

use winit::window::Window;

use crate::{base::*, Element, Widget, components::element::ElementEvent};

/// A `Send + Sync` wrapper around `UnsafeCell<Box<dyn Element>>`.
/// Safety: the rendering pipeline is single-threaded, so concurrent access does not occur.
struct SyncChild(UnsafeCell<Box<dyn Element>>);
unsafe impl Send for SyncChild {}
unsafe impl Sync for SyncChild {}

/// A handle that allows StatefulWidgets to trigger state mutations and rebuilds.
/// This is the Rust equivalent of Flutter's `setState`.
pub struct StateUpdater<S> {
    state: Arc<Mutex<S>>,
    dirty: Arc<AtomicBool>,
    window: Option<&'static Window>,
}

impl<S> Clone for StateUpdater<S> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            dirty: self.dirty.clone(),
            window: self.window,
        }
    }
}

impl<S> StateUpdater<S> {
    /// Create a new `StateUpdater` from shared state and a dirty flag.
    pub fn new(state: Arc<Mutex<S>>, dirty: Arc<AtomicBool>) -> Self {
        Self { state, dirty, window: None }
    }

    /// Set the window reference so `set_state` can request a redraw immediately.
    pub fn set_window(&mut self, window: &'static Window) {
        self.window = Some(window);
    }

    /// Mutate the state and mark the widget as dirty for rebuild.
    /// Similar to Flutter's `setState(() { ... })`.
    pub fn set_state(&self, f: impl FnOnce(&mut S)) {
        {
            let mut state = self.state.lock().unwrap();
            f(&mut *state);
        }
        self.dirty.store(true, Ordering::Relaxed);
        if let Some(window) = self.window {
            window.request_redraw();
        }
    }

    /// Read the current state without marking dirty.
    pub fn read<R>(&self, f: impl FnOnce(&S) -> R) -> R {
        let state = self.state.lock().unwrap();
        f(&*state)
    }
}

pub trait StatefulWidget: Sized + Send + Sync {
    type State: State<Self>;
    fn create_state(&self) -> Self::State;
}

pub trait State<W: StatefulWidget>: Send + Sync + 'static {
    /// Called once after the state is created, providing a [`StateUpdater`] handle.
    /// Store the updater in your state struct to later call `set_state()` from
    /// event handlers or callbacks — similar to Flutter's `setState`.
    fn init_state(&mut self, _updater: StateUpdater<Self>) where Self: Sized {}

    fn build(&self) -> impl Widget;
}
pub type RebuildCallBack = dyn Fn(&BuildContext) -> Box<dyn Element> + Send + Sync;
pub struct StatefulElement {
    child: SyncChild,
    pub dirty: Arc<AtomicBool>,
    pub rebuild_fn: Arc<RebuildCallBack>,
}

impl StatefulElement {
    /// Create a new StatefulElement from a StatefulWidget.
    /// Returns the element and a StateUpdater that can be used in callbacks.
    pub fn new<W: StatefulWidget + 'static>(widget: &W, ctx: &BuildContext) -> (Self, StateUpdater<W::State>) {
        let state = widget.create_state();
        let dirty = Arc::new(AtomicBool::new(false));

        let state = Arc::new(Mutex::new(state));

        // Create the updater and pass it into init_state.
        let mut init_updater = StateUpdater::new(state.clone(), dirty.clone());
        if let Some(window) = ctx.window {
            init_updater.set_window(window);
        }
        {
            let mut s = state.lock().unwrap();
            s.init_state(init_updater);
        }

        let state_for_build = state.clone();
        let rebuild_fn: Arc<RebuildCallBack> = Arc::new(move |ctx| {
            let s = state_for_build.lock().unwrap();
            let child_widget = s.build();
            Widget::to_element(&child_widget, ctx)
        });

        let child = {
            let s = state.lock().unwrap();
            let child_widget = s.build();
            Widget::to_element(&child_widget, ctx)
        };

        let mut updater = StateUpdater::new(state, dirty.clone());
        if let Some(window) = ctx.window {
            updater.set_window(window);
        }

        let element = StatefulElement {
            child: SyncChild(UnsafeCell::new(child)),
            dirty,
            rebuild_fn,
        };

        (element, updater)
    }

    /// Check if this element needs a rebuild and perform it if so.
    pub fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        if self.dirty.swap(false, Ordering::Relaxed) {
            let new_child = (self.rebuild_fn)(ctx);
            // Safety: single-threaded rendering pipeline
            unsafe { *self.child.0.get() = new_child; }
        }
    }

    /// Returns true if this element is marked dirty.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }
}

impl Element for StatefulElement {
    fn draw(&self, ctx: &BuildContext) {
        self.rebuild_if_dirty(ctx);
        // Safety: single-threaded rendering pipeline
        let child = unsafe { &*self.child.0.get() };
        child.draw(ctx);
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
    fn on_event(&self, event: &ElementEvent) -> bool {
        let child = unsafe { &*self.child.0.get() };
        crate::components::element::dispatch_event(child.as_ref(), match event {
            ElementEvent::PointerDown(p) => *p,
            ElementEvent::PointerUp(p) => *p,
            ElementEvent::PointerMove(p) => *p,
        }, event)
    }
    fn pos(&self) -> Option<Vec2d> {
        unsafe { &*self.child.0.get() }.pos()
    }
    fn size(&self) -> Option<Size> {
        unsafe { &*self.child.0.get() }.size()
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
