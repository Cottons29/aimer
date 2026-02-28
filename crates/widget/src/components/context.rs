use crate::style::BoxConstraint;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
#[cfg(not(target_arch = "wasm32"))]
use skia_safe::Canvas;
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Handle;
use winit::window::Window;

#[cfg(target_arch = "wasm32")]
type FLOAT = f64;
#[cfg(not(target_arch = "wasm32"))]
type FLOAT = f32;

pub struct BuildContext<'a> {
    pub parent_size: ResolvedSize,
    #[cfg(not(target_arch = "wasm32"))]
    pub canvas: &'a Canvas,
    #[cfg(target_arch = "wasm32")]
    pub canvas: &'a web_sys::CanvasRenderingContext2d,
    pub scale: FLOAT,
    pub parent_pos: Vec2d,
    pub box_constraint: BoxConstraint,
    pub window: &'static Window,
    #[cfg(not(target_arch = "wasm32"))]
    pub async_handle: Handle,
}

impl<'a> BuildContext<'a> {
    pub fn new(
        #[cfg(not(target_arch = "wasm32"))]
        canvas: &'a Canvas,
        #[cfg(target_arch = "wasm32")]
        canvas: &'a web_sys::CanvasRenderingContext2d,
        size: ResolvedSize,
        scale: FLOAT,
        parent_pos: Vec2d,
        window: &'static Window,
        #[cfg(not(target_arch = "wasm32"))]
        async_handle: Handle,
    ) -> Self {
        Self {
            canvas,
            parent_size: size,
            scale,
            parent_pos,
            box_constraint: BoxConstraint::default(),
            window,
            #[cfg(not(target_arch = "wasm32"))]
            async_handle,
        }
    }
}
