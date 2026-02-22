use skia_safe::Canvas;

use crate::{attribute::size::Size, base::Vec2d, style::BoxConstraint};

pub struct BuildContext<'a> {
    pub parent_size: Size,
    pub canvas: &'a Canvas,
    pub scale: f32,
    pub parent_pos: Vec2d,
    pub box_constraint: BoxConstraint,
}

impl<'a> BuildContext<'a> {
    pub fn new(canvas: &'a Canvas, size: Size, scale: f32, parent_pos: Vec2d) -> Self {
        Self { canvas, parent_size: size, scale, parent_pos, box_constraint: BoxConstraint::default() }
    }
}
