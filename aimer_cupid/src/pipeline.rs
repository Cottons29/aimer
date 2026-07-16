pub mod image_pipeline;
pub mod rect_pipeline;
pub mod svg_pipeline;
pub mod text_pipeline;

pub(crate) const RENDER_SAMPLE_COUNT: u32 = 4;

pub(crate) fn multisample_state() -> wgpu::MultisampleState {
    wgpu::MultisampleState { count: RENDER_SAMPLE_COUNT, ..Default::default() }
}
