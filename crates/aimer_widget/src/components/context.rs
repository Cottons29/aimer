use std::collections::HashMap;
use std::any::{Any, TypeId};
use std::sync::{Arc, RwLock};
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Handle;
use winit::window::Window;
use aimer_attribute::BoxConstraint;
use aimer_canvas::{Canvas};

#[derive(Clone)]
pub struct BuildContext<'a> {
    pub parent_size: ResolvedSize,
    pub canvas:  Canvas<'a>,
    pub scale: f32,
    pub parent_pos: Vec2d,
    pub cursor_pos: Vec2d,
    pub box_constraint: BoxConstraint,
    pub visible_rect: Option<(f32, f32, f32, f32)>, // (x, y, width, height)
    pub window: &'static Window,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_handle: Handle,
    pub inherited_states: Arc<RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>>,
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
        window: &'static Window,
        #[cfg(not(target_arch = "wasm32"))]
        async_handle: Handle,
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
            inherited_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn insert_state<T: Any + Send + Sync>(&self, state: T) {
        self.inherited_states.write().unwrap().insert(TypeId::of::<T>(), Arc::new(state));
    }

    pub fn get_state<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.inherited_states
            .read().unwrap()
            .get(&TypeId::of::<T>())
            .and_then(|arc| arc.clone().downcast::<T>().ok())
    }
}
