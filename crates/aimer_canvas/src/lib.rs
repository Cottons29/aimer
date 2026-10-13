mod canvas;
mod paint_background;
#[doc(hidden)]
pub use paint_background::composite_background;
pub mod material;
pub mod shape;

pub use canvas::TextInteractionLayout;
pub use canvas::{
    Canvas, CanvasContext, CanvasRendering, FontFamily, FontStyle, FrameCanvas, InnerCanvas, Mat3,
    RETAINED_LAYER_MAX_BYTES, RETAINED_LAYER_MAX_DIMENSION, RETAINED_LAYER_MAX_TILES_PER_FRAME,
    RETAINED_LAYER_TILE_SIZE, RetainedDrawList, RetainedLayerContent, RetainedLayerPadding,
    TextHorizontalAlign, TextOverflowMode, next_retained_layer_id,
};
pub use material::{
    MagnificationStyle, MaterialClip, MaterialDrawRequest, MaterialKind, MaterialMotionPolicy,
    record_material,
};
pub use shape::{DrawShape, ShapeDrawError, ShapeDrawResult, ShapeFallback};
