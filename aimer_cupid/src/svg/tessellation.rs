use std::collections::HashMap;
use std::sync::Arc;

use lyon::math::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, LineCap, LineJoin,
    StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};

use super::{SvgFillRule, SvgGeometry, SvgLineCap, SvgLineJoin, SvgPathCommand};

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
    Stroke { width: f32, line_cap: SvgLineCap, line_join: SvgLineJoin, miter_limit: f32 },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SvgToleranceBucket(u8);

impl SvgToleranceBucket {
    pub const COUNT: usize = 8;

    pub fn from_scale(scale: f32) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        let exponent = scale.log2()
                            .round()
                            .clamp(-3.0, 4.0) as i32;
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
    Stroke { width: u32, line_cap: SvgLineCap, line_join: SvgLineJoin, miter_limit: u32 },
}

impl From<SvgMeshStyle> for MeshStyleKey {
    fn from(style: SvgMeshStyle) -> Self {
        match style {
            SvgMeshStyle::Fill(rule) => Self::Fill(rule),
            SvgMeshStyle::Stroke { width, line_cap, line_join, miter_limit } => {
                Self::Stroke { width: width.to_bits(),
                               line_cap,
                               line_join,
                               miter_limit: miter_limit.to_bits() }
            }
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
        Self { entries: HashMap::new(),
               max_memory_bytes,
               max_entries,
               memory_bytes: 0,
               usage_clock: 0 }
    }

    pub fn mesh_for(&mut self,
                    geometry: &SvgGeometry,
                    style: SvgMeshStyle,
                    physical_scale: f32)
                    -> Result<Arc<SvgMesh>, SvgTessellationError> {
        self.usage_clock = self.usage_clock
                               .wrapping_add(1);
        let tolerance = SvgToleranceBucket::from_scale(physical_scale);
        let key = GeometryKey { path: path_key(geometry), style: style.into(), tolerance };
        if let Some(entry) = self.entries
                                 .get_mut(&key)
        {
            entry.last_used = self.usage_clock;
            return Ok(entry.mesh.clone());
        }

        let mesh = Arc::new(tessellate(geometry, style, tolerance.tolerance())?);
        let mesh_bytes = mesh.memory_bytes();
        if self.max_entries > 0 && mesh_bytes <= self.max_memory_bytes {
            self.memory_bytes += mesh_bytes;
            self.entries
                .insert(key, CacheEntry { mesh: mesh.clone(), last_used: self.usage_clock });
            self.evict_to_limits();
        }
        Ok(mesh)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries
            .is_empty()
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
            let Some(oldest_key) = self.entries
                                       .iter()
                                       .min_by_key(|(_, entry)| entry.last_used)
                                       .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries
                                     .remove(&oldest_key)
            {
                self.memory_bytes = self.memory_bytes
                                        .saturating_sub(entry.mesh
                                                             .memory_bytes());
            }
        }
    }
}

fn tessellate(geometry: &SvgGeometry,
              style: SvgMeshStyle,
              tolerance: f32)
              -> Result<SvgMesh, SvgTessellationError> {
    let path = lyon_path(geometry)?;
    let mut output: VertexBuffers<lyon::math::Point, u32> = VertexBuffers::new();
    match style {
        SvgMeshStyle::Fill(rule) => {
            let options =
                FillOptions::default().with_tolerance(tolerance)
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
        SvgMeshStyle::Stroke { width, line_cap, line_join, miter_limit } => {
            if !width.is_finite() || width <= 0.0 || !miter_limit.is_finite() {
                return Err(SvgTessellationError::Tessellation(
                    "invalid stroke parameters".to_owned(),
                ));
            }
            let options =
                StrokeOptions::default().with_tolerance(tolerance)
                                        .with_line_width(width)
                                        .with_line_cap(match line_cap {
                                                           SvgLineCap::Butt => LineCap::Butt,
                                                           SvgLineCap::Round => LineCap::Round,
                                                           SvgLineCap::Square => LineCap::Square,
                                                       })
                                        .with_line_join(match line_join {
                                                            SvgLineJoin::Miter => LineJoin::Miter,
                                                            SvgLineJoin::MiterClip => {
                                                                LineJoin::MiterClip
                                                            }
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
    Ok(SvgMesh { vertices: output.vertices
                                 .into_iter()
                                 .map(|point| [point.x, point.y])
                                 .collect::<Vec<_>>()
                                 .into(),
                 indices: output.indices
                                .into() })
}

fn lyon_path(geometry: &SvgGeometry) -> Result<Path, SvgTessellationError> {
    if geometry.commands
               .is_empty()
    {
        return Err(SvgTessellationError::EmptyPath);
    }
    let mut builder = Path::builder();
    let mut contour_open = false;
    for command in geometry.commands
                           .iter()
                           .copied()
    {
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
            SvgPathCommand::QuadraticTo { control_x, control_y, x, y } => {
                builder.quadratic_bezier_to(point(control_x, control_y), point(x, y));
            }
            SvgPathCommand::CubicTo { control1_x, control1_y, control2_x, control2_y, x, y } => {
                builder.cubic_bezier_to(point(control1_x, control1_y),
                                        point(control2_x, control2_y),
                                        point(x, y));
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
    let mut key = Vec::with_capacity(geometry.commands
                                             .len()
                                     * 7);
    for command in geometry.commands
                           .iter()
    {
        match *command {
            SvgPathCommand::MoveTo { x, y } => key.extend([0, x.to_bits(), y.to_bits()]),
            SvgPathCommand::LineTo { x, y } => key.extend([1, x.to_bits(), y.to_bits()]),
            SvgPathCommand::QuadraticTo { control_x, control_y, x, y } => {
                key.extend([2, control_x.to_bits(), control_y.to_bits(), x.to_bits(), y.to_bits()]);
            }
            SvgPathCommand::CubicTo { control1_x, control1_y, control2_x, control2_y, x, y } => {
                key.extend([3,
                            control1_x.to_bits(),
                            control1_y.to_bits(),
                            control2_x.to_bits(),
                            control2_y.to_bits(),
                            x.to_bits(),
                            y.to_bits()])
            }
            SvgPathCommand::Close => key.push(4),
        }
    }
    key
}
