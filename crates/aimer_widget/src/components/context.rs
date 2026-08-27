use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
#[cfg(feature = "portable-guest")]
use std::ops::Deref;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use aimer_attribute::BoxConstraint;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_canvas::Canvas;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Handle;
use winit::window::Window;

use crate::components::element::DirtySource;

/// A canvas available to native builds and deliberately unavailable to
/// portable widget-description builds.
///
/// Portable builds may run ordinary widget `build` methods, but they cannot
/// issue draw commands because no renderer exists in the guest. Dereferencing
/// an unavailable canvas therefore reports the invalid operation rather than
/// silently discarding it.
#[cfg(feature = "portable-guest")]
#[doc(hidden)]
#[derive(Clone)]
pub struct BuildCanvas<'a>(Option<Canvas<'a>>);

#[cfg(feature = "portable-guest")]
impl<'a> BuildCanvas<'a> {
    #[inline]
    fn native(canvas: Canvas<'a>) -> Self {
        Self(Some(canvas))
    }

    #[inline]
    const fn unavailable() -> Self {
        Self(None)
    }
}

#[cfg(feature = "portable-guest")]
impl<'a> Deref for BuildCanvas<'a> {
    type Target = Canvas<'a>;

    #[track_caller]
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap_or_else(|| {
            panic!("drawing is unavailable while building portable Widget IR")
        })
    }
}

/// A Tokio handle that is absent from portable widget-description builds.
#[cfg(all(feature = "portable-guest", not(target_arch = "wasm32")))]
#[doc(hidden)]
#[derive(Clone)]
pub struct BuildAsyncHandle(Option<Handle>);

#[cfg(all(feature = "portable-guest", not(target_arch = "wasm32")))]
impl BuildAsyncHandle {
    #[inline]
    fn native(handle: Handle) -> Self {
        Self(Some(handle))
    }

    #[inline]
    const fn unavailable() -> Self {
        Self(None)
    }
}

#[cfg(all(feature = "portable-guest", not(target_arch = "wasm32")))]
impl Deref for BuildAsyncHandle {
    type Target = Handle;

    #[track_caller]
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap_or_else(|| {
            panic!("asynchronous runtime access is unavailable while building portable Widget IR")
        })
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct HeadlessWindowState {
    width: AtomicU32,
    height: AtomicU32,
    scale_factor: AtomicU64,
    redraw_requested: AtomicBool,
    redraw_request_count: AtomicU64,
    coalesced_redraw_count: AtomicU64,
    display_tick_count: AtomicU64,
    cursor: Mutex<winit::window::CursorIcon>,
}

#[derive(Clone, Debug)]
pub enum WindowHandle {
    Native(&'static Window),
    Headless(Arc<HeadlessWindowState>),
    #[cfg(feature = "portable-guest")]
    Portable(Arc<HeadlessWindowState>),
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
            redraw_request_count: AtomicU64::new(0),
            coalesced_redraw_count: AtomicU64::new(0),
            display_tick_count: AtomicU64::new(0),
            cursor: Mutex::new(winit::window::CursorIcon::Default),
        }))
    }

    #[cfg(feature = "portable-guest")]
    #[doc(hidden)]
    pub(crate) fn portable() -> Self {
        Self::Portable(Arc::new(HeadlessWindowState {
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
            scale_factor: AtomicU64::new(1.0_f64.to_bits()),
            redraw_requested: AtomicBool::new(false),
            redraw_request_count: AtomicU64::new(0),
            coalesced_redraw_count: AtomicU64::new(0),
            display_tick_count: AtomicU64::new(0),
            cursor: Mutex::new(winit::window::CursorIcon::Default),
        }))
    }

    pub fn inner_size(&self) -> winit::dpi::PhysicalSize<u32> {
        match self {
            #[cfg(not(aimer_portable_guest))]
            Self::Native(window) => window.inner_size(),
            #[cfg(aimer_portable_guest)]
            Self::Native(_) => Default::default(),
            Self::Headless(state) => winit::dpi::PhysicalSize::new(
                state.width.load(Ordering::Relaxed),
                state.height.load(Ordering::Relaxed),
            ),
            #[cfg(feature = "portable-guest")]
            Self::Portable(state) => winit::dpi::PhysicalSize::new(
                state.width.load(Ordering::Relaxed),
                state.height.load(Ordering::Relaxed),
            ),
        }
    }

    pub fn scale_factor(&self) -> f64 {
        match self {
            #[cfg(not(aimer_portable_guest))]
            Self::Native(window) => window.scale_factor(),
            #[cfg(aimer_portable_guest)]
            Self::Native(_) => 1.0,
            Self::Headless(state) => f64::from_bits(state.scale_factor.load(Ordering::Relaxed)),
            #[cfg(feature = "portable-guest")]
            Self::Portable(state) => f64::from_bits(state.scale_factor.load(Ordering::Relaxed)),
        }
    }

    pub fn request_redraw(&self) {
        match self {
            #[cfg(not(aimer_portable_guest))]
            Self::Native(window) => window.request_redraw(),
            #[cfg(aimer_portable_guest)]
            Self::Native(_) => {},
            Self::Headless(state) => {
                if state.redraw_requested.swap(true, Ordering::AcqRel) {
                    state.coalesced_redraw_count.fetch_add(1, Ordering::Relaxed);
                } else {
                    state.redraw_request_count.fetch_add(1, Ordering::Relaxed);
                }
            }
            #[cfg(feature = "portable-guest")]
            Self::Portable(_) => {}
        }
    }

    pub fn set_cursor(&self, cursor: winit::window::CursorIcon) {
        match self {
            #[cfg(not(aimer_portable_guest))]
            Self::Native(window) => window.set_cursor(cursor),
            #[cfg(aimer_portable_guest)]
            Self::Native(_) => {},
            Self::Headless(state) => {
                *state
                    .cursor
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = cursor;
            }
            #[cfg(feature = "portable-guest")]
            Self::Portable(_) => {}
        }
    }

    #[doc(hidden)]
    pub fn headless_cursor(&self) -> Option<winit::window::CursorIcon> {
        match self {
            Self::Native(_) => None,
            Self::Headless(state) => Some(
                *state
                    .cursor
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            ),
            #[cfg(feature = "portable-guest")]
            Self::Portable(_) => None,
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
            #[cfg(not(aimer_portable_guest))]
            Self::Native(window) => Some(*window),
            #[cfg(aimer_portable_guest)]
            Self::Native(_) => None,
            Self::Headless(_) => None,
            #[cfg(feature = "portable-guest")]
            Self::Portable(_) => None,
        }
    }

    pub fn update_headless_metrics(&self, size: winit::dpi::PhysicalSize<u32>, scale_factor: f64) {
        match self {
            Self::Headless(state) => {
                state.width.store(size.width, Ordering::Relaxed);
                state.height.store(size.height, Ordering::Relaxed);
                state
                    .scale_factor
                    .store(scale_factor.to_bits(), Ordering::Relaxed);
            }
            #[cfg(feature = "portable-guest")]
            Self::Portable(state) => {
                state.width.store(size.width, Ordering::Relaxed);
                state.height.store(size.height, Ordering::Relaxed);
                state
                    .scale_factor
                    .store(scale_factor.to_bits(), Ordering::Relaxed);
            }
            Self::Native(_) => {}
        }
    }

    pub fn take_redraw_request(&self) -> bool {
        match self {
            Self::Native(_) => false,
            Self::Headless(state) => {
                let pending = state.redraw_requested.swap(false, Ordering::AcqRel);
                if pending {
                    state.display_tick_count.fetch_add(1, Ordering::Relaxed);
                }
                pending
            }
            #[cfg(feature = "portable-guest")]
            Self::Portable(_) => false,
        }
    }

    /// Returns headless redraw-request counters for native acceptance probes.
    #[doc(hidden)]
    pub fn headless_redraw_request_counts(&self) -> Option<(u64, u64, u64)> {
        match self {
            Self::Headless(state) => Some((
                state.redraw_request_count.load(Ordering::Relaxed),
                state.coalesced_redraw_count.load(Ordering::Relaxed),
                state.display_tick_count.load(Ordering::Relaxed),
            )),
            Self::Native(_) => None,
            #[cfg(feature = "portable-guest")]
            Self::Portable(_) => None,
        }
    }
}

#[derive(Clone)]
pub struct BuildContext<'a> {
    pub parent_size: ResolvedSize,
    #[cfg(not(feature = "portable-guest"))]
    pub canvas: Canvas<'a>,
    #[cfg(feature = "portable-guest")]
    pub canvas: BuildCanvas<'a>,
    pub scale: f32,
    pub parent_pos: Vec2d,
    pub cursor_pos: Vec2d,
    pub box_constraint: BoxConstraint,
    pub visible_rect: Option<(f32, f32, f32, f32)>, // (x, y, width, height)
    pub window: WindowHandle,
    #[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
    pub async_handle: Handle,
    #[cfg(all(not(target_arch = "wasm32"), feature = "portable-guest"))]
    pub async_handle: BuildAsyncHandle,
    pub inherited_states: Rc<RefCell<HashMap<TypeId, Rc<dyn Any>>>>,
}

#[doc(hidden)]
pub struct BuildConsumer {
    dirty_source: Rc<DirtySource>,
    cleanups: RefCell<Vec<Box<dyn FnOnce()>>>,
    dependencies: RefCell<HashSet<usize>>,
}

impl BuildConsumer {
    pub fn new(dirty: Rc<Cell<bool>>) -> Rc<Self> {
        Rc::new(Self {
            dirty_source: DirtySource::new(dirty.clone()),
            cleanups: RefCell::new(Vec::new()),
            dependencies: RefCell::new(HashSet::new()),
        })
    }

    pub(crate) fn dirty_source(&self) -> Rc<DirtySource> {
        self.dirty_source.clone()
    }

    pub(crate) fn begin_build(&self) {
        self.dependencies.borrow_mut().clear();
        for cleanup in self.cleanups.borrow_mut().drain(..) {
            cleanup();
        }
    }

    pub fn add_cleanup(&self, cleanup: impl FnOnce() + 'static) {
        self.cleanups.borrow_mut().push(Box::new(cleanup));
    }

    pub fn register_dependency(&self, identity: usize) -> bool {
        self.dependencies.borrow_mut().insert(identity)
    }

    pub fn mark_needs_rebuild(&self) {
        self.dirty_source.mark();
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
    states: Rc<RefCell<HashMap<TypeId, Rc<dyn Any>>>>,
    type_id: TypeId,
    previous: Option<Rc<dyn Any>>,
}

impl Drop for StateScopeGuard {
    fn drop(&mut self) {
        let mut states = self.states.borrow_mut();
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
            #[cfg(not(feature = "portable-guest"))]
            canvas,
            #[cfg(feature = "portable-guest")]
            canvas: BuildCanvas::native(canvas),
            parent_size: size,
            scale,
            parent_pos,
            cursor_pos,
            box_constraint: BoxConstraint::default(),
            visible_rect: None,
            window,
            #[cfg(all(not(target_arch = "wasm32"), not(feature = "portable-guest")))]
            async_handle,
            #[cfg(all(not(target_arch = "wasm32"), feature = "portable-guest"))]
            async_handle: BuildAsyncHandle::native(async_handle),
            inherited_states: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Returns whether a known rectangle in this context can overlap the
    /// current viewport.
    ///
    /// The rectangle and [`BuildContext::visible_rect`] use the same local
    /// coordinates. A missing viewport, an invalid rectangle, or invalid
    /// viewport dimensions is treated conservatively as visible so unknown
    /// bounds are never under-culled. Rectangles that only touch at an edge
    /// are also visible, matching the canvas clipping convention.
    #[inline]
    pub fn is_rect_visible(&self, x: f32, y: f32, width: f32, height: f32) -> bool {
        let Some((vx, vy, vw, vh)) = self.visible_rect else {
            return true;
        };
        if !x.is_finite()
            || !y.is_finite()
            || !width.is_finite()
            || !height.is_finite()
            || width < 0.0
            || height < 0.0
            || !vx.is_finite()
            || !vy.is_finite()
            || !vw.is_finite()
            || !vh.is_finite()
            || vw < 0.0
            || vh < 0.0
        {
            return true;
        }

        x + width >= vx && x <= vx + vw && y + height >= vy && y <= vy + vh
    }

    /// Creates the resource-free context passed to portable `build` methods.
    ///
    /// Layout-independent build logic, inherited state, and deterministic
    /// platform defaults remain available. Drawing and native asynchronous
    /// work fail explicitly because a portable guest owns neither resource.
    #[cfg(feature = "portable-guest")]
    #[inline]
    pub fn portable() -> BuildContext<'static> {
        Self::portable_with_inherited_states(Rc::new(RefCell::new(HashMap::new())))
    }

    /// Creates a portable build context backed by an existing inherited-state
    /// scope.
    ///
    /// Generated guest lowering uses this constructor so each nested `build`
    /// sees the same ambient provider map. The map is still scoped by the
    /// lowering operation; native handles and subscriptions remain outside the
    /// guest ABI.
    #[doc(hidden)]
    #[cfg(feature = "portable-guest")]
    #[inline]
    pub fn portable_with_inherited_states(
        inherited_states: Rc<RefCell<HashMap<TypeId, Rc<dyn Any>>>>,
    ) -> BuildContext<'static> {
        Self::portable_with_window(inherited_states, WindowHandle::portable())
    }

    #[doc(hidden)]
    #[cfg(feature = "portable-guest")]
    #[inline]
    pub(crate) fn portable_with_window(
        inherited_states: Rc<RefCell<HashMap<TypeId, Rc<dyn Any>>>>,
        window: WindowHandle,
    ) -> BuildContext<'static> {
        BuildContext {
            parent_size: ResolvedSize::default(),
            canvas: BuildCanvas::unavailable(),
            scale: 1.0,
            parent_pos: Vec2d::default(),
            cursor_pos: Vec2d::default(),
            box_constraint: BoxConstraint::default(),
            visible_rect: None,
            window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle: BuildAsyncHandle::unavailable(),
            inherited_states,
        }
    }

    /// Returns whether this context belongs to a portable Widget IR build.
    #[cfg(feature = "portable-guest")]
    #[inline]
    pub const fn is_portable(&self) -> bool {
        matches!(self.window, WindowHandle::Portable(_))
    }

    pub fn insert_state<T: Any>(&self, state: T) {
        self.inherited_states
            .borrow_mut()
            .insert(TypeId::of::<T>(), Rc::new(state));
    }

    pub fn get_state<T: Any>(&self) -> Option<Rc<T>> {
        self.inherited_states
            .borrow()
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }

    pub fn with_state<T: Any, R>(&self, state: T, callback: impl FnOnce(&Self) -> R) -> R {
        let type_id = TypeId::of::<T>();
        let previous = self
            .inherited_states
            .borrow_mut()
            .insert(type_id, Rc::new(state));
        let _guard = StateScopeGuard {
            states: self.inherited_states.clone(),
            type_id,
            previous,
        };
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

    /// Reads the window metrics and rebuilds the widget currently building
    /// whenever they change.
    ///
    /// This is the only way to observe the window from a `build` and stay
    /// correct: a resize rebuilds the widgets that registered here and nobody
    /// else, because rebuilding a tree that cannot have changed is what makes a
    /// window drag stutter. Reading [`BuildContext::window`] directly registers
    /// nothing, so a widget that lays itself out differently on a phone-sized
    /// window would keep the layout it was first built with.
    ///
    /// Outside a build — from layout, drawing or an event handler — there is
    /// nothing to register and the metrics are simply returned; such code reads
    /// them again on the next frame anyway.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn build(&self, ctx: &BuildContext) -> impl Widget {
    ///     let compact = ctx.watch_window_metrics().logical_size().width < 600.0;
    ///     // ... this widget is rebuilt when that answer can change
    /// }
    /// ```
    pub fn watch_window_metrics(&self) -> crate::WindowMetrics {
        if let Some(consumer) = self.current_build_consumer() {
            crate::window_metrics::subscribe(&consumer, &self.window);
        }
        crate::WindowMetrics::of(&self.window)
    }

    /// Reads the appearance the platform asks for and rebuilds the widget
    /// currently building whenever the user switches it.
    ///
    /// Desktop and mobile systems switch between light and dark appearance
    /// while an application is running, so a widget that chooses colors from
    /// the system appearance has to be told when the choice changes; reading
    /// [`crate::platform_brightness`] directly registers nothing and leaves the
    /// widget with the appearance it was first built with.
    ///
    /// Outside a build there is nothing to register and the appearance is
    /// simply returned.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn build(&self, ctx: &BuildContext) -> impl Widget {
    ///     let brightness = ctx.watch_platform_brightness();
    ///     // ... this widget is rebuilt when the system appearance changes
    /// }
    /// ```
    pub fn watch_platform_brightness(&self) -> crate::Brightness {
        if let Some(consumer) = self.current_build_consumer() {
            crate::platform_brightness::subscribe(&consumer);
        }
        crate::platform_brightness()
    }

    /// Reads the region the system reserves along the window edges and rebuilds
    /// the widget currently building whenever it changes.
    ///
    /// The status bar, the notch and the home indicator sit *over* the
    /// application's surface, and a rotation moves them; a widget that keeps
    /// something interactive out of that region has to be told when the region
    /// moves. Reading [`crate::safe_area_insets`] directly registers nothing.
    ///
    /// Outside a build there is nothing to register and the insets are simply
    /// returned.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn build(&self, ctx: &BuildContext) -> impl Widget {
    ///     let insets = ctx.watch_safe_area_insets();
    ///     // ... this widget is rebuilt when the status bar moves
    /// }
    /// ```
    pub fn watch_safe_area_insets(&self) -> crate::SafeAreaInsets {
        if let Some(consumer) = self.current_build_consumer() {
            crate::safe_area::subscribe(&consumer);
        }
        crate::safe_area_insets()
    }

    /// Answers one question about the window and rebuilds the widget currently
    /// building only when that answer changes.
    ///
    /// Almost no widget depends on the window itself; it depends on something
    /// derived from it — a breakpoint, a column count — whose answer changes
    /// once in a whole drag, if at all. Registering the question instead of the
    /// metrics is the difference between rebuilding on every pixel and
    /// rebuilding when the layout genuinely differs.
    ///
    /// Each call registers separately, so a build may ask several independent
    /// questions.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn build(&self, ctx: &BuildContext) -> impl Widget {
    ///     // Rebuilt when the window crosses 600, and at no other width.
    ///     let compact = ctx.select_window_metrics(|window| {
    ///         window.logical_size().width < 600.0
    ///     });
    /// }
    /// ```
    pub fn select_window_metrics<T: Clone + PartialEq + 'static>(
        &self,
        selector: impl Fn(&crate::WindowMetrics) -> T + 'static,
    ) -> T {
        match self.current_build_consumer() {
            Some(consumer) => {
                crate::window_metrics::subscribe_selected(&consumer, &self.window, selector)
            }
            None => selector(&crate::WindowMetrics::of(&self.window)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;

    #[test]
    fn headless_redraw_requests_coalesce_until_the_next_display_tick() {
        let window = WindowHandle::headless(Default::default(), 1.0);

        window.request_redraw();
        window.request_redraw();
        window.request_redraw();

        assert_eq!(
            window.headless_redraw_request_counts(),
            Some((1, 2, 0))
        );
        assert!(window.take_redraw_request());
        assert_eq!(
            window.headless_redraw_request_counts(),
            Some((1, 2, 1))
        );

        window.request_redraw();
        assert_eq!(
            window.headless_redraw_request_counts(),
            Some((2, 2, 1))
        );
    }

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

    #[cfg(feature = "portable-guest")]
    #[test]
    fn portable_context_builds_stateless_widgets_without_native_resources() {
        struct Probe;

        impl crate::StatelessWidget for Probe {
            fn build(&self, ctx: &BuildContext) -> impl crate::Widget {
                assert_eq!(ctx.scale, 1.0);
                assert_eq!(ctx.parent_size, ResolvedSize::default());
                crate::ErrorWidget::new("portable probe")
            }
        }

        let context = BuildContext::portable();
        let _widget = crate::StatelessWidget::build(&Probe, &context);
        assert!(context.is_portable());
    }

    #[cfg(feature = "portable-guest")]
    #[test]
    fn portable_context_rejects_drawing_instead_of_silently_ignoring_it() {
        let context = BuildContext::portable();
        let result = catch_unwind(AssertUnwindSafe(|| context.canvas.save()));

        assert!(result.is_err());
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
    fn rect_visibility_is_conservative_and_edge_inclusive() {
        let mut context = context();
        assert!(context.is_rect_visible(10.0, 10.0, 10.0, 10.0));

        context.visible_rect = Some((0.0, 0.0, 100.0, 100.0));
        assert!(context.is_rect_visible(100.0, 20.0, 10.0, 10.0));
        assert!(!context.is_rect_visible(101.0, 20.0, 10.0, 10.0));
        assert!(!context.is_rect_visible(20.0, 101.0, 10.0, 10.0));

        assert!(context.is_rect_visible(f32::NAN, 0.0, 10.0, 10.0));
        context.visible_rect = Some((0.0, 0.0, f32::NAN, 100.0));
        assert!(context.is_rect_visible(200.0, 200.0, 10.0, 10.0));
    }

    #[test]
    fn build_consumer_cleans_previous_dependencies_before_rebuild() {
        let context = context();
        let dirty = Rc::new(Cell::new(false));
        let consumer = BuildConsumer::new(dirty.clone());
        let cleanup_count = Rc::new(Cell::new(0));

        context.with_build_consumer(consumer.clone(), |context| {
            let current = context.current_build_consumer().unwrap();
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
