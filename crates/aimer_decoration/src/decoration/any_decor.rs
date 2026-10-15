use aimer_attribute::{ResolvedSize, Vec2d};
use aimer_canvas::Canvas;
use aimer_color::prelude::Color;

use super::{AnyDecor, Decoration};

impl Decoration for AnyDecor {
    #[inline]
    fn paint(&self, canvas: &Canvas, origin: Vec2d, size: ResolvedSize, scale: f32) {
        self.as_ref().paint(canvas, origin, size, scale);
    }

    #[inline]
    fn paint_layer(&self, canvas: &Canvas, origin: Vec2d, size: ResolvedSize, scale: f32) {
        self.as_ref().paint_layer(canvas, origin, size, scale);
    }

    #[inline]
    fn paint_with_background(
        &self, canvas: &Canvas, origin: Vec2d, size: ResolvedSize, scale: f32,
        background: Option<Color>,
    ) {
        self.as_ref().paint_with_background(canvas, origin, size, scale, background);
    }

    #[inline]
    fn paint_outsets(&self, size: ResolvedSize, scale: f32) -> [f32; 4] {
        self.as_ref().paint_outsets(size, scale)
    }

    #[inline]
    fn content_insets(&self, size: ResolvedSize, scale: f32) -> [f32; 4] {
        self.as_ref().content_insets(size, scale)
    }

    #[inline]
    fn child_clip_radius(&self, size: ResolvedSize, scale: f32) -> Option<[f32; 4]> {
        self.as_ref().child_clip_radius(size, scale)
    }

    #[inline]
    fn background_color(&self) -> Option<Color> {
        self.as_ref().background_color()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.as_ref().is_empty()
    }

    #[inline]
    fn revision(&self) -> u64 {
        self.as_ref().revision()
    }

    #[inline]
    fn notype(self) -> AnyDecor {
        self
    }
}
