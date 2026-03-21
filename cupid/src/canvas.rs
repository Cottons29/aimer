use crate::draw_cmd::DrawList;
use crate::utilities::{Color, Rect, TextureId, Vec2d};

pub struct CupidCanvas {
    draw_list: DrawList,
}

impl CupidCanvas {
    pub const fn new() -> Self {
        Self {
            draw_list: DrawList::new(),
        }
    }

    pub fn begin_frame(&mut self) {
        self.draw_list.clear();
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color, border_radius: f32) {
        self.draw_list.fill_rect(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            0.0,
            Color::transparent(),
        );
    }

    pub fn fill_rect_with_border(
        &mut self,
        x: f32, y: f32, width: f32, height: f32,
        color: Color,
        border_radius: f32,
        border_width: f32,
        border_color: Color,
    ) {
        self.draw_list.fill_rect(
            Rect::new(x, y, width, height),
            color,
            border_radius,
            border_width,
            border_color,
        );
    }

    pub fn clear_rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.draw_list.clear_rect(Rect::new(x, y, width, height));
    }

    pub fn translate(&mut self, x: f32, y: f32) {
        self.draw_list.translate(x, y);
    }

    pub fn save(&mut self) {
        self.draw_list.save();
    }

    pub fn restore(&mut self) {
        self.draw_list.restore();
    }

    pub fn draw_text(&mut self, x: f32, y: f32, text: &str, font_size: f32, color: Color) {
        self.draw_list.draw_text(
            Vec2d::new(x, y),
            text.to_string(),
            font_size,
            color,
        );
    }

    pub fn draw_image(&mut self, x: f32, y: f32, width: f32, height: f32, texture_id: TextureId) {
        self.draw_list.draw_image(
            Rect::new(x, y, width, height),
            texture_id,
        );
    }

    pub fn set_clip(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.draw_list.push_clip(Rect::new(x, y, width, height));
    }

    pub fn clear_clip(&mut self) {
        self.draw_list.pop_clip();
    }

    pub fn draw_list(&self) -> &DrawList {
        &self.draw_list
    }
}

impl Default for CupidCanvas {
    fn default() -> Self {
        Self::new()
    }
}