use super::glyph_rasterizer::{GlyphKey, GlyphRasterizer};
use unicode_bidi::BidiInfo;
use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;
use aimer_utils::{debug, time_consume};

pub type FontId = u32;

#[derive(Clone, Debug, PartialEq)]
pub struct FontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub line_height: f32,
}

impl FontMetrics {
    pub fn from_rasterizer(rasterizer: &GlyphRasterizer, font_size: f32) -> Self {
        let (ascent, descent, line_gap) = rasterizer.line_metrics(font_size);
        Self::new(ascent, descent, line_gap)
    }

    pub fn new(ascent: f32, descent: f32, line_gap: f32) -> Self {
        Self { ascent, descent, line_gap, line_height: ascent - descent + line_gap }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLayoutOptions {
    pub origin_x: f32,
    pub origin_y: f32,
    pub max_width: f32,
    pub max_height: f32,
    pub font_size: f32,
    pub ellipsis: bool,
}

impl TextLayoutOptions {
    pub fn new(font_size: f32, origin_x: f32, origin_y: f32, max_width: f32) -> Self {
        Self { origin_x, origin_y, max_width, max_height: 0.0, font_size, ellipsis: false }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PositionedShapedGlyph {
    pub font_id: FontId,
    pub glyph_id: u16,
    pub cluster: usize,
    pub text_range: std::ops::Range<usize>,
    pub x: f32,
    pub y: f32,
    pub x_offset: f32,
    pub y_offset: f32,
    pub advance: f32,
    pub font_size: f32,
    pub source: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text_range: std::ops::Range<usize>,
    pub level: unicode_bidi::Level,
    pub font_id: FontId,
    pub glyph_range: std::ops::Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub text_range: std::ops::Range<usize>,
    pub glyph_range: std::ops::Range<usize>,
    pub baseline: f32,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub hard_break: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphMetrics {
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub line_height: f32,
    pub line_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphLayout {
    pub text: String,
    pub glyphs: Vec<PositionedShapedGlyph>,
    pub lines: Vec<TextLine>,
    pub runs: Vec<TextRun>,
    pub metrics: ParagraphMetrics,
}

/// A positioned glyph ready for rendering.
pub struct PositionedGlyph {
    pub codepoint: char,
    pub glyph_key: GlyphKey,
    /// Screen-space X of the glyph quad's top-left corner.
    pub x: f32,
    /// Screen-space Y of the glyph quad's top-left corner.
    pub y: f32,
    pub width: u32,
    pub height: u32,
    pub font_size: f32,
}

pub fn layout_paragraph_with_shaper(
    text: &str,
    font_bytes: &[u8],
    font_id: FontId,
    metrics: FontMetrics,
    options: TextLayoutOptions,
) -> ParagraphLayout {
    let face = rustybuzz::Face::from_slice(font_bytes, 0);

    if let Some(face) = face {
        layout_paragraph(text, font_id, &metrics, &options, |segment, text_offset| {
            shape_segment(&face, segment, text_offset, font_id, options.font_size)
        })
    } else {
        layout_paragraph(text, font_id, &metrics, &options, |segment, text_offset| {
            fallback_shape_segment(segment, text_offset, font_id, options.font_size)
        })
    }
}

fn layout_paragraph<F>(text: &str, font_id: FontId, metrics: &FontMetrics, options: &TextLayoutOptions, mut shape: F) -> ParagraphLayout
where
    F: FnMut(&str, usize) -> Vec<PositionedShapedGlyph>,
{
    let bidi = BidiInfo::new(text, None);
    let paragraph = bidi.paragraphs.first();
    let levels = paragraph
        .map(|paragraph| bidi.visual_runs(paragraph, paragraph.range.clone()).1)
        .unwrap_or_default();
    let mut break_offsets: Vec<usize> = linebreaks(text)
        .filter_map(|(offset, opportunity)| match opportunity {
            BreakOpportunity::Mandatory | BreakOpportunity::Allowed => Some(offset),
        })
        .collect();
    break_offsets.push(text.len());
    break_offsets.sort_unstable();
    break_offsets.dedup();

    let mut glyphs = Vec::new();
    let mut runs = Vec::new();
    let mut lines = Vec::new();
    let mut line_start_text = 0;
    let mut line_start_glyph = 0;
    let mut line_width = 0.0;
    let mut baseline = options.origin_y;
    let max_width = options.max_width.max(0.0);
    let max_height = options.max_height.max(0.0);

    for (cluster_text, cluster_start) in text.grapheme_indices(true).map(|(i, s)| (s, i)) {
        let cluster_end = cluster_start + cluster_text.len();
        if cluster_text == "\n" {
            finish_line(&mut lines, line_start_text..cluster_start, line_start_glyph..glyphs.len(), baseline, line_width, metrics, true);
            line_start_text = cluster_end;
            line_start_glyph = glyphs.len();
            line_width = 0.0;
            baseline += metrics.line_height;
            if should_stop_for_height(options.origin_y, baseline, metrics.line_height, max_height) {
                break;
            }
            continue;
        }

        let mut shaped = shape(cluster_text, cluster_start);
        let cluster_width = shaped.iter().map(|glyph| glyph.advance).sum::<f32>();
        let break_allowed = break_offsets.binary_search(&cluster_end).is_ok();

        if max_width > 0.0
            && line_width > 0.0
            && line_width + cluster_width > max_width
            && (break_allowed || !cluster_text.chars().all(char::is_whitespace))
        {
            finish_line(&mut lines, line_start_text..cluster_start, line_start_glyph..glyphs.len(), baseline, line_width, metrics, false);
            line_start_text = cluster_start;
            line_start_glyph = glyphs.len();
            line_width = 0.0;
            baseline += metrics.line_height;
            if should_stop_for_height(options.origin_y, baseline, metrics.line_height, max_height) {
                break;
            }
        }

        let glyph_start = glyphs.len();
        for glyph in &mut shaped {
            glyph.x = options.origin_x + line_width + glyph.x;
            glyph.y = baseline + glyph.y;
        }
        line_width += cluster_width;
        glyphs.extend(shaped);
        let level = levels
            .iter()
            .find(|run| run.start <= cluster_start && cluster_start < run.end)
            .and_then(|run| bidi.levels.get(run.start).copied())
            .unwrap_or_else(unicode_bidi::Level::ltr);
        runs.push(TextRun { text_range: cluster_start..cluster_end, level, font_id, glyph_range: glyph_start..glyphs.len() });
    }

    if line_start_text <= text.len() && (lines.is_empty() || line_start_text < text.len()) {
        finish_line(&mut lines, line_start_text..text.len(), line_start_glyph..glyphs.len(), baseline, line_width, metrics, false);
    }

    if options.ellipsis && max_width > 0.0 {
        apply_ellipsis(&mut glyphs, &mut lines, font_id, options, metrics);
    }

    let width = lines.iter().map(|line| line.width).fold(0.0, f32::max);
    let line_count = lines.len();
    let height = line_count as f32 * metrics.line_height;
    ParagraphLayout {
        text: text.to_string(),
        glyphs,
        lines,
        runs,
        metrics: ParagraphMetrics {
            width,
            height,
            ascent: metrics.ascent,
            descent: metrics.descent,
            line_gap: metrics.line_gap,
            line_height: metrics.line_height,
            line_count,
        },
    }
}

fn shape_segment(face: &rustybuzz::Face, segment: &str, text_offset: usize, font_id: FontId, font_size: f32) -> Vec<PositionedShapedGlyph> {
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(segment);
    let output = rustybuzz::shape(face, &[], buffer);
    let upem = face.units_per_em() as f32;
    let scale = if upem > 0.0 { font_size / upem } else { 1.0 };

    output
        .glyph_infos()
        .iter()
        .zip(output.glyph_positions())
        .map(|(info, position)| {
            let cluster = text_offset + info.cluster as usize;
            PositionedShapedGlyph {
                font_id,
                glyph_id: info.glyph_id as u16,
                cluster,
                text_range: text_offset..text_offset + segment.len(),
                x: 0.0,
                y: 0.0,
                x_offset: position.x_offset as f32 * scale,
                y_offset: position.y_offset as f32 * scale,
                advance: position.x_advance as f32 * scale,
                font_size,
                source: segment.to_string(),
            }
        })
        .collect()
}

fn fallback_shape_segment(segment: &str, text_offset: usize, font_id: FontId, font_size: f32) -> Vec<PositionedShapedGlyph> {
    segment
        .chars()
        .map(|c| PositionedShapedGlyph {
            font_id,
            glyph_id: c as u32 as u16,
            cluster: text_offset,
            text_range: text_offset..text_offset + segment.len(),
            x: 0.0,
            y: 0.0,
            x_offset: 0.0,
            y_offset: 0.0,
            advance: font_size * 0.5,
            font_size,
            source: c.to_string(),
        })
        .collect()
}

fn finish_line(
    lines: &mut Vec<TextLine>,
    text_range: std::ops::Range<usize>,
    glyph_range: std::ops::Range<usize>,
    baseline: f32,
    width: f32,
    metrics: &FontMetrics,
    hard_break: bool,
) {
    lines.push(TextLine {
        text_range,
        glyph_range,
        baseline,
        width,
        ascent: metrics.ascent,
        descent: metrics.descent,
        line_gap: metrics.line_gap,
        hard_break,
    });
}

fn should_stop_for_height(origin_y: f32, next_baseline: f32, line_height: f32, max_height: f32) -> bool {
    max_height > 0.0 && next_baseline - origin_y + line_height > max_height
}

fn apply_ellipsis(
    glyphs: &mut Vec<PositionedShapedGlyph>,
    lines: &mut [TextLine],
    font_id: FontId,
    options: &TextLayoutOptions,
    metrics: &FontMetrics,
) {
    if let Some(line) = lines.last_mut() {
        let ellipsis_width = options.font_size * 0.5;
        while line.width + ellipsis_width > options.max_width && line.glyph_range.end > line.glyph_range.start {
            if let Some(glyph) = glyphs.pop() {
                line.glyph_range.end -= 1;
                line.width -= glyph.advance;
                line.text_range.end = glyph.cluster;
            } else {
                break;
            }
        }
        let x = options.origin_x + line.width;
        glyphs.push(PositionedShapedGlyph {
            font_id,
            glyph_id: '…' as u32 as u16,
            cluster: line.text_range.end,
            text_range: line.text_range.end..line.text_range.end,
            x,
            y: line.baseline,
            x_offset: 0.0,
            y_offset: 0.0,
            advance: ellipsis_width,
            font_size: options.font_size,
            source: "…".to_string(),
        });
        line.glyph_range.end = glyphs.len();
        line.width += ellipsis_width;
        line.ascent = metrics.ascent;
    }
}

/// Simple horizontal text layout with basic line breaking.
pub fn layout_text(
    rasterizer: &mut GlyphRasterizer,
    text: &str,
    font_size: f32,
    origin_x: f32,
    origin_y: f32,
    max_width: f32,
) -> Vec<PositionedGlyph> {
    let (ascent, _descent, line_gap) = rasterizer.line_metrics(font_size);
    let line_height = ascent - _descent + line_gap;

    let mut glyphs = Vec::new();
    let mut pen_x = origin_x;
    let mut pen_y = origin_y;

    for cluster in text.graphemes(true) {
        if cluster == "\n" {
            pen_x = origin_x;
            pen_y += line_height;
            continue;
        }
        let mut shaped = Vec::new();
        let mut cluster_width = 0.0;
        for c in cluster.chars() {
            if c.is_control() {
                continue;
            }

            let key = rasterizer.glyph_key_for_codepoint(c, font_size);
            let rg = rasterizer.glyph_metrics_for_key(key, font_size);
            cluster_width += rg.advance_width;
            shaped.push((c, key, rg));
        }

        if shaped.is_empty() {
            continue;
        }

        // Simple word-wrap: wrap only between grapheme clusters so UTF-8 text,
        // combining marks, and emoji sequences are never split mid-cluster.
        if max_width > 0.0 && pen_x + cluster_width > origin_x + max_width && pen_x > origin_x {
            pen_x = origin_x;
            pen_y += line_height;
        }

        for (c, glyph_key, rg) in shaped {
            let advance = rg.advance_width;
            if rg.width > 0 && rg.height > 0 {
                let gx = pen_x + rg.offset_x;
                // pen_y is the baseline; offset_y (ymin) is distance from baseline to
                // bottom of the glyph bitmap, so top of bitmap = baseline - offset_y - height.
                let gy = pen_y - rg.offset_y - rg.height as f32;

                glyphs.push(PositionedGlyph { codepoint: c, glyph_key, x: gx, y: gy, width: rg.width, height: rg.height, font_size });
            }

            pen_x += advance;
        }
    }
    glyphs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_layout(text: &str, max_width: f32) -> ParagraphLayout {
        let metrics = FontMetrics::new(8.0, -2.0, 2.0);
        let options = TextLayoutOptions::new(10.0, 0.0, 0.0, max_width);
        layout_paragraph(text, 0, &metrics, &options, |segment, text_offset| {
            fallback_shape_segment(segment, text_offset, 0, options.font_size)
        })
    }

    #[test]
    fn preserves_explicit_newlines() {
        let layout = test_layout("first\nsecond", 0.0);

        assert_eq!(layout.lines.len(), 2);
        assert!(layout.lines[0].hard_break);
        assert_eq!(layout.lines[0].text_range, 0..5);
        assert_eq!(layout.lines[1].text_range, 6..12);
        assert_eq!(layout.metrics.line_count, 2);
        assert_eq!(layout.metrics.height, layout.metrics.line_height * 2.0);
    }

    #[test]
    fn wraps_without_splitting_grapheme_clusters() {
        let layout = test_layout("Cafe\u{301} noir", 20.0);

        assert!(layout.lines.len() > 1);
        assert!(layout.glyphs.iter().any(|glyph| glyph.text_range == (3..6)));
        assert!(!layout.lines.iter().any(|line| line.text_range.end == 4));
    }

    #[test]
    fn ellipsis_truncates_at_cluster_boundary() {
        let metrics = FontMetrics::new(8.0, -2.0, 2.0);
        let mut options = TextLayoutOptions::new(10.0, 0.0, 0.0, 18.0);
        options.ellipsis = true;
        let layout = layout_paragraph("Cafe\u{301}", 0, &metrics, &options, |segment, text_offset| {
            fallback_shape_segment(segment, text_offset, 0, options.font_size)
        });

        assert_eq!(layout.glyphs.last().map(|glyph| glyph.source.as_str()), Some("…"));
        assert!(!layout.lines.iter().any(|line| line.text_range.end == 4));
        assert!(layout.metrics.width <= options.max_width);
    }
}
