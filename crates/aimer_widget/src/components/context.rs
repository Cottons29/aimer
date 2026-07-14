use std::any::{Any, TypeId};
use std::collections::HashMap;
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
                state
                    .width
                    .load(Ordering::Relaxed),
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
            .and_then(|arc| {
                arc.clone()
                    .downcast::<T>()
                    .ok()
            })
    }
}
