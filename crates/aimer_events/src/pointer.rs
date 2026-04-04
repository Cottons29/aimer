use winit::event::{ElementState, MouseButton, Touch, TouchPhase, WindowEvent};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerPosition {
    pub x: f32,
    pub y: f32,
}


#[derive(Clone, Debug)]
pub enum PointerEvent {
    Down(PointerPosition),
    Up(PointerPosition),
    Move(PointerPosition),
    Cancel,
}