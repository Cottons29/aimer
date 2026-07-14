use crate::Element;
use crate::base::BuildContext;
#[cfg(not(target_arch = "wasm32"))]
use aimer_attribute::size::ResolvedSize;
use std::rc::Rc;

pub mod stateful;
pub mod stateless;

pub trait Widget {
    fn key(&self) -> Option<crate::key::Key> {
        None
    }
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element>;
    fn debug_name(&self) -> &'static str {
        "Unknown"
    }

    fn boxed(self) -> Box<dyn Widget>
    where
        Self: Sized + 'static,
    {
        Box::new(self)
    }

    /// Returns the text content if this is a text widget.
    /// Used by the reconciliation system to update text elements in-place.
    fn text_content(&self) -> Option<&str> {
        None
    }
}

impl Widget for Box<dyn Widget> {
    fn key(&self) -> Option<crate::key::Key> {
        self.as_ref().key()
    }
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        self.as_ref().to_element(ctx)
    }
    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }
    // fn text_content(&self) -> Option<&str> {
    //     self.as_ref().text_content()
    // }
}

impl Widget for Rc<dyn Widget> {
    fn key(&self) -> Option<crate::key::Key> {
        self.as_ref().key()
    }
    fn to_element(&self, ctx: &BuildContext) -> Box<dyn Element> {
        self.as_ref().to_element(ctx)
    }
    fn debug_name(&self) -> &'static str {
        self.as_ref().debug_name()
    }
    // fn text_content(&self) -> Option<&str> {
    //     self.as_ref().text_content()
    // }
}

/// Draw a colored bounding box + label at the current canvas transform origin.
/// Called during the draw pass when the widget inspector is enabled.
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub(crate) fn draw_inspector_box(ctx: &BuildContext, size: ResolvedSize, name: &'static str) {
    use aimer_color::prelude::Color;

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
        [0.0; 4],
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
        [0.0; 4],
    );

    let text_color = Color::Rgba(255, 255, 255, 255);
    ctx.canvas
        .draw_text(&label, (2.0_f32, font_size).into(), font_size, text_color, 400);
}
