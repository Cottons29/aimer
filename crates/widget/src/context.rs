
use skia_safe::{Canvas, canvas};
use winit::window::Window;

use crate::size::Size;


pub struct BuildContext<'a> {
    pub size : Size,
    pub canvas: &'a Canvas 
}

impl<'a> From<&'a Canvas> for BuildContext<'a> {
    fn from(canvas: &'a Canvas) -> Self {
        BuildContext{
            canvas,
            size: Size::default()
        }
    }
}


impl<'a> BuildContext<'a> {
    pub fn new(canvas: &'a Canvas, size: Size) -> Self {
        Self {
            canvas, size
        }
    }
}


