use attribute::position::Vec2d;
use attribute::size::{ResolvedSize, Size};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::cell::UnsafeCell;
use std::panic::Location;
use std::process::exit;
use std::sync::Arc;
use winit::window::Window;

use crate::{base::*, components::element::ElementEvent, Drawable, Element, Widget};

/// A `Send + Sync` wrapper around `UnsafeCell<Box<dyn Element>>`.
/// Safety: the rendering pipeline is single-threaded, so concurrent access does not occur.
struct SyncChild(UnsafeCell<Box<dyn Element>>);
unsafe impl Send for SyncChild {}
unsafe impl Sync for SyncChild {}

/// A handle that allows StatefulWidgets to trigger state mutations and rebuilds.
/// This is the Rust equivalent of Flutter's `setState`.
pub struct StateUpdater<S> {
    inner: Option<StateUpdaterInner<S>>,
}

struct StateUpdaterInner<S> {
    state: Arc<Mutex<S>>,
    dirty: Arc<AtomicBool>,
    window: &'static Window,
}

impl<S> Clone for StateUpdater<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.as_ref().map(|inner| StateUpdaterInner {
                state: inner.state.clone(),
                dirty: inner.dirty.clone(),
                window: inner.window,
            }),
        }
    }
}

impl<S: Send + 'static> StateUpdater<S> {
    /// Create a new `StateUpdater` from shared state and a dirty flag.
    #[inline]
    pub fn new(state: Arc<Mutex<S>>, dirty: Arc<AtomicBool>, window: &'static Window) -> Self {
        Self { inner: Some(StateUpdaterInner { state, dirty, window }) }
    }

    /// Create an empty `StateUpdater` that is not yet initialized.
    /// Calling `set_state` or `read` on an empty updater will panic.
    #[inline]
    pub fn empty() -> Self {
        Self { inner: None }
    }

    /// Mutate the state and mark the widget as dirty for rebuild.
    /// Similar to Flutter's `setState(() { ... })`.
    ///
    /// Multiple calls between frames are coalesced: the dirty flag is set once
    /// and the generation counter is bumped, but only a single rebuild happens
    /// during the next `draw`.
    #[track_caller]
    pub fn set_state(&self, f: impl FnOnce(&mut S)) {
        let inner = match self.inner.as_ref() {
            Some(inner) => inner,
            None => {
                let loc = Location::caller();
                #[cfg(not(target_os = "ios"))]
                self.beautiful_error(loc);
                exit(1);
            }
        };
        {
            let mut state = inner.state.lock().unwrap();
            f(&mut *state);
        }
        // Only request a redraw if this is the first set_state since the last rebuild.
        // This coalesces multiple set_state calls into a single redraw request.
        if !inner.dirty.swap(true, Ordering::Release) {
            inner.window.request_redraw();
        }
    }

    /// Read the current state without marking dirty.
    #[track_caller]
    pub fn read<R>(&self, f: impl FnOnce(&S) -> R) -> R {
        let inner = match  self
            .inner
            .as_ref() {
            Some(inner) => inner,
            None => {
                let loc = Location::caller();
                #[cfg(not(target_os = "ios"))]
                self.beautiful_error(loc);
                exit(1);
            }
        };
        let state = inner.state.lock().unwrap();
        f(&*state)
    }

    #[inline]
    fn beautiful_error(&self, loc:  &Location) {
        #[cfg(not(target_os = "ios"))]
        #[cfg(not(target_arch = "wasm32"))]
        {
            use colored::Colorize;
            const BRACE: &str = "{";
            println!(
                "{}: State is not initialized
  {} {}:{}
   {}
   {} impl State<YourStatefulWidget> for YourWidgetState {BRACE}
   {}
   {}     fn init_state(&mut self, _updater: StateUpdater<Self>)
   {}         where
   {}             Self: Sized,
   {}         {{
   {}             self.updater = _updater;
   {}             {} override this method to set the updater
   {}         }}
   {}
   {}: call `self.updater = _updater` inside `init_state`
",
                "error".red().bold(),
                "-->".blue().bold(),
                loc.file(),
                loc.line(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "|".blue(),
                "              ^^^^^^^^^^^^^^^^^^^^^^^^^".red().bold(),
                // "|".blue(),
                // "|".blue(),
                "help".yellow().bold(),
            );
        }
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
    pub dirty: Arc<AtomicBool>,
    pub rebuild_fn: Arc<RebuildCallBack>,
    /// Monotonically increasing generation counter. Incremented on each rebuild
    /// so that multiple `set_state` calls between frames only trigger one rebuild.
    rebuild_generation: AtomicU64,
    /// The generation at which the last rebuild was performed.
    last_rebuilt_generation: AtomicU64,
}

impl StatefulElement {
    /// Create a new StatefulElement from a StatefulWidget.
    /// Returns the element and a StateUpdater that can be used in callbacks.
    pub fn new<W: StatefulWidget + 'static>(widget: &W, ctx: &BuildContext) -> (Self, StateUpdater<W::State>)
    where
        W::State: Send + Sync + 'static,
    {
        let state = widget.create_state();
        let state = Arc::new(Mutex::new(state));
        let dirty = Arc::new(AtomicBool::new(false));

        // Create the updater and pass it into init_state.
        let init_updater = StateUpdater::new(state.clone(), dirty.clone(), ctx.window);

        {
            let mut s = state.lock().unwrap();
            s.init_state(init_updater.clone());
        }

        let state_for_build = state.clone();
        let rebuild_fn: Arc<RebuildCallBack> = Arc::new(move |ctx| {
            let s = state_for_build.lock().unwrap();
            let child_widget = s.build();
            Widget::to_element(&child_widget, ctx)
        });

        let child = { Widget::to_element(&state.lock().unwrap().build(), ctx) };

        let updater = StateUpdater::new(state, dirty.clone(), ctx.window);

        let element = StatefulElement {
            child: SyncChild(UnsafeCell::new(child)),
            dirty,
            rebuild_fn,
            rebuild_generation: AtomicU64::new(0),
            last_rebuilt_generation: AtomicU64::new(0),
        };

        (element, updater)
    }

    /// Check if this element needs a rebuild and perform it if so.
    ///
    /// Before rebuilding itself, this method first walks the existing child tree
    /// to let any nested `StatefulElement`s rebuild independently. This avoids
    /// destroying and recreating the entire subtree when only a deeply-nested
    /// element's state has changed.
    pub fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        if !self.dirty.load(Ordering::Acquire) {
            // Self is clean — but a nested StatefulElement might be dirty.
            // Propagate rebuild through the existing child tree.
            let child = unsafe { &*self.child.0.get() };
            Self::propagate_rebuild(child.as_ref(), ctx);
            return;
        }

        // Coalesce: only rebuild once per generation bump.
        let current_gen = self.rebuild_generation.load(Ordering::Relaxed);
        let last = self.last_rebuilt_generation.load(Ordering::Relaxed);
        if current_gen == last && !self.dirty.load(Ordering::Acquire) {
            return;
        }

        let new_child = (self.rebuild_fn)(ctx);
        // Safety: single-threaded rendering pipeline
        unsafe {
            *self.child.0.get() = new_child;
        }
        self.dirty.store(false, Ordering::Release);
        self.rebuild_generation.fetch_add(1, Ordering::Relaxed);
        self.last_rebuilt_generation.store(
            self.rebuild_generation.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
    }

    /// Walk the element tree and rebuild any nested dirty `StatefulElement`s.
    /// This is called on the *existing* child tree so that inner stateful widgets
    /// can update in-place without the parent having to reconstruct the whole subtree.
    fn propagate_rebuild(element: &dyn Element, ctx: &BuildContext) {
        element.rebuild_if_dirty(ctx);
    }

    /// Returns true if this element is marked dirty.
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }
}

impl Drawable for StatefulElement {
    fn draw(&self, ctx: &BuildContext) {
        self.rebuild_if_dirty(ctx);
        // Safety: single-threaded rendering pipeline
        let child = unsafe { &*self.child.0.get() };
        child.draw(ctx);
    }
}

impl Element for StatefulElement {

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
                ElementEvent::Scroll(_) => Vec2d::default(),
                ElementEvent::Cancel => Vec2d::default(),
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
    fn rebuild_if_dirty(&self, ctx: &BuildContext) {
        StatefulElement::rebuild_if_dirty(self, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::AssertUnwindSafe;

    #[test]
    fn test_state_updater_empty_panic() {
        let updater: StateUpdater<i32> = StateUpdater::empty();
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            updater.set_state(|_s| {});
        }));
        assert!(result.is_err());

        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            updater.read(|_s| {});
        }));
        assert!(result.is_err());
    }
}
