use std::collections::HashMap;
use std::sync::Arc;

use aimer_cupid::svg::{
    SvgFitPolicy, SvgGradient, SvgNodeId, SvgPaint, SvgPathCommand, SvgPreserveAspectRatio,
    SvgScene, SvgTransform, SvgViewBox, parse_svg_document,
};

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

/// Parsed source paints retained beside Cupid's renderer-ready scene.
#[derive(Clone, Debug, Default)]
pub struct SvgNodePaint {
    pub fill: Option<SvgPaint>,
    pub stroke: Option<SvgPaint>,
}

#[derive(Clone)]
pub struct SvgDocument {
    source: Arc<str>,
    scene: Arc<SvgScene>,
    diagnostics: Arc<[SvgDiagnostic]>,
    view_box: SvgViewBox,
    preserve_aspect_ratio: SvgPreserveAspectRatio,
    fit_policy: SvgFitPolicy,
    root_transform: SvgTransform,
    gradients: Arc<[SvgGradient]>,
    node_paints: Arc<HashMap<SvgNodeId, SvgNodePaint>>,
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
        check_limit(
            "source bytes",
            bytes.len(),
            SvgLimits::default().max_source_bytes,
        )?;
        let source = std::str::from_utf8(bytes)
            .map_err(|error| SvgError::Parse(error.to_string()))?;
        reject_non_finite_literals(source)?;
        let parsed = parse_svg_document(bytes).map_err(map_parse_error)?;
        let viewport = parsed.scene.viewport;
        if !viewport.width.is_finite() || !viewport.height.is_finite() {
            return Err(SvgError::NonFinite);
        }
        if viewport.width <= 0.0 || viewport.height <= 0.0 {
            return Err(SvgError::Parse(
                "SVG viewport width and height must be positive".to_owned(),
            ));
        }
        check_limit(
            "viewport dimension",
            viewport.width.max(viewport.height) as usize,
            limits.max_viewport_dimension as usize,
        )?;
        check_limit("nodes", parsed.scene.nodes.len(), limits.max_nodes)?;
        let command_count = parsed
            .scene
            .geometries
            .iter()
            .map(|geometry| geometry.commands.len())
            .sum();
        check_limit("path commands", command_count, limits.max_path_commands)?;

        let fit_policy = if parsed.has_view_box {
            SvgFitPolicy::PreserveAspectRatio(parsed.preserve_aspect_ratio)
        } else {
            SvgFitPolicy::Stretch
        };
        let root_transform = parsed.root_transform;
        let node_paints = parsed
            .node_paints
            .into_iter()
            .map(|(node_id, paints)| {
                (
                    node_id,
                    SvgNodePaint {
                        fill: paints.fill,
                        stroke: paints.stroke,
                    },
                )
            })
            .collect();

        Ok(Self {
            source: Arc::from(source),
            scene: Arc::new(parsed.scene),
            diagnostics: collect_diagnostics(source).into(),
            view_box: parsed.view_box,
            preserve_aspect_ratio: parsed.preserve_aspect_ratio,
            fit_policy,
            root_transform,
            gradients: parsed.gradients,
            node_paints: Arc::new(node_paints),
        })
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub fn scene(&self) -> &Arc<SvgScene> {
        &self.scene
    }

    pub fn diagnostics(&self) -> &[SvgDiagnostic] {
        &self.diagnostics
    }

    pub fn view_box(&self) -> SvgViewBox {
        self.view_box
    }

    pub fn preserve_aspect_ratio(&self) -> SvgPreserveAspectRatio {
        self.preserve_aspect_ratio
    }

    pub fn fit_policy(&self) -> SvgFitPolicy {
        self.fit_policy
    }

    pub fn fit_transform(
        &self,
        destination_width: f32,
        destination_height: f32,
    ) -> Result<SvgTransform, SvgError> {
        self.view_box
            .fit_transform(destination_width, destination_height, self.fit_policy)
            .map_err(|error| match error {
                aimer_cupid::svg::SvgFitError::NonFinite(_) => SvgError::NonFinite,
                aimer_cupid::svg::SvgFitError::NonPositive(message) => {
                    SvgError::Parse(message.to_owned())
                }
                aimer_cupid::svg::SvgFitError::InvalidViewBox(message) => {
                    SvgError::InvalidViewBox(message)
                }
                aimer_cupid::svg::SvgFitError::InvalidPreserveAspectRatio(message) => {
                    SvgError::InvalidPreserveAspectRatio(message)
                }
            })
    }

    pub fn gradients(&self) -> &[SvgGradient] {
        &self.gradients
    }

    pub fn paint_for(&self, node_id: SvgNodeId) -> Option<&SvgNodePaint> {
        self.node_paints.get(&node_id)
    }

    pub(crate) fn fit_compensation(
        &self,
        destination_width: f32,
        destination_height: f32,
    ) -> Result<SvgTransform, SvgError> {
        let desired = self.fit_transform(destination_width, destination_height)?;
        let source_to_destination = SvgTransform {
            sx: destination_width / self.scene.viewport.width,
            sy: destination_height / self.scene.viewport.height,
            ..SvgTransform::default()
        };
        let source_inverse = source_to_destination
            .inverse()
            .ok_or(SvgError::NonFinite)?;
        let root_inverse = self.root_transform.inverse().ok_or(SvgError::NonFinite)?;
        let compensation = source_inverse.mul(desired).mul(root_inverse);
        compensation
            .is_finite()
            .then_some(compensation)
            .ok_or(SvgError::NonFinite)
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
            .filter(|node| node.element == aimer_cupid::svg::SvgElementKind::Path)
            .collect();
        if path_nodes.len() != 1 {
            return Err(SvgError::PathSelection(path_nodes.len()));
        }
        let geometry = document
            .scene
            .geometry(path_nodes[0])
            .ok_or(SvgError::PathSelection(0))?;
        Ok(Self {
            commands: geometry.commands.clone(),
        })
    }

    pub fn commands(&self) -> &[SvgPathCommand] {
        &self.commands
    }
}

fn map_parse_error(error: aimer_cupid::svg::SvgParseError) -> SvgError {
    use aimer_cupid::svg::SvgParseError as ParseError;
    match error {
        ParseError::Empty => SvgError::EmptyInput,
        ParseError::TooLarge => SvgError::LimitExceeded {
            resource: "source bytes",
            actual: 0,
            limit: SvgLimits::default().max_source_bytes,
        },
        ParseError::InvalidUtf8 => SvgError::Parse("SVG source is not UTF-8".to_owned()),
        ParseError::InvalidXml(message) => SvgError::Parse(message.to_owned()),
        ParseError::InvalidRoot => SvgError::Parse("invalid SVG root or dimensions".to_owned()),
        ParseError::InvalidNumber => SvgError::NonFinite,
        ParseError::InvalidPath => SvgError::InvalidPath("invalid SVG path data".to_owned()),
        ParseError::Unsupported(feature) => {
            SvgError::Parse(format!("unsupported SVG feature: {feature}"))
        }
        ParseError::InvalidViewBox => SvgError::InvalidViewBox("invalid viewBox".to_owned()),
        ParseError::InvalidPreserveAspectRatio => SvgError::InvalidPreserveAspectRatio(
            "invalid preserveAspectRatio".to_owned(),
        ),
        ParseError::ExternalResource(resource) => SvgError::ExternalResource(resource),
        ParseError::LimitExceeded(resource) => SvgError::LimitExceeded {
            resource,
            actual: 0,
            limit: 0,
        },
    }
}

fn collect_diagnostics(source: &str) -> Vec<SvgDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut found = std::collections::HashSet::new();
    let lower = source.to_ascii_lowercase();
    for (needle, feature, message) in [
        ("lineargradient", "gradient", "gradient is retained in the model; renderer support is deferred"),
        ("radialgradient", "gradient", "gradient is retained in the model; renderer support is deferred"),
        ("<pattern", "pattern", "pattern is retained in the model; renderer support is deferred"),
        ("<clippath", "clip-path", "clip path is retained in the model; renderer support is deferred"),
        ("<mask", "mask", "mask is retained in the model; renderer support is deferred"),
        ("<filter", "filter", "filter is retained in the model; renderer support is deferred"),
        ("<text", "text", "text is retained in the model; renderer support is deferred"),
        ("<script", "script", "script is retained in the model; renderer support is deferred"),
        ("<image", "image", "image is retained in the model; renderer support is deferred"),
    ] {
        if lower.contains(needle) && found.insert(feature) {
            diagnostics.push(SvgDiagnostic { feature, message: Arc::from(message) });
        }
    }
    if lower.contains("fill=\"url(") || lower.contains("fill='url(") || lower.contains("fill:url(") {
        found.insert("gradient-fill");
        diagnostics.push(SvgDiagnostic { feature: "gradient-fill", message: Arc::from("gradient fill is retained in the model; renderer support is deferred") });
    }
    if lower.contains("stroke=\"url(") || lower.contains("stroke='url(") || lower.contains("stroke:url(") {
        found.insert("gradient-stroke");
        diagnostics.push(SvgDiagnostic { feature: "gradient-stroke", message: Arc::from("gradient stroke is retained in the model; renderer support is deferred") });
    }
    if lower.contains("stroke-dasharray") && !lower.contains("stroke-dasharray=\"none\"") && !lower.contains("stroke-dasharray='none'") && found.insert("dashed-stroke") {
        diagnostics.push(SvgDiagnostic { feature: "dashed-stroke", message: Arc::from("dashed stroke is retained in the model; renderer support is deferred") });
    }
    diagnostics
}

fn reject_non_finite_literals(source: &str) -> Result<(), SvgError> {
    for value in source.split(|character: char| {
        !(character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'))
    }) {
        match value.to_ascii_lowercase().as_str() {
            "nan" | "+nan" | "-nan" | "inf" | "+inf" | "-inf" | "infinity"
            | "+infinity" | "-infinity" => return Err(SvgError::NonFinite),
            _ => {}
        }
    }
    Ok(())
}

fn check_limit(resource: &'static str, actual: usize, limit: usize) -> Result<(), SvgError> {
    if actual > limit {
        Err(SvgError::LimitExceeded { resource, actual, limit })
    } else {
        Ok(())
    }
}
