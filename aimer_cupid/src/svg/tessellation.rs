use std::collections::HashMap;
use std::sync::Arc;

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
    /// Linear edge coverage for each vertex. Values are one inside the shape
    /// and fade to zero across analytic antialiasing fringe geometry.
    pub coverages: Arc<[f32]>,
    pub indices: Arc<[u32]>,
}

impl SvgMesh {
    pub fn memory_bytes(&self) -> usize {
        self.vertices.len() * (size_of::<[f32; 2]>() + size_of::<f32>())
            + self.indices.len() * size_of::<u32>()
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
    analytic_aa: bool,
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
        self.mesh_for_with_analytic_aa(geometry, style, physical_scale, false)
    }

    pub(crate) fn mesh_for_with_analytic_aa(
        &mut self,
        geometry: &SvgGeometry,
        style: SvgMeshStyle,
        physical_scale: f32,
        analytic_aa: bool,
    ) -> Result<Arc<SvgMesh>, SvgTessellationError> {
        self.usage_clock = self.usage_clock.wrapping_add(1);
        let tolerance = SvgToleranceBucket::from_scale(physical_scale);
        let key = GeometryKey {
            path: path_key(geometry),
            style: style.into(),
            tolerance,
            analytic_aa,
        };
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = self.usage_clock;
            return Ok(entry.mesh.clone());
        }

        let mesh = Arc::new(tessellate(
            geometry,
            style,
            tolerance.tolerance(),
            analytic_aa,
        )?);
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
    analytic_aa: bool,
) -> Result<SvgMesh, SvgTessellationError> {
    if geometry.commands.is_empty() {
        return Err(SvgTessellationError::EmptyPath);
    }
    let fringe_width = tolerance * 4.0;
    match style {
        SvgMeshStyle::Fill(rule) => {
            let contours = close_fill_contours(flatten_for_dashing(geometry));
            let mut mesh = scanline_fill(&contours, rule)?;
            if analytic_aa && fringe_width.is_finite() && fringe_width > 0.0 {
                add_fill_fringe(&mut mesh, &contours, rule, fringe_width);
            }
            Ok(mesh)
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
                || miter_limit < 1.0
            {
                return Err(SvgTessellationError::Tessellation(
                    "invalid stroke parameters".to_owned(),
                ));
            }
            let mut mesh = stroke_mesh(
                &flatten_for_dashing(geometry), width, line_cap, line_join, miter_limit,
            );
            if analytic_aa && fringe_width.is_finite() && fringe_width > 0.0 {
                add_mesh_fringe(&mut mesh, fringe_width);
            }
            Ok(mesh)
        }
    }
}

#[derive(Clone, Copy)]
struct Crossing {
    left: (f32, f32),
    right: (f32, f32),
    x_mid: f32,
    winding: i32,
}

#[derive(Clone, Copy)]
struct ScanEdge {
    a: (f32, f32),
    b: (f32, f32),
    min_y: f32,
    max_y: f32,
    winding: i32,
}

fn scanline_fill(contours: &[Vec<(f32, f32)>], rule: SvgFillRule) -> Result<SvgMesh, SvgTessellationError> {
    let mut levels = Vec::new();
    let mut edges = Vec::new();
    for contour in contours {
        for &(x, y) in contour {
            if !x.is_finite() || !y.is_finite() {
                return Err(SvgTessellationError::Tessellation("non-finite path coordinate".into()));
            }
            levels.push(y);
        }
        for segment in contour.windows(2) {
            let (a, b) = (segment[0], segment[1]);
            if a.1 != b.1 {
                edges.push(ScanEdge {
                    a,
                    b,
                    min_y: a.1.min(b.1),
                    max_y: a.1.max(b.1),
                    winding: if b.1 > a.1 { 1 } else { -1 },
                });
            }
        }
    }
    levels.sort_by(f32::total_cmp);
    levels.dedup_by(|a, b| a.to_bits() == b.to_bits());
    edges.sort_unstable_by(|a, b| a.min_y.total_cmp(&b.min_y));
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut active_edges = Vec::new();
    let mut next_edge = 0;
    for band in levels.windows(2) {
        let (y0, y1) = (band[0], band[1]);
        if y1 - y0 <= f32::EPSILON { continue; }
        let mid = y0 * 0.5 + y1 * 0.5;
        while next_edge < edges.len() && edges[next_edge].min_y <= mid {
            active_edges.push(next_edge);
            next_edge += 1;
        }
        active_edges.retain(|&index| edges[index].max_y > mid);
        let mut crossings = Vec::with_capacity(active_edges.len());
        for &index in &active_edges {
            let edge = edges[index];
            let t = (mid - edge.a.1) / (edge.b.1 - edge.a.1);
            let x_mid = edge.a.0 + (edge.b.0 - edge.a.0) * t;
            let at = |y: f32| {
                let x = if y == edge.a.1 {
                    edge.a.0
                } else if y == edge.b.1 {
                    edge.b.0
                } else {
                    edge.a.0 + (edge.b.0 - edge.a.0) * ((y - edge.a.1) / (edge.b.1 - edge.a.1))
                };
                (x, y)
            };
            crossings.push(Crossing {
                left: at(y0),
                right: at(y1),
                x_mid,
                winding: edge.winding,
            });
        }
        crossings.sort_by(|a,b| a.x_mid.total_cmp(&b.x_mid));
        let mut start = None;
        let mut winding = 0i32;
        let mut parity = false;
        for crossing in crossings {
            let was_inside = match rule { SvgFillRule::NonZero => winding != 0, SvgFillRule::EvenOdd => parity };
            winding += crossing.winding;
            parity = !parity;
            let is_inside = match rule { SvgFillRule::NonZero => winding != 0, SvgFillRule::EvenOdd => parity };
            if !was_inside && is_inside { start = Some(crossing); }
            if was_inside && !is_inside {
                if let Some(left) = start.take() { append_quad(&mut vertices, &mut indices, left.left, crossing.left, crossing.right, left.right); }
            }
        }
    }
    let coverages = vec![1.0; vertices.len()];
    Ok(SvgMesh { vertices: vertices.into(), coverages: coverages.into(), indices: indices.into() })
}

fn close_fill_contours(mut contours: Vec<Vec<(f32, f32)>>) -> Vec<Vec<(f32, f32)>> {
    contours.retain(|contour| contour.len() >= 3);
    for contour in &mut contours {
        if contour.first() != contour.last() {
            contour.push(contour[0]);
        }
    }
    contours
}

fn add_fill_fringe(
    mesh: &mut SvgMesh,
    contours: &[Vec<(f32, f32)>],
    rule: SvgFillRule,
    width: f32,
) {
    let mut vertices = mesh.vertices.to_vec();
    let mut coverages = mesh.coverages.to_vec();
    let mut indices = mesh.indices.to_vec();
    let mut corners: HashMap<PointKey, ([f32; 2], Vec<[f32; 2]>)> = HashMap::new();
    let fill_query = FillQueryIndex::new(contours);

    for contour in contours {
        for segment in contour.windows(2) {
            let a = [segment[0].0, segment[0].1];
            let b = [segment[1].0, segment[1].1];
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            let length = dx.hypot(dy);
            if !length.is_finite() || length <= f32::EPSILON {
                continue;
            }
            let probe = (width * 0.5).min(length * 0.1).max(f32::EPSILON * 8.0);
            let normal = [-dy / length, dx / length];
            let midpoint = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
            let left = [midpoint[0] + normal[0] * probe, midpoint[1] + normal[1] * probe];
            let right = [midpoint[0] - normal[0] * probe, midpoint[1] - normal[1] * probe];
            let left_filled = fill_query.contains(left, rule);
            let right_filled = fill_query.contains(right, rule);
            if left_filled == right_filled {
                continue;
            }
            let outward = if left_filled { [-normal[0], -normal[1]] } else { normal };
            let outer_a = [a[0] + outward[0] * width, a[1] + outward[1] * width];
            let outer_b = [b[0] + outward[0] * width, b[1] + outward[1] * width];
            append_coverage_quad(
                &mut vertices,
                &mut coverages,
                &mut indices,
                a,
                b,
                outer_b,
                outer_a,
            );
            corners.entry(point_key(a)).or_insert_with(|| (a, Vec::new())).1.push(outer_a);
            corners.entry(point_key(b)).or_insert_with(|| (b, Vec::new())).1.push(outer_b);
        }
    }

    append_fringe_corner_fans(&mut vertices, &mut coverages, &mut indices, corners);
    mesh.vertices = vertices.into();
    mesh.coverages = coverages.into();
    mesh.indices = indices.into();
}

#[derive(Clone, Copy)]
struct FillEdge {
    a: (f32, f32),
    b: (f32, f32),
    min_y: f32,
    max_y: f32,
}

struct FillIntervalNode {
    center: f32,
    by_min: Vec<usize>,
    by_max: Vec<usize>,
    left: Option<Box<Self>>,
    right: Option<Box<Self>>,
}

struct FillQueryIndex {
    edges: Vec<FillEdge>,
    root: Option<Box<FillIntervalNode>>,
}

impl FillQueryIndex {
    fn new(contours: &[Vec<(f32, f32)>]) -> Self {
        let edges = contours
            .iter()
            .flat_map(|contour| contour.windows(2))
            .filter_map(|segment| {
                let (a, b) = (segment[0], segment[1]);
                (a.1 != b.1).then_some(FillEdge {
                    a,
                    b,
                    min_y: a.1.min(b.1),
                    max_y: a.1.max(b.1),
                })
            })
            .collect::<Vec<_>>();
        let indices = (0..edges.len()).collect::<Vec<_>>();
        let root = build_fill_interval_node(&edges, indices);
        Self { edges, root }
    }

    fn contains(&self, point: [f32; 2], rule: SvgFillRule) -> bool {
        let mut winding = 0_i32;
        let mut crossings = 0_u32;
        let mut node = self.root.as_deref();
        while let Some(current) = node {
            if point[1] < current.center {
                for &index in &current.by_min {
                    let edge = self.edges[index];
                    if edge.min_y > point[1] { break; }
                    add_fill_crossing(edge, point, &mut winding, &mut crossings);
                }
                node = current.left.as_deref();
            } else if point[1] > current.center {
                for &index in &current.by_max {
                    let edge = self.edges[index];
                    if edge.max_y <= point[1] { break; }
                    add_fill_crossing(edge, point, &mut winding, &mut crossings);
                }
                node = current.right.as_deref();
            } else {
                for &index in &current.by_min {
                    add_fill_crossing(self.edges[index], point, &mut winding, &mut crossings);
                }
                break;
            }
        }
        match rule {
            SvgFillRule::NonZero => winding != 0,
            SvgFillRule::EvenOdd => crossings % 2 == 1,
        }
    }
}

fn build_fill_interval_node(
    edges: &[FillEdge],
    indices: Vec<usize>,
) -> Option<Box<FillIntervalNode>> {
    if indices.is_empty() { return None; }
    let mut centers = indices
        .iter()
        .map(|&index| edges[index].min_y * 0.5 + edges[index].max_y * 0.5)
        .collect::<Vec<_>>();
    centers.sort_by(f32::total_cmp);
    let center = centers[centers.len() / 2];
    let mut by_min = Vec::new();
    let mut left = Vec::new();
    let mut right = Vec::new();
    for index in indices {
        let edge = edges[index];
        if edge.max_y < center {
            left.push(index);
        } else if edge.min_y > center {
            right.push(index);
        } else {
            by_min.push(index);
        }
    }
    by_min.sort_unstable_by(|&a, &b| edges[a].min_y.total_cmp(&edges[b].min_y));
    let mut by_max = by_min.clone();
    by_max.sort_unstable_by(|&a, &b| edges[b].max_y.total_cmp(&edges[a].max_y));
    Some(Box::new(FillIntervalNode {
        center,
        by_min,
        by_max,
        left: build_fill_interval_node(edges, left),
        right: build_fill_interval_node(edges, right),
    }))
}

fn add_fill_crossing(
    edge: FillEdge,
    point: [f32; 2],
    winding: &mut i32,
    crossings: &mut u32,
) {
    let (a, b) = (edge.a, edge.b);
    if !((a.1 <= point[1] && b.1 > point[1]) || (b.1 <= point[1] && a.1 > point[1])) {
        return;
    }
    let intersection_x = a.0 + (point[1] - a.1) * (b.0 - a.0) / (b.1 - a.1);
    if intersection_x > point[0] {
        *crossings += 1;
        *winding += if b.1 > a.1 { 1 } else { -1 };
    }
}

fn append_fringe_corner_fans(
    vertices: &mut Vec<[f32; 2]>,
    coverages: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    corners: HashMap<PointKey, ([f32; 2], Vec<[f32; 2]>)>,
) {
    for (_, (point, mut outer_points)) in corners {
        outer_points.sort_by(|a, b| {
            (a[1] - point[1]).atan2(a[0] - point[0])
                .total_cmp(&(b[1] - point[1]).atan2(b[0] - point[0]))
        });
        outer_points.dedup_by(|a, b| point_key(*a) == point_key(*b));
        if outer_points.len() < 2 { continue; }
        for pair in outer_points.windows(2) {
            append_coverage_triangle(vertices, coverages, indices, point, pair[0], pair[1]);
        }
        if outer_points.len() > 2 {
            append_coverage_triangle(
                vertices,
                coverages,
                indices,
                point,
                *outer_points.last().unwrap_or(&outer_points[0]),
                outer_points[0],
            );
        }
    }
}

fn append_quad(vertices: &mut Vec<[f32; 2]>, indices: &mut Vec<u32>, a: (f32,f32), b: (f32,f32), c: (f32,f32), d: (f32,f32)) {
    let Ok(base) = u32::try_from(vertices.len()) else { return; };
    vertices.extend([[a.0,a.1],[b.0,b.1],[c.0,c.1],[d.0,d.1]]);
    indices.extend([base,base+1,base+2,base,base+2,base+3]);
}

fn stroke_mesh(contours: &[Vec<(f32,f32)>], width: f32, cap: SvgLineCap, join: SvgLineJoin, miter_limit: f32) -> SvgMesh {
    let radius = width * 0.5;
    let mut vertices = Vec::new(); let mut indices = Vec::new();
    for contour in contours {
        let closed = contour.len() > 2 && contour.first() == contour.last();
        for (index, segment) in contour.windows(2).enumerate() {
            let (a,b)=(segment[0],segment[1]); let dx=b.0-a.0; let dy=b.1-a.1; let len=dx.hypot(dy);
            if len <= f32::EPSILON { continue; }
            let (nx,ny)=(-dy/len*radius,dx/len*radius);
            let (mut ax,mut ay,mut bx,mut by)=(a.0,a.1,b.0,b.1);
            if !closed && cap == SvgLineCap::Square && index == 0 { ax-=dx/len*radius; ay-=dy/len*radius; }
            if !closed && cap == SvgLineCap::Square && index+2 == contour.len() { bx+=dx/len*radius; by+=dy/len*radius; }
            append_quad(&mut vertices,&mut indices,(ax+nx,ay+ny),(ax-nx,ay-ny),(bx-nx,by-ny),(bx+nx,by+ny));
            if index > 0 { add_join(&mut vertices,&mut indices,contour[index-1],a,b,radius,join,miter_limit); }
            if !closed && cap == SvgLineCap::Round && index == 0 { append_disc(&mut vertices,&mut indices,a,radius); }
            if !closed && cap == SvgLineCap::Round && index+2 == contour.len() { append_disc(&mut vertices,&mut indices,b,radius); }
        }
        if closed && contour.len() > 3 { let count=contour.len();add_join(&mut vertices,&mut indices,contour[count-2],contour[0],contour[1],radius,join,miter_limit); }
    }
    let coverages = vec![1.0; vertices.len()];
    SvgMesh { vertices: vertices.into(), coverages: coverages.into(), indices: indices.into() }
}

type PointKey = (u32, u32);
type EdgeKey = (PointKey, PointKey);

fn point_key(point: [f32; 2]) -> PointKey {
    let bits = |value: f32| if value == 0.0 { 0 } else { value.to_bits() };
    (bits(point[0]), bits(point[1]))
}

fn edge_key(a: [f32; 2], b: [f32; 2]) -> EdgeKey {
    let a = point_key(a);
    let b = point_key(b);
    if a <= b { (a, b) } else { (b, a) }
}

struct TriangleGrid {
    bounds: [f32; 4],
    cell_size: [f32; 2],
    cells: Vec<Vec<usize>>,
    triangles: Vec<[usize; 3]>,
}

impl TriangleGrid {
    const SIDE: usize = 32;

    fn new(mesh: &SvgMesh) -> Self {
        let mut bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
        for point in mesh.vertices.iter() {
            bounds[0] = bounds[0].min(point[0]);
            bounds[1] = bounds[1].min(point[1]);
            bounds[2] = bounds[2].max(point[0]);
            bounds[3] = bounds[3].max(point[1]);
        }
        let cell_size = [
            ((bounds[2] - bounds[0]) / Self::SIDE as f32).max(f32::EPSILON),
            ((bounds[3] - bounds[1]) / Self::SIDE as f32).max(f32::EPSILON),
        ];
        let mut cells = (0..Self::SIDE * Self::SIDE).map(|_| Vec::new()).collect::<Vec<_>>();
        let triangles = mesh.indices
            .chunks_exact(3)
            .map(|triangle| [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize])
            .collect::<Vec<_>>();
        for (triangle_index, triangle) in triangles.iter().enumerate() {
            let a = mesh.vertices[triangle[0]];
            let b = mesh.vertices[triangle[1]];
            let c = mesh.vertices[triangle[2]];
            let min_x = a[0].min(b[0]).min(c[0]);
            let min_y = a[1].min(b[1]).min(c[1]);
            let max_x = a[0].max(b[0]).max(c[0]);
            let max_y = a[1].max(b[1]).max(c[1]);
            let (x0, y0) = grid_cell([min_x, min_y], bounds, cell_size);
            let (x1, y1) = grid_cell([max_x, max_y], bounds, cell_size);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    cells[y * Self::SIDE + x].push(triangle_index);
                }
            }
        }
        Self { bounds, cell_size, cells, triangles }
    }

    fn contains(&self, mesh: &SvgMesh, point: [f32; 2]) -> bool {
        if point[0] < self.bounds[0] || point[0] > self.bounds[2]
            || point[1] < self.bounds[1] || point[1] > self.bounds[3]
        {
            return false;
        }
        let (x, y) = grid_cell(point, self.bounds, self.cell_size);
        self.cells[y * Self::SIDE + x].iter().any(|&index| {
            let triangle = self.triangles[index];
            point_in_triangle(
                point,
                mesh.vertices[triangle[0]],
                mesh.vertices[triangle[1]],
                mesh.vertices[triangle[2]],
            )
        })
    }
}

fn grid_cell(point: [f32; 2], bounds: [f32; 4], cell_size: [f32; 2]) -> (usize, usize) {
    let x = ((point[0] - bounds[0]) / cell_size[0]).floor() as usize;
    let y = ((point[1] - bounds[1]) / cell_size[1]).floor() as usize;
    (x.min(TriangleGrid::SIDE - 1), y.min(TriangleGrid::SIDE - 1))
}

fn point_in_triangle(point: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    let cross = |from: [f32; 2], to: [f32; 2]| {
        (to[0] - from[0]) * (point[1] - from[1])
            - (to[1] - from[1]) * (point[0] - from[0])
    };
    let ab = cross(a, b);
    let bc = cross(b, c);
    let ca = cross(c, a);
    let epsilon = f32::EPSILON * 16.0;
    (ab >= -epsilon && bc >= -epsilon && ca >= -epsilon)
        || (ab <= epsilon && bc <= epsilon && ca <= epsilon)
}

fn add_mesh_fringe(mesh: &mut SvgMesh, width: f32) {
    if mesh.indices.len() < 3 || !width.is_finite() || width <= 0.0 {
        return;
    }

    #[derive(Clone, Copy)]
    struct BoundaryEdge {
        a: [f32; 2],
        b: [f32; 2],
        interior_side: f32,
        count: u8,
    }

    let triangle_grid = TriangleGrid::new(mesh);
    let mut edges: HashMap<EdgeKey, BoundaryEdge> = HashMap::new();
    for triangle in mesh.indices.chunks_exact(3) {
        let [ia, ib, ic] = [triangle[0] as usize, triangle[1] as usize, triangle[2] as usize];
        let a = mesh.vertices[ia];
        let b = mesh.vertices[ib];
        let c = mesh.vertices[ic];
        let center = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0];
        for (from, to) in [(a, b), (b, c), (c, a)] {
            let key = edge_key(from, to);
            let edge = edges.entry(key).or_insert_with(|| BoundaryEdge {
                a: from,
                b: to,
                interior_side: (to[0] - from[0]) * (center[1] - from[1])
                    - (to[1] - from[1]) * (center[0] - from[0]),
                count: 0,
            });
            edge.count = edge.count.saturating_add(1);
        }
    }

    let mut vertices = mesh.vertices.to_vec();
    let mut coverages = if mesh.coverages.len() == mesh.vertices.len() {
        mesh.coverages.to_vec()
    } else {
        vec![1.0; mesh.vertices.len()]
    };
    let mut indices = mesh.indices.to_vec();
    let mut corners: HashMap<PointKey, ([f32; 2], Vec<[f32; 2]>)> = HashMap::new();

    for edge in edges.values().filter(|edge| edge.count == 1) {
        let dx = edge.b[0] - edge.a[0];
        let dy = edge.b[1] - edge.a[1];
        let length = dx.hypot(dy);
        if !length.is_finite() || length <= f32::EPSILON {
            continue;
        }
        // The signed area tells which side of the directed boundary edge is
        // covered by its source triangle. Move the fringe to the other side.
        let sign = if edge.interior_side >= 0.0 { -1.0 } else { 1.0 };
        let normal = [sign * -dy / length, sign * dx / length];
        let probe = [
            (edge.a[0] + edge.b[0]) * 0.5 + normal[0] * (width * 0.25),
            (edge.a[1] + edge.b[1]) * 0.5 + normal[1] * (width * 0.25),
        ];
        if triangle_grid.contains(mesh, probe) {
            continue;
        }
        let outer_a = [edge.a[0] + normal[0] * width, edge.a[1] + normal[1] * width];
        let outer_b = [edge.b[0] + normal[0] * width, edge.b[1] + normal[1] * width];
        append_coverage_quad(
            &mut vertices,
            &mut coverages,
            &mut indices,
            edge.a,
            edge.b,
            outer_b,
            outer_a,
        );
        corners.entry(point_key(edge.a)).or_insert_with(|| (edge.a, Vec::new())).1.push(outer_a);
        corners.entry(point_key(edge.b)).or_insert_with(|| (edge.b, Vec::new())).1.push(outer_b);
    }

    for (_, (point, mut outer_points)) in corners {
        outer_points.sort_by(|a, b| {
            (a[1] - point[1]).atan2(a[0] - point[0])
                .total_cmp(&(b[1] - point[1]).atan2(b[0] - point[0]))
        });
        outer_points.dedup_by(|a, b| point_key(*a) == point_key(*b));
        if outer_points.len() < 2 { continue; }
        for pair in outer_points.windows(2) {
            append_coverage_triangle(
                &mut vertices,
                &mut coverages,
                &mut indices,
                point,
                pair[0],
                pair[1],
            );
        }
        if outer_points.len() > 2 {
            append_coverage_triangle(
                &mut vertices,
                &mut coverages,
                &mut indices,
                point,
                *outer_points.last().unwrap_or(&outer_points[0]),
                outer_points[0],
            );
        }
    }

    mesh.vertices = vertices.into();
    mesh.coverages = coverages.into();
    mesh.indices = indices.into();
}

fn append_coverage_quad(
    vertices: &mut Vec<[f32; 2]>,
    coverages: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    a: [f32; 2],
    b: [f32; 2],
    c: [f32; 2],
    d: [f32; 2],
) {
    let Ok(base) = u32::try_from(vertices.len()) else { return; };
    vertices.extend([a, b, c, d]);
    coverages.extend([1.0, 1.0, 0.0, 0.0]);
    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn append_coverage_triangle(
    vertices: &mut Vec<[f32; 2]>,
    coverages: &mut Vec<f32>,
    indices: &mut Vec<u32>,
    a: [f32; 2],
    b: [f32; 2],
    c: [f32; 2],
) {
    let Ok(base) = u32::try_from(vertices.len()) else { return; };
    vertices.extend([a, b, c]);
    coverages.extend([1.0, 0.0, 0.0]);
    indices.extend([base, base + 1, base + 2]);
}

fn add_join(vertices:&mut Vec<[f32;2]>,indices:&mut Vec<u32>,previous:(f32,f32),point:(f32,f32),next:(f32,f32),radius:f32,join:SvgLineJoin,miter_limit:f32){
    let d1=(point.0-previous.0,point.1-previous.1);let d2=(next.0-point.0,next.1-point.1);let l1=d1.0.hypot(d1.1);let l2=d2.0.hypot(d2.1);if l1<=f32::EPSILON||l2<=f32::EPSILON{return;}
    let d1=(d1.0/l1,d1.1/l1);let d2=(d2.0/l2,d2.1/l2);let cross=d1.0*d2.1-d1.1*d2.0;if cross.abs()<=f32::EPSILON{return;}
    if join==SvgLineJoin::Round{append_disc(vertices,indices,point,radius);return;}
    let side=if cross>0.0{-1.0}else{1.0};let a=(point.0-side*d1.1*radius,point.1+side*d1.0*radius);let b=(point.0-side*d2.1*radius,point.1+side*d2.0*radius);
    let miter=if matches!(join,SvgLineJoin::Miter|SvgLineJoin::MiterClip){let delta=(b.0-a.0,b.1-a.1);let t=(delta.0*d2.1-delta.1*d2.0)/cross;let candidate=(a.0+d1.0*t,a.1+d1.1*t);((candidate.0-point.0).hypot(candidate.1-point.1)<=miter_limit*radius).then_some(candidate)}else{None};
    if let Some(miter)=miter{append_triangle(vertices,indices,a,miter,b)}else{append_triangle(vertices,indices,point,a,b)}
}

fn append_triangle(vertices:&mut Vec<[f32;2]>,indices:&mut Vec<u32>,a:(f32,f32),b:(f32,f32),c:(f32,f32)){
    let Ok(base)=u32::try_from(vertices.len())else{return};vertices.extend([[a.0,a.1],[b.0,b.1],[c.0,c.1]]);indices.extend([base,base+1,base+2]);
}

fn append_disc(vertices: &mut Vec<[f32;2]>, indices: &mut Vec<u32>, center: (f32,f32), radius: f32) {
    const STEPS: usize = 12;
    let Ok(base)=u32::try_from(vertices.len()) else{return};
    vertices.push([center.0,center.1]);
    for step in 0..STEPS { let angle=std::f32::consts::TAU*step as f32/STEPS as f32; vertices.push([center.0+angle.cos()*radius,center.1+angle.sin()*radius]); }
    for step in 0..STEPS {let a=base+1+step as u32;let b=base+1+((step+1)%STEPS) as u32;indices.extend([base,a,b]);}
}

/// Tessellates a stroke with the SVG dash pattern retained by [`SvgStroke`].
///
/// The path is flattened into bounded line pieces before the stroke mesh is
/// generated. Each visible dash is an independent subpath, which preserves the
/// requested cap style and makes gaps real geometry gaps.
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
            false,
        );
    }
    if !stroke.dash_offset.is_finite()
        || !stroke.width.is_finite()
        || stroke.width <= 0.0
        || !stroke.miter_limit.is_finite()
        || stroke.miter_limit < 1.0
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
        false,
    )
}

fn empty_mesh() -> SvgMesh {
    SvgMesh {
        vertices: Arc::from([]),
        coverages: Arc::from([]),
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
