use crate::text_pipeline::TextOverflowMode;
use crate::utilities::{Color, Mat3, Rect, TextureId, Vec2d};
use std::any::Any;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct RichTextSegment {
    // Shared reference-counted text so cloning a segment (the draw list is
    // rebuilt every frame) does not reallocate the string.
    pub text: Arc<str>,
    pub font_size: Option<f32>,
    pub color: Option<Color>,
    pub font_weight: Option<u16>,
    pub italic: Option<bool>,
}

impl RichTextSegment {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self { text: text.into(), font_size: None, color: None, font_weight: None, italic: None }
    }

    pub fn with_style(mut self, font_size: Option<f32>, color: Option<Color>) -> Self {
        self.font_size = font_size;
        self.color = color;
        self
    }
}

pub enum DrawCommand {
    FillRect {
        rect: Rect,
        color: Color,
        /// Per-corner border radius: [top-left, top-right, bottom-right, bottom-left]
        border_radius: [f32; 4],
        /// Per-side border width: [top, right, bottom, left]
        border_width: [f32; 4],
        border_color: Color,
        /// Per-side outline width: [top, right, bottom, left]
        outline_width: [f32; 4],
        outline_color: Color,
    },
    ClearRect {
        rect: Rect,
    },
    DrawText {
        position: Vec2d,
        text: Arc<str>,
        font_size: f32,
        color: Color,
        bounds_width: Option<f32>,
        bounds_height: Option<f32>,
        overflow: TextOverflowMode,
        font_weight: u16,
    },
    DrawRichText {
        position: Vec2d,
        spans: Vec<RichTextSegment>,
        font_size: f32,
        color: Color,
        bounds_width: Option<f32>,
        bounds_height: Option<f32>,
        overflow: TextOverflowMode,
    },
    PushClip {
        rect: Rect,
        border_radius: [f32; 4],
    },
    PopClip,
    PushTransform {
        matrix: Mat3,
    },
    PopTransform,
    SetAlpha {
        alpha: f32,
    },
    RestoreAlpha,
    LoadImage {
        bytes: Vec<u8>,
        texture_id: TextureId,
        width: u32,
        height: u32,
    },
    LoadImageWithId {
        texture_id: TextureId,
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    DrawImage {
        rect: Rect,
        texture_id: TextureId,
    },
    SetTransform {
        matrix: Mat3,
    },
    DrawShadowRect {
        rect: Rect,
        shadow_color: Color,
        /// [offset_x, offset_y, blur, spread]
        shadow_params: [f32; 4],
        border_radius: [f32; 4],
        inset: bool,
        /// [side_type, angle_start, angle_end]
        side_params: [f32; 3],
    },
    /// Draw using a user-registered custom pipeline.
    /// `pipeline_name` must match the name returned by `CustomPipeline::name()`.
    /// `data` is an arbitrary payload forwarded to `CustomPipeline::prepare()`.
    Custom {
        pipeline_name: String,
        data: Box<dyn Any + Send>,
    },
}

pub struct DrawList {
    commands: Vec<DrawCommand>,
    transform_stack: Vec<Mat3>,
    current_transform: Mat3,
    texture_sizes: HashMap<TextureId, (u32, u32)>,
}

impl DrawList {
    pub fn new() -> Self {
        Self {
            commands: Vec::with_capacity(512),
            transform_stack: Vec::with_capacity(512),
            current_transform: Mat3::identity(),
            texture_sizes: HashMap::new(),
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

    pub fn fill_rect(&mut self, rect: Rect, color: Color, border_radius: [f32; 4], border_width: [f32; 4], border_color: Color) {
        self.commands.push(DrawCommand::FillRect {
            rect,
            color,
            border_radius,
            border_width,
            border_color,
            outline_width: [0.0; 4],
            outline_color: Color::transparent(),
        });
    }

    /// Enqueue a draw command for a user-registered custom pipeline.
    /// `pipeline_name` must match `CustomPipeline::name()` of a registered pipeline.
    /// `data` is an arbitrary payload that will be forwarded to `CustomPipeline::prepare()`.
    pub fn draw_custom(&mut self, pipeline_name: impl Into<String>, data: impl Any + Send) {
        self.commands
            .push(DrawCommand::Custom { pipeline_name: pipeline_name.into(), data: Box::new(data) });
    }

    pub fn draw_shadow_rect(
        &mut self,
        rect: Rect,
        shadow_color: Color,
        shadow_params: [f32; 4],
        border_radius: [f32; 4],
        inset: bool,
        side_params: [f32; 3],
    ) {
        self.commands
            .push(DrawCommand::DrawShadowRect { rect, shadow_color, shadow_params, border_radius, inset, side_params });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect_with_outline(
        &mut self,
        rect: Rect,
        color: Color,
        border_radius: [f32; 4],
        border_width: [f32; 4],
        border_color: Color,
        outline_width: [f32; 4],
        outline_color: Color,
    ) {
        self.commands
            .push(DrawCommand::FillRect { rect, color, border_radius, border_width, border_color, outline_width, outline_color });
    }

    pub fn clear_rect(&mut self, rect: Rect) {
        self.commands.push(DrawCommand::ClearRect { rect });
    }

    pub fn draw_image(&mut self, rect: Rect, texture_id: TextureId) {
        self.commands
            .push(DrawCommand::DrawImage { rect, texture_id });
    }

    pub fn draw_text(&mut self, position: Vec2d, text: Arc<str>, font_size: f32, color: Color, font_weight: u16) {
        self.draw_text_with_overflow(position, text, font_size, color, None, None, TextOverflowMode::Clip, font_weight);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text_with_overflow(
        &mut self,
        position: Vec2d,
        text: Arc<str>,
        font_size: f32,
        color: Color,
        bounds_width: Option<f32>,
        bounds_height: Option<f32>,
        overflow: TextOverflowMode,
        font_weight: u16,
    ) {
        self.commands
            .push(DrawCommand::DrawText { position, text, font_size, color, bounds_width, bounds_height, overflow, font_weight });
    }

    pub fn draw_rich_text(&mut self, position: Vec2d, spans: Vec<RichTextSegment>, font_size: f32, color: Color) {
        self.draw_rich_text_with_overflow(position, spans, font_size, color, None, None, TextOverflowMode::Clip);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_rich_text_with_overflow(
        &mut self,
        position: Vec2d,
        spans: Vec<RichTextSegment>,
        font_size: f32,
        color: Color,
        bounds_width: Option<f32>,
        bounds_height: Option<f32>,
        overflow: TextOverflowMode,
    ) {
        self.commands
            .push(DrawCommand::DrawRichText { position, spans, font_size, color, bounds_width, bounds_height, overflow });
    }

    pub fn push_clip(&mut self, rect: Rect) {
        self.commands
            .push(DrawCommand::PushClip { rect, border_radius: [0.0; 4] });
    }

    pub fn push_clip_rounded(&mut self, rect: Rect, border_radius: [f32; 4]) {
        self.commands
            .push(DrawCommand::PushClip { rect, border_radius });
    }

    pub fn pop_clip(&mut self) {
        self.commands.push(DrawCommand::PopClip);
    }

    pub fn set_alpha(&mut self, alpha: f32) {
        self.commands.push(DrawCommand::SetAlpha { alpha });
    }

    pub fn restore_alpha(&mut self) {
        self.commands.push(DrawCommand::RestoreAlpha);
    }

    pub fn load_image(&mut self, bytes: &[u8], width: u32, height: u32) -> TextureId {
        // Hash only a small sample of the buffer to avoid O(n) cost on large images.
        let texture_id = {
            let mut hasher = fxhash::FxHasher::default();
            width.hash(&mut hasher);
            height.hash(&mut hasher);
            let sample_len = 256.min(bytes.len());
            if sample_len > 0 {
                bytes[..sample_len].hash(&mut hasher);
                bytes[bytes.len() - sample_len..].hash(&mut hasher);
            }
            hasher.finish() as u32
        };
        // Record size always so future frames can query it, even if we already queued the load.
        self.set_texture_size(texture_id, width, height);
        if self.has_texture_id(texture_id) {
            return texture_id;
        }
        self.commands
            .push(DrawCommand::LoadImage { bytes: bytes.to_vec(), texture_id, width, height });
        texture_id
    }

    pub fn load_image_with_id(&mut self, texture_id: TextureId, bytes: &[u8], width: u32, height: u32) {
        self.set_texture_size(texture_id, width, height);
        self.commands
            .push(DrawCommand::LoadImageWithId { texture_id, bytes: bytes.to_vec(), width, height });
    }

    pub fn set_texture_size(&mut self, texture_id: TextureId, width: u32, height: u32) {
        self.texture_sizes.insert(texture_id, (width, height));
    }

    pub fn save(&mut self) {
        self.transform_stack.push(self.current_transform);
        self.commands
            .push(DrawCommand::PushTransform { matrix: self.current_transform });
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
        self.commands
            .push(DrawCommand::SetTransform { matrix: self.current_transform });
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        let s = Mat3::scale(sx, sy);
        self.current_transform = self.current_transform.mul(&s);
        self.commands
            .push(DrawCommand::SetTransform { matrix: self.current_transform });
    }

    pub fn rotate(&mut self, radians: f32) {
        let r = Mat3::rotate(radians);
        self.current_transform = self.current_transform.mul(&r);
        self.commands
            .push(DrawCommand::SetTransform { matrix: self.current_transform });
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

    pub fn has_texture_id(&self, texture_id: TextureId) -> bool {
        self.commands.iter().any(|cmd| match cmd {
            DrawCommand::DrawImage { texture_id: id, .. } => *id == texture_id,
            _ => false,
        })
    }

    pub fn get_texture_size(&self, texture_id: TextureId) -> Option<(u32, u32)> {
        self.texture_sizes.get(&texture_id).copied()
    }
}

impl Default for DrawList {
    fn default() -> Self {
        Self::new()
    }
}
