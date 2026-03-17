use widget::base::Vec2d;
use widget::Element;

pub struct InspectorOverlay;
impl InspectorOverlay {

    #[cfg(not(target_arch = "wasm32"))]
    pub fn draw(_element: &dyn Element, canvas: &skia_safe::Canvas, _cursor: Vec2d, scale: f32) {
        if let Ok(hovered) = widget::inspector_overlay::HOVERED_WIDGET.read() {
            // debug!("Drawing inspector overlay : {hovered:?}");
            if let Some((name, start, end)) = *hovered {

                canvas.save();
                canvas.scale((scale, scale));
                let w = end.x - start.x;
                let h = end.y - start.y;
                if w > 0.0 && h > 0.0 {
                    let mut paint = skia_safe::Paint::default();
                    paint.set_anti_alias(true);
                    paint.set_color(skia_safe::Color::from_argb(200, 0, 120, 255));
                    paint.set_style(skia_safe::paint::Style::Stroke);
                    paint.set_stroke_width(1.5);
                    let rect = skia_safe::Rect::from_xywh(start.x, start.y, w, h);
                    canvas.draw_rect(rect, &paint);

                    let mut background_paint = skia_safe::Paint::default();
                    background_paint.set_color(skia_safe::Color::from_argb(46, 66, 135, 245));
                    background_paint.set_style(skia_safe::paint::Style::Fill);
                    canvas.draw_rect(rect, &background_paint);


                    let label = format!("{name} {:.1}x{:.1}", w, h);
                    let font_size = 12.0_f32;
                    let mgr = skia_safe::FontMgr::new();
                    let typeface = mgr.match_family_style("Arial", skia_safe::font_style::FontStyle::default())
                        .or_else(|| mgr.match_family_style("", skia_safe::font_style::FontStyle::default()))
                        .expect("Unable to load any typeface");
                    let font = skia_safe::Font::new(typeface, font_size);
                    let text_bounds = font.measure_str(&label, None).1;
                    let label_w = text_bounds.width() + 8.0;
                    let label_h = font_size + 4.0;
                    let lx = start.x;
                    let ly = (start.y - label_h).max(0.0);

                    let mut bg_paint = skia_safe::Paint::default();
                    bg_paint.set_color(skia_safe::Color::from_argb(200, 0, 0, 0));
                    bg_paint.set_style(skia_safe::paint::Style::Fill);
                    canvas.draw_rect(skia_safe::Rect::from_xywh(lx, ly, label_w, label_h), &bg_paint);

                    let mut text_paint = skia_safe::Paint::default();
                    text_paint.set_color(skia_safe::Color::WHITE);
                    text_paint.set_anti_alias(true);
                    canvas.draw_str(&label, (lx + 4.0, ly + font_size), &font, &text_paint);
                }
                canvas.restore();
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn draw(_element: &dyn Element, canvas: &web_sys::CanvasRenderingContext2d, _cursor: Vec2d, scale: f32) {
        use wasm_bindgen::JsValue;
        if let Ok(hovered) = widget::inspector_overlay::HOVERED_WIDGET.read() {
            if let Some((name, start, end)) = *hovered {
                canvas.save();
                let _ = canvas.scale(scale as f64, scale as f64);
                let w = end.x - start.x;
                let h = end.y - start.y;
                if w > 0.0 && h > 0.0 {
                    canvas.set_stroke_style_str("rgba(0, 120, 255, 0.78)");
                    canvas.set_line_width(1.5);
                    canvas.stroke_rect(start.x, start.y, w, h );
                    canvas.set_fill_style_str("rgba(66, 135, 245, 0.18)");
                    canvas.fill_rect(start.x, start.y, w, h );

                    let label = format!("{name} {:.1}x{:.1}", w, h);
                    let font_size = 12.0_f64;
                    canvas.set_font(&format!("{}px Arial", font_size));

                    let text_metrics = canvas.measure_text(&label).unwrap();
                    let label_w = text_metrics.width() + 8.0;
                    let label_h = font_size + 4.0;

                    let lx = start.x as f64;
                    let ly = (start.y as f64 - label_h).max(0.0);

                    canvas.set_fill_style_str("rgba(66, 135, 245, 0.78)");
                    canvas.fill_rect(lx, ly, label_w, label_h);

                    canvas.set_fill_style_str("white");
                    let _ = canvas.fill_text(&label, lx + 4.0, ly + font_size);
                }
                canvas.restore();
            }
        }
    }

}