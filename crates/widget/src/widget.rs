use crate::base::BuildContext;
use crate::{Element, StatefulWidget, StatelessWidget};
#[cfg(not(target_arch = "wasm32"))]
use attribute::size::ResolvedSize;

pub mod stateful;
pub mod stateless;

pub use stateless::NamedWidget;

pub trait Widget{
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element>;
    fn debug_name(&self) -> &'static str {
        "Unknown"
    }
}

impl Widget for Box<dyn Widget> {
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        self.as_ref().to_element(ctx)
    }
    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }
}

/// Draw a colored bounding box + label at the current canvas transform origin.
/// Called during the draw pass when the widget inspector is enabled.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn draw_inspector_box(ctx: &BuildContext, size: ResolvedSize, name: &'static str) {
    use color::prelude::Color;

    let w = size.width;
    let h = size.height;
    if w <= 0.0 || h <= 0.0 {
        return;
    }

    // Bounding box stroke
    let stroke_color = Color::Rgba(0, 120, 255, 200);
    ctx.canvas.stroke_rect(
        (0.0_f32, 0.0_f32).into(),
        ResolvedSize { width: w, height: h },
        stroke_color,
        1.5,
        0.0,
    );

    // Label
    let font_size = 10.0_f32;
    let label = format!("{} {:.0}×{:.0}", name, w, h);
    let label_w = (label.len() as f32) * font_size * 0.55 + 4.0;
    let label_h = font_size + 4.0;

    let bg_color = Color::Rgba(0, 0, 0, 180);
    ctx.canvas.fill_color_rect(
        (0.0_f32, 0.0_f32).into(),
        ResolvedSize { width: label_w, height: label_h },
        bg_color,
        0.0,
    );

    let text_color = Color::Rgba(255, 255, 255, 255);
    ctx.canvas.draw_text(&label, (2.0_f32, font_size).into(), font_size, text_color);
}



