use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use color::prelude::Color;
use widget::Element;
use canvas::Canvas;

pub struct InspectorOverlay;
impl InspectorOverlay {

    pub fn draw(_element: &dyn Element, canvas: &Canvas<'_>, _cursor: Vec2d, scale: f32) {
        if let Ok(hovered) = widget::inspector_overlay::HOVERED_WIDGET.read() {
            if let Some((name, start, end)) = *hovered {

                canvas.save();
                canvas.scale(scale, scale);
                let w = end.x - start.x;
                let h = end.y - start.y;
                if w > 0.0 && h > 0.0 {
                    // Stroke border
                    canvas.stroke_rect(
                        Vec2d::from((start.x, start.y)),
                        ResolvedSize { width: w, height: h },
                        Color::Rgba(0, 120, 255, 200),
                        1.5, [0.0; 4],
                    );
                    // Fill background
                    canvas.fill_color_rect(
                        Vec2d::from((start.x, start.y)),
                        ResolvedSize { width: w, height: h },
                        Color::Rgba(66, 135, 245, 46),
                        [0.0; 4],
                    );

                    let label = format!("{name} {:.1}x{:.1}", w, h);
                    #[cfg(target_arch = "wasm32")]
                    let font_size = 13.0_f32;
                    #[cfg(not(target_arch = "wasm32"))]
                    let font_size = 16.0_f32;
                    let label_w_raw = canvas.measure_text(&label, font_size);
                    let label_w = label_w_raw + 8.0;
                    let label_h = font_size  + 4.0;
                    let lx = start.x;
                    let ly = (start.y - label_h).max(0.0);

                    // Label background
                    canvas.fill_color_rect(
                        Vec2d::from((lx, ly)),
                        ResolvedSize { width: label_w, height: label_h },
                        Color::Rgba(66, 135, 245, 200),
                        [0.0; 4],
                    );

                    // Label text
                    canvas.draw_text(
                        &label,
                        Vec2d::from((lx + 4.0, ly + font_size)),
                        font_size,
                        Color::Rgba(255, 255, 255, 255),
                    );
                }
                canvas.restore();
            }
        }
    }

}