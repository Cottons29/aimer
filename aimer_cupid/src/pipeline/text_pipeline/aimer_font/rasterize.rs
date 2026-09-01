use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::color::{ColorLayer, ColorRgba};
use super::cff::{CffGlyphOutline, CffPathCommand};
use super::outline::{GlyphOutline, OutlinePoint};
use super::svg::{SvgGlyph, SvgPath};
use super::{FontMetrics, SfntError, SfntFace};
use crate::svg::{SvgFillRule, SvgPathCommand};
use crate::text_pipeline::glyph_rasterizer::RasterizedGlyph;

const CURVE_FLATTEN_TOLERANCE: f32 = 0.125;
const MAX_CURVE_DEPTH: u8 = 10;
const MAX_FLATTENED_POINTS: usize = 1 << 20;
const MAX_RASTER_EXTENT: i64 = 4096;
const SUBPIXEL_PHASES: u8 = 8;
// Replaying a cached bitmap is faster than rebuilding coverage spans, but a
// bounded entry size keeps large display glyphs from turning the shared
// flattened cache into an unbounded bitmap store.
const MAX_CACHED_COVERAGE_BYTES: usize = 64 * 1024;
// The normal unhinted profile uses a 4x4 box grid. It keeps diagonal and
// curved edges from collapsing into the coarse coverage levels produced by a
// 2x2 grid while remaining substantially cheaper than the 8x8 quality retry.
const SAMPLE_GRID: u32 = 4;
// A tiny curved outline can fall between all coarse sample centers. It is only
// used as a retry when the fast pass would otherwise return a blank bitmap.
const SMALL_GLYPH_SAMPLE_GRID: u32 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CoverageFillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy, Debug)]
enum PathCommand {
    MoveTo(Point),
    LineTo(Point),
    QuadTo {
        control: Point,
        point: Point,
    },
    CurveTo {
        control_1: Point,
        control_2: Point,
        point: Point,
    },
    Close,
}

#[derive(Clone, Copy, Debug)]
struct Edge {
    start: Point,
    end: Point,
    min_y: f32,
    max_y: f32,
    inverse_slope: f32,
    winding: i8,
}

impl Edge {
    fn new(start: Point, end: Point) -> Self {
        let inverse_slope = if end.y != start.y {
            (end.x - start.x) / (end.y - start.y)
        } else {
            0.0
        };
        Self {
            start,
            end,
            min_y: start.y.min(end.y),
            max_y: start.y.max(end.y),
            inverse_slope,
            winding: edge_winding(start, end),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ScanlineIntersection {
    x: f32,
    winding: i8,
}

#[derive(Clone, Copy, Debug)]
struct CoverageSpan {
    start: f32,
    end: f32,
}

#[derive(Clone)]
struct ScanlinePlan {
    sample_grid: u32,
    offsets: Vec<usize>,
    spans: Vec<CoverageSpan>,
}

#[derive(Clone, Copy, Debug)]
struct ActiveScanlineEdge {
    edge_index: usize,
    x: f32,
    winding: i8,
}

#[derive(Clone)]
struct OutlinePath {
    bounds: [f32; 4],
    commands: Vec<PathCommand>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct FlattenedKey {
    glyph_id: u16,
    size_tenths: u32,
    subpixel_x: u8,
    subpixel_y: u8,
    weight: u16,
    variation_id: u32,
}

impl FlattenedKey {
    fn new(
        glyph_id: u16,
        font_size: f32,
        subpixel_x: u8,
        subpixel_y: u8,
        weight: u16,
        variation_id: u32,
    ) -> Self {
        Self {
            glyph_id,
            size_tenths: font_size.to_bits(),
            subpixel_x: subpixel_x % SUBPIXEL_PHASES,
            subpixel_y: subpixel_y % SUBPIXEL_PHASES,
            weight,
            variation_id,
        }
    }
}

#[derive(Clone, Default)]
struct EdgeRows {
    offsets: Vec<usize>,
    indices: Vec<usize>,
}

impl EdgeRows {
    #[inline]
    fn row(&self, row: usize) -> &[usize] {
        let Some((&start, &end)) = self.offsets.get(row).zip(self.offsets.get(row + 1)) else {
            return &[];
        };
        self.indices.get(start..end).unwrap_or(&[])
    }
}

#[derive(Clone)]
struct FlattenedGlyph {
    left: i32,
    bottom: i32,
    width: usize,
    height: usize,
    phase_x: f32,
    phase_y: f32,
    edges: Vec<Edge>,
    row_edges: EdgeRows,
    coverage: Arc<OnceLock<Arc<[u8]>>>,
}

#[derive(Default)]
pub(crate) struct RasterScratch {
    active_edges: Vec<ActiveScanlineEdge>,
    active_positions: Vec<usize>,
}

#[derive(Default)]
pub(crate) struct SharedGlyphRasterCache {
    outlines: Mutex<HashMap<(u16, u16, u32), Result<Option<Arc<OutlinePath>>, SfntError>>>,
    flattened: Mutex<HashMap<FlattenedKey, Option<Arc<FlattenedGlyph>>>>,
}

const SHARED_OUTLINE_CACHE_CAPACITY: usize = 4096;
const SHARED_FLATTENED_CACHE_CAPACITY: usize = 4096;

#[derive(Default)]
pub(crate) struct GlyphRasterCache {
    outlines: HashMap<(u16, u16, u32), Result<Option<Arc<OutlinePath>>, SfntError>>,
    flattened: HashMap<FlattenedKey, Option<Arc<FlattenedGlyph>>>,
    pub(crate) scratch: RasterScratch,
    shared: Option<Arc<SharedGlyphRasterCache>>,
}

impl GlyphRasterCache {
    pub(crate) fn with_shared(shared: Arc<SharedGlyphRasterCache>) -> Self {
        Self {
            shared: Some(shared),
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(crate) fn outline_cache_len(&self) -> usize {
        self.outlines.len()
    }

    #[cfg(test)]
    pub(crate) fn flattened_edge_cache_len(&self) -> usize {
        self.flattened.len()
    }
}

/// Rasterizes one static SFNT glyph into the text pipeline's monochrome bitmap
/// contract. `subpixel_x` and `subpixel_y` are eighth-pixel pen phases; the
/// returned bearings compensate for that phase so the bitmap remains anchored
/// to the same baseline origin.
pub(crate) fn rasterize_font_glyph(
    bytes: &[u8],
    collection_index: u32,
    glyph_id: u16,
    font_size: f32,
    subpixel_x: u8,
    subpixel_y: u8,
) -> Option<RasterizedGlyph> {
    rasterize_font_glyphs(
        bytes,
        collection_index,
        &[(glyph_id, subpixel_x, subpixel_y)],
        font_size,
    )
    .into_iter()
    .next()
    .flatten()
}

/// Rasterizes a pending run while reusing one validated face and one metrics
/// read. The tuple fields are `(glyph_id, subpixel_x, subpixel_y)`.
pub(crate) fn rasterize_font_glyphs(
    bytes: &[u8],
    collection_index: u32,
    glyphs: &[(u16, u8, u8)],
    font_size: f32,
) -> Vec<Option<RasterizedGlyph>> {
    if !font_size.is_finite() || font_size <= 0.0 {
        return vec![None; glyphs.len()];
    }

    let Ok(face) = SfntFace::from_bytes(bytes, collection_index) else {
        return vec![None; glyphs.len()];
    };
    let Ok(metrics) = face.metrics() else {
        return vec![None; glyphs.len()];
    };
    rasterize_face_glyphs(&face, &metrics, glyphs, font_size)
}

/// Rasterizes a pending glyph slice through an already parsed face and
/// validated metrics record.
pub(crate) fn rasterize_face_glyphs(
    face: &SfntFace<'_>,
    metrics: &FontMetrics,
    glyphs: &[(u16, u8, u8)],
    font_size: f32,
) -> Vec<Option<RasterizedGlyph>> {
    let mut cache = GlyphRasterCache::default();
    rasterize_face_glyphs_cached(face, metrics, glyphs, font_size, &mut cache)
}

/// Rasterizes a pending glyph slice while retaining decoded outlines and
/// flattened paths in the face-local cache.
pub(crate) fn rasterize_face_glyphs_cached(
    face: &SfntFace<'_>,
    metrics: &FontMetrics,
    glyphs: &[(u16, u8, u8)],
    font_size: f32,
    cache: &mut GlyphRasterCache,
) -> Vec<Option<RasterizedGlyph>> {
    glyphs
        .iter()
        .map(|(glyph_id, subpixel_x, subpixel_y)| {
            rasterize_face_glyph(
                face,
                metrics,
                *glyph_id,
                font_size,
                *subpixel_x,
                *subpixel_y,
                crate::text_pipeline::glyph_rasterizer::NORMAL_GLYPH_WEIGHT,
                cache,
            )
        })
        .collect()
}

/// Rasterizes a pending key slice while reading the face's horizontal
/// advances once for the whole batch.
pub(crate) fn rasterize_face_glyphs_into<F>(
    face: &SfntFace<'_>,
    metrics: &FontMetrics,
    glyphs: &[crate::text_pipeline::glyph_rasterizer::GlyphKey],
    font_size: f32,
    cache: &mut GlyphRasterCache,
    emit: F,
) -> bool
where
    F: FnMut(crate::text_pipeline::glyph_rasterizer::GlyphKey, RasterizedGlyph),
{
    rasterize_face_glyphs_into_with_coordinates(
        face,
        metrics,
        glyphs,
        font_size,
        cache,
        |key, visit| {
            let (coordinates, coordinate_count) =
                face.coordinates_for_weight_instance(key.weight);
            visit(&coordinates[..coordinate_count]);
        },
        emit,
    )
}

/// Rasterizes a pending key slice while resolving each key through a caller's
/// normalized variation-coordinate provider. The provider invokes `visit`
/// exactly once for a valid key; keeping the callback borrowed avoids cloning
/// the shared coordinate vector into every glyph request.
pub(crate) fn rasterize_face_glyphs_into_with_coordinates<F, C>(
    face: &SfntFace<'_>,
    metrics: &FontMetrics,
    glyphs: &[crate::text_pipeline::glyph_rasterizer::GlyphKey],
    font_size: f32,
    cache: &mut GlyphRasterCache,
    mut coordinates_for: C,
    mut emit: F,
) -> bool
where
    F: FnMut(crate::text_pipeline::glyph_rasterizer::GlyphKey, RasterizedGlyph),
    C: FnMut(
        crate::text_pipeline::glyph_rasterizer::GlyphKey,
        &mut dyn FnMut(&[f32]),
    ),
{
    let has_metric_variations = face.has_horizontal_metric_variations();
    let advances = if has_metric_variations {
        None
    } else {
        let Ok(advances) = face.glyph_advances_with_metrics(*metrics) else {
            return false;
        };
        Some(advances)
    };
    // First-use batches commonly contain a whole run of distinct glyphs. The
    // two face-local maps otherwise grow through several rehashes while the
    // batch is already walking the same known key set.
    cache.outlines.reserve(glyphs.len());
    cache.flattened.reserve(glyphs.len());
    let scale = font_size / f32::from(metrics.units_per_em);
    let mut complete = true;
    for key in glyphs {
        let mut visited = false;
        let mut rasterized = None;
        coordinates_for(*key, &mut |coordinates| {
            visited = true;
            let (advance, left_side_bearing_delta) = if let Some(advances) = advances {
                let Some(advance) = advances
                    .get(usize::from(key.glyph_id))
                    .copied()
                    .map(i32::from)
                else {
                    return;
                };
                (advance, 0.0)
            } else {
                let Some(base_advance) = face
                    .glyph_advance_with_metrics(key.glyph_id, *metrics)
                    .ok()
                    .flatten()
                else {
                    return;
                };
                let Ok(deltas) = face.horizontal_metric_deltas_at_coordinates(
                    key.glyph_id,
                    coordinates,
                ) else {
                    return;
                };
                (
                    (i32::from(base_advance) + deltas[0]).max(0),
                    deltas[1] as f32,
                )
            };
            rasterized = rasterize_face_glyph_with_advance_at_coordinates(
                face,
                metrics,
                key.glyph_id,
                font_size,
                key.subpixel_x,
                key.subpixel_y,
                advance as f32 * scale,
                left_side_bearing_delta,
                key.weight,
                key.variation_id,
                coordinates,
                cache,
            );
        });
        let Some(glyph) = rasterized else {
            let _ = visited;
            complete = false;
            continue;
        };
        emit(*key, glyph);
    }
    complete
}

pub(crate) fn rasterize_face_glyph(
    face: &SfntFace<'_>,
    metrics: &FontMetrics,
    glyph_id: u16,
    font_size: f32,
    subpixel_x: u8,
    subpixel_y: u8,
    weight: u16,
    cache: &mut GlyphRasterCache,
) -> Option<RasterizedGlyph> {
    let scale = font_size / f32::from(metrics.units_per_em);
    let base_advance = face
        .glyph_advance_with_metrics(glyph_id, *metrics)
        .ok()??;
    let (coordinates, coordinate_count) = face.coordinates_for_weight_instance(weight);
    let coordinates = &coordinates[..coordinate_count];
    let deltas = face
        .horizontal_metric_deltas_at_coordinates(glyph_id, coordinates)
        .ok()?;
    let advance_width = (i32::from(base_advance) + deltas[0]).max(0) as f32 * scale;
    rasterize_face_glyph_with_advance_at_coordinates(
        face,
        metrics,
        glyph_id,
        font_size,
        subpixel_x,
        subpixel_y,
        advance_width,
        deltas[1] as f32,
        weight,
        0,
        coordinates,
        cache,
    )
}

/// Rasterizes one glyph after its scaled advance has already been read.
fn rasterize_face_glyph_with_advance_at_coordinates(
    face: &SfntFace<'_>,
    metrics: &FontMetrics,
    glyph_id: u16,
    font_size: f32,
    subpixel_x: u8,
    subpixel_y: u8,
    advance_width: f32,
    left_side_bearing_delta: f32,
    weight: u16,
    variation_id: u32,
    coordinates: &[f32],
    cache: &mut GlyphRasterCache,
) -> Option<RasterizedGlyph> {
    // `hvgl` and `emjc` are private Apple payloads. A public table with a
    // similar shape is not evidence that either payload is safe to decode, so
    // private-only faces leave the owned path before bitmap, color, or outline
    // parsing. A face carrying a public outline may still use that outline;
    // this makes mixed faces deterministic without guessing glyph precedence.
    if face.requires_platform_rasterization() {
        return None;
    }
    if face.has_color_tables() {
        // Bitmap strikes are the authored artwork for emoji faces. Prefer the
        // nearest bounded strike before considering layered vector outlines;
        // this keeps the portable path visually aligned with the compatibility
        // renderer while avoiding a platform text API.
        if let Some(bitmap) = face.bitmap_glyph(glyph_id, font_size, advance_width) {
            return Some(bitmap);
        }
        if let Some(svg) = face.svg_glyph(glyph_id) {
            return rasterize_svg_glyph(
                svg.as_ref(),
                metrics,
                font_size,
                subpixel_x,
                subpixel_y,
                advance_width,
                cache,
            );
        }
        if let Ok(Some(layers)) = face.color_layers(glyph_id)
            && let Some(rasterized) = rasterize_colr_v0_layers(
                face,
                metrics,
                layers,
                font_size,
                subpixel_x,
                subpixel_y,
                advance_width,
                left_side_bearing_delta,
                weight,
                variation_id,
                coordinates,
                cache,
            )
        {
            return Some(rasterized);
        }
    }

    let path = cached_outline_path(
        face,
        metrics,
        glyph_id,
        weight,
        variation_id,
        coordinates,
        cache,
    )?;

    if path.commands.is_empty() {
        return Some(empty_glyph(advance_width));
    }

    let key = FlattenedKey::new(
        glyph_id,
        font_size,
        subpixel_x,
        subpixel_y,
        weight,
        variation_id,
    );
    let shared = cache.shared.clone();
    let flattened = cache.flattened.entry(key).or_insert_with(|| {
        if let Some(shared) = &shared
            && let Ok(global) = shared.flattened.lock()
            && let Some(flattened) = global.get(&key)
        {
            return flattened.clone();
        }

        let flattened = flattened_path(
            &path,
            font_size / f32::from(metrics.units_per_em),
            subpixel_x,
            subpixel_y,
            left_side_bearing_delta,
        )
        .map(Arc::new);
        if let Some(shared) = &shared
            && let Ok(mut global) = shared.flattened.lock()
        {
            if let Some(existing) = global.get(&key) {
                return existing.clone();
            }
            if global.len() >= SHARED_FLATTENED_CACHE_CAPACITY {
                global.clear();
            }
            global.insert(key, flattened.clone());
        }
        flattened
    });
    rasterize_flattened_path(
        flattened.as_ref()?.as_ref(),
        advance_width,
        &mut cache.scratch,
    )
}

fn cached_outline_path(
    face: &SfntFace<'_>,
    metrics: &FontMetrics,
    glyph_id: u16,
    weight: u16,
    variation_id: u32,
    coordinates: &[f32],
    cache: &mut GlyphRasterCache,
) -> Option<Arc<OutlinePath>> {
    let shared = cache.shared.clone();
    let outline_key = (glyph_id, weight, variation_id);
    let path = cache.outlines.entry(outline_key).or_insert_with(|| {
        if let Some(shared) = &shared
            && let Ok(global) = shared.outlines.lock()
            && let Some(path) = global.get(&outline_key)
        {
            return path.clone();
        }

        let path = match face.outline_with_metrics_at_coordinates(
            glyph_id,
            *metrics,
            coordinates,
        ) {
            Ok(Some(outline)) => Ok(Some(Arc::new(from_true_type_outline(&outline)))),
            Ok(None) => face
                .cff_outline(glyph_id)
                .map(|outline| outline.map(|outline| Arc::new(from_cff_outline(outline)))),
            Err(error) => Err(error),
        };
        if let Some(shared) = &shared
            && let Ok(mut global) = shared.outlines.lock()
        {
            if let Some(existing) = global.get(&outline_key) {
                return existing.clone();
            }
            if global.len() >= SHARED_OUTLINE_CACHE_CAPACITY {
                global.clear();
            }
            global.insert(outline_key, path.clone());
        }
        path
    });
    path.as_ref().ok().and_then(Option::as_ref).cloned()
}

const MAX_COLOR_LAYERS_PER_GLYPH: usize = 1024;

fn rasterize_colr_v0_layers(
    face: &SfntFace<'_>,
    metrics: &FontMetrics,
    layers: &[ColorLayer],
    font_size: f32,
    subpixel_x: u8,
    subpixel_y: u8,
    advance_width: f32,
    left_side_bearing_delta: f32,
    weight: u16,
    variation_id: u32,
    coordinates: &[f32],
    cache: &mut GlyphRasterCache,
) -> Option<RasterizedGlyph> {
    if layers.is_empty() || layers.len() > MAX_COLOR_LAYERS_PER_GLYPH {
        return None;
    }

    let mut paths = Vec::with_capacity(layers.len());
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for layer in layers {
        let color = face.palette_color(layer.palette_index).ok()??;
        let path = cached_outline_path(
            face,
            metrics,
            layer.glyph_id,
            weight,
            variation_id,
            coordinates,
            cache,
        )?;
        if path.commands.is_empty() || !path.bounds.iter().all(|value| value.is_finite()) {
            return None;
        }
        bounds[0] = bounds[0].min(path.bounds[0]);
        bounds[1] = bounds[1].min(path.bounds[1]);
        bounds[2] = bounds[2].max(path.bounds[2]);
        bounds[3] = bounds[3].max(path.bounds[3]);
        paths.push((path, color));
    }

    let scale = font_size / f32::from(metrics.units_per_em);
    let phase_x = f32::from(subpixel_x % SUBPIXEL_PHASES) / f32::from(SUBPIXEL_PHASES);
    let phase_y = f32::from(subpixel_y % SUBPIXEL_PHASES) / f32::from(SUBPIXEL_PHASES);
    let min_x = bounds[0] + left_side_bearing_delta;
    let max_x = bounds[2] + left_side_bearing_delta;
    if ![min_x, bounds[1], max_x, bounds[3], scale]
        .iter()
        .all(|value| value.is_finite())
        || min_x > max_x
        || bounds[1] > bounds[3]
    {
        return None;
    }
    let left = checked_floor_to_i32(min_x * scale + phase_x)?;
    let bottom = checked_floor_to_i32(bounds[1] * scale + phase_y)?;
    let right = checked_ceil_to_i32(max_x * scale + phase_x)?;
    let top = checked_ceil_to_i32(bounds[3] * scale + phase_y)?;
    let width = (i64::from(right) - i64::from(left)).max(1);
    let height = (i64::from(top) - i64::from(bottom)).max(1);
    if width > MAX_RASTER_EXTENT || height > MAX_RASTER_EXTENT {
        return None;
    }
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    let bitmap_len = width.checked_mul(height)?;
    let mut bitmap = vec![0_u8; bitmap_len.checked_mul(4)?];

    for (path, color) in paths {
        let edges = flatten_path_edges(
            &path,
            scale,
            phase_x,
            phase_y,
            left_side_bearing_delta,
            left,
            bottom,
        )?;
        let row_edges = edge_rows(&edges, height)?;
        let flattened = FlattenedGlyph {
            left,
            bottom,
            width,
            height,
            phase_x,
            phase_y,
            edges,
            row_edges,
            coverage: Arc::new(OnceLock::new()),
        };
        let coverage = build_coverage_bitmap(&flattened, &mut cache.scratch)?;
        composite_color_layer(&mut bitmap, &coverage, color);
    }

    Some(RasterizedGlyph {
        bitmap,
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
        offset_x: left as f32 - phase_x,
        offset_y: bottom as f32 - phase_y,
        advance_width,
        is_color: true,
    })
}

fn rasterize_svg_glyph(
    glyph: &SvgGlyph,
    metrics: &FontMetrics,
    font_size: f32,
    subpixel_x: u8,
    subpixel_y: u8,
    advance_width: f32,
    cache: &mut GlyphRasterCache,
) -> Option<RasterizedGlyph> {
    if glyph.paths.is_empty() {
        return None;
    }
    let scale = font_size / f32::from(metrics.units_per_em);
    let phase_x = f32::from(subpixel_x % SUBPIXEL_PHASES) / f32::from(SUBPIXEL_PHASES);
    let phase_y = f32::from(subpixel_y % SUBPIXEL_PHASES) / f32::from(SUBPIXEL_PHASES);
    let [min_x, min_y, max_x, max_y] = glyph.bounds;
    if ![min_x, min_y, max_x, max_y, scale]
        .iter()
        .all(|value| value.is_finite())
        || scale <= 0.0
        || min_x > max_x
        || min_y > max_y
    {
        return None;
    }
    let left = checked_floor_to_i32(min_x * scale + phase_x)?;
    let bottom = checked_floor_to_i32(min_y * scale + phase_y)?;
    let right = checked_ceil_to_i32(max_x * scale + phase_x)?;
    let top = checked_ceil_to_i32(max_y * scale + phase_y)?;
    let width = (i64::from(right) - i64::from(left)).max(1);
    let height = (i64::from(top) - i64::from(bottom)).max(1);
    if width > MAX_RASTER_EXTENT || height > MAX_RASTER_EXTENT {
        return None;
    }
    let width = usize::try_from(width).ok()?;
    let height = usize::try_from(height).ok()?;
    let bitmap_len = width.checked_mul(height)?;
    let mut bitmap = vec![0_u8; bitmap_len.checked_mul(4)?];

    for path in glyph.paths.iter() {
        let outline = svg_outline_path(path);
        let edges = flatten_path_edges(
            &outline,
            scale,
            phase_x,
            phase_y,
            0.0,
            left,
            bottom,
        )?;
        if edges.is_empty() {
            continue;
        }
        let row_edges = edge_rows(&edges, height)?;
        let flattened = FlattenedGlyph {
            left,
            bottom,
            width,
            height,
            phase_x,
            phase_y,
            edges,
            row_edges,
            coverage: Arc::new(OnceLock::new()),
        };
        let fill_rule = match path.fill_rule {
            SvgFillRule::NonZero => CoverageFillRule::NonZero,
            SvgFillRule::EvenOdd => CoverageFillRule::EvenOdd,
        };
        let coverage = build_coverage_bitmap_with_fill_rule(
            &flattened,
            &mut cache.scratch,
            fill_rule,
        )?;
        composite_color_layer(&mut bitmap, &coverage, path.color);
    }

    Some(RasterizedGlyph {
        bitmap,
        width: u32::try_from(width).ok()?,
        height: u32::try_from(height).ok()?,
        offset_x: left as f32 - phase_x,
        offset_y: bottom as f32 - phase_y,
        advance_width,
        is_color: true,
    })
}

fn svg_outline_path(path: &SvgPath) -> OutlinePath {
    let commands = path
        .commands
        .iter()
        .map(|command| match *command {
            SvgPathCommand::MoveTo { x, y } => PathCommand::MoveTo(Point { x, y }),
            SvgPathCommand::LineTo { x, y } => PathCommand::LineTo(Point { x, y }),
            SvgPathCommand::QuadraticTo {
                control_x,
                control_y,
                x,
                y,
            } => PathCommand::QuadTo {
                control: Point {
                    x: control_x,
                    y: control_y,
                },
                point: Point { x, y },
            },
            SvgPathCommand::CubicTo {
                control1_x,
                control1_y,
                control2_x,
                control2_y,
                x,
                y,
            } => PathCommand::CurveTo {
                control_1: Point {
                    x: control1_x,
                    y: control1_y,
                },
                control_2: Point {
                    x: control2_x,
                    y: control2_y,
                },
                point: Point { x, y },
            },
            SvgPathCommand::Close => PathCommand::Close,
        })
        .collect();
    OutlinePath {
        bounds: path.bounds,
        commands,
    }
}

fn composite_color_layer(bitmap: &mut [u8], coverage: &[u8], color: ColorRgba) {
    for (pixel, coverage) in bitmap.chunks_exact_mut(4).zip(coverage) {
        let source_alpha = (u32::from(*coverage) * u32::from(color.alpha) + 127) / 255;
        if source_alpha == 0 {
            continue;
        }
        let destination_alpha = u32::from(pixel[3]);
        let inverse_source_alpha = 255 - source_alpha;
        let output_alpha =
            source_alpha + (destination_alpha * inverse_source_alpha + 127) / 255;
        let destination_factor = destination_alpha * inverse_source_alpha;
        pixel[0] = ((u32::from(color.red) * source_alpha
            + (u32::from(pixel[0]) * destination_factor + 127) / 255
            + output_alpha / 2)
            / output_alpha) as u8;
        pixel[1] = ((u32::from(color.green) * source_alpha
            + (u32::from(pixel[1]) * destination_factor + 127) / 255
            + output_alpha / 2)
            / output_alpha) as u8;
        pixel[2] = ((u32::from(color.blue) * source_alpha
            + (u32::from(pixel[2]) * destination_factor + 127) / 255
            + output_alpha / 2)
            / output_alpha) as u8;
        pixel[3] = output_alpha as u8;
    }
}

fn empty_glyph(advance_width: f32) -> RasterizedGlyph {
    RasterizedGlyph {
        bitmap: Vec::new(),
        width: 0,
        height: 0,
        offset_x: 0.0,
        offset_y: 0.0,
        advance_width,
        is_color: false,
    }
}

fn from_true_type_outline(outline: &GlyphOutline) -> OutlinePath {
    let command_capacity = outline.contours.iter().fold(0_usize, |capacity, contour| {
        capacity.saturating_add((contour.end - contour.start).saturating_add(1))
    });
    let mut commands = Vec::with_capacity(command_capacity);
    let mut point_bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for contour in &outline.contours {
        append_true_type_contour(
            &outline.points[contour.start..contour.end],
            &mut commands,
            &mut point_bounds,
        );
    }
    let bounds = outline.bounds.map(f32::from);
    let point_bounds = if point_bounds[0].is_finite() {
        point_bounds
    } else {
        [0.0; 4]
    };
    OutlinePath {
        // A valid TrueType bbox encloses every point and curve extremum. Use
        // the decoded points when a malformed bbox does not even enclose the
        // points; this keeps rasterization bounded and makes the fallback
        // behavior deterministic for otherwise recoverable glyph data.
        bounds: if bounds_enclose(bounds, point_bounds) {
            bounds
        } else {
            point_bounds
        },
        commands,
    }
}

fn bounds_enclose(bounds: [f32; 4], points: [f32; 4]) -> bool {
    bounds[0] <= points[0]
        && bounds[1] <= points[1]
        && bounds[2] >= points[2]
        && bounds[3] >= points[3]
}

fn append_true_type_contour(
    contour: &[OutlinePoint],
    commands: &mut Vec<PathCommand>,
    point_bounds: &mut [f32; 4],
) {
    let Some(first) = contour.first().copied() else {
        return;
    };
    let last = contour.last().copied().unwrap_or(first);
    let start = if first.on_curve {
        Point {
            x: first.x,
            y: first.y,
        }
    } else if last.on_curve {
        Point {
            x: last.x,
            y: last.y,
        }
    } else {
        midpoint(point_from_outline(last), point_from_outline(first))
    };

    commands.push(PathCommand::MoveTo(start));
    let mut current = start;
    let mut index = 0;
    let mut consumed = 0;
    while consumed < contour.len() {
        let point = contour[index % contour.len()];
        include_point_bounds(point_bounds, point);
        if point.on_curve {
            let next = point_from_outline(point);
            if !same_point(current, next) {
                commands.push(PathCommand::LineTo(next));
            }
            current = next;
            index += 1;
            consumed += 1;
            continue;
        }

        let next = contour[(index + 1) % contour.len()];
        include_point_bounds(point_bounds, next);
        let control = point_from_outline(point);
        if next.on_curve {
            let endpoint = point_from_outline(next);
            commands.push(PathCommand::QuadTo {
                control,
                point: endpoint,
            });
            current = endpoint;
            index += 2;
            consumed += 2;
        } else {
            let endpoint = midpoint(control, point_from_outline(next));
            commands.push(PathCommand::QuadTo {
                control,
                point: endpoint,
            });
            current = endpoint;
            index += 1;
            consumed += 1;
        }
    }
    commands.push(PathCommand::Close);
}

fn include_point_bounds(bounds: &mut [f32; 4], point: OutlinePoint) {
    bounds[0] = bounds[0].min(point.x);
    bounds[1] = bounds[1].min(point.y);
    bounds[2] = bounds[2].max(point.x);
    bounds[3] = bounds[3].max(point.y);
}

fn point_from_outline(point: OutlinePoint) -> Point {
    Point {
        x: point.x,
        y: point.y,
    }
}

fn from_cff_outline(outline: CffGlyphOutline) -> OutlinePath {
    let commands = outline
        .commands
        .into_iter()
        .map(|command| match command {
            CffPathCommand::MoveTo { x, y } => PathCommand::MoveTo(Point { x, y }),
            CffPathCommand::LineTo { x, y } => PathCommand::LineTo(Point { x, y }),
            CffPathCommand::CurveTo {
                control_1_x,
                control_1_y,
                control_2_x,
                control_2_y,
                x,
                y,
            } => PathCommand::CurveTo {
                control_1: Point {
                    x: control_1_x,
                    y: control_1_y,
                },
                control_2: Point {
                    x: control_2_x,
                    y: control_2_y,
                },
                point: Point { x, y },
            },
            CffPathCommand::Close => PathCommand::Close,
        })
        .collect();
    OutlinePath {
        bounds: outline.bounds,
        commands,
    }
}

fn rasterize_path(
    path: &OutlinePath,
    scale: f32,
    subpixel_x: u8,
    subpixel_y: u8,
    advance_width: f32,
) -> Option<RasterizedGlyph> {
    let flattened = flattened_path(path, scale, subpixel_x, subpixel_y, 0.0)?;
    let mut scratch = RasterScratch::default();
    rasterize_flattened_path(&flattened, advance_width, &mut scratch)
}

fn flattened_path(
    path: &OutlinePath,
    scale: f32,
    subpixel_x: u8,
    subpixel_y: u8,
    translate_x: f32,
) -> Option<FlattenedGlyph> {
    let phase_x = f32::from(subpixel_x % SUBPIXEL_PHASES) / f32::from(SUBPIXEL_PHASES);
    let phase_y = f32::from(subpixel_y % SUBPIXEL_PHASES) / f32::from(SUBPIXEL_PHASES);
    let [path_min_x, min_y, path_max_x, max_y] = path.bounds;
    let min_x = path_min_x + translate_x;
    let max_x = path_max_x + translate_x;
    if ![min_x, min_y, max_x, max_y, scale].iter().all(|value| value.is_finite())
        || min_x > max_x
        || min_y > max_y
    {
        return None;
    }

    let left = checked_floor_to_i32(min_x * scale + phase_x)?;
    let bottom = checked_floor_to_i32(min_y * scale + phase_y)?;
    let right = checked_ceil_to_i32(max_x * scale + phase_x)?;
    let top = checked_ceil_to_i32(max_y * scale + phase_y)?;
    let width = i64::from(right) - i64::from(left);
    let height = i64::from(top) - i64::from(bottom);
    let width = width.max(1);
    let height = height.max(1);
    if width > MAX_RASTER_EXTENT || height > MAX_RASTER_EXTENT {
        return None;
    }

    let width_usize = usize::try_from(width).ok()?;
    let height_usize = usize::try_from(height).ok()?;
    let edges = flatten_path_edges(
        path,
        scale,
        phase_x,
        phase_y,
        translate_x,
        left,
        bottom,
    )?;
    let row_edges = edge_rows(&edges, height_usize)?;
    Some(FlattenedGlyph {
        left,
        bottom,
        width: width_usize,
        height: height_usize,
        phase_x,
        phase_y,
        edges,
        row_edges,
        coverage: Arc::new(OnceLock::new()),
    })
}

fn rasterize_flattened_path(
    flattened: &FlattenedGlyph,
    advance_width: f32,
    scratch: &mut RasterScratch,
) -> Option<RasterizedGlyph> {
    if flattened.edges.is_empty() {
        return Some(empty_glyph(advance_width));
    }
    let bitmap = if let Some(coverage) = flattened.coverage.get() {
        coverage.as_ref().to_vec()
    } else {
        let bitmap = build_coverage_bitmap(flattened, scratch)?;
        if bitmap.len() <= MAX_CACHED_COVERAGE_BYTES {
            let coverage: Arc<[u8]> = Arc::from(bitmap.into_boxed_slice());
            let _ = flattened.coverage.set(coverage.clone());
            coverage.as_ref().to_vec()
        } else {
            bitmap
        }
    };
    Some(RasterizedGlyph {
        bitmap,
        width: u32::try_from(flattened.width).ok()?,
        height: u32::try_from(flattened.height).ok()?,
        offset_x: flattened.left as f32 - flattened.phase_x,
        offset_y: flattened.bottom as f32 - flattened.phase_y,
        advance_width,
        is_color: false,
    })
}

fn build_coverage_bitmap(
    flattened: &FlattenedGlyph,
    scratch: &mut RasterScratch,
) -> Option<Vec<u8>> {
    build_coverage_bitmap_with_fill_rule(flattened, scratch, CoverageFillRule::NonZero)
}

fn build_coverage_bitmap_with_fill_rule(
    flattened: &FlattenedGlyph,
    scratch: &mut RasterScratch,
    fill_rule: CoverageFillRule,
) -> Option<Vec<u8>> {
    let bitmap_len = flattened.width.checked_mul(flattened.height)?;
    let mut bitmap = vec![0; bitmap_len];
    let has_coverage = scan_convert_into_with_fill_rule(
        &flattened.edges,
        Some(&flattened.row_edges),
        flattened.width,
        flattened.height,
        SAMPLE_GRID,
        &mut bitmap,
        scratch,
        fill_rule,
    );
    if !has_coverage {
        scan_convert_into_with_fill_rule(
            &flattened.edges,
            Some(&flattened.row_edges),
            flattened.width,
            flattened.height,
            SMALL_GLYPH_SAMPLE_GRID,
            &mut bitmap,
            scratch,
            fill_rule,
        );
    }
    Some(bitmap)
}

fn checked_floor_to_i32(value: f32) -> Option<i32> {
    if !value.is_finite() || value < i32::MIN as f32 || value > i32::MAX as f32 {
        return None;
    }
    Some(value.floor() as i32)
}

fn checked_ceil_to_i32(value: f32) -> Option<i32> {
    if !value.is_finite() || value < i32::MIN as f32 || value > i32::MAX as f32 {
        return None;
    }
    Some(value.ceil() as i32)
}

fn flatten_path_edges(
    path: &OutlinePath,
    scale: f32,
    phase_x: f32,
    phase_y: f32,
    translate_x: f32,
    left: i32,
    bottom: i32,
) -> Option<Vec<Edge>> {
    let transform = |point: Point| Point {
        x: (point.x + translate_x) * scale + phase_x - left as f32,
        y: point.y * scale + phase_y - bottom as f32,
    };
    // Emit edges while flattening instead of collecting every curve endpoint
    // in a temporary contour vector first. The recursive subdivision keeps
    // the same endpoint order, so this removes one cold-path allocation
    // without changing the coverage geometry.
    let edge_capacity = path.commands.len().min(256);
    let mut edges = Vec::with_capacity(edge_capacity);
    let mut start = None;
    let mut current = None;
    let mut flattened_points = 0;
    let mut contour_points = 0;

    for command in &path.commands {
        match *command {
            PathCommand::MoveTo(point) => {
                finish_contour_edges(
                    &mut edges,
                    start,
                    current,
                    contour_points,
                    &mut flattened_points,
                )?;
                let point = transform(point);
                flattened_points = flattened_points.checked_add(1)?;
                start = Some(point);
                current = Some(point);
                contour_points = 1;
            }
            PathCommand::LineTo(point) => {
                let point = transform(point);
                let previous = current?;
                push_edge(
                    &mut edges,
                    previous,
                    point,
                    &mut contour_points,
                    &mut flattened_points,
                )?;
                current = Some(point);
            }
            PathCommand::QuadTo { control, point } => {
                let previous = current?;
                let control = transform(control);
                let point = transform(point);
                flatten_quad_edges(
                    previous,
                    control,
                    point,
                    CURVE_FLATTEN_TOLERANCE,
                    0,
                    &mut edges,
                    &mut contour_points,
                    &mut flattened_points,
                )?;
                current = Some(point);
            }
            PathCommand::CurveTo {
                control_1,
                control_2,
                point,
            } => {
                let previous = current?;
                let control_1 = transform(control_1);
                let control_2 = transform(control_2);
                let point = transform(point);
                flatten_cubic_edges(
                    previous,
                    control_1,
                    control_2,
                    point,
                    CURVE_FLATTEN_TOLERANCE,
                    0,
                    &mut edges,
                    &mut contour_points,
                    &mut flattened_points,
                )?;
                current = Some(point);
            }
            PathCommand::Close => {
                finish_contour_edges(
                    &mut edges,
                    start,
                    current,
                    contour_points,
                    &mut flattened_points,
                )?;
                start = None;
                current = None;
                contour_points = 0;
            }
        }
        if flattened_points > MAX_FLATTENED_POINTS {
            return None;
        }
    }
    finish_contour_edges(
        &mut edges,
        start,
        current,
        contour_points,
        &mut flattened_points,
    )?;
    if flattened_points > MAX_FLATTENED_POINTS {
        return None;
    }
    Some(edges)
}

fn finish_contour_edges(
    edges: &mut Vec<Edge>,
    start: Option<Point>,
    current: Option<Point>,
    contour_points: usize,
    flattened_points: &mut usize,
) -> Option<()> {
    if contour_points >= 2 {
        if let (Some(start), Some(current)) = (start, current) {
            let mut ignored_contour_points = 0;
            push_edge(
                edges,
                current,
                start,
                &mut ignored_contour_points,
                flattened_points,
            )?;
        }
    }
    Some(())
}

fn push_edge(
    edges: &mut Vec<Edge>,
    start: Point,
    end: Point,
    contour_points: &mut usize,
    flattened_points: &mut usize,
) -> Option<()> {
    if !same_point(start, end) {
        if *flattened_points >= MAX_FLATTENED_POINTS {
            return None;
        }
        edges.push(Edge::new(start, end));
        *contour_points = (*contour_points).checked_add(1)?;
        *flattened_points = (*flattened_points).checked_add(1)?;
    }
    Some(())
}

fn flatten_quad_edges(
    start: Point,
    control: Point,
    end: Point,
    tolerance: f32,
    depth: u8,
    edges: &mut Vec<Edge>,
    contour_points: &mut usize,
    flattened_points: &mut usize,
) -> Option<()> {
    if depth >= MAX_CURVE_DEPTH || point_line_distance(control, start, end) <= tolerance {
        return push_edge(edges, start, end, contour_points, flattened_points);
    }
    let start_control = midpoint(start, control);
    let control_end = midpoint(control, end);
    let center = midpoint(start_control, control_end);
    flatten_quad_edges(
        start,
        start_control,
        center,
        tolerance,
        depth + 1,
        edges,
        contour_points,
        flattened_points,
    )?;
    flatten_quad_edges(
        center,
        control_end,
        end,
        tolerance,
        depth + 1,
        edges,
        contour_points,
        flattened_points,
    )
}

fn flatten_cubic_edges(
    start: Point,
    control_1: Point,
    control_2: Point,
    end: Point,
    tolerance: f32,
    depth: u8,
    edges: &mut Vec<Edge>,
    contour_points: &mut usize,
    flattened_points: &mut usize,
) -> Option<()> {
    let flatness = point_line_distance(control_1, start, end)
        .max(point_line_distance(control_2, start, end));
    if depth >= MAX_CURVE_DEPTH || flatness <= tolerance {
        return push_edge(edges, start, end, contour_points, flattened_points);
    }

    let start_control = midpoint(start, control_1);
    let control_center = midpoint(control_1, control_2);
    let center_end = midpoint(control_2, end);
    let left_center = midpoint(start_control, control_center);
    let right_center = midpoint(control_center, center_end);
    let center = midpoint(left_center, right_center);
    flatten_cubic_edges(
        start,
        start_control,
        left_center,
        center,
        tolerance,
        depth + 1,
        edges,
        contour_points,
        flattened_points,
    )?;
    flatten_cubic_edges(
        center,
        right_center,
        center_end,
        end,
        tolerance,
        depth + 1,
        edges,
        contour_points,
        flattened_points,
    )
}

fn point_line_distance(point: Point, start: Point, end: Point) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let denominator = dx.hypot(dy);
    if denominator <= f32::EPSILON {
        return (point.x - start.x).hypot(point.y - start.y);
    }
    ((point.x - start.x) * dy - (point.y - start.y) * dx).abs() / denominator
}

fn contours_to_edges(contours: &[Vec<Point>]) -> Vec<Edge> {
    contours
        .iter()
        .flat_map(|contour| {
            contour
                .windows(2)
                .map(|edge| Edge::new(edge[0], edge[1]))
        })
        .collect()
}

fn edge_winding(start: Point, end: Point) -> i8 {
    match end.y.partial_cmp(&start.y) {
        Some(Ordering::Greater) => 1,
        Some(Ordering::Less) => -1,
        _ => 0,
    }
}

fn edge_rows(edges: &[Edge], height: usize) -> Option<EdgeRows> {
    let mut counts = vec![0_usize; height];
    let height = height as f32;
    for edge in edges {
        if edge.min_y >= edge.max_y {
            continue;
        }
        let first_row = (height - edge.max_y).floor().max(0.0) as usize;
        let last_row = (height - edge.min_y).ceil().min(height) as usize;
        for row in first_row.min(counts.len())..last_row.min(counts.len()) {
            let row_top = height - row as f32;
            let row_bottom = row_top - 1.0;
            if edge.max_y > row_bottom && edge.min_y < row_top {
                counts[row] = counts[row].checked_add(1)?;
            }
        }
    }

    let mut offsets: Vec<usize> = Vec::with_capacity(counts.len().checked_add(1)?);
    offsets.push(0);
    for &count in &counts {
        let next = offsets.last().copied()?.checked_add(count)?;
        offsets.push(next);
    }

    let mut indices = vec![0; offsets.last().copied()?];
    // Reuse the counting array as the write cursor. The previous version
    // cloned one cursor per row, which added a second allocation to every
    // cold glyph flattening.
    for (row, cursor) in counts.iter_mut().enumerate() {
        *cursor = offsets[row];
    }
    let height_f32 = height;
    for (index, edge) in edges.iter().enumerate() {
        if edge.min_y >= edge.max_y {
            continue;
        }
        let first_row = (height_f32 - edge.max_y).floor().max(0.0) as usize;
        let last_row = (height_f32 - edge.min_y).ceil().min(height_f32) as usize;
        for row in first_row.min(counts.len())..last_row.min(counts.len()) {
            let row_top = height_f32 - row as f32;
            let row_bottom = row_top - 1.0;
            if edge.max_y > row_bottom && edge.min_y < row_top {
                let position = counts[row];
                counts[row] = position.checked_add(1)?;
                *indices.get_mut(position)? = index;
            }
        }
    }

    Some(EdgeRows { offsets, indices })
}

fn build_scanline_plan(
    edges: &[Edge],
    row_edges: &EdgeRows,
    width: usize,
    height: usize,
    sample_grid: u32,
) -> Option<ScanlinePlan> {
    build_scanline_plan_with(edges, row_edges, width, height, sample_grid, |_, _| {})
}

fn build_scanline_plan_and_fill(
    edges: &[Edge],
    row_edges: &EdgeRows,
    width: usize,
    height: usize,
    sample_grid: u32,
    bitmap: &mut [u8],
) -> Option<(ScanlinePlan, bool)> {
    let bitmap_len = width.checked_mul(height)?;
    if bitmap.len() < bitmap_len {
        return None;
    }
    bitmap[..bitmap_len].fill(0);

    let sample_grid_usize = usize::try_from(sample_grid).ok()?;
    let mut has_coverage = false;
    let plan = build_scanline_plan_with(
        edges,
        row_edges,
        width,
        height,
        sample_grid,
        |scanline, spans| {
            let row = scanline / sample_grid_usize;
            has_coverage |= fill_coverage_spans(
                spans,
                row * width,
                width,
                sample_grid,
                bitmap,
            );
        },
    )?;
    normalize_bitmap(&mut bitmap[..bitmap_len], sample_grid);
    Some((plan, has_coverage))
}

fn build_scanline_plan_with<F>(
    edges: &[Edge],
    row_edges: &EdgeRows,
    width: usize,
    height: usize,
    sample_grid: u32,
    mut emit_spans: F,
) -> Option<ScanlinePlan>
where
    F: FnMut(usize, &[CoverageSpan]),
{
    if sample_grid == 0 {
        return None;
    }
    let sample_grid_usize = usize::try_from(sample_grid).ok()?;
    let scanline_count = height.checked_mul(sample_grid_usize)?;
    let mut offsets = Vec::new();
    offsets
        .try_reserve(scanline_count.checked_add(1)?)
        .ok()?;
    offsets.push(0);

    let mut intersections = Vec::new();
    intersections.try_reserve(edges.len()).ok()?;
    let mut spans = Vec::new();

    for row in 0..height {
        let edges_for_row = row_edges.row(row);
        for sample_y in 0..sample_grid {
            let y = height as f32
                - (row as f32 + (sample_y as f32 + 0.5) / sample_grid as f32);
            intersections.clear();
            for &edge_index in edges_for_row {
                let edge = edges[edge_index];
                if y < edge.min_y || y >= edge.max_y {
                    continue;
                }
                let crossing = edge.inverse_slope * (y - edge.start.y) + edge.start.x;
                if crossing.is_finite() {
                    intersections.push(ScanlineIntersection {
                        x: crossing,
                        winding: edge.winding,
                    });
                }
            }
            intersections.sort_unstable_by(|first, second| {
                first.x.total_cmp(&second.x)
            });

            let line_start = spans.len();
            let mut winding = 0_i32;
            let mut span_start = 0.0_f32;
            for intersection in &intersections {
                let start = span_start.max(0.0);
                let end = intersection.x.min(width as f32);
                if start >= end {
                    winding += i32::from(intersection.winding);
                    span_start = intersection.x;
                    continue;
                }
                if winding != 0 {
                    if spans.len() >= MAX_FLATTENED_POINTS {
                        return None;
                    }
                    spans.push(CoverageSpan { start, end });
                }
                winding += i32::from(intersection.winding);
                span_start = intersection.x;
            }
            emit_spans(
                row * sample_grid_usize + sample_y as usize,
                &spans[line_start..],
            );
            offsets.push(spans.len());
        }
    }

    Some(ScanlinePlan {
        sample_grid,
        offsets,
        spans,
    })
}

fn fill_scanline_plan(
    plan: &ScanlinePlan,
    width: usize,
    height: usize,
    bitmap: &mut [u8],
) -> bool {
    let Some(bitmap_len) = width.checked_mul(height) else {
        return false;
    };
    if bitmap.len() < bitmap_len || plan.sample_grid == 0 {
        return false;
    }
    let Ok(sample_grid) = usize::try_from(plan.sample_grid) else {
        return false;
    };
    let Some(scanline_count) = height.checked_mul(sample_grid) else {
        return false;
    };
    if plan.offsets.len() < scanline_count.saturating_add(1) {
        return false;
    }
    bitmap[..bitmap_len].fill(0);

    let mut has_coverage = false;
    for row in 0..height {
        let row_offset = row * width;
        for sample_y in 0..sample_grid {
            let scanline = row * sample_grid + sample_y;
            let start = plan.offsets[scanline];
            let end = plan.offsets[scanline + 1];
            has_coverage |= fill_coverage_spans(
                &plan.spans[start..end],
                row_offset,
                width,
                plan.sample_grid,
                bitmap,
            );
        }
    }

    normalize_bitmap(&mut bitmap[..bitmap_len], plan.sample_grid);
    has_coverage
}

fn fill_coverage_spans(
    spans: &[CoverageSpan],
    row_offset: usize,
    width: usize,
    sample_grid: u32,
    bitmap: &mut [u8],
) -> bool {
    let mut has_coverage = false;
    for span in spans {
        let first_column = span.start.floor() as usize;
        let last_column = span.end.ceil().min(width as f32) as usize;
        for column in first_column..last_column {
            let column_start = column as f32;
            let covered_samples = if span.start <= column_start && span.end >= column_start + 1.0 {
                sample_grid
            } else {
                horizontal_coverage_samples(span.start, span.end, column_start, sample_grid)
            };
            if covered_samples != 0 {
                bitmap[row_offset + column] += covered_samples as u8;
                has_coverage = true;
            }
        }
    }
    has_coverage
}

fn normalize_bitmap(bitmap: &mut [u8], sample_grid: u32) {
    let denominator = sample_grid * sample_grid;
    for covered in bitmap {
        *covered = (u32::from(*covered) * 255 + denominator / 2)
            .checked_div(denominator)
            .unwrap_or(0) as u8;
    }
}

fn scan_convert_reference(
    contours: &[Vec<Point>],
    width: usize,
    height: usize,
    bitmap_len: usize,
) -> Vec<u8> {
    let edges = contours
        .iter()
        .flat_map(|contour| contour.windows(2).map(|edge| (edge[0], edge[1])))
        .collect::<Vec<_>>();
    let mut intersections = Vec::with_capacity(edges.len());
    let mut covered_samples = vec![0_u8; bitmap_len];

    for row in 0..height {
        for sample_y in 0..SAMPLE_GRID {
            let y = height as f32 - (row as f32 + (sample_y as f32 + 0.5) / SAMPLE_GRID as f32);
            intersections.clear();
            for (start, end) in &edges {
                if (start.y > y) == (end.y > y) {
                    continue;
                }
                let crossing = (end.x - start.x) * (y - start.y) / (end.y - start.y) + start.x;
                if crossing.is_finite() {
                    intersections.push(ScanlineIntersection {
                        x: crossing,
                        winding: edge_winding(*start, *end),
                    });
                }
            }
            intersections.sort_unstable_by(|first, second| {
                first.x.total_cmp(&second.x)
            });

            let mut winding = 0_i32;
            let mut span_start = 0.0_f32;
            for intersection in &intersections {
                let start = span_start.max(0.0);
                let end = intersection.x.min(width as f32);
                if start >= end {
                    winding += i32::from(intersection.winding);
                    span_start = intersection.x;
                    continue;
                }
                if winding != 0 {
                    let first_column = start.floor() as usize;
                    let last_column = end.ceil().min(width as f32) as usize;
                    for column in first_column..last_column {
                        for sample_x in 0..SAMPLE_GRID {
                            let x = column as f32
                                + (sample_x as f32 + 0.5) / SAMPLE_GRID as f32;
                            if x >= start && x < end {
                                covered_samples[row * width + column] += 1;
                            }
                        }
                    }
                }
                winding += i32::from(intersection.winding);
                span_start = intersection.x;
            }
        }
    }

    covered_samples
        .into_iter()
        .map(|covered| {
            (u32::from(covered) * 255 + SAMPLE_GRID * SAMPLE_GRID / 2)
                .checked_div(SAMPLE_GRID * SAMPLE_GRID)
                .unwrap_or(0) as u8
        })
        .collect()
}

fn scan_convert_into_with_rows(
    edges: &[Edge],
    row_edges: Option<&EdgeRows>,
    width: usize,
    height: usize,
    sample_grid: u32,
    bitmap: &mut [u8],
    scratch: &mut RasterScratch,
) -> bool {
    scan_convert_into_with_fill_rule(
        edges,
        row_edges,
        width,
        height,
        sample_grid,
        bitmap,
        scratch,
        CoverageFillRule::NonZero,
    )
}

fn scan_convert_into_with_fill_rule(
    edges: &[Edge],
    row_edges: Option<&EdgeRows>,
    width: usize,
    height: usize,
    sample_grid: u32,
    bitmap: &mut [u8],
    scratch: &mut RasterScratch,
    fill_rule: CoverageFillRule,
) -> bool {
    let Some(bitmap_len) = width.checked_mul(height) else {
        return false;
    };
    if bitmap.len() < bitmap_len {
        return false;
    }
    if sample_grid == 0 {
        return false;
    }
    bitmap[..bitmap_len].fill(0);

    scratch.active_edges.clear();
    scratch.active_positions.resize(edges.len(), usize::MAX);
    scratch.active_positions.fill(usize::MAX);
    let mut has_coverage = false;

    for row in 0..height {
        scratch.active_edges.clear();
        let candidates = row_edges.map(|rows| rows.row(row));

        for sample_y in 0..sample_grid {
            let y = height as f32
                - (row as f32 + (sample_y as f32 + 0.5) / sample_grid as f32);
            update_active_edges(
                edges,
                candidates,
                y,
                scratch,
            );
            sort_active_edges(scratch, sample_y == 0);
            has_coverage |= fill_active_scanline(
                row,
                width,
                sample_grid,
                bitmap,
                scratch,
                fill_rule,
            );
        }

        for active in &scratch.active_edges {
            scratch.active_positions[active.edge_index] = usize::MAX;
        }
    }

    scratch.active_edges.clear();
    for covered in &mut bitmap[..bitmap_len] {
        *covered = (u32::from(*covered) * 255 + sample_grid * sample_grid / 2)
            .checked_div(sample_grid * sample_grid)
            .unwrap_or(0) as u8;
    }
    has_coverage
}

fn update_active_edges(
    edges: &[Edge],
    candidates: Option<&[usize]>,
    y: f32,
    scratch: &mut RasterScratch,
) {
    let mut active_index = 0;
    while active_index < scratch.active_edges.len() {
        let edge_index = scratch.active_edges[active_index].edge_index;
        let edge = edges[edge_index];
        if y < edge.min_y || y >= edge.max_y {
            scratch.active_positions[edge_index] = usize::MAX;
            scratch.active_edges.swap_remove(active_index);
            if let Some(moved) = scratch.active_edges.get(active_index) {
                scratch.active_positions[moved.edge_index] = active_index;
            }
            continue;
        }
        // Recompute from the sample coordinate instead of accumulating a
        // slope delta. Accumulation can cross a coverage-sample boundary due
        // to float drift and change a glyph's edge by one antialiasing sample.
        scratch.active_edges[active_index].x =
            edge.inverse_slope * (y - edge.start.y) + edge.start.x;
        active_index += 1;
    }

    let mut activate = |edge_index: usize| {
        let edge = edges[edge_index];
        if y < edge.min_y || y >= edge.max_y {
            return;
        }
        if scratch.active_positions[edge_index] != usize::MAX {
            return;
        }
        let active_index = scratch.active_edges.len();
        scratch.active_edges.push(ActiveScanlineEdge {
            edge_index,
            x: edge.inverse_slope * (y - edge.start.y) + edge.start.x,
            winding: edge.winding,
        });
        scratch.active_positions[edge_index] = active_index;
    };

    if let Some(candidates) = candidates {
        for &edge_index in candidates {
            activate(edge_index);
        }
    } else {
        for edge_index in 0..edges.len() {
            activate(edge_index);
        }
    }
}

fn sort_active_edges(scratch: &mut RasterScratch, full_sort: bool) {
    if full_sort {
        scratch
            .active_edges
            .sort_unstable_by(|first, second| first.x.total_cmp(&second.x));
        for (index, active) in scratch.active_edges.iter().enumerate() {
            scratch.active_positions[active.edge_index] = index;
        }
        return;
    }

    // The crossings are already in the previous sample's order. Insertion
    // sort repairs only the edges that crossed between samples and is much
    // cheaper than starting a general comparison sort for every scanline.
    for index in 1..scratch.active_edges.len() {
        let mut position = index;
        while position > 0
            && scratch.active_edges[position]
                .x
                .total_cmp(&scratch.active_edges[position - 1].x)
                .is_lt()
        {
            scratch.active_edges.swap(position, position - 1);
            let right = scratch.active_edges[position].edge_index;
            let left = scratch.active_edges[position - 1].edge_index;
            scratch.active_positions[right] = position;
            scratch.active_positions[left] = position - 1;
            position -= 1;
        }
    }
}

fn fill_active_scanline(
    row: usize,
    width: usize,
    sample_grid: u32,
    bitmap: &mut [u8],
    scratch: &RasterScratch,
    fill_rule: CoverageFillRule,
) -> bool {
    let mut has_coverage = false;
    let mut winding = 0_i32;
    let mut span_start = 0.0_f32;
    let row_offset = row * width;

    for intersection in &scratch.active_edges {
        let start = span_start.max(0.0);
        let end = intersection.x.min(width as f32);
        if start >= end {
            winding += i32::from(intersection.winding);
            span_start = intersection.x;
            continue;
        }
        let inside = match fill_rule {
            CoverageFillRule::NonZero => winding != 0,
            CoverageFillRule::EvenOdd => winding % 2 != 0,
        };
        if inside {
            let first_column = start.floor() as usize;
            let last_column = end.ceil().min(width as f32) as usize;
            for column in first_column..last_column {
                let column_start = column as f32;
                let covered_samples = if start <= column_start && end >= column_start + 1.0 {
                    // A non-zero-winding span contributes at most
                    // `sample_grid` samples for this Y sample. The spans on a
                    // scanline are disjoint, so the accumulator stays below
                    // u8::MAX.
                    sample_grid
                } else {
                    horizontal_coverage_samples(start, end, column_start, sample_grid)
                };
                if covered_samples != 0 {
                    bitmap[row_offset + column] += covered_samples as u8;
                    has_coverage = true;
                }
            }
        }
        winding += i32::from(intersection.winding);
        span_start = intersection.x;
    }

    has_coverage
}

fn horizontal_coverage_samples(
    start: f32,
    end: f32,
    column_start: f32,
    sample_grid: u32,
) -> u32 {
    let grid = sample_grid as f32;
    let first = (((start - column_start) * grid) - 0.5)
        .ceil()
        .clamp(0.0, grid) as u32;
    let last = (((end - column_start) * grid) - 0.5)
        .ceil()
        .clamp(0.0, grid) as u32;
    last.saturating_sub(first)
}

fn midpoint(first: Point, second: Point) -> Point {
    Point {
        x: (first.x + second.x) * 0.5,
        y: (first.y + second.y) * 0.5,
    }
}

fn same_point(first: Point, second: Point) -> bool {
    first.x == second.x && first.y == second.y
}

#[cfg(test)]
mod tests {
    use super::{
        CoverageFillRule, OutlinePath, PathCommand, Point, RasterScratch, SAMPLE_GRID,
        build_scanline_plan,
        build_coverage_bitmap, contours_to_edges, edge_rows, fill_scanline_plan, flatten_path_edges,
        flattened_path,
        rasterize_flattened_path, scan_convert_into_with_fill_rule, scan_convert_into_with_rows,
        scan_convert_reference,
        composite_color_layer,
    };
    use super::super::color::ColorRgba;

    #[test]
    fn scan_conversion_uses_nonzero_winding_fill_for_holes() {
        let contours = vec![
            vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 4.0, y: 0.0 },
                Point { x: 4.0, y: 4.0 },
                Point { x: 0.0, y: 4.0 },
                Point { x: 0.0, y: 0.0 },
            ],
            vec![
                Point { x: 1.0, y: 1.0 },
                Point { x: 1.0, y: 3.0 },
                Point { x: 3.0, y: 3.0 },
                Point { x: 3.0, y: 1.0 },
                Point { x: 1.0, y: 1.0 },
            ],
        ];

        let bitmap = scan_convert_reference(&contours, 4, 4, 16);

        assert_eq!(bitmap[0], 255, "the outer contour should remain filled");
        assert_eq!(bitmap[5], 0, "the inner contour should remain a hole");
    }

    #[test]
    fn scan_conversion_keeps_overlapping_contours_filled() {
        // Both contours have the same winding direction. This is how variable
        // and overlap-flagged TrueType glyphs join adjacent strokes: even-odd
        // filling would turn the overlap into a transparent dropout.
        let contours = vec![
            vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 4.0, y: 0.0 },
                Point { x: 4.0, y: 4.0 },
                Point { x: 0.0, y: 4.0 },
                Point { x: 0.0, y: 0.0 },
            ],
            vec![
                Point { x: 1.0, y: 1.0 },
                Point { x: 3.0, y: 1.0 },
                Point { x: 3.0, y: 3.0 },
                Point { x: 1.0, y: 3.0 },
                Point { x: 1.0, y: 1.0 },
            ],
        ];
        let edges = contours_to_edges(&contours);
        let row_edges = edge_rows(&edges, 4).expect("the test rows should build");
        let mut bitmap = vec![0; 16];

        scan_convert_into_with_rows(
            &edges,
            Some(&row_edges),
            4,
            4,
            SAMPLE_GRID,
            &mut bitmap,
            &mut RasterScratch::default(),
        );

        assert_eq!(bitmap[5], 255, "overlapping contours must not create a hole");
    }

    #[test]
    fn scan_conversion_supports_even_odd_svg_fills() {
        let contours = vec![
            vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 4.0, y: 0.0 },
                Point { x: 4.0, y: 4.0 },
                Point { x: 0.0, y: 4.0 },
                Point { x: 0.0, y: 0.0 },
            ],
            vec![
                Point { x: 1.0, y: 1.0 },
                Point { x: 3.0, y: 1.0 },
                Point { x: 3.0, y: 3.0 },
                Point { x: 1.0, y: 3.0 },
                Point { x: 1.0, y: 1.0 },
            ],
        ];
        let edges = contours_to_edges(&contours);
        let row_edges = edge_rows(&edges, 4).expect("the test rows should build");
        let mut bitmap = vec![0; 16];

        scan_convert_into_with_fill_rule(
            &edges,
            Some(&row_edges),
            4,
            4,
            SAMPLE_GRID,
            &mut bitmap,
            &mut RasterScratch::default(),
            CoverageFillRule::EvenOdd,
        );

        assert_eq!(bitmap[0], 255, "the outer contour should remain filled");
        assert_eq!(bitmap[5], 0, "even-odd SVG fills should preserve the hole");
    }

    #[test]
    fn default_scan_conversion_preserves_fine_edge_coverage() {
        assert!(
            SAMPLE_GRID >= 4,
            "the default converter needs at least 4x4 coverage samples"
        );

        let contours = vec![vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 4.0, y: 0.0 },
            Point { x: 0.0, y: 4.0 },
            Point { x: 0.0, y: 0.0 },
        ]];
        let edges = contours_to_edges(&contours);
        let row_edges = edge_rows(&edges, 4).expect("the test rows should build");
        let mut bitmap = vec![0; 16];

        scan_convert_into_with_rows(
            &edges,
            Some(&row_edges),
            4,
            4,
            SAMPLE_GRID,
            &mut bitmap,
            &mut RasterScratch::default(),
        );

        assert!(
            bitmap.iter().any(|coverage| {
                *coverage > 0
                    && *coverage < 255
                    && !matches!(*coverage, 63 | 64 | 127 | 128 | 191 | 192)
            }),
            "the diagonal edge should contain coverage finer than a 2x2 grid: {bitmap:?}"
        );
    }

    #[test]
    fn optimized_scan_conversion_reuses_scratch_and_matches_reference() {
        let contours = vec![vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 3.5, y: 0.0 },
            Point { x: 3.5, y: 3.5 },
            Point { x: 0.0, y: 3.5 },
            Point { x: 0.0, y: 0.0 },
        ]];
        let edges = contours_to_edges(&contours);
        let row_edges = edge_rows(&edges, 4).expect("the test rows should build");
        let expected = scan_convert_reference(&contours, 4, 4, 16);
        let mut actual = vec![0; 16];
        let mut scratch = RasterScratch::default();

        scan_convert_into_with_rows(
            &edges,
            Some(&row_edges),
            4,
            4,
            SAMPLE_GRID,
            &mut actual,
            &mut scratch,
        );
        let active_capacity = scratch.active_edges.capacity();
        let position_capacity = scratch.active_positions.capacity();
        scan_convert_into_with_rows(
            &edges,
            Some(&row_edges),
            4,
            4,
            SAMPLE_GRID,
            &mut actual,
            &mut scratch,
        );

        assert_eq!(actual, expected);
        assert!(active_capacity > 0);
        assert_eq!(scratch.active_edges.capacity(), active_capacity);
        assert_eq!(scratch.active_positions.capacity(), position_capacity);
    }

    #[test]
    fn scan_conversion_reports_whether_it_wrote_coverage() {
        let contours = vec![vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 2.0, y: 0.0 },
            Point { x: 0.0, y: 2.0 },
            Point { x: 0.0, y: 0.0 },
        ]];
        let edges = contours_to_edges(&contours);
        let row_edges = edge_rows(&edges, 2).expect("the test rows should build");
        let mut bitmap = vec![0; 4];

        let has_coverage = scan_convert_into_with_rows(
            &edges,
            Some(&row_edges),
            2,
            2,
            SAMPLE_GRID,
            &mut bitmap,
            &mut RasterScratch::default(),
        );

        assert!(has_coverage);
        assert!(bitmap.iter().any(|coverage| *coverage != 0));
    }

    #[test]
    fn color_layers_are_composited_in_source_over_order_as_straight_rgba() {
        let mut bitmap = vec![0_u8; 4];
        composite_color_layer(&mut bitmap, &[255], ColorRgba::new(255, 0, 0, 255));
        composite_color_layer(&mut bitmap, &[128], ColorRgba::new(0, 255, 0, 255));

        assert_eq!(bitmap, [127, 128, 0, 255]);
    }

    #[test]
    fn precomputed_scanline_plan_matches_reference_for_slanted_holes() {
        let contours = vec![
            vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: 5.0, y: 0.0 },
                Point { x: 4.0, y: 5.0 },
                Point { x: 0.0, y: 4.0 },
                Point { x: 0.0, y: 0.0 },
            ],
            vec![
                Point { x: 1.0, y: 1.0 },
                Point { x: 1.0, y: 3.0 },
                Point { x: 3.0, y: 3.0 },
                Point { x: 3.0, y: 1.0 },
                Point { x: 1.0, y: 1.0 },
            ],
        ];
        let edges = contours_to_edges(&contours);
        let row_edges = edge_rows(&edges, 5).expect("the test rows should build");
        let plan = build_scanline_plan(&edges, &row_edges, 5, 5, SAMPLE_GRID)
            .expect("the bounded test plan should build");
        let mut actual = vec![0; 25];

        assert!(fill_scanline_plan(&plan, 5, 5, &mut actual));
        assert_eq!(actual, scan_convert_reference(&contours, 5, 5, 25));
    }

    #[test]
    fn rasterized_coverage_is_cached_and_replayed_exactly() {
        let path = OutlinePath {
            bounds: [0.0, 0.0, 4.0, 4.0],
            commands: vec![
                PathCommand::MoveTo(Point { x: 0.0, y: 0.0 }),
                PathCommand::LineTo(Point { x: 4.0, y: 0.0 }),
                PathCommand::LineTo(Point { x: 4.0, y: 4.0 }),
                PathCommand::LineTo(Point { x: 0.0, y: 4.0 }),
                PathCommand::Close,
            ],
        };
        let flattened =
            flattened_path(&path, 1.0, 0, 0, 0.0).expect("the test path should flatten");
        let mut scratch = RasterScratch::default();

        let first = rasterize_flattened_path(&flattened, 4.0, &mut scratch)
            .expect("the first rasterization should succeed");
        let cached = flattened
            .coverage
            .get()
            .expect("the first rasterization should publish coverage");
        assert_eq!(cached.as_ref(), first.bitmap.as_slice());

        let second = rasterize_flattened_path(&flattened, 4.0, &mut scratch)
            .expect("the cached rasterization should succeed");
        assert_eq!(second.bitmap, first.bitmap);
        assert_eq!(second.width, first.width);
        assert_eq!(second.height, first.height);
    }

    #[test]
    fn active_edge_coverage_matches_reference_for_a_slanted_outline() {
        let contours = vec![vec![
            Point { x: 0.0, y: 0.0 },
            Point { x: 5.0, y: 0.0 },
            Point { x: 4.0, y: 5.0 },
            Point { x: 0.0, y: 4.0 },
            Point { x: 0.0, y: 0.0 },
        ]];
        let path = OutlinePath {
            bounds: [0.0, 0.0, 5.0, 5.0],
            commands: vec![
                PathCommand::MoveTo(Point { x: 0.0, y: 0.0 }),
                PathCommand::LineTo(Point { x: 5.0, y: 0.0 }),
                PathCommand::LineTo(Point { x: 4.0, y: 5.0 }),
                PathCommand::LineTo(Point { x: 0.0, y: 4.0 }),
                PathCommand::Close,
            ],
        };
        let flattened =
            flattened_path(&path, 1.0, 0, 0, 0.0).expect("the test path should flatten");
        let actual = build_coverage_bitmap(&flattened, &mut RasterScratch::default())
            .expect("the active-edge bitmap should build");

        assert_eq!(actual, scan_convert_reference(&contours, 5, 5, 25));
    }

    #[test]
    fn direct_flattening_emits_the_expected_closed_edges() {
        let path = OutlinePath {
            bounds: [0.0, 0.0, 4.0, 4.0],
            commands: vec![
                PathCommand::MoveTo(Point { x: 0.0, y: 0.0 }),
                PathCommand::LineTo(Point { x: 4.0, y: 0.0 }),
                PathCommand::LineTo(Point { x: 4.0, y: 4.0 }),
                PathCommand::LineTo(Point { x: 0.0, y: 4.0 }),
                PathCommand::Close,
            ],
        };

        let edges = flatten_path_edges(&path, 1.0, 0.0, 0.0, 0.0, 0, 0)
            .expect("the test path should flatten directly to edges");
        let points = edges
            .iter()
            .map(|edge| [edge.start.x, edge.start.y, edge.end.x, edge.end.y])
            .collect::<Vec<_>>();

        assert_eq!(points, vec![
            [0.0, 0.0, 4.0, 0.0],
            [4.0, 0.0, 4.0, 4.0],
            [4.0, 4.0, 0.0, 4.0],
            [0.0, 4.0, 0.0, 0.0],
        ]);
    }
}
