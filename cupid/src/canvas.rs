use std::cell::{Ref, RefCell};
use std::rc::Rc;

use crate::draw_cmd::DrawList;
use crate::text_pipeline::glyph_rasterizer::GlyphRasterizer;
use crate::utilities::{Color, Rect, TextureId, Vec2d};

#[derive(Clone)]
pub struct CupidCanvas {
    draw_list: Rc<RefCell<DrawList>>,
    rasterizer: Rc<RefCell<GlyphRasterizer>>,
}


impl CupidCanvas {
    pub fn new() -> Self {
        Self {
            draw_list: Rc::new(RefCell::new(DrawList::new())),
            rasterizer: Rc::new(RefCell::new(GlyphRasterizer::primary_only())),
        }
    }

    pub fn begin_frame(&self) {
        self.draw_list.borrow_mut().clear();
    }
    

    pub fn fill_rect(&self, x: f32, y: f32, width: f32, height: f32, color: Color, border_radius: [f32; 4]) {
        self.draw_list.borrow_mut().fill_rect(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            [0.0; 4],
            Color::transparent(),
        );
    }

    pub fn fill_rect_with_border(
        &self,
        x: f32, y: f32, width: f32, height: f32,
        color: Color,
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
    ) {
        self.draw_list.borrow_mut().fill_rect(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            [border_width; 4],
            border_color,
        );
    }

    /// Draws a filled rectangle with per-corner border radii and per-side border widths.
    /// `border_radius`: [top-left, top-right, bottom-right, bottom-left]
    /// `border_width`: [top, right, bottom, left]
    pub fn fill_rect_with_per_side_border(
        &self,
        x: f32, y: f32, width: f32, height: f32,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
    ) {
        self.draw_list.borrow_mut().fill_rect(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            border_width,
            border_color,
        );
    }

    pub fn clear_rect(&self, x: f32, y: f32, width: f32, height: f32) {
        self.draw_list.borrow_mut().clear_rect(Rect::new(x, y, width, height));
    }

    pub fn translate(&self, x: f32, y: f32) {
        self.draw_list.borrow_mut().translate(x, y);
    }

    pub fn scale(&self, sx: f32, sy: f32) {
        self.draw_list.borrow_mut().scale(sx, sy);
    }

    pub fn rotate(&self, radians: f32) {
        self.draw_list.borrow_mut().rotate(radians);
    }

    pub fn save(&self) {
        self.draw_list.borrow_mut().save();
    }

    pub fn restore(&self) {
        self.draw_list.borrow_mut().restore();
    }

    pub fn draw_text(&self, x: f32, y: f32, text: &str, font_size: f32, color: Color) {
        self.draw_list.borrow_mut().draw_text(
            Vec2d::new(x, y),
            text.to_string(),
            font_size,
            color,
        );
    }

    pub fn draw_image(&self, x: f32, y: f32, width: f32, height: f32, texture_id: TextureId) {
        self.draw_list.borrow_mut().draw_image(
            Rect::new(x, y, width, height),
            texture_id,
        );
    }

    /// Measure text width using the cached fontdue rasterizer.
    pub fn measure_text(&self, text: &str, font_size: f32) -> f32 {
        self.rasterizer.borrow_mut().measure_text(text, font_size)
    }

    /// Draws a filled rectangle with border and outline in a single pass (no gap).
    pub fn fill_rect_with_border_and_outline(
        &self,
        x: f32, y: f32, width: f32, height: f32,
        color: Color,
        border_radius: [f32; 4],
        border_width: f32,
        border_color: Color,
        outline_width: f32,
        outline_color: Color,
    ) {
        self.draw_list.borrow_mut().fill_rect_with_outline(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            [border_width; 4],
            border_color,
            [outline_width; 4],
            outline_color,
        );
    }

    /// Draws a filled rectangle with border and outline with per-corner/per-side control.
    pub fn fill_rect_with_border_and_outline_per_side(
        &self,
        x: f32, y: f32, width: f32, height: f32,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
        outline_width: [f32; 4],
        outline_color: Color,
    ) {
        self.draw_list.borrow_mut().fill_rect_with_outline(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            border_width,
            border_color,
            outline_width,
            outline_color,
        );
    }

    /// Draws a stroked (outline-only) rectangle.
    pub fn stroke_rect(
        &self,
        x: f32, y: f32, width: f32, height: f32,
        stroke_color: Color,
        stroke_width: f32,
        border_radius: [f32; 4],
    ) {
        self.draw_list.borrow_mut().fill_rect(
            Rect::new(x, y, width, height),
            Color::transparent(),
            border_radius,
            [stroke_width; 4],
            stroke_color,
        );
    }

    /// Draws a stroked (outline-only) rectangle with per-corner radii and per-side widths.
    /// `border_radius`: [top-left, top-right, bottom-right, bottom-left]
    /// `stroke_width`: [top, right, bottom, left]
    pub fn stroke_rect_per_side(
        &self,
        x: f32, y: f32, width: f32, height: f32,
        stroke_color: Color,
        stroke_width: [f32; 4],
        border_radius: [f32; 4],
    ) {
        self.draw_list.borrow_mut().fill_rect(
            Rect::new(x, y, width, height),
            Color::transparent(),
            border_radius,
            stroke_width,
            stroke_color,
        );
    }

    /// Draws a filled rectangle with a specific color (convenience method).
    pub fn fill_color_rect(
        &self,
        x: f32, y: f32, width: f32, height: f32,
        color: Color,
        border_radius: [f32; 4],
    ) {
        self.draw_list.borrow_mut().fill_rect(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            [0.0; 4],
            Color::transparent(),
        );
    }

    /// Draws a filled rectangle with per-corner border radii.
    /// `border_radius`: [top-left, top-right, bottom-right, bottom-left]
    pub fn fill_color_rect_per_corner(
        &self,
        x: f32, y: f32, width: f32, height: f32,
        color: Color,
        border_radius: [f32; 4],
    ) {
        self.draw_list.borrow_mut().fill_rect(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            [0.0; 4],
            Color::transparent(),
        );
    }

    pub fn set_clip(&self, x: f32, y: f32, width: f32, height: f32) {
        self.draw_list.borrow_mut().push_clip(Rect::new(x, y, width, height));
    }

    pub fn set_clip_rounded(&self, x: f32, y: f32, width: f32, height: f32, border_radius: [f32; 4]) {
        self.draw_list.borrow_mut().push_clip_rounded(Rect::new(x, y, width, height), border_radius);
    }

    pub fn clear_clip(&self) {
        self.draw_list.borrow_mut().pop_clip();
    }

    pub fn get_transform_translation(&self) -> (f32, f32) {
        let transform = self.draw_list.borrow();
        let t = transform.current_transform();
        (t.cols[2][0], t.cols[2][1])
    }

    pub fn set_alpha(&self, alpha: f32) {
        self.draw_list.borrow_mut().set_alpha(alpha);
    }

    pub fn restore_alpha(&self) {
        self.draw_list.borrow_mut().restore_alpha();
    }

    pub fn load_image(&self, bytes: &[u8], width: u32, height: u32) -> TextureId {
        self.draw_list.borrow_mut().load_image(bytes, width, height)
    }

    pub fn load_image_with_id(&self, texture_id: TextureId, bytes: &[u8], width: u32, height: u32) {
        self.draw_list.borrow_mut().load_image_with_id(texture_id, bytes, width, height)
    }

    pub fn set_texture_size(&self, texture_id: TextureId, width: u32, height: u32) {
        self.draw_list.borrow_mut().set_texture_size(texture_id, width, height);
    }

    pub fn draw_list(&self) -> Ref<'_, DrawList> {
        self.draw_list.borrow()
    }

    pub fn get_image_size(&self, texture_id: TextureId) -> Option<(u32, u32)> {
        self.draw_list.borrow().get_texture_size(texture_id)
    }
}

impl Default for CupidCanvas {
    fn default() -> Self {
        Self::new()
    }
}