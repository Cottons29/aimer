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
    use skia_safe::{Color, Font, Paint, Rect, paint::Style};
    let w = size.width;
    let h = size.height;
    if w <= 0.0 || h <= 0.0 {
        return;
    }

    // Bounding box stroke
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    paint.set_color(Color::from_argb(200, 0, 120, 255));
    paint.set_style(Style::Stroke);
    paint.set_stroke_width(1.5);
    ctx.canvas.draw_rect(Rect::from_xywh(0.0, 0.0, w, h), &paint);

    // Label
    let font_size = 10.0_f32;
    let label = format!("{} {:.0}×{:.0}", name, w, h);
    let label_w = (label.len() as f32) * font_size * 0.55 + 4.0;
    let label_h = font_size + 4.0;

    let mut bg = Paint::default();
    bg.set_color(Color::from_argb(180, 0, 0, 0));
    bg.set_style(Style::Fill);
    ctx.canvas.draw_rect(Rect::from_xywh(0.0, 0.0, label_w, label_h), &bg);

    let mut font = Font::default();
    font.set_size(font_size);
    let mut tp = Paint::default();
    tp.set_color(Color::WHITE);
    tp.set_anti_alias(true);
    ctx.canvas.draw_str(&label, (2.0_f32, font_size), &font, &tp);
}



