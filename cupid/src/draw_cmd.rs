use crate::utilities::{Color, Mat3, Rect, TextureId, Vec2d};

pub enum DrawCommand {
    FillRect {
        rect: Rect,
        color: Color,
        border_radius: f32,
    },
    ClearRect {
        rect: Rect,
    },
    DrawImage {
        rect: Rect,
        texture_id: TextureId,
    },
    DrawText {
        position: Vec2d,
        text: String,
        font_size: f32,
        color: Color,
    },
    PushClip {
        rect: Rect,
    },
    PopClip,
    PushTransform {
        matrix: Mat3,
    },
    PopTransform,
}

pub struct DrawList {
    commands: Vec<DrawCommand>,
    transform_stack: Vec<Mat3>,
    current_transform: Mat3,
}

impl DrawList {
    pub const fn new() -> Self {
        Self {
            commands: Vec::new(),
            transform_stack: Vec::new(),
            current_transform: Mat3::identity(),
        }
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.transform_stack.clear();
        self.current_transform = Mat3::identity();
    }

    pub fn push(&mut self, cmd: DrawCommand) {
        self.commands.push(cmd);
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color, border_radius: f32) {
        self.commands.push(DrawCommand::FillRect {
            rect,
            color,
            border_radius,
        });
    }

    pub fn clear_rect(&mut self, rect: Rect) {
        self.commands.push(DrawCommand::ClearRect { rect });
    }

    pub fn draw_image(&mut self, rect: Rect, texture_id: TextureId) {
        self.commands.push(DrawCommand::DrawImage { rect, texture_id });
    }

    pub fn draw_text(&mut self, position: Vec2d, text: String, font_size: f32, color: Color) {
        self.commands.push(DrawCommand::DrawText {
            position,
            text,
            font_size,
            color,
        });
    }

    pub fn push_clip(&mut self, rect: Rect) {
        self.commands.push(DrawCommand::PushClip { rect });
    }

    pub fn pop_clip(&mut self) {
        self.commands.push(DrawCommand::PopClip);
    }

    pub fn save(&mut self) {
        self.transform_stack.push(self.current_transform);
        self.commands.push(DrawCommand::PushTransform {
            matrix: self.current_transform,
        });
    }

    pub fn restore(&mut self) {
        if let Some(prev) = self.transform_stack.pop() {
            self.current_transform = prev;
            self.commands.push(DrawCommand::PopTransform);
        }
    }

    pub fn translate(&mut self, x: f32, y: f32) {
        let t = Mat3::translate(x, y);
        self.current_transform = self.current_transform.mul(&t);
    }

    pub fn current_transform(&self) -> &Mat3 {
        &self.current_transform
    }

    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    pub fn drain_commands(&mut self) -> Vec<DrawCommand> {
        std::mem::take(&mut self.commands)
    }
}

impl Default for DrawList {
    fn default() -> Self {
        Self::new()
    }
}