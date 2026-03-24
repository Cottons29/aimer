use crate::canvas::CanvasRendering;
use attribute::position::Vec2d;
use attribute::size::ResolvedSize;
use color::prelude::Color;
use cupid::canvas::CupidCanvas;
use cupid::utilities::Color as CupidColor;

#[allow(dead_code)]
impl CanvasRendering for CupidCanvas {

    #[inline]
    fn begin_frame(&self) {
        CupidCanvas::begin_frame(self);
    }

    #[inline]
    fn fill_rect(&self, pos: Vec2d, size: ResolvedSize) {
        CupidCanvas::fill_rect(self, pos.x, pos.y, size.width, size.height, CupidColor::black(), 0.0);
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
        CupidCanvas::fill_rect_with_border(
            self,
            pos.x, pos.y, size.width, size.height,
            CupidColor::from(color),
            border_radius,
            border_width,
            CupidColor::from(border_color),
        );
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
        CupidCanvas::fill_rect_with_per_side_border(
            self,
            pos.x, pos.y, size.width, size.height,
            CupidColor::from(color),
            border_radius,
            border_width,
            CupidColor::from(border_color),
        );
    }

    #[inline]
    fn clear_rect(&self, pos: Vec2d, size: ResolvedSize) {
        CupidCanvas::clear_rect(self, pos.x, pos.y, size.width, size.height);
    }

    #[inline]
    fn translate(&self, pos: Vec2d) {
        CupidCanvas::translate(self, pos.x, pos.y);
    }

    #[inline]
    fn scale(&self, sx: f32, sy: f32) {
        CupidCanvas::scale(self, sx, sy);
    }

    #[inline]
    fn rotate(&self, radians: f32) {
        CupidCanvas::rotate(self, radians);
    }

    #[inline]
    fn save(&self) {
        CupidCanvas::save(self);
    }

    #[inline]
    fn restore(&self) {
        CupidCanvas::restore(self);
    }

    #[inline]
    fn draw_text(&self, text: &str, pos: Vec2d, font_size: f32, color: Color) {
        CupidCanvas::draw_text(self, pos.x, pos.y, text, font_size, CupidColor::from(color));
    }

    #[inline]
    fn draw_image(&self, image_id: u32, pos: Vec2d, size: ResolvedSize) {
        CupidCanvas::draw_image(self, pos.x, pos.y, size.width, size.height, image_id);
    }

    #[inline]
    fn set_clip(&self, pos: Vec2d, size: ResolvedSize) {
        CupidCanvas::set_clip(self, pos.x, pos.y, size.width, size.height);
    }

    #[inline]
    fn set_clip_rounded(&self, pos: Vec2d, size: ResolvedSize, border_radius: f32) {
        CupidCanvas::set_clip_rounded(self, pos.x, pos.y, size.width, size.height, border_radius);
    }

    #[inline]
    fn clear_clip(&self) {
        CupidCanvas::clear_clip(self);
    }

    #[inline]
    fn measure_text(&self, text: &str, font_size: f32) -> f32 {
        CupidCanvas::measure_text(self, text, font_size)
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
        CupidCanvas::stroke_rect(
            self,
            pos.x, pos.y, size.width, size.height,
            CupidColor::from(stroke_color),
            stroke_width,
            border_radius,
        );
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
        CupidCanvas::stroke_rect_per_side(
            self,
            pos.x, pos.y, size.width, size.height,
            CupidColor::from(stroke_color),
            stroke_width,
            border_radius,
        );
    }

    #[inline]
    fn fill_rect_with_border_and_outline(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: f32,
        border_width: f32,
        border_color: Color,
        outline_width: f32,
        outline_color: Color,
    ) {
        CupidCanvas::fill_rect_with_border_and_outline(
            self,
            pos.x, pos.y, size.width, size.height,
            CupidColor::from(color),
            border_radius,
            border_width,
            CupidColor::from(border_color),
            outline_width,
            CupidColor::from(outline_color),
        );
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
        CupidCanvas::fill_rect_with_border_and_outline_per_side(
            self,
            pos.x, pos.y, size.width, size.height,
            CupidColor::from(color),
            border_radius,
            border_width,
            CupidColor::from(border_color),
            outline_width,
            CupidColor::from(outline_color),
        );
    }

    #[inline]
    fn fill_color_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: f32,
    ) {
        CupidCanvas::fill_color_rect(
            self,
            pos.x, pos.y, size.width, size.height,
            CupidColor::from(color),
            border_radius,
        );
    }

    #[inline]
    fn fill_color_rect_per_corner(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        border_radius: [f32; 4],
    ) {
        CupidCanvas::fill_color_rect_per_corner(
            self,
            pos.x, pos.y, size.width, size.height,
            CupidColor::from(color),
            border_radius,
        );
    }

    #[inline]
    fn set_alpha(&self, alpha: f32) {
        CupidCanvas::set_alpha(self, alpha);
    }

    #[inline]
    fn restore_alpha(&self) {
        CupidCanvas::restore_alpha(self);
    }

    #[inline]
    fn get_transform_translation(&self) -> (f64, f64) {
        let (tx, ty) = CupidCanvas::get_transform_translation(self);
        (tx as f64, ty as f64)
    }
}
