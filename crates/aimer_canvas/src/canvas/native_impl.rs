use aimer_attribute::position::Vec2d;
use aimer_attribute::size::ResolvedSize;
use aimer_color::prelude::Color;
use aimer_cupid::canvas::CupidCanvas;
use aimer_cupid::svg::{SvgNodeStyleOverride, SvgScene};
use aimer_cupid::text_pipeline::TextOverflowMode;
use aimer_cupid::utilities::Color as CupidColor;
use aimer_font::{FontFamily, FontStyle};
use std::sync::Arc;

use crate::canvas::CanvasRendering;

#[allow(dead_code)]
impl CanvasRendering for CupidCanvas {
    #[inline]
    fn begin_frame(&self) {
        CupidCanvas::begin_frame(self);
    }

    #[inline]
    fn fill_rect(&self, pos: Vec2d, size: ResolvedSize) {
        CupidCanvas::fill_rect(
            self,
            pos.x,
            pos.y,
            size.width,
            size.height,
            CupidColor::black(),
            [0.0; 4],
        );
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
        CupidCanvas::fill_rect_with_border(
            self,
            pos.x,
            pos.y,
            size.width,
            size.height,
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
            pos.x,
            pos.y,
            size.width,
            size.height,
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
    fn draw_text(&self, text: &str, pos: Vec2d, font_size: f32, color: Color, font_weight: u16) {
        CupidCanvas::draw_text(
            self,
            pos.x,
            pos.y,
            text,
            font_size,
            CupidColor::from(color),
            font_weight,
        );
    }

    #[inline]
    fn draw_text_styled(
        &self,
        text: &str,
        pos: Vec2d,
        font_size: f32,
        color: Color,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) {
        CupidCanvas::draw_text_styled(
            self,
            pos.x,
            pos.y,
            text,
            font_size,
            CupidColor::from(color),
            font_family,
            font_style,
            font_weight,
        );
    }

    #[inline]
    fn draw_text_wrapped(
        &self,
        text: &str,
        pos: Vec2d,
        font_size: f32,
        color: Color,
        max_width: f32,
        font_weight: u16,
    ) {
        CupidCanvas::draw_text_wrapped(
            self,
            pos.x,
            pos.y,
            text,
            font_size,
            CupidColor::from(color),
            max_width,
            font_weight,
        );
    }

    #[inline]
    fn draw_text_wrapped_styled(
        &self,
        text: &str,
        pos: Vec2d,
        font_size: f32,
        color: Color,
        max_width: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) {
        CupidCanvas::draw_text_wrapped_styled(
            self,
            pos.x,
            pos.y,
            text,
            font_size,
            CupidColor::from(color),
            max_width,
            font_family,
            font_style,
            font_weight,
        );
    }

    #[inline]
    fn draw_text_with_overflow(
        &self,
        text: &str,
        pos: Vec2d,
        font_size: f32,
        color: Color,
        bounds_width: f32,
        bounds_height: f32,
        overflow: TextOverflowMode,
        font_weight: u16,
    ) {
        CupidCanvas::draw_text_with_overflow(
            self,
            pos.x,
            pos.y,
            text,
            font_size,
            CupidColor::from(color),
            bounds_width,
            bounds_height,
            overflow,
            font_weight,
        );
    }

    #[inline]
    fn draw_text_with_overflow_styled(
        &self,
        text: &str,
        pos: Vec2d,
        font_size: f32,
        color: Color,
        bounds_width: f32,
        bounds_height: f32,
        overflow: TextOverflowMode,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) {
        CupidCanvas::draw_text_with_overflow_styled(
            self,
            pos.x,
            pos.y,
            text,
            font_size,
            CupidColor::from(color),
            bounds_width,
            bounds_height,
            overflow,
            font_family,
            font_style,
            font_weight,
        );
    }

    #[inline]
    fn draw_image(&self, image_id: u32, pos: Vec2d, size: ResolvedSize) {
        CupidCanvas::draw_image(self, pos.x, pos.y, size.width, size.height, image_id);
    }

    #[inline]
    fn draw_svg(
        &self,
        scene: Arc<SvgScene>,
        pos: Vec2d,
        size: ResolvedSize,
        overrides: Arc<[SvgNodeStyleOverride]>,
    ) {
        CupidCanvas::draw_svg(self, scene, pos.x, pos.y, size.width, size.height, overrides);
    }

    #[inline]
    fn get_image_size(&self, image_id: u32) -> Option<(u32, u32)> {
        CupidCanvas::get_image_size(self, image_id)
    }

    #[inline]
    fn set_clip(&self, pos: Vec2d, size: ResolvedSize) {
        CupidCanvas::set_clip(self, pos.x, pos.y, size.width, size.height);
    }

    #[inline]
    fn set_clip_rounded(&self, pos: Vec2d, size: ResolvedSize, border_radius: [f32; 4]) {
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
    fn measure_text_styled(
        &self,
        text: &str,
        font_size: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) -> f32 {
        CupidCanvas::measure_text_styled(
            self,
            text,
            font_size,
            font_family,
            font_style,
            font_weight,
        )
    }

    #[inline]
    fn measure_text_metrics(
        &self,
        text: &str,
        font_size: f32,
        max_width: f32,
    ) -> crate::canvas::TextMetrics {
        CupidCanvas::measure_text_metrics(self, text, font_size, max_width)
    }

    #[inline]
    fn measure_text_metrics_styled(
        &self,
        text: &str,
        font_size: f32,
        max_width: f32,
        font_family: FontFamily,
        font_style: FontStyle,
        font_weight: u16,
    ) -> crate::canvas::TextMetrics {
        CupidCanvas::measure_text_metrics_styled(
            self,
            text,
            font_size,
            max_width,
            font_family,
            font_style,
            font_weight,
        )
    }

    #[inline]
    fn stroke_rect(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        stroke_color: Color,
        stroke_width: f32,
        border_radius: [f32; 4],
    ) {
        CupidCanvas::stroke_rect(
            self,
            pos.x,
            pos.y,
            size.width,
            size.height,
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
            pos.x,
            pos.y,
            size.width,
            size.height,
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
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
        outline_width: f32,
        outline_color: Color,
    ) {
        CupidCanvas::fill_rect_with_border_and_outline(
            self,
            pos.x,
            pos.y,
            size.width,
            size.height,
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
            pos.x,
            pos.y,
            size.width,
            size.height,
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
        border_radius: [f32; 4],
    ) {
        CupidCanvas::fill_color_rect(
            self,
            pos.x,
            pos.y,
            size.width,
            size.height,
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
            pos.x,
            pos.y,
            size.width,
            size.height,
            CupidColor::from(color),
            border_radius,
        );
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    fn draw_text_decoration(
        &self,
        pos: Vec2d,
        size: ResolvedSize,
        color: Color,
        style: u32,
        thickness: f32,
        period: f32,
    ) {
        CupidCanvas::draw_text_decoration(
            self,
            pos.x,
            pos.y,
            size.width,
            size.height,
            CupidColor::from(color),
            style,
            thickness,
            period,
        );
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
        side_params: [f32; 3],
    ) {
        CupidCanvas::draw_shadow_rect(
            self,
            pos.x,
            pos.y,
            size.width,
            size.height,
            CupidColor::from(shadow_color),
            shadow_params,
            border_radius,
            inset,
            side_params,
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
    fn set_italic(&self, italic: bool) {
        CupidCanvas::set_italic(self, italic);
    }

    #[inline]
    fn load_image(&self, bytes: &[u8], width: u32, height: u32) -> u32 {
        self.load_image(bytes, width, height)
    }

    fn load_image_with_id(&self, image_id: u32, bytes: &[u8], width: u32, height: u32) {
        self.load_image_with_id(image_id, bytes, width, height)
    }

    fn remove_texture(&self, image_id: u32) {
        self.remove_texture(image_id)
    }

    #[inline]
    fn set_texture_size(&self, image_id: u32, width: u32, height: u32) {
        self.set_texture_size(image_id, width, height)
    }

    #[inline]
    fn get_transform_translation(&self) -> (f32, f32) {
        let (tx, ty) = CupidCanvas::get_transform_translation(self);
        (tx, ty)
    }
}
