use crate::canvas::AimerCanvas;

#[cfg(not(target_arch = "wasm32"))]
pub type Canvas = cupid::canvas::CupidCanvas;
#[cfg(target_arch = "wasm32")]
pub type Canvas = web_sys::CanvasRenderingContext2d;


pub(crate) struct AimerCanvasInner<'a> {
    pub(crate) canvas: &'a Canvas,
}

impl<'a> AimerCanvasInner<'a> {
    pub fn canvas(&'a  self) -> &'a Canvas {
        self.canvas
    }
}