use std::sync::Arc;

mod tessellation;

pub use tessellation::{
    SvgGeometryCache, SvgMesh, SvgMeshStyle, SvgTessellationError, SvgToleranceBucket,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SvgViewport {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgTransform {
    pub sx: f32,
    pub ky: f32,
    pub kx: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Default for SvgTransform {
    fn default() -> Self {
        Self { sx: 1.0, ky: 0.0, kx: 0.0, sy: 1.0, tx: 0.0, ty: 0.0 }
    }
}

impl SvgTransform {
    pub fn is_finite(self) -> bool {
        [self.sx, self.ky, self.kx, self.sy, self.tx, self.ty]
            .into_iter()
            .all(f32::is_finite)
    }

    pub fn inverse(self) -> Option<Self> {
        let determinant = self.sx * self.sy - self.kx * self.ky;
        if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
            return None;
        }
        let inverse = 1.0 / determinant;
        Some(Self {
            sx: self.sy * inverse,
            ky: -self.ky * inverse,
            kx: -self.kx * inverse,
            sy: self.sx * inverse,
            tx: (self.kx * self.ty - self.sy * self.tx) * inverse,
            ty: (self.ky * self.tx - self.sx * self.ty) * inverse,
        })
    }

    pub fn transform_point(self, x: f32, y: f32) -> (f32, f32) {
        (self.sx * x + self.kx * y + self.tx, self.ky * x + self.sy * y + self.ty)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SvgNodeId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SvgElementKind {
    Group,
    Path,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SvgFillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SvgLineCap {
    Butt,
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SvgLineJoin {
    Miter,
    MiterClip,
    Round,
    Bevel,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SvgPaintOrder {
    FillAndStroke,
    StrokeAndFill,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgColor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl SvgColor {
    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: a as f32 / 255.0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SvgFill {
    pub color: SvgColor,
    pub rule: SvgFillRule,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SvgStroke {
    pub color: SvgColor,
    pub width: f32,
    pub line_cap: SvgLineCap,
    pub line_join: SvgLineJoin,
    pub miter_limit: f32,
    pub dash_array: Arc<[f32]>,
    pub dash_offset: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SvgPathCommand {
    MoveTo { x: f32, y: f32 },
    LineTo { x: f32, y: f32 },
    QuadraticTo { control_x: f32, control_y: f32, x: f32, y: f32 },
    CubicTo { control1_x: f32, control1_y: f32, control2_x: f32, control2_y: f32, x: f32, y: f32 },
    Close,
}

#[derive(Clone, Debug)]
pub struct SvgGeometry {
    pub commands: Arc<[SvgPathCommand]>,
}

#[derive(Clone, Debug)]
pub struct SvgNode {
    pub node_id: SvgNodeId,
    pub svg_id: Option<Arc<str>>,
    pub classes: Arc<[Arc<str>]>,
    pub element: SvgElementKind,
    pub parent: Option<SvgNodeId>,
    pub children: Arc<[SvgNodeId]>,
    pub transform: SvgTransform,
    pub opacity: f32,
    pub geometry: Option<usize>,
    pub fill: Option<SvgFill>,
    pub stroke: Option<SvgStroke>,
    pub paint_order: SvgPaintOrder,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgNodeStyleOverride {
    pub node_id: SvgNodeId,
    pub fill: Option<Option<SvgColor>>,
    pub stroke: Option<Option<SvgColor>>,
    pub opacity: Option<f32>,
    pub transform: Option<SvgTransform>,
}

#[derive(Clone, Debug)]
pub struct SvgScene {
    pub viewport: SvgViewport,
    pub nodes: Arc<[SvgNode]>,
    pub geometries: Arc<[SvgGeometry]>,
}

impl SvgScene {
    pub fn node(&self, node_id: SvgNodeId) -> Option<&SvgNode> {
        self.nodes.get(node_id.0 as usize)
    }

    pub fn geometry(&self, node: &SvgNode) -> Option<&SvgGeometry> {
        self.geometries.get(node.geometry?)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn geometry(commands: impl IntoIterator<Item = SvgPathCommand>) -> SvgGeometry {
        SvgGeometry {
            commands: commands
                .into_iter()
                .collect::<Vec<_>>()
                .into(),
        }
    }

    fn rectangle() -> SvgGeometry {
        geometry([
            SvgPathCommand::MoveTo { x: 0.0, y: 0.0 },
            SvgPathCommand::LineTo { x: 10.0, y: 0.0 },
            SvgPathCommand::LineTo { x: 10.0, y: 10.0 },
            SvgPathCommand::LineTo { x: 0.0, y: 10.0 },
            SvgPathCommand::Close,
        ])
    }

    #[test]
    fn tessellates_concave_and_holed_fills_for_both_rules() {
        let concave = geometry([
            SvgPathCommand::MoveTo { x: 0.0, y: 0.0 },
            SvgPathCommand::LineTo { x: 10.0, y: 0.0 },
            SvgPathCommand::LineTo { x: 5.0, y: 4.0 },
            SvgPathCommand::LineTo { x: 10.0, y: 10.0 },
            SvgPathCommand::LineTo { x: 0.0, y: 10.0 },
            SvgPathCommand::Close,
            SvgPathCommand::MoveTo { x: 2.0, y: 2.0 },
            SvgPathCommand::LineTo { x: 2.0, y: 3.0 },
            SvgPathCommand::LineTo { x: 3.0, y: 3.0 },
            SvgPathCommand::LineTo { x: 3.0, y: 2.0 },
            SvgPathCommand::Close,
        ]);
        let mut cache = SvgGeometryCache::new(1024 * 1024, 16);

        let non_zero = cache
            .mesh_for(&concave, SvgMeshStyle::Fill(SvgFillRule::NonZero), 1.0)
            .unwrap();
        let even_odd = cache
            .mesh_for(&concave, SvgMeshStyle::Fill(SvgFillRule::EvenOdd), 1.0)
            .unwrap();

        assert!(!non_zero.vertices.is_empty());
        assert_eq!(non_zero.indices.len() % 3, 0);
        assert!(!even_odd.indices.is_empty());
        assert!(!Arc::ptr_eq(&non_zero, &even_odd));
    }

    #[test]
    fn tessellates_strokes_and_geometry_parameters_invalidate_cache() {
        let line = geometry([
            SvgPathCommand::MoveTo { x: 0.0, y: 0.0 },
            SvgPathCommand::QuadraticTo { control_x: 5.0, control_y: 8.0, x: 10.0, y: 0.0 },
        ]);
        let mut cache = SvgGeometryCache::new(1024 * 1024, 16);
        let thin = SvgMeshStyle::Stroke {
            width: 1.0,
            line_cap: SvgLineCap::Round,
            line_join: SvgLineJoin::Bevel,
            miter_limit: 4.0,
        };
        let thick = SvgMeshStyle::Stroke {
            width: 4.0,
            line_cap: SvgLineCap::Round,
            line_join: SvgLineJoin::Bevel,
            miter_limit: 4.0,
        };

        let first = cache
            .mesh_for(&line, thin, 1.0)
            .unwrap();
        let reused = cache
            .mesh_for(&line, thin, 1.0)
            .unwrap();
        let changed = cache
            .mesh_for(&line, thick, 1.0)
            .unwrap();

        assert!(Arc::ptr_eq(&first, &reused));
        assert!(!Arc::ptr_eq(&first, &changed));
        assert!(!first.vertices.is_empty());
    }

    #[test]
    fn dynamic_paint_and_transform_values_do_not_enter_geometry_key() {
        let geometry = rectangle();
        let mut cache = SvgGeometryCache::new(1024 * 1024, 16);
        let style = SvgMeshStyle::Fill(SvgFillRule::NonZero);

        let before_dynamic_change = cache
            .mesh_for(&geometry, style, 1.0)
            .unwrap();
        let _new_color = SvgColor::rgba8(255, 0, 0, 255);
        let _new_opacity = 0.25;
        let _new_transform = SvgTransform { tx: 30.0, ..SvgTransform::default() };
        let after_dynamic_change = cache
            .mesh_for(&geometry, style, 1.0)
            .unwrap();

        assert!(Arc::ptr_eq(&before_dynamic_change, &after_dynamic_change));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn scale_buckets_are_finite_and_cache_eviction_is_bounded() {
        let unique_buckets = (1..=10_000)
            .map(|scale| SvgToleranceBucket::from_scale(scale as f32 / 100.0))
            .collect::<std::collections::HashSet<_>>();
        assert!(unique_buckets.len() <= SvgToleranceBucket::COUNT);

        let mut cache = SvgGeometryCache::new(1024 * 1024, 2);
        for offset in 0..3 {
            let translated = geometry([
                SvgPathCommand::MoveTo { x: offset as f32, y: 0.0 },
                SvgPathCommand::LineTo { x: offset as f32 + 2.0, y: 0.0 },
                SvgPathCommand::LineTo { x: offset as f32, y: 2.0 },
                SvgPathCommand::Close,
            ]);
            cache
                .mesh_for(&translated, SvgMeshStyle::Fill(SvgFillRule::NonZero), 1.0)
                .unwrap();
        }

        assert_eq!(cache.len(), 2);
        assert!(cache.memory_bytes() <= cache.max_memory_bytes());
    }
}
