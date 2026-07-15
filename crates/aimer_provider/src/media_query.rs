use aimer_widget::base::{BuildContext, ResolvedSize};

pub struct MediaQuery {
    pub size: ResolvedSize,
    pub scale_factor: f32,
}

impl MediaQuery {
    pub fn of(ctx: &BuildContext) -> Self {
        let window_size = ctx.window.inner_size();
        let scale_factor = ctx.window.scale_factor() as f32;
        Self {
            size: ResolvedSize {
                width: window_size.width as f32 / scale_factor,
                height: window_size.height as f32 / scale_factor,
            },
            scale_factor: ctx.window.scale_factor() as f32,
        }
    }
}
