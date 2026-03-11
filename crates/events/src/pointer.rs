use winit::event::{ElementState, MouseButton, Touch, TouchPhase, WindowEvent};

#[cfg(not(target_arch = "wasm32"))]
type Float = f32;
#[cfg(target_arch = "wasm32")]
type Float = f64;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerPosition {
    pub x: Float,
    pub y: Float,
}


#[derive(Clone, Debug)]
pub enum PointerEvent {
    Down(PointerPosition),
    Up(PointerPosition),
    Move(PointerPosition),
    Cancel,
}