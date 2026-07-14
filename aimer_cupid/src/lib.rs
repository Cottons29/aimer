pub mod custom_pipeline;
pub mod draw_cmd;
pub mod gpu_context;
pub mod utilities;

pub mod canvas;
mod pipeline;
pub mod pipeline_cache;
pub mod renderer;
#[cfg(target_arch = "wasm32")]
pub mod wasm_fonts;

pub use pipeline::{image_pipeline, rect_pipeline, text_pipeline};

pub use crate::text_pipeline::{glyph_atlas, glyph_rasterizer, text_layout};
