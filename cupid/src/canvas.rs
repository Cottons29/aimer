use std::cell::{Ref, RefCell};
use std::rc::Rc;

use crate::draw_cmd::DrawList;
use crate::pipeline::glyph_rasterizer::GlyphRasterizer;
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

    pub fn fill_rect(&self, x: f32, y: f32, width: f32, height: f32, color: Color, border_radius: f32) {
        self.draw_list.borrow_mut().fill_rect(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            0.0,
            Color::transparent(),
        );
    }

    pub fn fill_rect_with_border(
        &self,
        x: f32, y: f32, width: f32, height: f32,
        color: Color,
        border_radius: f32,
        border_width: f32,
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

    /// Draws a stroked (outline-only) rectangle.
    pub fn stroke_rect(
        &self,
        x: f32, y: f32, width: f32, height: f32,
        stroke_color: Color,
        stroke_width: f32,
        border_radius: f32,
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
        border_radius: f32,
    ) {
        self.draw_list.borrow_mut().fill_rect(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            0.0,
            Color::transparent(),
        );
    }

    pub fn set_clip(&self, x: f32, y: f32, width: f32, height: f32) {
        self.draw_list.borrow_mut().push_clip(Rect::new(x, y, width, height));
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

    pub fn draw_list(&self) -> Ref<'_, DrawList> {
        self.draw_list.borrow()
    }
}

impl Default for CupidCanvas {
    fn default() -> Self {
        Self::new()
    }
}