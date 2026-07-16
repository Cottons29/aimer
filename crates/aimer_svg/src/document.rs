use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use aimer_cupid::svg::{
    SvgColor, SvgElementKind, SvgFill, SvgFillRule, SvgGeometry, SvgLineCap, SvgLineJoin, SvgNode,
    SvgNodeId, SvgPaintOrder, SvgPathCommand, SvgScene, SvgStroke, SvgTransform, SvgViewport,
};
use usvg::tiny_skia_path::PathSegment;

use crate::{SvgError, SvgSelector};

#[derive(Clone, Copy, Debug)]
pub struct SvgLimits {
    pub max_source_bytes: usize,
    pub max_nodes: usize,
    pub max_path_commands: usize,
    pub max_viewport_dimension: f32,
}

impl Default for SvgLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 4 * 1024 * 1024,
            max_nodes: 16_384,
            max_path_commands: 1_000_000,
            max_viewport_dimension: 1_000_000.0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SvgDiagnostic {
    pub feature: &'static str,
    pub message: Arc<str>,
}

#[derive(Clone)]
pub struct SvgDocument {
    scene: Arc<SvgScene>,
    diagnostics: Arc<[SvgDiagnostic]>,
}

impl SvgDocument {
    pub fn from_svg(bytes: impl AsRef<[u8]>) -> Result<Self, SvgError> {
        Self::from_svg_with_limits(bytes, SvgLimits::default())
    }

    pub fn from_svg_with_limits(
        bytes: impl AsRef<[u8]>,
        limits: SvgLimits,
    ) -> Result<Self, SvgError> {
        let bytes = bytes.as_ref();
        if bytes.is_empty() {
            return Err(SvgError::EmptyInput);
        }
        check_limit("source bytes", bytes.len(), limits.max_source_bytes)?;
        let source =
            std::str::from_utf8(bytes).map_err(|error| SvgError::Parse(error.to_string()))?;
        let xml = usvg::roxmltree::Document::parse(source)
            .map_err(|error| SvgError::Parse(error.to_string()))?;
        reject_non_finite_literals(&xml)?;
        reject_external_resources(&xml)?;
        let diagnostics = collect_diagnostics(&xml);
        let metadata = collect_metadata(&xml, limits.max_nodes)?;

        let tree = usvg::Tree::from_data(bytes, &usvg::Options::default())
            .map_err(|error| SvgError::Parse(error.to_string()))?;
        let size = tree.size();
        let viewport = SvgViewport { width: size.width(), height: size.height() };
        if !viewport.width.is_finite() || !viewport.height.is_finite() {
            return Err(SvgError::NonFinite);
        }
        if viewport.width > limits.max_viewport_dimension
            || viewport.height > limits.max_viewport_dimension
        {
            return Err(SvgError::LimitExceeded {
                resource: "viewport dimension",
                actual: viewport.width.max(viewport.height) as usize,
                limit: limits.max_viewport_dimension as usize,
            });
        }

        let mut builder = SceneBuilder::new(viewport, metadata, limits);
        builder.add_group(tree.root(), None, 1.0)?;
        Ok(Self { scene: Arc::new(builder.finish()), diagnostics: diagnostics.into() })
    }

    pub fn scene(&self) -> &Arc<SvgScene> {
        &self.scene
    }

    pub fn diagnostics(&self) -> &[SvgDiagnostic] {
        &self.diagnostics
    }

    pub fn select(
        &self,
        selector: impl TryInto<SvgSelector, Error = SvgError>,
    ) -> Result<Vec<SvgNodeId>, SvgError> {
        let selector = selector.try_into()?;
        Ok(self
            .scene
            .nodes
            .iter()
            .filter(|node| selector.matches(node))
            .map(|node| node.node_id)
            .collect())
    }
}

#[derive(Clone)]
pub struct SvgPath {
    commands: Arc<[SvgPathCommand]>,
}

impl SvgPath {
    pub fn from_path_data(data: &str) -> Result<Self, SvgError> {
        if data.trim().is_empty() {
            return Err(SvgError::InvalidPath("path data is empty".to_owned()));
        }
        let svg = format!(
            r#"<svg width="1" height="1" xmlns="http://www.w3.org/2000/svg"><path id="aimer-path" d="{data}"/></svg>"#
        );
        Self::from_svg(svg.as_bytes(), "#aimer-path").map_err(|error| match error {
            SvgError::Parse(message) => SvgError::InvalidPath(message),
            SvgError::PathSelection(count) => {
                SvgError::InvalidPath(format!("normalized path count was {count}"))
            }
            SvgError::NonFinite => {
                SvgError::InvalidPath("path contains a non-finite value".to_owned())
            }
            other => other,
        })
    }

    pub fn from_svg(
        bytes: impl AsRef<[u8]>,
        selector: impl TryInto<SvgSelector, Error = SvgError>,
    ) -> Result<Self, SvgError> {
        let document = SvgDocument::from_svg(bytes)?;
        let matches = document.select(selector)?;
        let path_nodes: Vec<_> = matches
            .into_iter()
            .filter_map(|node_id| document.scene.node(node_id))
            .filter(|node| node.element == SvgElementKind::Path)
            .collect();
        if path_nodes.len() != 1 {
            return Err(SvgError::PathSelection(path_nodes.len()));
        }
        let geometry = document
            .scene
            .geometry(path_nodes[0])
            .ok_or(SvgError::PathSelection(0))?;
        Ok(Self { commands: geometry.commands.clone() })
    }

    pub fn commands(&self) -> &[SvgPathCommand] {
        &self.commands
    }
}

#[derive(Clone, Debug)]
struct SourceMetadata {
    source_index: usize,
    parent_group_index: Option<usize>,
    svg_id: Option<Arc<str>>,
    classes: Arc<[Arc<str>]>,
    element: SvgElementKind,
}

fn collect_metadata(
    document: &usvg::roxmltree::Document<'_>,
    max_nodes: usize,
) -> Result<Vec<SourceMetadata>, SvgError> {
    let supported =
        ["svg", "g", "path", "rect", "circle", "ellipse", "line", "polyline", "polygon"];
    let mut metadata = Vec::new();
    let mut group_indices = HashMap::new();
    for node in document
        .descendants()
        .filter(|node| {
            node.is_element()
                && supported.contains(&node.tag_name().name())
                && !node.ancestors().any(|ancestor| {
                    ancestor.is_element()
                        && matches!(
                            ancestor.tag_name().name(),
                            "defs" | "clipPath" | "mask" | "pattern" | "symbol"
                        )
                })
        })
    {
        if node.tag_name().name() == "svg" {
            continue;
        }
        check_limit("nodes", metadata.len() + 1, max_nodes)?;
        let parent_group_index = node
            .ancestors()
            .find(|ancestor| ancestor.is_element() && ancestor.tag_name().name() == "g")
            .and_then(|ancestor| {
                group_indices
                    .get(&ancestor.id())
                    .copied()
            });
        let source_index = metadata.len();
        let classes = node
            .attribute("class")
            .unwrap_or_default()
            .split_ascii_whitespace()
            .map(Arc::<str>::from)
            .collect::<Vec<_>>()
            .into();
        metadata.push(SourceMetadata {
            source_index,
            parent_group_index,
            svg_id: node
                .attribute("id")
                .filter(|id| !id.is_empty())
                .map(Arc::from),
            classes,
            element: if node.tag_name().name() == "g" {
                SvgElementKind::Group
            } else {
                SvgElementKind::Path
            },
        });
        if node.tag_name().name() == "g" {
            group_indices.insert(node.id(), source_index);
        }
    }
    Ok(metadata)
}

fn reject_non_finite_literals(document: &usvg::roxmltree::Document<'_>) -> Result<(), SvgError> {
    for attribute in document
        .descendants()
        .filter(|node| node.is_element())
        .flat_map(|node| node.attributes())
    {
        let value = attribute
            .value()
            .to_ascii_lowercase();
        if value
            .split(|character: char| {
                !(character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'))
            })
            .any(|token| {
                matches!(
                    token,
                    "nan" | "inf" | "+inf" | "-inf" | "infinity" | "+infinity" | "-infinity"
                )
            })
        {
            return Err(SvgError::NonFinite);
        }
    }
    Ok(())
}

fn collect_diagnostics(document: &usvg::roxmltree::Document<'_>) -> Vec<SvgDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut found = HashSet::new();
    for node in document
        .descendants()
        .filter(|node| node.is_element())
    {
        let feature = match node.tag_name().name() {
            "linearGradient" | "radialGradient" => Some("gradient"),
            "pattern" => Some("pattern"),
            "clipPath" => Some("clip-path"),
            "mask" => Some("mask"),
            "filter" => Some("filter"),
            "text" | "tspan" => Some("text"),
            "script" => Some("script"),
            _ => None,
        };
        if let Some(feature) = feature
            && found.insert(feature)
        {
            diagnostics.push(SvgDiagnostic {
                feature,
                message: Arc::from(format!("{feature} is deferred and was skipped")),
            });
        }
    }
    diagnostics
}

fn reject_external_resources(document: &usvg::roxmltree::Document<'_>) -> Result<(), SvgError> {
    for node in document
        .descendants()
        .filter(|node| node.is_element())
    {
        for attribute in node.attributes() {
            if matches!(attribute.name(), "href" | "xlink:href") {
                let value = attribute.value().trim();
                if !value.is_empty() && !value.starts_with('#') {
                    return Err(SvgError::ExternalResource(value.to_owned()));
                }
            }
        }
    }
    Ok(())
}

struct SceneBuilder {
    viewport: SvgViewport,
    metadata_by_id: HashMap<Arc<str>, SourceMetadata>,
    unnamed_path_metadata: Vec<SourceMetadata>,
    group_nodes: HashMap<usize, SvgNodeId>,
    path_metadata_index: usize,
    nodes: Vec<SvgNode>,
    geometries: Vec<SvgGeometry>,
    command_count: usize,
    limits: SvgLimits,
}

impl SceneBuilder {
    fn new(viewport: SvgViewport, metadata: Vec<SourceMetadata>, limits: SvgLimits) -> Self {
        let metadata_by_id = metadata
            .iter()
            .filter_map(|metadata| {
                metadata
                    .svg_id
                    .clone()
                    .map(|id| (id, metadata.clone()))
            })
            .collect();
        let unnamed_path_metadata = metadata
            .iter()
            .filter(|metadata| {
                metadata.element == SvgElementKind::Path && metadata.svg_id.is_none()
            })
            .cloned()
            .collect();
        let mut group_nodes = HashMap::new();
        let mut nodes = Vec::new();
        for group in metadata
            .iter()
            .filter(|metadata| metadata.element == SvgElementKind::Group)
        {
            let node_id = SvgNodeId(nodes.len() as u32);
            let parent = group
                .parent_group_index
                .and_then(|source_index| {
                    group_nodes
                        .get(&source_index)
                        .copied()
                });
            nodes.push(SvgNode {
                node_id,
                svg_id: group.svg_id.clone(),
                classes: group.classes.clone(),
                element: SvgElementKind::Group,
                parent,
                children: Arc::from([]),
                transform: SvgTransform::default(),
                opacity: 1.0,
                geometry: None,
                fill: None,
                stroke: None,
                paint_order: SvgPaintOrder::FillAndStroke,
                visible: true,
            });
            group_nodes.insert(group.source_index, node_id);
        }
        Self {
            viewport,
            metadata_by_id,
            unnamed_path_metadata,
            group_nodes,
            path_metadata_index: 0,
            nodes,
            geometries: Vec::new(),
            command_count: 0,
            limits,
        }
    }

    fn finish(self) -> SvgScene {
        let mut nodes = self.nodes;
        for index in 0..nodes.len() {
            if nodes[index].element != SvgElementKind::Group {
                continue;
            }
            let node_id = nodes[index].node_id;
            nodes[index].children = nodes
                .iter()
                .filter(|node| node.parent == Some(node_id))
                .map(|node| node.node_id)
                .collect::<Vec<_>>()
                .into();
        }
        SvgScene {
            viewport: self.viewport,
            nodes: nodes.into(),
            geometries: self.geometries.into(),
        }
    }

    fn add_group(
        &mut self,
        group: &usvg::Group,
        parent: Option<SvgNodeId>,
        inherited_opacity: f32,
    ) -> Result<(), SvgError> {
        let group_opacity = inherited_opacity * group.opacity().get();
        let group_node_id = if group.id().is_empty() {
            parent
        } else if let Some(metadata) = self.metadata_by_id.get(group.id()) {
            let node_id = self
                .group_nodes
                .get(&metadata.source_index)
                .copied();
            if let Some(node_id) = node_id {
                let node = &mut self.nodes[node_id.0 as usize];
                node.transform = convert_transform(group.abs_transform());
                node.opacity = group_opacity;
            }
            node_id.or(parent)
        } else {
            parent
        };
        for child in group.children() {
            match child {
                usvg::Node::Group(group) => self.add_group(group, group_node_id, group_opacity)?,
                usvg::Node::Path(path) => self.add_path(path, group_node_id, group_opacity)?,
                usvg::Node::Image(_) | usvg::Node::Text(_) => {}
            }
        }
        Ok(())
    }

    fn add_path(
        &mut self,
        path: &usvg::Path,
        parent: Option<SvgNodeId>,
        opacity: f32,
    ) -> Result<(), SvgError> {
        check_limit("nodes", self.nodes.len() + 1, self.limits.max_nodes)?;
        let metadata = if path.id().is_empty() {
            let metadata = self
                .unnamed_path_metadata
                .get(self.path_metadata_index)
                .cloned();
            self.path_metadata_index += 1;
            metadata
        } else {
            self.metadata_by_id
                .get(path.id())
                .cloned()
        };
        let source_parent = metadata
            .as_ref()
            .and_then(|metadata| metadata.parent_group_index)
            .and_then(|source_index| {
                self.group_nodes
                    .get(&source_index)
                    .copied()
            })
            .or(parent);
        let commands = convert_path(path.data());
        self.command_count += commands.len();
        check_limit("path commands", self.command_count, self.limits.max_path_commands)?;
        let transform = convert_transform(path.abs_transform());
        if !transform.is_finite() || !opacity.is_finite() {
            return Err(SvgError::NonFinite);
        }
        let geometry = self.geometries.len();
        self.geometries
            .push(SvgGeometry { commands: commands.into() });
        let node_id = SvgNodeId(self.nodes.len() as u32);
        self.nodes.push(SvgNode {
            node_id,
            svg_id: metadata
                .as_ref()
                .and_then(|metadata| metadata.svg_id.clone())
                .or_else(|| (!path.id().is_empty()).then(|| Arc::from(path.id()))),
            classes: metadata
                .map(|metadata| metadata.classes)
                .unwrap_or_default(),
            element: SvgElementKind::Path,
            parent: source_parent,
            children: Arc::from([]),
            transform,
            opacity,
            geometry: Some(geometry),
            fill: path.fill().and_then(convert_fill),
            stroke: path
                .stroke()
                .and_then(convert_stroke),
            paint_order: match path.paint_order() {
                usvg::PaintOrder::FillAndStroke => SvgPaintOrder::FillAndStroke,
                usvg::PaintOrder::StrokeAndFill => SvgPaintOrder::StrokeAndFill,
            },
            visible: path.is_visible(),
        });
        Ok(())
    }
}

fn convert_path(path: &usvg::tiny_skia_path::Path) -> Vec<SvgPathCommand> {
    path.segments()
        .map(|segment| match segment {
            PathSegment::MoveTo(point) => SvgPathCommand::MoveTo { x: point.x, y: point.y },
            PathSegment::LineTo(point) => SvgPathCommand::LineTo { x: point.x, y: point.y },
            PathSegment::QuadTo(control, point) => SvgPathCommand::QuadraticTo {
                control_x: control.x,
                control_y: control.y,
                x: point.x,
                y: point.y,
            },
            PathSegment::CubicTo(control1, control2, point) => SvgPathCommand::CubicTo {
                control1_x: control1.x,
                control1_y: control1.y,
                control2_x: control2.x,
                control2_y: control2.y,
                x: point.x,
                y: point.y,
            },
            PathSegment::Close => SvgPathCommand::Close,
        })
        .collect()
}

fn convert_transform(transform: usvg::Transform) -> SvgTransform {
    SvgTransform {
        sx: transform.sx,
        ky: transform.ky,
        kx: transform.kx,
        sy: transform.sy,
        tx: transform.tx,
        ty: transform.ty,
    }
}

fn convert_fill(fill: &usvg::Fill) -> Option<SvgFill> {
    let color = convert_paint(fill.paint(), fill.opacity().get())?;
    Some(SvgFill {
        color,
        rule: match fill.rule() {
            usvg::FillRule::NonZero => SvgFillRule::NonZero,
            usvg::FillRule::EvenOdd => SvgFillRule::EvenOdd,
        },
    })
}

fn convert_stroke(stroke: &usvg::Stroke) -> Option<SvgStroke> {
    Some(SvgStroke {
        color: convert_paint(stroke.paint(), stroke.opacity().get())?,
        width: stroke.width().get(),
        line_cap: match stroke.linecap() {
            usvg::LineCap::Butt => SvgLineCap::Butt,
            usvg::LineCap::Round => SvgLineCap::Round,
            usvg::LineCap::Square => SvgLineCap::Square,
        },
        line_join: match stroke.linejoin() {
            usvg::LineJoin::Miter => SvgLineJoin::Miter,
            usvg::LineJoin::MiterClip => SvgLineJoin::MiterClip,
            usvg::LineJoin::Round => SvgLineJoin::Round,
            usvg::LineJoin::Bevel => SvgLineJoin::Bevel,
        },
        miter_limit: stroke.miterlimit().get(),
        dash_array: stroke
            .dasharray()
            .unwrap_or_default()
            .to_vec()
            .into(),
        dash_offset: stroke.dashoffset(),
    })
}

fn convert_paint(paint: &usvg::Paint, opacity: f32) -> Option<SvgColor> {
    match paint {
        usvg::Paint::Color(color) => Some(SvgColor::rgba8(
            color.red,
            color.green,
            color.blue,
            (opacity * 255.0).round() as u8,
        )),
        usvg::Paint::LinearGradient(_)
        | usvg::Paint::RadialGradient(_)
        | usvg::Paint::Pattern(_) => None,
    }
}

fn check_limit(resource: &'static str, actual: usize, limit: usize) -> Result<(), SvgError> {
    if actual > limit { Err(SvgError::LimitExceeded { resource, actual, limit }) } else { Ok(()) }
}
