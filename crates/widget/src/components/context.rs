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

#[derive(Debug, Clone)]
pub struct BuildContext<'a> {
    pub parent_size: ResolvedSize,
    #[cfg(not(target_arch = "wasm32"))]
    pub canvas: &'a Canvas,
    #[cfg(target_arch = "wasm32")]
    pub canvas: &'a web_sys::CanvasRenderingContext2d,
    pub scale: FLOAT,
    pub parent_pos: Vec2d,
    pub cursor_pos: Vec2d,
    pub box_constraint: BoxConstraint,
    pub visible_rect: Option<(FLOAT, FLOAT, FLOAT, FLOAT)>, // (x, y, width, height)
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
        }
    }
}
