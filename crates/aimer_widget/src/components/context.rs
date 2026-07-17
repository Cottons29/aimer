use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use aimer_attribute::BoxConstraint;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_canvas::Canvas;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Handle;
use winit::window::Window;

#[doc(hidden)]
#[derive(Debug)]
pub struct HeadlessWindowState {
    width: AtomicU32,
    height: AtomicU32,
    scale_factor: AtomicU64,
    redraw_requested: AtomicBool,
}

#[derive(Clone, Debug)]
pub enum WindowHandle {
    Native(&'static Window),
    Headless(Arc<HeadlessWindowState>),
}

impl WindowHandle {
    pub fn native(window: &'static Window) -> Self {
        Self::Native(window)
    }

    pub fn headless(size: winit::dpi::PhysicalSize<u32>, scale_factor: f64) -> Self {
        Self::Headless(Arc::new(HeadlessWindowState {
            width: AtomicU32::new(size.width),
            height: AtomicU32::new(size.height),
            scale_factor: AtomicU64::new(scale_factor.to_bits()),
            redraw_requested: AtomicBool::new(false),
        }))
    }

    pub fn inner_size(&self) -> winit::dpi::PhysicalSize<u32> {
        match self {
            Self::Native(window) => window.inner_size(),
            Self::Headless(state) => winit::dpi::PhysicalSize::new(
                state.width.load(Ordering::Relaxed),
                state
                    .height
                    .load(Ordering::Relaxed),
            ),
        }
    }

    pub fn scale_factor(&self) -> f64 {
        match self {
            Self::Native(window) => window.scale_factor(),
            Self::Headless(state) => f64::from_bits(
                state
                    .scale_factor
                    .load(Ordering::Relaxed),
            ),
        }
    }

    pub fn request_redraw(&self) {
        match self {
            Self::Native(window) => window.request_redraw(),
            Self::Headless(state) => state
                .redraw_requested
                .store(true, Ordering::Release),
        }
    }

    pub fn set_cursor(&self, cursor: winit::window::CursorIcon) {
        if let Self::Native(window) = self {
            window.set_cursor(cursor);
        }
    }

    pub fn set_text_cursor(&self) {
        self.set_cursor(winit::window::CursorIcon::Text);
    }

    pub fn set_pointer_cursor(&self) {
        self.set_cursor(winit::window::CursorIcon::Pointer);
    }

    pub fn reset_cursor(&self) {
        self.set_cursor(winit::window::CursorIcon::Default);
    }

    pub fn native_window(&self) -> Option<&'static Window> {
        match self {
            Self::Native(window) => Some(*window),
            Self::Headless(_) => None,
        }
    }

    pub fn update_headless_metrics(&self, size: winit::dpi::PhysicalSize<u32>, scale_factor: f64) {
        if let Self::Headless(state) = self {
            state
                .width
                .store(size.width, Ordering::Relaxed);
            state
                .height
                .store(size.height, Ordering::Relaxed);
            state
                .scale_factor
                .store(scale_factor.to_bits(), Ordering::Relaxed);
        }
    }

    pub fn take_redraw_request(&self) -> bool {
        match self {
            Self::Native(_) => false,
            Self::Headless(state) => state
                .redraw_requested
                .swap(false, Ordering::AcqRel),
        }
    }
}

#[derive(Clone)]
pub struct BuildContext<'a> {
    pub parent_size: ResolvedSize,
    pub canvas: Canvas<'a>,
    pub scale: f32,
    pub parent_pos: Vec2d,
    pub cursor_pos: Vec2d,
    pub box_constraint: BoxConstraint,
    pub visible_rect: Option<(f32, f32, f32, f32)>, // (x, y, width, height)
    pub window: WindowHandle,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_handle: Handle,
    pub inherited_states: Rc<RwLock<HashMap<TypeId, Rc<dyn Any>>>>,
}

#[doc(hidden)]
pub struct BuildConsumer {
    dirty: Rc<Cell<bool>>,
    cleanups: RefCell<Vec<Box<dyn FnOnce()>>>,
    dependencies: RefCell<HashSet<usize>>,
}

impl BuildConsumer {
    pub fn new(dirty: Rc<Cell<bool>>) -> Rc<Self> {
        Rc::new(Self {
            dirty,
            cleanups: RefCell::new(Vec::new()),
            dependencies: RefCell::new(HashSet::new()),
        })
    }

    fn begin_build(&self) {
        self.dependencies
            .borrow_mut()
            .clear();
        for cleanup in self
            .cleanups
            .borrow_mut()
            .drain(..)
        {
            cleanup();
        }
    }

    pub fn add_cleanup(&self, cleanup: impl FnOnce() + 'static) {
        self.cleanups
            .borrow_mut()
            .push(Box::new(cleanup));
    }

    pub fn register_dependency(&self, identity: usize) -> bool {
        self.dependencies
            .borrow_mut()
            .insert(identity)
    }

    pub fn mark_needs_rebuild(&self) {
        self.dirty.set(true);
    }
}

impl Drop for BuildConsumer {
    fn drop(&mut self) {
        for cleanup in self.cleanups.get_mut().drain(..) {
            cleanup();
        }
    }
}

#[derive(Clone)]
struct CurrentBuildConsumer(Rc<BuildConsumer>);

struct StateScopeGuard {
    states: Rc<RwLock<HashMap<TypeId, Rc<dyn Any>>>>,
    type_id: TypeId,
    previous: Option<Rc<dyn Any>>,
}

impl Drop for StateScopeGuard {
    fn drop(&mut self) {
        let mut states = self.states.write().unwrap();
        if let Some(previous) = self.previous.take() {
            states.insert(self.type_id, previous);
        } else {
            states.remove(&self.type_id);
        }
    }
}

impl<'a> std::fmt::Debug for BuildContext<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuildContext")
            .field("parent_size", &self.parent_size)
            .field("scale", &self.scale)
            .field("parent_pos", &self.parent_pos)
            .field("cursor_pos", &self.cursor_pos)
            .field("box_constraint", &self.box_constraint)
            .finish()
    }
}

impl<'a> BuildContext<'a> {
    pub fn new(
        canvas: Canvas<'a>,
        size: ResolvedSize,
        scale: f32,
        parent_pos: Vec2d,
        cursor_pos: Vec2d,
        window: WindowHandle,
        #[cfg(not(target_arch = "wasm32"))] async_handle: Handle,
    ) -> Self {
        Self {
            canvas,
            parent_size: size,
            scale,
            parent_pos,
            cursor_pos,
            box_constraint: BoxConstraint::default(),
            visible_rect: None,
            window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle,
            inherited_states: Rc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn insert_state<T: Any>(&self, state: T) {
        self.inherited_states
            .write()
            .unwrap()
            .insert(TypeId::of::<T>(), Rc::new(state));
    }

    pub fn get_state<T: Any>(&self) -> Option<Rc<T>> {
        self.inherited_states
            .read()
            .unwrap()
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }

    pub fn with_state<T: Any, R>(&self, state: T, callback: impl FnOnce(&Self) -> R) -> R {
        let type_id = TypeId::of::<T>();
        let previous = self
            .inherited_states
            .write()
            .unwrap()
            .insert(type_id, Rc::new(state));
        let _guard = StateScopeGuard { states: self.inherited_states.clone(), type_id, previous };
        callback(self)
    }

    #[doc(hidden)]
    pub fn with_build_consumer<R>(
        &self,
        consumer: Rc<BuildConsumer>,
        callback: impl FnOnce(&Self) -> R,
    ) -> R {
        consumer.begin_build();
        self.with_state(CurrentBuildConsumer(consumer), callback)
    }

    #[doc(hidden)]
    pub fn current_build_consumer(&self) -> Option<Rc<BuildConsumer>> {
        self.get_state::<CurrentBuildConsumer>()
            .map(|consumer| consumer.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    fn dummy_async_handle() -> tokio::runtime::Handle {
        use std::sync::OnceLock;

        static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        let runtime = RUNTIME.get_or_init(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
        });
        let _guard = runtime.enter();
        tokio::runtime::Handle::current()
    }

    fn context() -> BuildContext<'static> {
        let canvas = {
            let inner = Box::leak(Box::new(aimer_canvas::InnerCanvas::new()));
            aimer_canvas::Canvas::new(inner)
        };
        BuildContext::new(
            canvas,
            Default::default(),
            1.0,
            Default::default(),
            Default::default(),
            WindowHandle::headless(Default::default(), 1.0),
            #[cfg(not(target_arch = "wasm32"))]
            dummy_async_handle(),
        )
    }

    #[test]
    fn scoped_state_shadows_and_restores_the_outer_value() {
        let context = context();
        context.insert_state(1_u32);

        context.with_state(2_u32, |context| {
            assert_eq!(*context.get_state::<u32>().unwrap(), 2);
        });

        assert_eq!(*context.get_state::<u32>().unwrap(), 1);
    }

    #[test]
    fn scoped_state_is_restored_after_a_panic() {
        let context = context();
        context.insert_state(1_u32);

        let result = catch_unwind(AssertUnwindSafe(|| {
            context.with_state(2_u32, |_| panic!("stop"));
        }));

        assert!(result.is_err());
        assert_eq!(*context.get_state::<u32>().unwrap(), 1);
    }

    #[test]
    fn build_consumer_cleans_previous_dependencies_before_rebuild() {
        let context = context();
        let dirty = Rc::new(Cell::new(false));
        let consumer = BuildConsumer::new(dirty.clone());
        let cleanup_count = Rc::new(Cell::new(0));

        context.with_build_consumer(consumer.clone(), |context| {
            let current = context
                .current_build_consumer()
                .unwrap();
            let cleanup_count = cleanup_count.clone();
            current.add_cleanup(move || cleanup_count.set(cleanup_count.get() + 1));
        });
        assert_eq!(cleanup_count.get(), 0);

        context.with_build_consumer(consumer.clone(), |_| {});

        assert_eq!(cleanup_count.get(), 1);
        consumer.mark_needs_rebuild();
        assert!(dirty.get());
    }
}
