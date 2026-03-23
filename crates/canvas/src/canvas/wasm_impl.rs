use std::panic::Location;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use color::prelude::{Color, ColorMixer};
use utils::error;
use crate::canvas::CanvasRendering;
#[cfg(target_arch = "wasm32")]
use web_sys::CanvasRenderingContext2d as H5Canvas;
#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
impl CanvasRendering for H5Canvas {

    #[inline]
    fn begin_frame(&self) {
        // self.begin_path();
    }

    #[inline]
    fn fill_rect(&self, pos: Vec2d, size: ResolvedSize) {
        H5Canvas::fill_rect(self, pos.x, pos.y, size.width, size.height);
    }

    #[inline]
    fn fill_rect_with_border(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: f32,
        border_width: f32,
        border_color: Color,
    ) {
        // Fill
        let argb = color.to_u32();
        let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
        let r = (argb >> 16) & 0xFF;
        let g = (argb >> 8) & 0xFF;
        let b = argb & 0xFF;
        let fill_style = format!("rgba({},{},{},{})", r, g, b, a);
        self.set_fill_style_str(&fill_style);

        if border_radius > 0.0 {
            self.begin_path();
            let x = pos.x;
            let y = pos.y;
            let w = size.width;
            let h = size.height;
            let br = border_radius as f64;
            self.move_to(x + br, y);
            self.line_to(x + w - br, y);
            self.arc_to(x + w, y, x + w, y + br, br).unwrap_or(());
            self.line_to(x + w, y + h - br);
            self.arc_to(x + w, y + h, x + w - br, y + h, br).unwrap_or(());
            self.line_to(x + br, y + h);
            self.arc_to(x, y + h, x, y + h - br, br).unwrap_or(());
            self.line_to(x, y + br);
            self.arc_to(x, y, x + br, y, br).unwrap_or(());
            self.close_path();
            self.fill();
        } else {
            H5Canvas::fill_rect(self, pos.x, pos.y, size.width, size.height);
        }

        // Border
        if border_width > 0.0 {
            let bargb = border_color.to_u32();
            let ba = ((bargb >> 24) & 0xFF) as f64 / 255.0;
            let br_c = (bargb >> 16) & 0xFF;
            let bg = (bargb >> 8) & 0xFF;
            let bb = bargb & 0xFF;
            let stroke_style = format!("rgba({},{},{},{})", br_c, bg, bb, ba);
            self.set_stroke_style_str(&stroke_style);
            self.set_line_width(border_width as f64);

            if border_radius > 0.0 {
                self.begin_path();
                let x = pos.x;
                let y = pos.y;
                let w = size.width;
                let h = size.height;
                let br = border_radius as f64;
                self.move_to(x + br, y);
                self.line_to(x + w - br, y);
                self.arc_to(x + w, y, x + w, y + br, br).unwrap_or(());
                self.line_to(x + w, y + h - br);
                self.arc_to(x + w, y + h, x + w - br, y + h, br).unwrap_or(());
                self.line_to(x + br, y + h);
                self.arc_to(x, y + h, x, y + h - br, br).unwrap_or(());
                self.line_to(x, y + br);
                self.arc_to(x, y, x + br, y, br).unwrap_or(());
                self.close_path();
                self.stroke();
            } else {
                self.stroke_rect(pos.x, pos.y, size.width, size.height);
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
        // Fill with per-corner radii
        let argb = color.to_u32();
        let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
        let r = (argb >> 16) & 0xFF;
        let g = (argb >> 8) & 0xFF;
        let b = argb & 0xFF;
        let fill_style = format!("rgba({},{},{},{})", r, g, b, a);
        self.set_fill_style_str(&fill_style);

        let x = pos.x;
        let y = pos.y;
        let w = size.width;
        let h = size.height;
        let tl = border_radius[0] as f64;
        let tr = border_radius[1] as f64;
        let br = border_radius[2] as f64;
        let bl = border_radius[3] as f64;

        self.begin_path();
        self.move_to(x + tl, y);
        self.line_to(x + w - tr, y);
        self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
        self.line_to(x + w, y + h - br);
        self.arc_to(x + w, y + h, x + w - br, y + h, br).unwrap_or(());
        self.line_to(x + bl, y + h);
        self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
        self.line_to(x, y + tl);
        self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
        self.close_path();
        self.fill();

        // Border with per-side widths
        let max_w = border_width[0].max(border_width[1]).max(border_width[2]).max(border_width[3]);
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
            self.arc_to(x + w, y + h, x + w - br, y + h, br).unwrap_or(());
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
        H5Canvas::clear_rect(self, pos.x, pos.y, size.width, size.height);
    }

    #[inline]
    #[track_caller]
    fn translate(&self, pos: Vec2d) {
        #[cfg(not(debug_assertions))]
        H5Canvas::translate(self, pos.x, pos.y).unwrap();
        #[cfg(debug_assertions)]
        {
            if let Err(err) = H5Canvas::translate(self, pos.x, pos.y) {
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
        let _ = self.fill_text(text, pos.x, pos.y);
    }

    #[inline]
    fn draw_image(&self, _image_id: u32, _pos: Vec2d, _size: ResolvedSize) {
        // Image drawing on wasm requires HtmlImageElement lookup by image_id.
        // This is a placeholder — the actual implementation depends on the
        // image asset management strategy for the wasm target.
    }

    #[inline]
    fn set_clip(&self, pos: Vec2d, size: ResolvedSize) {
        H5Canvas::save(self);
        self.begin_path();
        // Clamp dimensions to avoid invalid clip regions with huge values (e.g. f64::MAX
        // from unbounded scroll containers), which cause the browser to produce an empty clip.
        let max_dim = 1e7;
        let w = size.width.min(max_dim);
        let h = size.height.min(max_dim);
        self.rect(pos.x, pos.y, w, h);
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
    fn stroke_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        stroke_color: Color,
        stroke_width: f32,
        border_radius: f32,
    ) {
        let argb = stroke_color.to_u32();
        let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
        let r = (argb >> 16) & 0xFF;
        let g = (argb >> 8) & 0xFF;
        let b = argb & 0xFF;
        let stroke_style = format!("rgba({},{},{},{})", r, g, b, a);
        self.set_stroke_style_str(&stroke_style);
        self.set_line_width(stroke_width as f64);

        if border_radius > 0.0 {
            self.begin_path();
            let x = pos.x;
            let y = pos.y;
            let w = size.width;
            let h = size.height;
            let br = border_radius as f64;
            self.move_to(x + br, y);
            self.line_to(x + w - br, y);
            self.arc_to(x + w, y, x + w, y + br, br).unwrap_or(());
            self.line_to(x + w, y + h - br);
            self.arc_to(x + w, y + h, x + w - br, y + h, br).unwrap_or(());
            self.line_to(x + br, y + h);
            self.arc_to(x, y + h, x, y + h - br, br).unwrap_or(());
            self.line_to(x, y + br);
            self.arc_to(x, y, x + br, y, br).unwrap_or(());
            self.close_path();
            self.stroke();
        } else {
            H5Canvas::stroke_rect(self, pos.x, pos.y, size.width, size.height);
        }
    }

    #[inline]
    fn stroke_rect_per_side(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        stroke_color: Color,
        stroke_width: [f32; 4],
        border_radius: [f32; 4],
    ) {
        let argb = stroke_color.to_u32();
        let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
        let r = (argb >> 16) & 0xFF;
        let g = (argb >> 8) & 0xFF;
        let b = argb & 0xFF;
        let stroke_style = format!("rgba({},{},{},{})", r, g, b, a);
        self.set_stroke_style_str(&stroke_style);
        // Use the max border width for the stroke; the visual per-side effect
        // is approximated by the rounded-rect path.
        let max_w = stroke_width[0].max(stroke_width[1]).max(stroke_width[2]).max(stroke_width[3]);
        self.set_line_width(max_w as f64);

        self.begin_path();
        let x = pos.x;
        let y = pos.y;
        let w = size.width;
        let h = size.height;
        let tl = border_radius[0] as f64;
        let tr = border_radius[1] as f64;
        let br = border_radius[2] as f64;
        let bl = border_radius[3] as f64;
        self.move_to(x + tl, y);
        self.line_to(x + w - tr, y);
        self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
        self.line_to(x + w, y + h - br);
        self.arc_to(x + w, y + h, x + w - br, y + h, br).unwrap_or(());
        self.line_to(x + bl, y + h);
        self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
        self.line_to(x, y + tl);
        self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
        self.close_path();
        self.stroke();
    }

    #[inline]
    fn fill_color_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: f32,
    ) {
        let argb = color.to_u32();
        let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
        let r = (argb >> 16) & 0xFF;
        let g = (argb >> 8) & 0xFF;
        let b = argb & 0xFF;
        let fill_style = format!("rgba({},{},{},{})", r, g, b, a);
        self.set_fill_style_str(&fill_style);

        if border_radius > 0.0 {
            self.begin_path();
            let x = pos.x;
            let y = pos.y;
            let w = size.width;
            let h = size.height;
            let br = border_radius as f64;
            self.move_to(x + br, y);
            self.line_to(x + w - br, y);
            self.arc_to(x + w, y, x + w, y + br, br).unwrap_or(());
            self.line_to(x + w, y + h - br);
            self.arc_to(x + w, y + h, x + w - br, y + h, br).unwrap_or(());
            self.line_to(x + br, y + h);
            self.arc_to(x, y + h, x, y + h - br, br).unwrap_or(());
            self.line_to(x, y + br);
            self.arc_to(x, y, x + br, y, br).unwrap_or(());
            self.close_path();
            self.fill();
        } else {
            H5Canvas::fill_rect(self, pos.x, pos.y, size.width, size.height);
        }
    }

    #[inline]
    fn fill_color_rect_per_corner(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
    ) {
        let argb = color.to_u32();
        let a = ((argb >> 24) & 0xFF) as f64 / 255.0;
        let r = (argb >> 16) & 0xFF;
        let g = (argb >> 8) & 0xFF;
        let b = argb & 0xFF;
        let fill_style = format!("rgba({},{},{},{})", r, g, b, a);
        self.set_fill_style_str(&fill_style);

        self.begin_path();
        let x = pos.x;
        let y = pos.y;
        let w = size.width;
        let h = size.height;
        let tl = border_radius[0] as f64;
        let tr = border_radius[1] as f64;
        let br = border_radius[2] as f64;
        let bl = border_radius[3] as f64;
        self.move_to(x + tl, y);
        self.line_to(x + w - tr, y);
        self.arc_to(x + w, y, x + w, y + tr, tr).unwrap_or(());
        self.line_to(x + w, y + h - br);
        self.arc_to(x + w, y + h, x + w - br, y + h, br).unwrap_or(());
        self.line_to(x + bl, y + h);
        self.arc_to(x, y + h, x, y + h - bl, bl).unwrap_or(());
        self.line_to(x, y + tl);
        self.arc_to(x, y, x + tl, y, tl).unwrap_or(());
        self.close_path();
        self.fill();
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
    fn get_transform_translation(&self) -> (f64, f64) {
        let matrix = self.get_transform().unwrap();
        (matrix.e(), matrix.f())
    }
}

