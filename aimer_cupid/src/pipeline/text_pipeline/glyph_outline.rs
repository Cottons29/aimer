use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{GlyphId, MetadataProvider};

use crate::glyph_rasterizer::{RasterizedGlyph, point_inside};
use crate::text_pipeline::font_resolver::{FontRecord, advance_width_from_face, font_ref};

#[derive(Default)]
struct GlyphOutline {
    contours: Vec<Vec<(f32, f32)>>,
    current: Vec<(f32, f32)>,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl GlyphOutline {
    fn new(scale: f32, offset_x: f32, offset_y: f32) -> Self {
        Self {
            scale,
            offset_x,
            offset_y,
            ..Self::default()
        }
    }

    fn push_point(&mut self, x: f32, y: f32) {
        self.current.push((
            x * self.scale - self.offset_x,
            y * self.scale - self.offset_y,
        ));
    }

    fn finish_contour(&mut self) {
        if self.current.len() >= 2 {
            self.contours.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
    }
}

impl OutlinePen for GlyphOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish_contour();
        self.push_point(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push_point(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let Some(&(x0, y0)) = self.current.last() else {
            return;
        };
        let x1 = x1 * self.scale - self.offset_x;
        let y1 = y1 * self.scale - self.offset_y;
        let x2 = x * self.scale - self.offset_x;
        let y2 = y * self.scale - self.offset_y;
        for step in 1..=12 {
            let t = step as f32 / 12.0;
            let mt = 1.0 - t;
            self.current.push((
                mt * mt * x0 + 2.0 * mt * t * x1 + t * t * x2,
                mt * mt * y0 + 2.0 * mt * t * y1 + t * t * y2,
            ));
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let Some(&(x0, y0)) = self.current.last() else {
            return;
        };
        let x1 = x1 * self.scale - self.offset_x;
        let y1 = y1 * self.scale - self.offset_y;
        let x2 = x2 * self.scale - self.offset_x;
        let y2 = y2 * self.scale - self.offset_y;
        let x3 = x * self.scale - self.offset_x;
        let y3 = y * self.scale - self.offset_y;
        for step in 1..=16 {
            let t = step as f32 / 16.0;
            let mt = 1.0 - t;
            self.current.push((
                mt * mt * mt * x0 + 3.0 * mt * mt * t * x1 + 3.0 * mt * t * t * x2 + t * t * t * x3,
                mt * mt * mt * y0 + 3.0 * mt * mt * t * y1 + 3.0 * mt * t * t * y2 + t * t * t * y3,
            ));
        }
    }

    fn close(&mut self) {
        self.finish_contour();
    }
}

pub(crate) fn rasterize_outline_glyph(
    record: &FontRecord,
    glyph_id: u16,
    font_size: f32,
) -> Option<RasterizedGlyph> {
    let data = record.data()?;
    let data = data.as_ref();
    let face = font_ref(data, record.collection_index)?;
    let glyph = GlyphId::new(glyph_id as u32);
    let metrics = face.glyph_metrics(Size::unscaled(), LocationRef::default());
    let bbox = metrics.bounds(glyph)?;
    let units_per_em = f32::from(
        face.metrics(Size::unscaled(), LocationRef::default())
            .units_per_em,
    );
    let scale = font_size / units_per_em;
    let offset_x = bbox.x_min * scale;
    let offset_y = bbox.y_min * scale;
    let width = ((bbox.x_max - bbox.x_min) * scale).ceil().max(1.0) as u32;
    let height = ((bbox.y_max - bbox.y_min) * scale).ceil().max(1.0) as u32;

    let mut outline = GlyphOutline::new(scale, offset_x, offset_y);
    face.outline_glyphs()
        .get(glyph)?
        .draw(
            DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
            &mut outline,
        )
        .ok()?;
    outline.finish_contour();

    let mut bitmap = vec![0u8; (width * height) as usize];
    const SAMPLES: u32 = 4;
    let sample_count = (SAMPLES * SAMPLES) as f32;
    for y in 0..height {
        for x in 0..width {
            let mut covered = 0u32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let py = height as f32 - (y as f32 + (sy as f32 + 0.5) / SAMPLES as f32);
                    if point_inside(&outline.contours, px, py) {
                        covered += 1;
                    }
                }
            }
            bitmap[(y * width + x) as usize] =
                ((covered as f32 / sample_count) * 255.0).round() as u8;
        }
    }

    Some(RasterizedGlyph {
        bitmap,
        width,
        height,
        offset_x,
        offset_y,
        advance_width: advance_width_from_face(data, record.collection_index, glyph_id, font_size)?,
        is_color: false,
    })
}
