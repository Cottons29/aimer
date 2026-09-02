mod canvas;
pub mod material;
pub mod shape;

pub use canvas::{
    AimerCanvas as Canvas, CanvasRendering, FontFamily, FontStyle, InnerCanvas,
    Mat3, RETAINED_LAYER_MAX_BYTES, RETAINED_LAYER_MAX_DIMENSION,
    RETAINED_LAYER_MAX_TILES_PER_FRAME, RETAINED_LAYER_TILE_SIZE, RetainedDrawList,
    RetainedLayerContent, TextHorizontalAlign, TextOverflowMode,
};
#[doc(hidden)]
pub use aimer_cupid::damage_region::{
    DamageAddResult, DamageBounds, DamageGeometry, DamageLayerChange, DamagePolicy, DamageRect,
    DamageSet, DamageTracker, DamageTransform,
};
pub use canvas::TextInteractionLayout;
pub use material::{
    record_material, MaterialClip, MaterialDrawRequest, MaterialKind, MaterialMotionPolicy,
};
pub use shape::{DrawShape, ShapeCanvasRendering, ShapeDrawError, ShapeDrawResult, ShapeFallback};
