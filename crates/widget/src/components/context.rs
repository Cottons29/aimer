use skia_safe::Canvas;
use tokio::runtime::Handle;
use winit::window::Window;

use crate::{attribute::size::ResolvedSize, base::Vec2d, style::BoxConstraint};

pub struct BuildContext<'a> {
    pub parent_size: ResolvedSize,
    pub canvas: &'a Canvas,
    pub scale: f32,
    pub parent_pos: Vec2d,
    pub box_constraint: BoxConstraint,
    pub window: &'static Window,
    pub async_handle: Handle,
}

impl<'a> BuildContext<'a> {
    pub fn new(canvas: &'a Canvas, size: ResolvedSize, scale: f32, parent_pos: Vec2d, window: &'static Window, async_handle: Handle) -> Self {
        Self { canvas, parent_size: size, scale, parent_pos, box_constraint: BoxConstraint::default(), window, async_handle }
    }
}
