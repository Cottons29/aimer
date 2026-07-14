use std::collections::VecDeque;

use aimer_utils::time_cost;
use unicode_bidi::BidiInfo;
use unicode_linebreak::{BreakOpportunity, linebreaks};
use unicode_segmentation::UnicodeSegmentation;

use super::glyph_rasterizer::{GlyphKey, GlyphRasterizer};

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
#[derive(Clone)]
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

#[derive(Clone)]
pub struct ShapedCluster {
    pub text: String,
    pub base_codepoint: char,
    pub glyphs: Vec<(GlyphKey, f32, f32, f32)>,
    pub width: f32,
}

#[derive(Clone)]
pub struct ShapedText {
    pub font_size: f32,
    pub line_height: f32,
    pub clusters: Vec<ShapedCluster>,
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

/// A contiguous run of text that shares the same BiDi level and can be shaped
/// as a unit.
struct ShapingRun<'a> {
    text: &'a str,
    start: usize,
    level: unicode_bidi::Level,
}

/// Collect grapheme clusters into shaping runs: contiguous clusters that share
/// the same BiDi level are merged into a single run so that complex-script
/// shaping (Arabic, Devanagari, etc.) operates on the full context instead of
/// individual clusters.
fn collect_shaping_runs<'a>(
    text: &'a str,
    bidi: &BidiInfo,
    visual_runs: &[(std::ops::Range<usize>, unicode_bidi::Level)],
) -> Vec<ShapingRun<'a>> {
    let mut result: Vec<ShapingRun<'a>> = Vec::new();

    for (cluster, cluster_start) in text
        .grapheme_indices(true)
        .map(|(i, s)| (s, i))
    {
        // Find the BiDi level for this cluster.
        let level = visual_runs
            .iter()
            .find(|(range, _)| range.start <= cluster_start && cluster_start < range.end)
            .map(|(_, lvl)| *lvl)
            .or_else(|| {
                bidi.levels
                    .get(cluster_start)
                    .copied()
            })
            .unwrap_or_else(unicode_bidi::Level::ltr);

        let merge = result
            .last()
            .is_some_and(|last| {
                last.level == level
                    && cluster != "\n"
                    && !last
                        .text
                        .ends_with('\n')
            });

        if merge {
            // Extend the last run to include this cluster.  Because both slices
            // are sub-slices of `text` we can reconstruct a single slice from
            // the original pointer.
            let last = result
                .last_mut()
                .unwrap();
            let new_end = cluster_start + cluster.len();
            // Safety: both are valid sub-slices of the same UTF-8 `text`.
            last.text = &text[last.start..new_end];
        } else {
            result.push(ShapingRun { text: cluster, start: cluster_start, level });
        }
    }
    result
}

fn layout_paragraph<F>(
    text: &str,
    font_id: FontId,
    metrics: &FontMetrics,
    options: &TextLayoutOptions,
    mut shape: F,
) -> ParagraphLayout
where
    F: FnMut(&str, usize) -> Vec<PositionedShapedGlyph>,
{
    let bidi = BidiInfo::new(text, None);
    let paragraph = bidi
        .paragraphs
        .first();
    // `visual_runs` returns (Vec<Level>, Vec<Range<usize>>); we zip them into
    // (Range, Level) pairs for use in `collect_shaping_runs`.
    let visual_run_ranges: Vec<(std::ops::Range<usize>, unicode_bidi::Level)> = paragraph
        .map(|para| {
            let (levels, ranges) = bidi.visual_runs(
                para,
                para.range
                    .clone(),
            );
            ranges
                .into_iter()
                .zip(levels)
                .collect()
        })
        .unwrap_or_default();

    #[allow(clippy::unnecessary_filter_map)]
    let mut break_offsets: Vec<usize> = linebreaks(text)
        .filter_map(|(offset, opportunity)| match opportunity {
            BreakOpportunity::Mandatory | BreakOpportunity::Allowed => Some(offset),
        })
        .collect();
    break_offsets.push(text.len());
    break_offsets.sort_unstable();
    break_offsets.dedup();

    // Collect shaping runs (merged by BiDi level) before the layout loop.
    let shaping_runs = collect_shaping_runs(text, &bidi, &visual_run_ranges);

    let mut glyphs = Vec::new();
    let mut runs = Vec::new();
    let mut lines = Vec::new();
    let mut line_start_text = 0;
    let mut line_start_glyph = 0;
    let mut line_width = 0.0;
    let mut baseline = options.origin_y;
    let max_width = options
        .max_width
        .max(0.0);
    let max_height = options
        .max_height
        .max(0.0);

    // Use a queue so remainder runs from word-wrapping are re-evaluated for
    // overflow on subsequent lines.  A plain `for` loop would emit the
    // remainder once and `continue`, skipping the overflow check — causing
    // long words to render past the second line's edge.
    let mut queue: VecDeque<ShapingRun<'_>> = shaping_runs
        .into_iter()
        .collect();

    while let Some(shaping_run) = queue.pop_front() {
        let run_start = shaping_run.start;
        let run_text = shaping_run.text;
        let run_end = run_start + run_text.len();
        let level = shaping_run.level;
        let is_rtl = level.is_rtl();

        // Handle newline runs.
        if run_text == "\n" {
            finish_line(
                &mut lines,
                line_start_text..run_start,
                line_start_glyph..glyphs.len(),
                baseline,
                line_width,
                metrics,
                true,
            );
            line_start_text = run_end;
            line_start_glyph = glyphs.len();
            line_width = 0.0;
            baseline += metrics.line_height;
            if should_stop_for_height(options.origin_y, baseline, metrics.line_height, max_height) {
                break;
            }
            continue;
        }

        // Shape the entire run at once (correct for Arabic, Devanagari, etc.).
        let mut shaped = shape(run_text, run_start);

        // For RTL runs, reverse the glyph order so they render right-to-left.
        if is_rtl {
            shaped.reverse();
        }

        // Determine total advance for this shaped run.
        let run_width: f32 = shaped
            .iter()
            .map(|g| g.advance)
            .sum();

        // Check whether a line break is allowed at the run boundary.
        let break_allowed = break_offsets
            .binary_search(&run_end)
            .is_ok();

        if max_width > 0.0
            && line_width + run_width > max_width
            && (break_allowed
                || !run_text
                    .chars()
                    .all(char::is_whitespace))
        {
            // Try to break the run at grapheme-cluster boundaries to avoid
            // splitting across lines at awkward positions.  We walk clusters
            // and emit them onto the current line until we'd overflow, then
            // start a new line for the remainder.
            //
            // `sub_x > options.origin_x` ensures we never split before placing
            // at least one cluster (avoids an infinite wrapping loop on a single
            // wide cluster that can never fit).
            let mut sub_x = options.origin_x + line_width;
            let mut remainder_start: Option<(usize, bool)> = None;
            let mut last_word_break: Option<usize> = None;
            let mut cluster_offset = 0usize;
            for (_, cluster_str) in run_text.grapheme_indices(true) {
                // Each cluster contributes its share of the total advance.
                // We can't re-shape individual clusters without losing context,
                // so we approximate by summing the shaped glyphs whose cluster
                // index falls inside this cluster's byte range.
                let cluster_byte_start = run_start + cluster_offset;
                let cluster_byte_end = cluster_byte_start + cluster_str.len();
                let cluster_advance: f32 = shaped
                    .iter()
                    .filter(|g| g.cluster >= cluster_byte_start && g.cluster < cluster_byte_end)
                    .map(|g| g.advance)
                    .sum();

                // Track the last whitespace cluster as a preferred word break.
                // Use the *start* of the space so that when we break here the
                // space glyph is included on the current line (its advance is
                // already part of `sub_x`).  The glyph filter below uses
                // `<= break_point` to include it.
                if cluster_str
                    .chars()
                    .all(char::is_whitespace)
                {
                    last_word_break = Some(cluster_byte_start);
                }

                if sub_x + cluster_advance > options.origin_x + max_width
                    && sub_x > options.origin_x
                {
                    // Prefer breaking at the last word boundary rather than
                    // mid-word.  If no word break was seen, fall back to the
                    // cluster-level break.
                    if let Some(wb) = last_word_break {
                        remainder_start = Some((wb, true));
                    } else {
                        remainder_start = Some((cluster_byte_start, false));
                    }
                    break;
                }
                sub_x += cluster_advance;
                cluster_offset += cluster_str.len();
            }

            if let Some((break_point, is_word_break)) = remainder_start {
                // Emit glyphs up to break_point onto the current line.
                // For word breaks we include the space glyph on the current
                // line but position it separately at the accumulated width
                // (not at line_start x), since it belongs after the word.
                // For character breaks we exclude the overflowing char.
                let glyph_start = glyphs.len();
                let line_glyphs: Vec<_> = shaped
                    .iter()
                    .filter(|g| {
                        if is_word_break {
                            g.cluster <= break_point
                        } else {
                            g.cluster < break_point
                        }
                    })
                    .cloned()
                    .collect();
                // Track accumulated width for per-glyph positioning so the
                // space lands after the preceding characters, not at the
                // line's start x.
                let mut acc_w = 0.0_f32;
                // let mut space_advance = 0.0_f32;
                for mut glyph in line_glyphs {
                    // let is_space = is_word_break && glyph.cluster == break_point;
                    glyph.x = options.origin_x + line_width + acc_w;
                    glyph.y = baseline;
                    // if is_space {
                    //     space_advance = glyph.advance;
                    // }
                    acc_w += glyph.advance;
                    glyphs.push(glyph);
                }
                // For word breaks the text_range must include the space byte.
                let text_end = if is_word_break { break_point + 1 } else { break_point };
                runs.push(TextRun {
                    text_range: run_start..text_end,
                    level,
                    font_id,
                    glyph_range: glyph_start..glyphs.len(),
                });
                // line_run_width is the width of glyphs BEFORE the space;
                // add the space advance for the total line width.
                let line_run_width = acc_w;
                finish_line(
                    &mut lines,
                    line_start_text..text_end,
                    line_start_glyph..glyphs.len(),
                    baseline,
                    line_run_width + line_width,
                    metrics,
                    false,
                );

                // Skip leading whitespace at the start of the new line so
                // wrapped lines don't begin with a space character.
                let mut trimmed = text_end;
                while trimmed < run_end && text.as_bytes()[trimmed] == b' ' {
                    trimmed += 1;
                }

                // Start a new line with the trimmed remainder.
                line_start_text = trimmed;
                line_start_glyph = glyphs.len();
                line_width = 0.0;
                baseline += metrics.line_height;
                if should_stop_for_height(
                    options.origin_y,
                    baseline,
                    metrics.line_height,
                    max_height,
                ) {
                    break;
                }

                // Push the remainder back to the queue so it is re-shaped
                // and checked for overflow on the new line (fixes second-line
                // word wrapping).  We do NOT emit remainder glyphs here — the
                // queue iteration will emit them via the normal path below,
                // avoiding double-emission.
                if trimmed < run_end {
                    queue.push_front(ShapingRun {
                        text: &text[trimmed..run_end],
                        start: trimmed,
                        level,
                    });
                }
                continue;
            } else {
                // Couldn't split — emit whole run on a new line.
                finish_line(
                    &mut lines,
                    line_start_text..run_start,
                    line_start_glyph..glyphs.len(),
                    baseline,
                    line_width,
                    metrics,
                    false,
                );
                line_start_text = run_start;
                line_start_glyph = glyphs.len();
                line_width = 0.0;
                baseline += metrics.line_height;
                if should_stop_for_height(
                    options.origin_y,
                    baseline,
                    metrics.line_height,
                    max_height,
                ) {
                    break;
                }
            }
        }

        let glyph_start = glyphs.len();
        for mut glyph in shaped {
            glyph.x += options.origin_x + line_width;
            glyph.y += baseline;
            glyphs.push(glyph);
        }
        line_width += run_width;
        runs.push(TextRun {
            text_range: run_start..run_end,
            level,
            font_id,
            glyph_range: glyph_start..glyphs.len(),
        });
    }

    if line_start_text <= text.len() && (lines.is_empty() || line_start_text < text.len()) {
        finish_line(
            &mut lines,
            line_start_text..text.len(),
            line_start_glyph..glyphs.len(),
            baseline,
            line_width,
            metrics,
            false,
        );
    }

    if options.ellipsis && max_width > 0.0 {
        apply_ellipsis(&mut glyphs, &mut lines, font_id, options, metrics);
    }

    let width = lines
        .iter()
        .map(|line| line.width)
        .fold(0.0, f32::max);
    let line_count = lines.len();
    // line_height includes one line_gap per line, but line_gap only appears
    // *between* lines — subtract the trailing one so the reported height
    // matches the actual rendered extent (first-line ascent through last-line
    // descent).
    let height = line_count as f32 * metrics.line_height - metrics.line_gap;
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

fn shape_segment(
    face: &rustybuzz::Face,
    segment: &str,
    text_offset: usize,
    font_id: FontId,
    font_size: f32,
) -> Vec<PositionedShapedGlyph> {
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

fn fallback_shape_segment(
    segment: &str,
    text_offset: usize,
    font_id: FontId,
    font_size: f32,
) -> Vec<PositionedShapedGlyph> {
    // Group by grapheme cluster so that combining marks (e.g. "e\u{301}")
    // are emitted as a single glyph with the cluster's full byte range.
    segment
        .grapheme_indices(true)
        .map(|(cluster_byte_offset, cluster_str)| {
            let cluster_start = text_offset + cluster_byte_offset;
            let cluster_end = cluster_start + cluster_str.len();
            // Use the first (base) codepoint as the representative glyph id.
            let glyph_char = cluster_str
                .chars()
                .next()
                .unwrap_or('\0');
            PositionedShapedGlyph {
                font_id,
                glyph_id: glyph_char as u32 as u16,
                cluster: cluster_start,
                text_range: cluster_start..cluster_end,
                x: 0.0,
                y: 0.0,
                x_offset: 0.0,
                y_offset: 0.0,
                advance: font_size * 0.5,
                font_size,
                source: cluster_str.to_string(),
            }
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

fn should_stop_for_height(
    origin_y: f32,
    next_baseline: f32,
    line_height: f32,
    max_height: f32,
) -> bool {
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
        while line.width + ellipsis_width > options.max_width
            && line
                .glyph_range
                .end
                > line
                    .glyph_range
                    .start
        {
            if let Some(glyph) = glyphs.pop() {
                line.glyph_range
                    .end -= 1;
                line.width -= glyph.advance;
                line.text_range
                    .end = glyph.cluster;
            } else {
                break;
            }
        }
        let x = options.origin_x + line.width;
        glyphs.push(PositionedShapedGlyph {
            font_id,
            glyph_id: '…' as u32 as u16,
            cluster: line
                .text_range
                .end,
            text_range: line
                .text_range
                .end
                ..line
                    .text_range
                    .end,
            x,
            y: line.baseline,
            x_offset: 0.0,
            y_offset: 0.0,
            advance: ellipsis_width,
            font_size: options.font_size,
            source: "…".to_string(),
        });
        line.glyph_range
            .end = glyphs.len();
        line.width += ellipsis_width;
        line.ascent = metrics.ascent;
    }
}

pub fn shape_text(rasterizer: &mut GlyphRasterizer, text: &str, font_size: f32) -> ShapedText {
    let (ascent, _descent, line_gap) = time_cost!("text_layout::LayoutText - line_metrics", {
        rasterizer.line_metrics(font_size)
    });
    let line_height = ascent - _descent + line_gap;

    let clusters = time_cost!("text_layout::LayoutText - text.graphemes", { text.graphemes(true) });
    let chars: Vec<&str> = clusters
        .into_iter()
        .collect();

    let clusters = time_cost!("text_layout::LayoutText - text.graphemes loops", {
        chars
            .into_iter()
            .filter_map(|cluster| {
                if cluster == "\n" {
                    return Some(ShapedCluster {
                        text: cluster.to_string(),
                        base_codepoint: '\n',
                        glyphs: Vec::new(),
                        width: 0.0,
                    });
                }

                // Shape each grapheme cluster once.  The resulting advances and
                // glyph keys are independent from wrapping width, so resize can
                // reuse them and only recompute positions.
                let glyphs = rasterizer.shape_cluster(cluster, font_size);
                if glyphs.is_empty() {
                    return None;
                }

                let width = glyphs
                    .iter()
                    .map(|(_, adv, _, _)| adv)
                    .sum();
                let base_codepoint = cluster
                    .chars()
                    .next()
                    .unwrap_or('\0');
                Some(ShapedCluster { text: cluster.to_string(), base_codepoint, glyphs, width })
            })
            .collect()
    });

    ShapedText { font_size, line_height, clusters }
}

pub fn layout_shaped_text(
    rasterizer: &mut GlyphRasterizer,
    shaped_text: &ShapedText,
    origin_x: f32,
    origin_y: f32,
    max_width: f32,
) -> Vec<PositionedGlyph> {
    let font_size = shaped_text.font_size;
    let line_height = shaped_text.line_height;

    let mut glyphs: Vec<PositionedGlyph> = Vec::new();
    let mut pen_x = origin_x;
    let mut pen_y = origin_y;

    // Word-wrap state: track the last space position so we can break the line
    // at word boundaries instead of mid-word.  If a single word is wider than
    // max_width we fall back to character-level wrapping so text never overflows.
    let mut last_space_glyph_idx: usize = usize::MAX;
    let mut last_space_pen_x: f32 = origin_x;

    time_cost!("text_layout::LayoutText - positioned shaped clusters", {
        for cluster in &shaped_text.clusters {
            if cluster.text == "\n" {
                pen_x = origin_x;
                pen_y += line_height;
                last_space_glyph_idx = usize::MAX;
                continue;
            }

            // Track the last space cluster so we know where to break.
            if cluster
                .text
                .chars()
                .all(char::is_whitespace)
            {
                last_space_glyph_idx = glyphs.len();
                last_space_pen_x = pen_x + cluster.width;
            }

            if max_width > 0.0 && pen_x + cluster.width > origin_x + max_width && pen_x > origin_x {
                if last_space_glyph_idx < glyphs.len() {
                    // Word-wrap: move the part of the word that was already
                    // placed after the last space down to a new line, keeping
                    // those glyphs (they must not be discarded) and shifting
                    // them so the word starts at the left margin.
                    let wrap_offset = last_space_pen_x - origin_x;
                    // Width of the already-placed glyphs that belong to the
                    // overflowing word (everything after the last space).
                    let moved_width = pen_x - last_space_pen_x;
                    for glyph in &mut glyphs[last_space_glyph_idx..] {
                        glyph.x -= wrap_offset;
                        glyph.y += line_height;
                    }
                    // Continue the new line right after the moved glyphs so the
                    // current cluster is appended (not overlapped) below.
                    pen_x = origin_x + moved_width;
                    pen_y += line_height;
                    last_space_glyph_idx = usize::MAX;
                    // Fall through to the normal emit path below, which places
                    // the current cluster at the updated pen position.
                } else {
                    // No word break available (word wider than max_width) — fall
                    // back to character-level wrapping.
                    pen_x = origin_x;
                    pen_y += line_height;
                }
            }

            for &(glyph_key, advance, x_offset, y_offset) in &cluster.glyphs {
                let rg =
                    time_cost!("RasterizeKey", || rasterizer.rasterize_key(glyph_key, font_size));
                if rg.width > 0 && rg.height > 0 {
                    let gx = pen_x + rg.offset_x + x_offset;
                    // pen_y is the baseline; offset_y (ymin) is distance from baseline to
                    // bottom of the glyph bitmap, so top of bitmap = baseline - offset_y - height.
                    let gy = pen_y - rg.offset_y - rg.height as f32 + y_offset;

                    glyphs.push(PositionedGlyph {
                        codepoint: cluster.base_codepoint,
                        glyph_key,
                        x: gx,
                        y: gy,
                        width: rg.width,
                        height: rg.height,
                        font_size,
                    });
                }

                pen_x += advance;
            }
        }
    });
    glyphs
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
    let shaped_text = shape_text(rasterizer, text, font_size);
    layout_shaped_text(rasterizer, &shaped_text, origin_x, origin_y, max_width)
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

        assert_eq!(
            layout
                .lines
                .len(),
            2
        );
        assert!(layout.lines[0].hard_break);
        assert_eq!(layout.lines[0].text_range, 0..5);
        assert_eq!(layout.lines[1].text_range, 6..12);
        assert_eq!(
            layout
                .metrics
                .line_count,
            2
        );
        assert_eq!(
            layout
                .metrics
                .height,
            layout
                .metrics
                .line_height
                * 2.0
                - layout
                    .metrics
                    .line_gap
        );
    }

    #[test]
    fn wraps_without_splitting_grapheme_clusters() {
        let layout = test_layout("Cafe\u{301} noir", 20.0);

        assert!(
            layout
                .lines
                .len()
                > 1
        );
        assert!(
            layout
                .glyphs
                .iter()
                .any(|glyph| glyph.text_range == (3..6))
        );
        assert!(
            !layout
                .lines
                .iter()
                .any(|line| line
                    .text_range
                    .end
                    == 4)
        );
    }

    #[test]
    fn ellipsis_truncates_at_cluster_boundary() {
        let metrics = FontMetrics::new(8.0, -2.0, 2.0);
        let mut options = TextLayoutOptions::new(10.0, 0.0, 0.0, 18.0);
        options.ellipsis = true;
        let layout =
            layout_paragraph("Cafe\u{301}", 0, &metrics, &options, |segment, text_offset| {
                fallback_shape_segment(segment, text_offset, 0, options.font_size)
            });

        assert_eq!(
            layout
                .glyphs
                .last()
                .map(|glyph| glyph
                    .source
                    .as_str()),
            Some("…")
        );
        assert!(
            !layout
                .lines
                .iter()
                .any(|line| line
                    .text_range
                    .end
                    == 4)
        );
        assert!(
            layout
                .metrics
                .width
                <= options.max_width
        );
    }
}
