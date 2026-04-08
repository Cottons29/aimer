use crate::canvas::CanvasRendering;
use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_color::prelude::{Color, ColorMixer};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::panic::Location;

use aimer_utils::{debug, error};
use std::sync::Mutex;

use web_sys::{CanvasRenderingContext2d as H5Canvas, HtmlImageElement};

static IMAGE_REGISTRY: Lazy<Mutex<HashMap<u32, HtmlImageElement>>> = Lazy::new(|| Mutex::new(HashMap::new()));

static NEXT_IMAGE_ID: Mutex<u32> = Mutex::new(1);

#[allow(dead_code)]
impl CanvasRendering for H5Canvas {
    #[inline]
    fn begin_frame(&self) {
        // self.begin_path();
    }

    #[inline]
    fn fill_rect(&self, pos: Vec2d, size: ResolvedSize) {
        H5Canvas::fill_rect(self, pos.x as f64, pos.y as f64, size.width as f64, size.height as f64);
    }

    #[inline]
    fn fill_rect_with_border(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
    ) {
        self.set_fill_style_str(&color.to_css_color());

        let x = pos.x as f64;
        let y = pos.y as f64;
        let w = size.width as f64;
        let h = size.height as f64;
        let tl = border_radius[0] as f64;
        let tr = border_radius[1] as f64;
        let br = border_radius[2] as f64;
        let bl = border_radius[3] as f64;

        if tl > 0.0 || tr > 0.0 || br > 0.0 || bl > 0.0 {
            self.begin_path();
            self.move_to(x + tl, y);
            self.line_to(x + w - tr, y);
            self.arc_to(x + w, y, x + w, y + br, br).unwrap_or(());
            self.line_to(x + w, y + h - br);
            self.arc_to(x + w, y + h, x + w - br, y + h, br)
                .unwrap_or(());
            self.line_to(x + bl, y + h);
            self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
            self.line_to(x, y + tl);
            self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
            self.close_path();
            self.fill();
        } else {
            H5Canvas::fill_rect(self, x, y, w, h);
        }

        // Border
        if border_width > 0.0 {
            self.set_stroke_style_str(&border_color.to_css_color());
            self.set_line_width(border_width as f64);

            if tl > 0.0 || tr > 0.0 || br > 0.0 || bl > 0.0 {
                self.begin_path();
                self.move_to(x + tl, y);
                self.line_to(x + w - tr, y);
                self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
                self.line_to(x + w, y + h - br);
                self.arc_to(x + w, y + h, x + w - br, y + h, br)
                    .unwrap_or(());
                self.line_to(x + bl, y + h);
                self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
                self.line_to(x, y + tl);
                self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
                self.close_path();
                self.stroke();
            } else {
                self.stroke_rect(x, y, w, h);
            }
        }
    }

    #[inline]
    fn fill_rect_with_per_side_border(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
    ) {
        self.set_fill_style_str(&color.to_css_color());

        let x = pos.x as f64;
        let y = pos.y as f64;
        let w = size.width as f64;
        let h = size.height as f64;
        let tl = border_radius[0] as f64;
        let tr = border_radius[1] as f64;
        let br = border_radius[2] as f64;
        let bl = border_radius[3] as f64;

        self.begin_path();
        self.move_to(x + tl, y);
        self.line_to(x + w - tr, y);
        self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
        self.line_to(x + w, y + h - br);
        self.arc_to(x + w, y + h, x + w - br, y + h, br)
            .unwrap_or(());
        self.line_to(x + bl, y + h);
        self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
        self.line_to(x, y + tl);
        self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
        self.close_path();
        self.fill();

        // Border with per-side widths
        let max_w = border_width[0]
            .max(border_width[1])
            .max(border_width[2])
            .max(border_width[3]);
        if max_w > 0.0 {
            let bargb = border_color.to_u32();
            let ba = ((bargb >> 24) & 0xFF) as f64 / 255.0;
            let br_c = (bargb >> 16) & 0xFF;
            let bg = (bargb >> 8) & 0xFF;
            let bb = bargb & 0xFF;
            let stroke_style = format!("rgba({},{},{},{})", br_c, bg, bb, ba);
            self.set_stroke_style_str(&stroke_style);
            self.set_line_width(max_w as f64);

            self.begin_path();
            self.move_to(x + tl, y);
            self.line_to(x + w - tr, y);
            self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
            self.line_to(x + w, y + h - br);
            self.arc_to(x + w, y + h, x + w - br, y + h, br)
                .unwrap_or(());
            self.line_to(x + bl, y + h);
            self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
            self.line_to(x, y + tl);
            self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
            self.close_path();
            self.stroke();
        }
    }

    #[inline]
    fn clear_rect(&self, pos: Vec2d, size: ResolvedSize) {
        H5Canvas::clear_rect(self, pos.x as f64, pos.y as f64, size.width as f64, size.height as f64);
    }

    #[inline]
    #[track_caller]
    fn translate(&self, pos: Vec2d) {
        #[cfg(not(debug_assertions))]
        H5Canvas::translate(self, pos.x, pos.y).unwrap();
        #[cfg(debug_assertions)]
        {
            if let Err(err) = H5Canvas::translate(self, pos.x as f64, pos.y as f64) {
                let err = err.as_string().unwrap_or_default();
                let location = Location::caller();
                let file_name = location.file();
                let line = location.line();
                let column = location.column();
                let error_str = format!("{}:{}:{}", file_name, line, column);
                error!("Translation error: {err} \nat {error_str}");
            }
        }
    }

    #[inline]
    fn scale(&self, sx: f32, sy: f32) {
        let _ = H5Canvas::scale(self, sx as f64, sy as f64);
    }

    #[inline]
    fn rotate(&self, radians: f32) {
        let _ = H5Canvas::rotate(self, radians as f64);
    }

    #[inline]
    fn save(&self) {
        H5Canvas::save(self);
    }

    #[inline]
    fn restore(&self) {
        H5Canvas::restore(self);
    }

    #[inline]
    fn draw_text(&self, text: &str, pos: Vec2d, font_size: f32, color: Color) {
        let argb = color.to_u32();
        let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
        let r = (argb >> 16) & 0xFF;
        let g = (argb >> 8) & 0xFF;
        let b = argb & 0xFF;
        let fill_style = format!("rgba({},{},{},{})", r, g, b, a);
        self.set_fill_style_str(&fill_style);
        self.set_font(&format!("{}px sans-serif", font_size));
        let _ = self.fill_text(text, pos.x as f64, pos.y as f64);
    }

    #[inline]
    fn draw_image(&self, image_id: u32, pos: Vec2d, size: ResolvedSize) {
        let registry = IMAGE_REGISTRY.lock().unwrap();
        if let Some(img) = registry.get(&image_id) {
            let _ = self.draw_image_with_html_image_element_and_dw_and_dh(
                img,
                pos.x as f64,
                pos.y as f64,
                size.width as f64,
                size.height as f64,
            );
        }
    }

    #[inline]
    fn get_image_size(&self, image_id: u32) -> Option<(u32, u32)> {
        let registry = IMAGE_REGISTRY.lock().unwrap();
        registry
            .get(&image_id)
            .map(|img: &HtmlImageElement| (img.natural_width(), img.natural_height()))
    }

    #[inline]
    fn set_clip(&self, pos: Vec2d, size: ResolvedSize) {
        H5Canvas::save(self);
        self.begin_path();
        let max_dim = 1e7;
        let w = size.width.min(max_dim) as f64;
        let h = size.height.min(max_dim) as f64;
        self.rect(pos.x as f64, pos.y as f64, w, h);
        let _ = self.clip();
    }

    #[inline]
    fn set_clip_rounded(&self, pos: Vec2d, size: ResolvedSize, border_radius: [f32; 4]) {
        H5Canvas::save(self);
        self.begin_path();
        let max_dim = 1e7;
        let w = size.width.min(max_dim) as f64;
        let h = size.height.min(max_dim) as f64;
        let x = pos.x as f64;
        let y = pos.y as f64;

        let tl = border_radius[0] as f64;
        let tr = border_radius[1] as f64;
        let br = border_radius[2] as f64;
        let bl = border_radius[3] as f64;

        if tl > 0.0 || tr > 0.0 || br > 0.0 || bl > 0.0 {
            self.move_to(x + tl, y);
            self.line_to(x + w - tr, y);
            self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
            self.line_to(x + w, y + h - br);
            self.arc_to(x + w, y + h, x + w - br, y + h, br)
                .unwrap_or(());
            self.line_to(x + bl, y + h);
            self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
            self.line_to(x, y + tl);
            self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
            self.close_path();
        } else {
            self.rect(x, y, w, h);
        }
        let _ = self.clip();
    }

    #[inline]
    fn clear_clip(&self) {
        H5Canvas::restore(self);
    }

    #[inline]
    fn measure_text(&self, text: &str, font_size: f32) -> f32 {
        self.set_font(&format!("{}px sans-serif", font_size));
        if let Ok(metrics) = H5Canvas::measure_text(self, text) {
            metrics.width() as f32
        } else {
            text.chars().count() as f32 * font_size * 0.6
        }
    }

    #[inline]
    fn stroke_rect(&self, pos: Vec2d, size: ResolvedSize, stroke_color: Color, stroke_width: f32, border_radius: [f32; 4]) {
        let argb = stroke_color.to_css_color();
        self.set_stroke_style_str(&argb);
        self.set_line_width(stroke_width as f64);

        let x = pos.x as f64;
        let y = pos.y as f64;
        let w = size.width as f64;
        let h = size.height as f64;
        let tl = border_radius[0] as f64;
        let tr = border_radius[1] as f64;
        let br = border_radius[2] as f64;
        let bl = border_radius[3] as f64;

        if tl > 0.0 || tr > 0.0 || br > 0.0 || bl > 0.0 {
            self.begin_path();
            self.move_to(x + tl, y);
            self.line_to(x + w - tr, y);
            self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
            self.line_to(x + w, y + h - br);
            self.arc_to(x + w, y + h, x + w - br, y + h, br)
                .unwrap_or(());
            self.line_to(x + bl, y + h);
            self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
            self.line_to(x, y + tl);
            self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
            self.close_path();
            self.stroke();
        } else {
            H5Canvas::stroke_rect(self, x, y, w, h);
        }
    }

    #[inline]
    fn stroke_rect_per_side(&self, pos: Vec2d, size: ResolvedSize, stroke_color: Color, stroke_width: [f32; 4], border_radius: [f32; 4]) {
        let stroke_style = stroke_color.to_css_color();
        self.set_stroke_style_str(&stroke_style);
        // Use the max border width for the stroke; the visual per-side effect
        // is approximated by the rounded-rect path.
        let max_w = stroke_width[0]
            .max(stroke_width[1])
            .max(stroke_width[2])
            .max(stroke_width[3]);
        self.set_line_width(max_w as f64);

        self.begin_path();
        let x = pos.x as f64;
        let y = pos.y as f64;
        let w = size.width as f64;
        let h = size.height as f64;
        let tl = border_radius[0] as f64;
        let tr = border_radius[1] as f64;
        let br = border_radius[2] as f64;
        let bl = border_radius[3] as f64;
        self.move_to(x + tl, y);
        self.line_to(x + w - tr, y);
        self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
        self.line_to(x + w, y + h - br);
        self.arc_to(x + w, y + h, x + w - br, y + h, br)
            .unwrap_or(());
        self.line_to(x + bl, y + h);
        self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
        self.line_to(x, y + tl);
        self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
        self.close_path();
        self.stroke();
    }

    #[inline]
    fn fill_rect_with_border_and_outline(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
        outline_width: f32,
        outline_color: Color,
    ) {
        // debug!("fill_rect_with_border_and_outline");
        // Wasm fallback: draw border rect then outline rect separately
        self.fill_rect_with_border(pos, size, color, border_radius, border_width, border_color);
        if outline_width > 0.0 {
            let outline_radius = border_radius.map(|r| if r > 0.0 { r + outline_width / 2.0 } else { 0.0 });
            <H5Canvas as CanvasRendering>::stroke_rect(
                self,
                Vec2d { x: (pos.x as f32 - outline_width / 2.0), y: (pos.y as f32 - outline_width / 2.0) },
                ResolvedSize { width: size.width + outline_width, height: size.height + outline_width },
                outline_color,
                outline_width,
                outline_radius,
            );
        }
    }

    #[inline]
    fn fill_rect_with_border_and_outline_per_side(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
        outline_width: [f32; 4],
        outline_color: Color,
    ) {
        self.fill_rect_with_per_side_border(pos, size, color, border_radius, border_width, border_color);
        let has_outline = outline_width.iter().any(|w| *w > 0.0);
        if has_outline {
            let (b1, b2, b3, b4) = (border_width[0], border_width[1], border_width[2], border_width[3]);
            let new_pos = pos - (b1, b2);
            let new_size = ResolvedSize { width: size.width + b1 + b3, height: size.height + b2 + b4 };
            let new_radius = border_radius.map(|r| r * 1.18);
            self.stroke_rect_per_side(new_pos, new_size, outline_color, border_width, new_radius);
        }
    }

    #[inline]
    fn fill_color_rect(&self, pos: Vec2d, size: ResolvedSize, color: Color, border_radius: [f32; 4]) {
        self.set_fill_style_str(&color.to_css_color());

        let x = pos.x as f64;
        let y = pos.y as f64;
        let w = size.width as f64;
        let h = size.height as f64;
        let tl = border_radius[0] as f64;
        let tr = border_radius[1] as f64;
        let br = border_radius[2] as f64;
        let bl = border_radius[3] as f64;

        if tl > 0.0 || tr > 0.0 || br > 0.0 || bl > 0.0 {
            self.begin_path();
            self.move_to(x + tl, y);
            self.line_to(x + w - tr, y);
            self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
            self.line_to(x + w, y + h - br);
            self.arc_to(x + w, y + h, x + w - br, y + h, br)
                .unwrap_or(());
            self.line_to(x + bl, y + h);
            self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
            self.line_to(x, y + tl);
            self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
            self.close_path();
            self.fill();
        } else {
            H5Canvas::fill_rect(self, x, y, w, h);
        }
    }

    #[inline]
    fn fill_color_rect_per_corner(&self, pos: Vec2d, size: ResolvedSize, color: Color, border_radius: [f32; 4]) {
        self.set_fill_style_str(&color.to_css_color());

        self.begin_path();
        let x = pos.x as f64;
        let y = pos.y as f64;
        let w = size.width as f64;
        let h = size.height as f64;
        let tl = border_radius[0] as f64;
        let tr = border_radius[1] as f64;
        let br = border_radius[2] as f64;
        let bl = border_radius[3] as f64;
        self.move_to(x + tl, y);
        self.line_to(x + w - tr, y);
        self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
        self.line_to(x + w, y + h - br);
        self.arc_to(x + w, y + h, x + w - br, y + h, br)
            .unwrap_or(());
        self.line_to(x + bl, y + h);
        self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
        self.line_to(x, y + tl);
        self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
        self.close_path();
        self.fill();
    }

    #[inline]
    fn draw_shadow_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        shadow_color: Color,
        shadow_params: [f32; 4],
        border_radius: [f32; 4],
        inset: bool,
        _side_params: [f32; 3],
    ) {
        // WASM fallback: use Canvas2D shadowBlur/shadowOffset
        let argb = shadow_color.to_u32();
        let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
        let r = (argb >> 16) & 0xFF;
        let g = (argb >> 8) & 0xFF;
        let b = argb & 0xFF;
        let color_str = format!("rgba({},{},{},{})", r, g, b, a);

        self.save();
        self.set_shadow_color(&color_str);
        self.set_shadow_blur(shadow_params[2] as f64);
        self.set_shadow_offset_x(shadow_params[0] as f64);
        self.set_shadow_offset_y(shadow_params[1] as f64);

        // Draw a filled rect to trigger the shadow
        self.set_fill_style_str("rgba(0,0,0,0)");
        H5Canvas::fill_rect(self, pos.x as f64, pos.y as f64, size.width as f64, size.height as f64);

        self.restore();
    }

    #[inline]
    fn set_alpha(&self, alpha: f32) {
        self.set_global_alpha(alpha as f64);
    }

    #[inline]
    fn restore_alpha(&self) {
        self.set_global_alpha(1.0);
    }

    #[inline]
    fn load_image(&self, bytes: &[u8], _width: u32, _height: u32) -> u32 {
        let mut next_id = NEXT_IMAGE_ID.lock().unwrap();
        let id = *next_id;
        *next_id += 1;

        let array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
        array.copy_from(bytes);
        let blob_parts = js_sys::Array::new();
        blob_parts.push(&array);
        let blob = web_sys::Blob::new_with_u8_array_sequence(&blob_parts).unwrap();
        let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();

        let img = HtmlImageElement::new().unwrap();
        img.set_src(&url);

        let mut registry = IMAGE_REGISTRY.lock().unwrap();
        registry.insert(id, img);
        id
    }

    fn load_image_with_id(&self, image_id: u32, bytes: &[u8], _width: u32, _height: u32) {
        let array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
        array.copy_from(bytes);
        let blob_parts = js_sys::Array::new();
        blob_parts.push(&array);
        let blob = web_sys::Blob::new_with_u8_array_sequence(&blob_parts).unwrap();
        let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();

        let img = HtmlImageElement::new().unwrap();
        img.set_src(&url);

        let mut registry = IMAGE_REGISTRY.lock().unwrap();
        registry.insert(image_id, img);
    }

    #[inline]
    fn set_texture_size(&self, _image_id: u32, _width: u32, _height: u32) {}

    #[inline]
    fn get_transform_translation(&self) -> (f32, f32) {
        let matrix = self.get_transform().unwrap();
        (matrix.e() as f32, matrix.f() as f32)
    }
}
