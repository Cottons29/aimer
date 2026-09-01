use std::collections::HashMap;
use std::sync::Arc;

use lyon::math::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};

use super::{SvgFillRule, SvgGeometry, SvgLineCap, SvgLineJoin, SvgPathCommand, SvgStroke};

#[derive(Debug, thiserror::Error)]
pub enum SvgTessellationError {
    #[error("SVG path is empty")]
    EmptyPath,
    #[error("SVG tessellation failed: {0}")]
    Tessellation(String),
}

#[derive(Clone, Debug)]
pub struct SvgMesh {
    pub vertices: Arc<[[f32; 2]]>,
    pub indices: Arc<[u32]>,
}

impl SvgMesh {
    pub fn memory_bytes(&self) -> usize {
        self.vertices.len() * size_of::<[f32; 2]>() + self.indices.len() * size_of::<u32>()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SvgMeshStyle {
    Fill(SvgFillRule),
    Stroke {
        width: f32,
        line_cap: SvgLineCap,
        line_join: SvgLineJoin,
        miter_limit: f32,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SvgToleranceBucket(u8);

impl SvgToleranceBucket {
    pub const COUNT: usize = 8;

    pub fn from_scale(scale: f32) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let exponent = scale.log2().round().clamp(-3.0, 4.0) as i32;
        Self((exponent + 3) as u8)
    }

    fn tolerance(self) -> f32 {
        let representative_scale = 2.0_f32.powi(self.0 as i32 - 3);
        0.25 / representative_scale
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct GeometryKey {
    path: Vec<u32>,
    style: MeshStyleKey,
    tolerance: SvgToleranceBucket,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum MeshStyleKey {
    Fill(SvgFillRule),
    Stroke {
        width: u32,
        line_cap: SvgLineCap,
        line_join: SvgLineJoin,
        miter_limit: u32,
    },
}

impl From<SvgMeshStyle> for MeshStyleKey {
    fn from(style: SvgMeshStyle) -> Self {
        match style {
            SvgMeshStyle::Fill(rule) => Self::Fill(rule),
            SvgMeshStyle::Stroke {
                width,
                line_cap,
                line_join,
                miter_limit,
            } => Self::Stroke {
                width: width.to_bits(),
                line_cap,
                line_join,
                miter_limit: miter_limit.to_bits(),
            },
        }
    }
}

struct CacheEntry {
    mesh: Arc<SvgMesh>,
    last_used: u64,
}

pub struct SvgGeometryCache {
    entries: HashMap<GeometryKey, CacheEntry>,
    max_memory_bytes: usize,
    max_entries: usize,
    memory_bytes: usize,
    usage_clock: u64,
}

impl SvgGeometryCache {
    pub fn new(max_memory_bytes: usize, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_memory_bytes,
            max_entries,
            memory_bytes: 0,
            usage_clock: 0,
        }
    }

    pub fn mesh_for(
        &mut self,
        geometry: &SvgGeometry,
        style: SvgMeshStyle,
        physical_scale: f32,
    ) -> Result<Arc<SvgMesh>, SvgTessellationError> {
        self.usage_clock = self.usage_clock.wrapping_add(1);
        let tolerance = SvgToleranceBucket::from_scale(physical_scale);
        let key = GeometryKey {
            path: path_key(geometry),
            style: style.into(),
            tolerance,
        };
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = self.usage_clock;
            return Ok(entry.mesh.clone());
        }

        let mesh = Arc::new(tessellate(geometry, style, tolerance.tolerance())?);
        let mesh_bytes = mesh.memory_bytes();
        if self.max_entries > 0 && mesh_bytes <= self.max_memory_bytes {
            self.memory_bytes += mesh_bytes;
            self.entries.insert(
                key,
                CacheEntry {
                    mesh: mesh.clone(),
                    last_used: self.usage_clock,
                },
            );
            self.evict_to_limits();
        }
        Ok(mesh)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn memory_bytes(&self) -> usize {
        self.memory_bytes
    }

    pub fn max_memory_bytes(&self) -> usize {
        self.max_memory_bytes
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.memory_bytes = 0;
    }

    fn evict_to_limits(&mut self) {
        while self.entries.len() > self.max_entries || self.memory_bytes > self.max_memory_bytes {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest_key) {
                self.memory_bytes = self.memory_bytes.saturating_sub(entry.mesh.memory_bytes());
            }
        }
    }
}

fn tessellate(
    geometry: &SvgGeometry,
    style: SvgMeshStyle,
    tolerance: f32,
) -> Result<SvgMesh, SvgTessellationError> {
    let path = lyon_path(geometry)?;
    let mut output: VertexBuffers<lyon::math::Point, u32> = VertexBuffers::new();
    match style {
        SvgMeshStyle::Fill(rule) => {
            let options = FillOptions::default()
                .with_tolerance(tolerance)
                .with_fill_rule(match rule {
                    SvgFillRule::NonZero => FillRule::NonZero,
                    SvgFillRule::EvenOdd => FillRule::EvenOdd,
                });
            FillTessellator::new()
                .tessellate_path(
                    &path,
                    &options,
                    &mut BuffersBuilder::new(&mut output, |vertex: FillVertex| vertex.position()),
                )
                .map_err(|error| SvgTessellationError::Tessellation(error.to_string()))?;
        }
        SvgMeshStyle::Stroke {
            width,
            line_cap,
            line_join,
            miter_limit,
        } => {
            if !width.is_finite()
                || width <= 0.0
                || !miter_limit.is_finite()
                || miter_limit < StrokeOptions::MINIMUM_MITER_LIMIT
            {
                return Err(SvgTessellationError::Tessellation(
                    "invalid stroke parameters".to_owned(),
                ));
            }
            let options = StrokeOptions::default()
                .with_tolerance(tolerance)
                .with_line_width(width)
                .with_line_cap(match line_cap {
                    SvgLineCap::Butt => LineCap::Butt,
                    SvgLineCap::Round => LineCap::Round,
                    SvgLineCap::Square => LineCap::Square,
                })
                .with_line_join(match line_join {
                    SvgLineJoin::Miter => LineJoin::Miter,
                    SvgLineJoin::MiterClip => LineJoin::MiterClip,
                    SvgLineJoin::Round => LineJoin::Round,
                    SvgLineJoin::Bevel => LineJoin::Bevel,
                })
                .with_miter_limit(miter_limit);
            StrokeTessellator::new()
                .tessellate_path(
                    &path,
                    &options,
                    &mut BuffersBuilder::new(&mut output, |vertex: StrokeVertex| vertex.position()),
                )
                .map_err(|error| SvgTessellationError::Tessellation(error.to_string()))?;
        }
    }
    Ok(SvgMesh {
        vertices: output
            .vertices
            .into_iter()
            .map(|point| [point.x, point.y])
            .collect::<Vec<_>>()
            .into(),
        indices: output.indices.into(),
    })
}

/// Tessellates a stroke with the SVG dash pattern retained by [`SvgStroke`].
///
/// Lyon does not expose a dash option, so the path is flattened into bounded
/// line pieces before the normal stroke tessellator is used. Each visible dash
/// is an independent subpath, which preserves the requested cap style and
/// makes gaps real geometry gaps rather than a color approximation. The GPU
/// pipeline may still choose not to submit this helper until its instance
/// format carries dash-aware paint data.
pub fn tessellate_dashed_stroke(
    geometry: &SvgGeometry,
    stroke: &SvgStroke,
    physical_scale: f32,
) -> Result<SvgMesh, SvgTessellationError> {
    if stroke.dash_array.is_empty() {
        return tessellate(
            geometry,
            SvgMeshStyle::Stroke {
                width: stroke.width,
                line_cap: stroke.line_cap,
                line_join: stroke.line_join,
                miter_limit: stroke.miter_limit,
            },
            SvgToleranceBucket::from_scale(physical_scale).tolerance(),
        );
    }
    if !stroke.dash_offset.is_finite()
        || !stroke.width.is_finite()
        || stroke.width <= 0.0
        || !stroke.miter_limit.is_finite()
        || stroke.miter_limit < StrokeOptions::MINIMUM_MITER_LIMIT
        || stroke
            .dash_array
            .iter()
            .any(|dash| !dash.is_finite() || *dash < 0.0)
    {
        return Err(SvgTessellationError::Tessellation(
            "invalid dashed stroke parameters".to_owned(),
        ));
    }

    let pattern_sum = stroke.dash_array.iter().copied().sum::<f32>();
    if !pattern_sum.is_finite() {
        return Err(SvgTessellationError::Tessellation(
            "dashed stroke pattern is not finite".to_owned(),
        ));
    }
    if pattern_sum <= f32::EPSILON {
        return Ok(empty_mesh());
    }

    let mut pattern = stroke.dash_array.to_vec();
    if pattern.len() % 2 == 1 {
        pattern.extend_from_within(..);
    }
    let commands = dashed_commands(geometry, &pattern, stroke.dash_offset);
    if commands.is_empty() {
        return Ok(empty_mesh());
    }
    tessellate(
        &SvgGeometry {
            commands: commands.into(),
        },
        SvgMeshStyle::Stroke {
            width: stroke.width,
            line_cap: stroke.line_cap,
            line_join: stroke.line_join,
            miter_limit: stroke.miter_limit,
        },
        SvgToleranceBucket::from_scale(physical_scale).tolerance(),
    )
}

fn empty_mesh() -> SvgMesh {
    SvgMesh {
        vertices: Arc::from([]),
        indices: Arc::from([]),
    }
}

fn dashed_commands(
    geometry: &SvgGeometry,
    pattern: &[f32],
    dash_offset: f32,
) -> Vec<SvgPathCommand> {
    let Some(period) = pattern.iter().copied().reduce(|sum, value| sum + value) else {
        return Vec::new();
    };
    if !period.is_finite() || period <= f32::EPSILON {
        return Vec::new();
    }

    let mut commands = Vec::new();
    for contour in flatten_for_dashing(geometry) {
        if contour.len() < 2 {
            continue;
        }
        let (mut pattern_index, mut remaining, mut on) =
            dash_cursor(pattern, dash_offset.rem_euclid(period));
        for segment in contour.windows(2) {
            let start = segment[0];
            let end = segment[1];
            let delta = (end.0 - start.0, end.1 - start.1);
            let length = delta.0.hypot(delta.1);
            if !length.is_finite() || length <= f32::EPSILON {
                continue;
            }
            let mut distance = 0.0;
            while distance < length {
                while remaining <= f32::EPSILON {
                    advance_dash_cursor(pattern, &mut pattern_index, &mut remaining, &mut on);
                }
                let step = remaining.min(length - distance);
                if on && step > f32::EPSILON {
                    let start_t = distance / length;
                    let end_t = (distance + step) / length;
                    commands.push(SvgPathCommand::MoveTo {
                        x: start.0 + delta.0 * start_t,
                        y: start.1 + delta.1 * start_t,
                    });
                    commands.push(SvgPathCommand::LineTo {
                        x: start.0 + delta.0 * end_t,
                        y: start.1 + delta.1 * end_t,
                    });
                }
                distance += step;
                remaining -= step;
                if remaining <= f32::EPSILON {
                    advance_dash_cursor(pattern, &mut pattern_index, &mut remaining, &mut on);
                }
            }
        }
    }
    commands
}

fn dash_cursor(pattern: &[f32], mut phase: f32) -> (usize, f32, bool) {
    let mut index = 0;
    while pattern[index] <= f32::EPSILON || phase >= pattern[index] {
        if pattern[index] > f32::EPSILON {
            phase -= pattern[index];
        }
        index = (index + 1) % pattern.len();
        if phase <= f32::EPSILON {
            phase = 0.0;
            break;
        }
    }
    (index, (pattern[index] - phase).max(0.0), index % 2 == 0)
}

fn advance_dash_cursor(
    pattern: &[f32],
    index: &mut usize,
    remaining: &mut f32,
    on: &mut bool,
) {
    for _ in 0..pattern.len() {
        *index = (*index + 1) % pattern.len();
        *remaining = pattern[*index];
        *on = *index % 2 == 0;
        if *remaining > f32::EPSILON {
            return;
        }
    }
    *remaining = f32::MAX;
    *on = false;
}

fn flatten_for_dashing(geometry: &SvgGeometry) -> Vec<Vec<(f32, f32)>> {
    let mut contours = Vec::new();
    let mut contour = Vec::new();
    let mut current = (0.0, 0.0);
    for command in geometry.commands.iter().copied() {
        match command {
            SvgPathCommand::MoveTo { x, y } => {
                if contour.len() >= 2 {
                    contours.push(std::mem::take(&mut contour));
                } else {
                    contour.clear();
                }
                current = (x, y);
                contour.push(current);
            }
            SvgPathCommand::LineTo { x, y } => {
                if contour.is_empty() {
                    contour.push(current);
                }
                current = (x, y);
                contour.push(current);
            }
            SvgPathCommand::QuadraticTo {
                control_x,
                control_y,
                x,
                y,
            } => {
                if contour.is_empty() {
                    contour.push(current);
                }
                let start = current;
                for step in 1..=16 {
                    let t = step as f32 / 16.0;
                    let inverse = 1.0 - t;
                    contour.push((
                        inverse * inverse * start.0 + 2.0 * inverse * t * control_x + t * t * x,
                        inverse * inverse * start.1 + 2.0 * inverse * t * control_y + t * t * y,
                    ));
                }
                current = (x, y);
            }
            SvgPathCommand::CubicTo {
                control1_x,
                control1_y,
                control2_x,
                control2_y,
                x,
                y,
            } => {
                if contour.is_empty() {
                    contour.push(current);
                }
                let start = current;
                for step in 1..=24 {
                    let t = step as f32 / 24.0;
                    let inverse = 1.0 - t;
                    contour.push((
                        inverse.powi(3) * start.0
                            + 3.0 * inverse * inverse * t * control1_x
                            + 3.0 * inverse * t * t * control2_x
                            + t.powi(3) * x,
                        inverse.powi(3) * start.1
                            + 3.0 * inverse * inverse * t * control1_y
                            + 3.0 * inverse * t * t * control2_y
                            + t.powi(3) * y,
                    ));
                }
                current = (x, y);
            }
            SvgPathCommand::Close => {
                if let Some(first) = contour.first().copied()
                    && Some(first) != contour.last().copied()
                {
                    contour.push(first);
                }
                current = contour.first().copied().unwrap_or(current);
            }
        }
    }
    if contour.len() >= 2 {
        contours.push(contour);
    }
    contours
}

fn lyon_path(geometry: &SvgGeometry) -> Result<Path, SvgTessellationError> {
    if geometry.commands.is_empty() {
        return Err(SvgTessellationError::EmptyPath);
    }
    let mut builder = Path::builder();
    let mut contour_open = false;
    for command in geometry.commands.iter().copied() {
        match command {
            SvgPathCommand::MoveTo { x, y } => {
                if contour_open {
                    builder.end(false);
                }
                builder.begin(point(x, y));
                contour_open = true;
            }
            SvgPathCommand::LineTo { x, y } => {
                builder.line_to(point(x, y));
            }
            SvgPathCommand::QuadraticTo {
                control_x,
                control_y,
                x,
                y,
            } => {
                builder.quadratic_bezier_to(point(control_x, control_y), point(x, y));
            }
            SvgPathCommand::CubicTo {
                control1_x,
                control1_y,
                control2_x,
                control2_y,
                x,
                y,
            } => {
                builder.cubic_bezier_to(
                    point(control1_x, control1_y),
                    point(control2_x, control2_y),
                    point(x, y),
                );
            }
            SvgPathCommand::Close => {
                builder.close();
                contour_open = false;
            }
        }
    }
    if contour_open {
        builder.end(false);
    }
    Ok(builder.build())
}

fn path_key(geometry: &SvgGeometry) -> Vec<u32> {
    let mut key = Vec::with_capacity(geometry.commands.len() * 7);
    for command in geometry.commands.iter() {
        match *command {
            SvgPathCommand::MoveTo { x, y } => key.extend([0, x.to_bits(), y.to_bits()]),
            SvgPathCommand::LineTo { x, y } => key.extend([1, x.to_bits(), y.to_bits()]),
            SvgPathCommand::QuadraticTo {
                control_x,
                control_y,
                x,
                y,
            } => {
                key.extend([
                    2,
                    control_x.to_bits(),
                    control_y.to_bits(),
                    x.to_bits(),
                    y.to_bits(),
                ]);
            }
            SvgPathCommand::CubicTo {
                control1_x,
                control1_y,
                control2_x,
                control2_y,
                x,
                y,
            } => key.extend([
                3,
                control1_x.to_bits(),
                control1_y.to_bits(),
                control2_x.to_bits(),
                control2_y.to_bits(),
                x.to_bits(),
                y.to_bits(),
            ]),
            SvgPathCommand::Close => key.push(4),
        }
    }
    key
}
