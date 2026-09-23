//! Dependency-free parser for Cupid's retained SVG scene format.
//!
//! The parser intentionally converts source documents to Cupid's own path and
//! scene types. GPU rendering and path tessellation operate on those types.

use std::sync::Arc;
use std::collections::HashMap;

use super::{
    SvgColor, SvgElementKind, SvgFill, SvgFillRule, SvgGeometry, SvgLineCap, SvgLineJoin,
    SvgNode, SvgNodeId, SvgPaintOrder, SvgPathCommand, SvgScene, SvgStroke, SvgTransform,
    SvgViewport, SvgGradient, SvgGradientStop, SvgGradientUnits, SvgPaint, SvgSpreadMethod,
};
use super::{SvgFitPolicy, SvgPreserveAspectRatio, SvgViewBox};

const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_NODES: usize = 16_384;
const MAX_COMMANDS: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq)]
pub enum SvgParseError {
    Empty,
    TooLarge,
    InvalidUtf8,
    InvalidXml(&'static str),
    InvalidRoot,
    InvalidNumber,
    InvalidPath,
    Unsupported(&'static str),
    InvalidViewBox,
    InvalidPreserveAspectRatio,
    ExternalResource(String),
    LimitExceeded(&'static str),
}

impl std::fmt::Display for SvgParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("SVG source is empty"),
            Self::TooLarge => formatter.write_str("SVG source exceeds the parser limit"),
            Self::InvalidUtf8 => formatter.write_str("SVG source is not UTF-8"),
            Self::InvalidXml(message) => write!(formatter, "invalid SVG XML: {message}"),
            Self::InvalidRoot => formatter.write_str("SVG root requires positive width and height"),
            Self::InvalidNumber => formatter.write_str("SVG contains an invalid number"),
            Self::InvalidPath => formatter.write_str("SVG contains invalid path data"),
            Self::Unsupported(feature) => write!(formatter, "unsupported SVG feature: {feature}"),
            Self::InvalidViewBox => formatter.write_str("invalid SVG viewBox"),
            Self::InvalidPreserveAspectRatio => formatter.write_str("invalid SVG preserveAspectRatio"),
            Self::ExternalResource(resource) => write!(formatter, "external SVG resource is not allowed: {resource}"),
            Self::LimitExceeded(resource) => write!(formatter, "SVG {resource} limit exceeded"),
        }
    }
}

/// Parsed SVG paint metadata retained beside the renderer-ready scene.
#[derive(Clone, Debug, Default)]
pub struct SvgParsedPaints {
    /// Fill paint, including references to parsed gradients.
    pub fill: Option<SvgPaint>,
    /// Stroke paint, including references to parsed gradients.
    pub stroke: Option<SvgPaint>,
}

/// Full Cupid-owned result of parsing an SVG document.
#[derive(Clone, Debug)]
pub struct SvgParsedDocument {
    /// Retained geometry and styling consumed by the renderer.
    pub scene: SvgScene,
    /// Root user-space rectangle, or a viewport-sized fallback.
    pub view_box: SvgViewBox,
    /// Root aspect-ratio rule.
    pub preserve_aspect_ratio: SvgPreserveAspectRatio,
    /// Whether the source declared a `viewBox`.
    pub has_view_box: bool,
    /// Root mapping already composed into retained node transforms.
    pub root_transform: SvgTransform,
    /// Gradient definitions found in the document.
    pub gradients: Arc<[SvgGradient]>,
    /// Source paint values keyed by retained scene node id.
    pub node_paints: HashMap<SvgNodeId, SvgParsedPaints>,
}

impl std::error::Error for SvgParseError {}

#[derive(Clone, Copy)]
struct Style {
    fill: Option<SvgColor>,
    stroke: Option<SvgColor>,
    stroke_width: f32,
    fill_rule: SvgFillRule,
    opacity: f32,
    visible: bool,
    cap: SvgLineCap,
    join: SvgLineJoin,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Some(SvgColor::rgba8(0, 0, 0, 255)),
            stroke: None,
            stroke_width: 1.0,
            fill_rule: SvgFillRule::NonZero,
            opacity: 1.0,
            visible: true,
            cap: SvgLineCap::Butt,
            join: SvgLineJoin::Miter,
        }
    }
}

struct OpenNode {
    id: SvgNodeId,
    style: Style,
    transform: SvgTransform,
    tag: String,
}

/// Parses an SVG document into Cupid-owned scene and geometry data.
///
/// Supported elements are `svg`, `g`, `path`, `rect`, `circle`, `ellipse`,
/// `line`, `polyline`, and `polygon`. Paths support the SVG M/L/H/V/C/S/Q/T/Z
/// commands in both absolute and relative forms. External resources, scripts,
/// filters, masks, and animation are rejected.
pub fn parse_svg(bytes: impl AsRef<[u8]>) -> Result<SvgScene, SvgParseError> {
    parse_svg_document(bytes).map(|document| document.scene)
}

/// Parses an SVG while retaining its root fit policy and deferred paints.
pub fn parse_svg_document(bytes: impl AsRef<[u8]>) -> Result<SvgParsedDocument, SvgParseError> {
    let bytes = bytes.as_ref();
    if bytes.is_empty() {
        return Err(SvgParseError::Empty);
    }
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(SvgParseError::TooLarge);
    }
    let source = std::str::from_utf8(bytes).map_err(|_| SvgParseError::InvalidUtf8)?;
    if source.contains("<!DOCTYPE") || source.contains("<!ENTITY") {
        return Err(SvgParseError::Unsupported("DTD and entities"));
    }

    let mut viewport = None;
    let mut root_transform = SvgTransform::default();
    let mut root_style = Style::default();
    let mut declared_view_box = None;
    let mut preserve_aspect_ratio = SvgPreserveAspectRatio::default();
    let mut nodes: Vec<SvgNode> = Vec::new();
    let mut geometries = Vec::new();
    let mut stack: Vec<OpenNode> = Vec::new();
    let mut cursor = 0;
    let mut command_count = 0usize;
    while let Some(relative) = source[cursor..].find('<') {
        cursor += relative;
        if source[cursor..].starts_with("<!--") {
            let end = source[cursor + 4..]
                .find("-->")
                .ok_or(SvgParseError::InvalidXml("unterminated comment"))?;
            cursor += 4 + end + 3;
            continue;
        }
        if source[cursor..].starts_with("<?") {
            let end = source[cursor + 2..]
                .find("?>")
                .ok_or(SvgParseError::InvalidXml("unterminated declaration"))?;
            cursor += 2 + end + 2;
            continue;
        }
        let end = find_tag_end(source, cursor + 1)?;
        let mut body = source[cursor + 1..end].trim();
        cursor = end + 1;
        if body.starts_with('!') {
            return Err(SvgParseError::Unsupported("XML declarations"));
        }
        if let Some(close) = body.strip_prefix('/') {
            let name = close.trim();
            let open = stack.pop().ok_or(SvgParseError::InvalidXml("unexpected close tag"))?;
            if open.tag != name {
                return Err(SvgParseError::InvalidXml("mismatched close tag"));
            }
            continue;
        }
        let self_closing = body.ends_with('/');
        if self_closing {
            body = body[..body.len() - 1].trim_end();
        }
        let (tag, attrs) = parse_tag(body)?;
        if tag == "image" {
            if let Some(href) = attrs.get("href").or_else(|| attrs.get("xlink:href")) {
                if !href.trim().is_empty() && !href.trim().starts_with('#') {
                    return Err(SvgParseError::ExternalResource(href.clone()));
                }
            }
        }
        if stack.is_empty() && tag != "svg" {
            return Err(SvgParseError::InvalidRoot);
        }
        if tag == "svg" {
            if viewport.is_some() {
                return Err(SvgParseError::Unsupported("nested SVG viewport"));
            }
            let view_box = attrs.get("viewBox").map(|v| parse_viewbox(v).ok_or(SvgParseError::InvalidViewBox)).transpose()?;
            preserve_aspect_ratio = attrs.get("preserveAspectRatio").map(|v| v.parse::<SvgPreserveAspectRatio>().map_err(|_| SvgParseError::InvalidPreserveAspectRatio)).transpose()?.unwrap_or_default();
            let w = attrs.get("width").and_then(|v| parse_length(v)).or_else(|| view_box.map(|b| b[2])).unwrap_or(300.0);
            let h = attrs.get("height").and_then(|v| parse_length(v)).or_else(|| view_box.map(|b| b[3])).unwrap_or(150.0);
            if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
                return Err(SvgParseError::InvalidRoot);
            }
            viewport = Some(SvgViewport { width: w, height: h });
            if let Some([x,y,width,height]) = view_box {
                let box_ = SvgViewBox::try_new(x,y,width,height).map_err(|_| SvgParseError::InvalidRoot)?;
                root_transform = box_.fit_transform(w,h,SvgFitPolicy::PreserveAspectRatio(preserve_aspect_ratio)).map_err(|_| SvgParseError::InvalidViewBox)?;
                declared_view_box = Some(box_);
            }
            root_style = parse_style(Style::default(), &attrs)?;
            let content_transform = attrs
                .get("transform")
                .map(|value| parse_transform(value))
                .transpose()?
                .map_or(root_transform, |transform| root_transform.mul(transform));
            if !self_closing {
                stack.push(OpenNode { id: SvgNodeId(u32::MAX), style: root_style, transform: content_transform, tag: tag.to_owned() });
            }
            continue;
        }
        if tag == "defs" {
            if !self_closing {
                stack.push(OpenNode { id: SvgNodeId(u32::MAX), style: Style::default(), transform: root_transform, tag: tag.to_owned() });
            }
            continue;
        }
        if matches!(tag, "title" | "desc" | "metadata" | "style") {
            if !self_closing {
                stack.push(OpenNode { id: SvgNodeId(u32::MAX), style: root_style, transform: root_transform, tag: tag.to_owned() });
            }
            continue;
        }
        if stack.iter().any(|node| node.tag == "defs") {
            if !self_closing {
                stack.push(OpenNode { id: SvgNodeId(u32::MAX), style: Style::default(), transform: root_transform, tag: tag.to_owned() });
            }
            continue;
        }
        if matches!(tag, "defs" | "symbol" | "clipPath" | "mask" | "filter" | "pattern") {
            return Err(SvgParseError::Unsupported("definitions, clip paths, masks, filters, and patterns"));
        }
        if !matches!(tag, "g" | "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon") {
            return Err(SvgParseError::Unsupported("element"));
        }
        if nodes.len() >= MAX_NODES {
            return Err(SvgParseError::LimitExceeded("node"));
        }
        let inherited = stack.last().map(|n| n.style).unwrap_or_default();
        let style = parse_style(inherited, &attrs)?;
        let own_transform = attrs.get("transform").map(|v| parse_transform(v)).transpose()?.unwrap_or_default();
        let parent_transform = stack.last().map(|n| n.transform).unwrap_or_default();
        let transform = parent_transform.mul(own_transform);
        let id = SvgNodeId(nodes.len() as u32);
        let parent = stack.last().map(|n| n.id).filter(|id| id.0 != u32::MAX);
        let classes: Arc<[Arc<str>]> = attrs.get("class").map(|v| v.split_ascii_whitespace().map(Arc::<str>::from).collect::<Vec<_>>().into()).unwrap_or_else(|| Arc::from([]));
        let svg_id = attrs.get("id").filter(|v| !v.is_empty()).map(|v| Arc::<str>::from(v.as_str()));
        let geometry = if tag == "g" { None } else {
            let commands = element_path(tag, &attrs)?;
            command_count = command_count.checked_add(commands.len()).ok_or(SvgParseError::LimitExceeded("path command"))?;
            if command_count > MAX_COMMANDS { return Err(SvgParseError::LimitExceeded("path command")); }
            let index = geometries.len();
            geometries.push(SvgGeometry { commands: commands.into() });
            Some(index)
        };
        let fill = style.fill.map(|color| SvgFill { color, rule: style.fill_rule });
        let stroke = style.stroke.map(|color| SvgStroke { color, width: style.stroke_width, line_cap: style.cap, line_join: style.join, miter_limit: 4.0, dash_array: Arc::from([]), dash_offset: 0.0 });
        nodes.push(SvgNode { node_id: id, svg_id, classes, element: if tag == "g" { SvgElementKind::Group } else { SvgElementKind::Path }, parent, children: Arc::from([]), transform, opacity: style.opacity, geometry, fill, stroke, paint_order: SvgPaintOrder::FillAndStroke, visible: style.visible });
        if let Some(parent) = parent {
            let mut children = nodes[parent.0 as usize].children.to_vec();
            children.push(id);
            nodes[parent.0 as usize].children = children.into();
        }
        if !self_closing {
            stack.push(OpenNode { id, style, transform, tag: tag.to_owned() });
        }
    }
    if !stack.is_empty() { return Err(SvgParseError::InvalidXml("unclosed tag")); }
    let viewport = viewport.ok_or(SvgParseError::InvalidRoot)?;
    let scene = SvgScene { viewport, nodes: nodes.into(), geometries: geometries.into() };
    let view_box = declared_view_box.unwrap_or(SvgViewBox::try_new(0.0, 0.0, viewport.width, viewport.height).map_err(|_| SvgParseError::InvalidViewBox)?);
    let gradients = collect_gradients(source)?;
    let node_paints = collect_node_paints(source, &scene, &gradients);
    Ok(SvgParsedDocument { scene, view_box, preserve_aspect_ratio, has_view_box: declared_view_box.is_some(), root_transform, gradients: gradients.into(), node_paints })
}

fn find_tag_end(source: &str, mut cursor: usize) -> Result<usize, SvgParseError> {
    let bytes = source.as_bytes();
    let mut quote = 0;
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if quote != 0 { if byte == quote { quote = 0; } }
        else if byte == b'\'' || byte == b'"' { quote = byte; }
        else if byte == b'>' { return Ok(cursor); }
        cursor += 1;
    }
    Err(SvgParseError::InvalidXml("unterminated tag"))
}

fn parse_tag(body: &str) -> Result<(&str, std::collections::HashMap<String, String>), SvgParseError> {
    let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
    let name = &body[..name_end];
    if name.is_empty() { return Err(SvgParseError::InvalidXml("empty tag name")); }
    let mut attrs = std::collections::HashMap::new();
    let bytes = body.as_bytes();
    let mut i = name_end;
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if i == bytes.len() { break; }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b':' | b'_' | b'-')) { i += 1; }
        if start == i { return Err(SvgParseError::InvalidXml("invalid attribute name")); }
        let key = &body[start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        if bytes.get(i) != Some(&b'=') { return Err(SvgParseError::InvalidXml("attribute missing value")); }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        let quote = *bytes.get(i).ok_or(SvgParseError::InvalidXml("attribute missing quote"))?;
        if quote != b'\'' && quote != b'"' { return Err(SvgParseError::InvalidXml("attribute must be quoted")); }
        i += 1;
        let value_start = i;
        while i < bytes.len() && bytes[i] != quote { i += 1; }
        if i == bytes.len() { return Err(SvgParseError::InvalidXml("unterminated attribute")); }
        let value = body[value_start..i].replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&");
        i += 1;
        attrs.insert(key.to_owned(), value);
    }
    Ok((name, attrs))
}

fn parse_style(mut style: Style, attrs: &std::collections::HashMap<String, String>) -> Result<Style, SvgParseError> {
    let mut declarations = std::collections::HashMap::new();
    for (key, value) in attrs { if matches!(key.as_str(), "fill" | "stroke" | "stroke-width" | "fill-rule" | "opacity" | "fill-opacity" | "stroke-opacity" | "display" | "visibility" | "stroke-linecap" | "stroke-linejoin") { declarations.insert(key.clone(), value.clone()); } }
    if let Some(inline) = attrs.get("style") { for declaration in inline.split(';') { if let Some((key, value)) = declaration.split_once(':') { declarations.insert(key.trim().to_owned(), value.trim().to_owned()); } } }
    if let Some(value) = declarations.get("fill") { style.fill = parse_color(value)?; }
    if let Some(value) = declarations.get("stroke") { style.stroke = parse_color(value)?; }
    if let Some(value) = declarations.get("stroke-width") { style.stroke_width = parse_length(value).ok_or(SvgParseError::InvalidNumber)?; }
    if let Some(value) = declarations.get("fill-rule") { style.fill_rule = match value.as_str() { "nonzero" => SvgFillRule::NonZero, "evenodd" => SvgFillRule::EvenOdd, _ => return Err(SvgParseError::Unsupported("fill-rule")) }; }
    if let Some(value) = declarations.get("opacity") { style.opacity *= parse_unit(value)?; }
    if let Some(value) = declarations.get("fill-opacity") { if let Some(color) = style.fill.as_mut() { color.a *= parse_unit(value)?; } }
    if let Some(value) = declarations.get("stroke-opacity") { if let Some(color) = style.stroke.as_mut() { color.a *= parse_unit(value)?; } }
    if declarations.get("display").is_some_and(|v| v == "none") || declarations.get("visibility").is_some_and(|v| v == "hidden") { style.visible = false; }
    if let Some(value) = declarations.get("stroke-linecap") { style.cap = match value.as_str() { "butt" => SvgLineCap::Butt, "round" => SvgLineCap::Round, "square" => SvgLineCap::Square, _ => return Err(SvgParseError::Unsupported("stroke-linecap")) }; }
    if let Some(value) = declarations.get("stroke-linejoin") { style.join = match value.as_str() { "miter" => SvgLineJoin::Miter, "round" => SvgLineJoin::Round, "bevel" => SvgLineJoin::Bevel, _ => return Err(SvgParseError::Unsupported("stroke-linejoin")) }; }
    Ok(style)
}

fn parse_color(value: &str) -> Result<Option<SvgColor>, SvgParseError> {
    let value = value.trim();
    if value == "none" { return Ok(None); }
    if value.starts_with("url(") { return Ok(None); }
    let color = if let Some(hex) = value.strip_prefix('#') {
        match hex.len() {
            3 => SvgColor::rgba8(expand(hex.as_bytes()[0])?, expand(hex.as_bytes()[1])?, expand(hex.as_bytes()[2])?, 255),
            4 => SvgColor::rgba8(expand(hex.as_bytes()[0])?, expand(hex.as_bytes()[1])?, expand(hex.as_bytes()[2])?, expand(hex.as_bytes()[3])?),
            6 => SvgColor::rgba8(byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?, 255),
            8 => SvgColor::rgba8(byte(&hex[0..2])?, byte(&hex[2..4])?, byte(&hex[4..6])?, byte(&hex[6..8])?),
            _ => return Err(SvgParseError::Unsupported("color syntax")),
        }
    } else if let Some(args) = value.strip_prefix("rgb(").and_then(|v|v.strip_suffix(')')) {
        let values=parse_numbers(args);if values.len()!=3{return Err(SvgParseError::InvalidNumber)}
        let channel=|v:f32|->Result<u8,SvgParseError>{if !v.is_finite(){return Err(SvgParseError::InvalidNumber)}Ok(v.round().clamp(0.0,255.0) as u8)};
        SvgColor::rgba8(channel(values[0])?,channel(values[1])?,channel(values[2])?,255)
    } else { match value { "black" => SvgColor::rgba8(0,0,0,255), "white" => SvgColor::rgba8(255,255,255,255), "red" => SvgColor::rgba8(255,0,0,255), "green" => SvgColor::rgba8(0,128,0,255), "blue" => SvgColor::rgba8(0,0,255,255), "yellow" => SvgColor::rgba8(255,255,0,255), "transparent" => SvgColor::rgba8(0,0,0,0), _ => return Err(SvgParseError::Unsupported("named color")) } };
    Ok(Some(color))
}

fn byte(value: &str) -> Result<u8, SvgParseError> { u8::from_str_radix(value, 16).map_err(|_| SvgParseError::InvalidNumber) }
fn expand(value: u8) -> Result<u8, SvgParseError> { let digit = (value as char).to_digit(16).ok_or(SvgParseError::InvalidNumber)? as u8; Ok(digit * 17) }
fn parse_unit(value: &str) -> Result<f32, SvgParseError> { let v = value.trim().strip_suffix('%').map(|v| v.parse::<f32>().map(|n| n / 100.0)).unwrap_or_else(|| value.trim().parse::<f32>()).map_err(|_| SvgParseError::InvalidNumber)?; if v.is_finite() { Ok(v.clamp(0.0, 1.0)) } else { Err(SvgParseError::InvalidNumber) } }
fn parse_length(value: &str) -> Option<f32> { let trimmed = value.trim(); let end = trimmed.find(|c: char| !(c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'))).unwrap_or(trimmed.len()); let unit = trimmed[end..].trim(); if !unit.is_empty() && !matches!(unit, "px" | "pt" | "pc" | "mm" | "cm" | "in") { return None; } let n = trimmed[..end].parse::<f32>().ok()?; n.is_finite().then_some(n) }
fn parse_viewbox(value: &str) -> Option<[f32; 4]> { let values = parse_numbers(value); (values.len() == 4 && values.iter().all(|v| v.is_finite()) && values[2] > 0.0 && values[3] > 0.0).then(|| [values[0], values[1], values[2], values[3]]) }

enum GradientBuilder {
    Linear { id: Arc<str>, x1:f32, y1:f32, x2:f32, y2:f32, units:SvgGradientUnits, transform:SvgTransform, spread:SvgSpreadMethod, stops:Vec<SvgGradientStop> },
    Radial { id: Arc<str>, cx:f32, cy:f32, radius:f32, fx:f32, fy:f32, focal_radius:f32, units:SvgGradientUnits, transform:SvgTransform, spread:SvgSpreadMethod, stops:Vec<SvgGradientStop> },
}

fn collect_gradients(source:&str)->Result<Vec<SvgGradient>,SvgParseError>{
    let mut gradients=Vec::new();let mut current:Option<GradientBuilder>=None;let mut cursor=0;
    while let Some(offset)=source[cursor..].find('<'){cursor+=offset;if source[cursor..].starts_with("<!--"){let end=source[cursor+4..].find("-->").ok_or(SvgParseError::InvalidXml("unterminated comment"))?;cursor+=4+end+3;continue;}let end=find_tag_end(source,cursor+1)?;let body=source[cursor+1..end].trim();cursor=end+1;if body.starts_with('/')||body.starts_with('?')||body.starts_with('!'){if body.starts_with('/')&&matches!(body.trim_start_matches('/').trim(),"linearGradient"|"radialGradient"){if let Some(builder)=current.take(){gradients.push(finish_gradient(builder)?);}}continue;}let self_closing=body.ends_with('/');let body=body.strip_suffix('/').unwrap_or(body).trim_end();let(tag,attrs)=parse_tag(body)?;
        if tag=="linearGradient"||tag=="radialGradient"{let id=attrs.get("id").cloned().ok_or(SvgParseError::InvalidXml("gradient id is required"))?;let units=if attrs.get("gradientUnits").is_some_and(|v|v=="userSpaceOnUse"){SvgGradientUnits::UserSpaceOnUse}else{SvgGradientUnits::ObjectBoundingBox};let transform=attrs.get("gradientTransform").map(|v|parse_transform(v)).transpose()?.unwrap_or_default();let spread=match attrs.get("spreadMethod").map(String::as_str).unwrap_or("pad"){"pad"=>SvgSpreadMethod::Pad,"reflect"=>SvgSpreadMethod::Reflect,"repeat"=>SvgSpreadMethod::Repeat,_=>return Err(SvgParseError::Unsupported("gradient spread method"))};current=Some(if tag=="linearGradient"{GradientBuilder::Linear{id:Arc::from(id),x1:gradient_number(&attrs,"x1",0.0)?,y1:gradient_number(&attrs,"y1",0.0)?,x2:gradient_number(&attrs,"x2",1.0)?,y2:gradient_number(&attrs,"y2",0.0)?,units,transform,spread,stops:Vec::new()}}else{let cx=gradient_number(&attrs,"cx",0.5)?;let cy=gradient_number(&attrs,"cy",0.5)?;GradientBuilder::Radial{id:Arc::from(id),cx,cy,radius:gradient_number(&attrs,"r",0.5)?,fx:gradient_number(&attrs,"fx",cx)?,fy:gradient_number(&attrs,"fy",cy)?,focal_radius:gradient_number(&attrs,"fr",0.0)?,units,transform,spread,stops:Vec::new()}});if self_closing{if let Some(builder)=current.take(){gradients.push(finish_gradient(builder)?);}}continue;}
        if tag=="stop"{if let Some(builder)=current.as_mut(){let offset=attrs.get("offset").map(|v|gradient_offset(v)).transpose()?.unwrap_or(0.0);let mut color=attrs.get("stop-color").map(|v|parse_color(v)).transpose()?.flatten().unwrap_or(SvgColor::rgba8(0,0,0,255));if let Some(opacity)=attrs.get("stop-opacity"){color.a*=parse_unit(opacity)?;}let stops=match builder{GradientBuilder::Linear{stops,..}|GradientBuilder::Radial{stops,..}=>stops};stops.push(SvgGradientStop{offset:offset.clamp(0.0,1.0),color});} }
    }
    if let Some(builder)=current{gradients.push(finish_gradient(builder)?);}Ok(gradients)
}

fn finish_gradient(builder:GradientBuilder)->Result<SvgGradient,SvgParseError>{let gradient=match builder{GradientBuilder::Linear{id,x1,y1,x2,y2,units,transform,spread,stops}=>SvgGradient::Linear{id,x1,y1,x2,y2,units,transform,spread,stops:stops.into()},GradientBuilder::Radial{id,cx,cy,radius,fx,fy,focal_radius,units,transform,spread,stops}=>SvgGradient::Radial{id,cx,cy,radius,fx,fy,focal_radius,units,transform,spread,stops:stops.into()}};if gradient.is_finite(){Ok(gradient)}else{Err(SvgParseError::InvalidNumber)}}

fn gradient_number(attrs:&HashMap<String,String>,name:&str,default:f32)->Result<f32,SvgParseError>{let Some(value)=attrs.get(name)else{return Ok(default)};let value=value.trim();let (number,percent)=value.strip_suffix('%').map_or((value,false),|v|(v,true));let number=number.parse::<f32>().map_err(|_|SvgParseError::InvalidNumber)?;let number=if percent{number/100.0}else{number};number.is_finite().then_some(number).ok_or(SvgParseError::InvalidNumber)}
fn gradient_offset(value:&str)->Result<f32,SvgParseError>{let(value,percent)=value.trim().strip_suffix('%').map_or((value.trim(),false),|v|(v,true));let number=value.parse::<f32>().map_err(|_|SvgParseError::InvalidNumber)?;let number=if percent{number/100.0}else{number};number.is_finite().then_some(number).ok_or(SvgParseError::InvalidNumber)}

fn collect_node_paints(source:&str,scene:&SvgScene,gradients:&[SvgGradient])->HashMap<SvgNodeId,SvgParsedPaints>{
    let mut paint_refs=HashMap::new();let mut unnamed=Vec::new();let mut cursor=0;
    while let Some(offset)=source[cursor..].find('<'){cursor+=offset;let Ok(end)=find_tag_end(source,cursor+1)else{break};let body=source[cursor+1..end].trim();cursor=end+1;if body.starts_with('/')||body.starts_with('!')||body.starts_with('?'){continue;}let body=body.strip_suffix('/').unwrap_or(body).trim_end();let Ok((tag,attrs))=parse_tag(body)else{continue};if !matches!(tag,"path"|"rect"|"circle"|"ellipse"|"line"|"polyline"|"polygon"){continue;}let paint=(style_paint(&attrs,"fill",gradients),style_paint(&attrs,"stroke",gradients));if let Some(id)=attrs.get("id"){paint_refs.insert(id.clone(),paint);}else{unnamed.push(paint);}}
    let mut unnamed_iter=unnamed.into_iter();let mut result=HashMap::new();for node in scene.nodes.iter(){if node.element!=SvgElementKind::Path{continue;}let authored=node.svg_id.as_deref().and_then(|id|paint_refs.get(id).cloned()).or_else(||unnamed_iter.next());let paints=if let Some((fill,stroke))=authored{SvgParsedPaints{fill:fill.or_else(||node.fill.as_ref().map(|v|SvgPaint::Solid(v.color))),stroke:stroke.or_else(||node.stroke.as_ref().map(|v|SvgPaint::Solid(v.color)))}}else{SvgParsedPaints{fill:node.fill.as_ref().map(|v|SvgPaint::Solid(v.color)),stroke:node.stroke.as_ref().map(|v|SvgPaint::Solid(v.color))}};result.insert(node.node_id,paints);}result
}

fn style_paint(attrs:&HashMap<String,String>,name:&str,gradients:&[SvgGradient])->Option<SvgPaint>{let value=attrs.get(name).cloned().or_else(||attrs.get("style").and_then(|s|s.split(';').find_map(|d|{let(k,v)=d.split_once(':')?;(k.trim()==name).then(||v.trim().to_owned())})))?;if let Some(reference)=value.trim().strip_prefix("url(#").and_then(|v|v.strip_suffix(')')){return gradients.iter().find(|g|g.id()==reference).map(|g|match g{SvgGradient::Linear{..}=>SvgPaint::Linear(g.clone()),SvgGradient::Radial{..}=>SvgPaint::Radial(g.clone())}).or_else(||Some(SvgPaint::Pattern{id:Arc::from(reference)}));}let color=parse_color(&value).ok().flatten()?;Some(SvgPaint::Solid(color))}

fn parse_transform(value: &str) -> Result<SvgTransform, SvgParseError> {
    let mut result = SvgTransform::default();
    for part in value.split(')').filter(|s| !s.trim().is_empty()) {
        let (name, args) = part.split_once('(').ok_or(SvgParseError::InvalidNumber)?;
        let values = parse_numbers(args);
        let current = match name.trim() {
            "matrix" if values.len() == 6 => SvgTransform { sx: values[0], ky: values[1], kx: values[2], sy: values[3], tx: values[4], ty: values[5] },
            "translate" if (1..=2).contains(&values.len()) => SvgTransform { tx: values[0], ty: *values.get(1).unwrap_or(&0.0), ..SvgTransform::default() },
            "scale" if (1..=2).contains(&values.len()) => SvgTransform { sx: values[0], sy: *values.get(1).unwrap_or(&values[0]), ..SvgTransform::default() },
            "rotate" if (1..=3).contains(&values.len()) => { let a = values[0].to_radians(); let rotation = SvgTransform { sx: a.cos(), ky: a.sin(), kx: -a.sin(), sy: a.cos(), ..SvgTransform::default() }; if values.len() == 3 { SvgTransform { tx: values[1], ty: values[2], ..SvgTransform::default() }.mul(rotation).mul(SvgTransform { tx: -values[1], ty: -values[2], ..SvgTransform::default() }) } else { rotation } },
            "skewX" if values.len() == 1 => SvgTransform { kx: values[0].to_radians().tan(), ..SvgTransform::default() },
            "skewY" if values.len() == 1 => SvgTransform { ky: values[0].to_radians().tan(), ..SvgTransform::default() },
            _ => return Err(SvgParseError::Unsupported("transform")),
        };
        if !current.is_finite() { return Err(SvgParseError::InvalidNumber); }
        result = result.mul(current);
    }
    Ok(result)
}

fn element_path(tag: &str, attrs: &std::collections::HashMap<String, String>) -> Result<Vec<SvgPathCommand>, SvgParseError> {
    let number = |name: &str, default: f32| attrs.get(name).map(|s| parse_length(s).ok_or(SvgParseError::InvalidNumber)).transpose().map(|v| v.unwrap_or(default));
    match tag {
        "path" => parse_path(attrs.get("d").ok_or(SvgParseError::InvalidPath)?),
        "rect" => { let x=number("x",0.0)?; let y=number("y",0.0)?; let w=number("width",0.0)?; let h=number("height",0.0)?; if w < 0.0 || h < 0.0 { return Err(SvgParseError::InvalidNumber); } Ok(vec![m(x,y), l(x+w,y), l(x+w,y+h), l(x,y+h), SvgPathCommand::Close]) },
        "circle" | "ellipse" => { let cx=number("cx",0.0)?; let cy=number("cy",0.0)?; let rx=if tag=="circle" { number("r",0.0)? } else { number("rx",0.0)? }; let ry=if tag=="circle" { rx } else { number("ry",0.0)? }; if rx < 0.0 || ry < 0.0 { return Err(SvgParseError::InvalidNumber); } ellipse_path(cx,cy,rx,ry) },
        "line" => Ok(vec![m(number("x1",0.0)?,number("y1",0.0)?), l(number("x2",0.0)?,number("y2",0.0)?)]),
        "polyline" | "polygon" => { let points=parse_numbers(attrs.get("points").map(String::as_str).unwrap_or("")); if points.len()<4 || points.len()%2!=0 { return Err(SvgParseError::InvalidPath); } let mut p=vec![m(points[0],points[1])]; for pair in points[2..].chunks_exact(2) { p.push(l(pair[0],pair[1])); } if tag=="polygon" { p.push(SvgPathCommand::Close); } Ok(p) },
        _ => Err(SvgParseError::Unsupported("shape")),
    }
}

fn ellipse_path(cx:f32,cy:f32,rx:f32,ry:f32)->Result<Vec<SvgPathCommand>,SvgParseError>{
    if rx==0.0 || ry==0.0 { return Ok(Vec::new()); }
    let k=0.552_284_8; Ok(vec![m(cx+rx,cy), c(cx+rx,cy+k*ry,cx+k*rx,cy+ry,cx,cy+ry), c(cx-k*rx,cy+ry,cx-rx,cy+k*ry,cx-rx,cy), c(cx-rx,cy-k*ry,cx-k*rx,cy-ry,cx,cy-ry), c(cx+k*rx,cy-ry,cx+rx,cy-k*ry,cx+rx,cy), SvgPathCommand::Close])
}

fn parse_path(data:&str)->Result<Vec<SvgPathCommand>,SvgParseError>{
    let mut scan=Numbers::new(data); let mut out=Vec::new(); let mut command=' '; let mut current=(0.0,0.0); let mut start=(0.0,0.0); let mut last_cubic=None; let mut last_quad=None;
    while scan.skip_separators() { if let Some(c)=scan.peek_char().filter(|c| c.is_ascii_alphabetic()) { command=c; scan.bump(); } else if command==' ' { return Err(SvgParseError::InvalidPath); }
        let relative=command.is_ascii_lowercase(); let upper=command.to_ascii_uppercase();
        match upper {
            'Z'=>{ out.push(SvgPathCommand::Close); current=start; last_cubic=None; last_quad=None; command=' '; },
            'M'|'L' => { let (mut x,mut y)=(scan.number()?,scan.number()?); if relative {x+=current.0;y+=current.1;} if upper=='M' { out.push(m(x,y)); start=(x,y); command=if relative {'l'} else {'L'}; } else {out.push(l(x,y));} current=(x,y); last_cubic=None;last_quad=None; },
            'H' => {let mut x=scan.number()?; if relative{x+=current.0;} current.0=x;out.push(l(x,current.1));last_cubic=None;last_quad=None;},
            'V' => {let mut y=scan.number()?;if relative{y+=current.1;}current.1=y;out.push(l(current.0,y));last_cubic=None;last_quad=None;},
            'C' => {let(mut x1,mut y1,mut x2,mut y2,mut x,mut y)=(scan.number()?,scan.number()?,scan.number()?,scan.number()?,scan.number()?,scan.number()?);if relative{x1+=current.0;y1+=current.1;x2+=current.0;y2+=current.1;x+=current.0;y+=current.1;}out.push(c(x1,y1,x2,y2,x,y));current=(x,y);last_cubic=Some((x2,y2));last_quad=None;},
            'S' => {let(mut x2,mut y2,mut x,mut y)=(scan.number()?,scan.number()?,scan.number()?,scan.number()?);if relative{x2+=current.0;y2+=current.1;x+=current.0;y+=current.1;}let(x1,y1)=last_cubic.map(|p|(2.0*current.0-p.0,2.0*current.1-p.1)).unwrap_or(current);out.push(c(x1,y1,x2,y2,x,y));current=(x,y);last_cubic=Some((x2,y2));last_quad=None;},
            'Q' => {let(mut x1,mut y1,mut x,mut y)=(scan.number()?,scan.number()?,scan.number()?,scan.number()?);if relative{x1+=current.0;y1+=current.1;x+=current.0;y+=current.1;}out.push(q(x1,y1,x,y));current=(x,y);last_quad=Some((x1,y1));last_cubic=None;},
            'T' => {let(mut x,mut y)=(scan.number()?,scan.number()?);if relative{x+=current.0;y+=current.1;}let(x1,y1)=last_quad.map(|p|(2.0*current.0-p.0,2.0*current.1-p.1)).unwrap_or(current);out.push(q(x1,y1,x,y));current=(x,y);last_quad=Some((x1,y1));last_cubic=None;},
            'A' => {
                let (rx, ry, rotation) = (scan.number()?.abs(), scan.number()?.abs(), scan.number()?.to_radians());
                let large = scan.number()?;
                let sweep = scan.number()?;
                let (mut x, mut y) = (scan.number()?, scan.number()?);
                if !matches!(large, 0.0 | 1.0) || !matches!(sweep, 0.0 | 1.0) { return Err(SvgParseError::InvalidPath); }
                if relative { x += current.0; y += current.1; }
                if rx == 0.0 || ry == 0.0 { out.push(l(x,y)); }
                else { out.extend(arc_to_cubics(current, (x,y), rx, ry, rotation, large != 0.0, sweep != 0.0)?); }
                current=(x,y); last_cubic=None;last_quad=None;
            },
            _ => return Err(SvgParseError::InvalidPath),
        }
        if out.len()>MAX_COMMANDS{return Err(SvgParseError::LimitExceeded("path command"));}
    }
    if out.is_empty() {Err(SvgParseError::InvalidPath)} else {Ok(out)}
}

fn arc_to_cubics(start:(f32,f32),end:(f32,f32),mut rx:f32,mut ry:f32,phi:f32,large:bool,sweep:bool)->Result<Vec<SvgPathCommand>,SvgParseError>{
    if start==end { return Ok(Vec::new()); }
    let (sin_phi,cos_phi)=phi.sin_cos();
    let dx=(start.0-end.0)*0.5; let dy=(start.1-end.1)*0.5;
    let xp=cos_phi*dx+sin_phi*dy; let yp=-sin_phi*dx+cos_phi*dy;
    let lambda=xp*xp/(rx*rx)+yp*yp/(ry*ry);
    if lambda>1.0 {let scale=lambda.sqrt();rx*=scale;ry*=scale;}
    let numerator=(rx*rx*ry*ry-rx*rx*yp*yp-ry*ry*xp*xp).max(0.0);
    let denominator=rx*rx*yp*yp+ry*ry*xp*xp;
    let sign=if large==sweep{-1.0}else{1.0};
    let factor=if denominator<=f32::EPSILON{0.0}else{sign*(numerator/denominator).sqrt()};
    let cxp=factor*(rx*yp/ry); let cyp=factor*(-ry*xp/rx);
    let cx=cos_phi*cxp-sin_phi*cyp+(start.0+end.0)*0.5;
    let cy=sin_phi*cxp+cos_phi*cyp+(start.1+end.1)*0.5;
    let angle=|ux:f32,uy:f32,vx:f32,vy:f32| (ux*vy-uy*vx).atan2(ux*vx+uy*vy);
    let ux=(xp-cxp)/rx;let uy=(yp-cyp)/ry;let vx=(-xp-cxp)/rx;let vy=(-yp-cyp)/ry;
    let theta=uy.atan2(ux); let mut delta=angle(ux,uy,vx,vy);
    if !sweep && delta>0.0 {delta-=std::f32::consts::TAU;} else if sweep && delta<0.0 {delta+=std::f32::consts::TAU;}
    let count=(delta.abs()/(std::f32::consts::FRAC_PI_2)).ceil() as usize;
    if count==0 || count>4 {return Err(SvgParseError::InvalidPath)}
    let map=|u:f32,v:f32|(cx+rx*cos_phi*u-ry*sin_phi*v,cy+rx*sin_phi*u+ry*cos_phi*v);
    let mut out=Vec::with_capacity(count);
    for index in 0..count {let a=theta+delta*index as f32/count as f32;let b=theta+delta*(index+1) as f32/count as f32;let step=b-a;let k=4.0/3.0*(step*0.25).tan();let (sa,ca)=a.sin_cos();let(sb,cb)=b.sin_cos();let p1=map(ca-k*sa,sa+k*ca);let p2=map(cb+k*sb,sb-k*cb);let p=map(cb,sb);out.push(c(p1.0,p1.1,p2.0,p2.1,p.0,p.1));}
    if let Some(SvgPathCommand::CubicTo{ x,y,.. })=out.last_mut(){*x=end.0;*y=end.1;}
    if out.iter().all(command_finite){Ok(out)}else{Err(SvgParseError::InvalidNumber)}
}

fn command_finite(command:&SvgPathCommand)->bool { match command { SvgPathCommand::MoveTo{x,y}|SvgPathCommand::LineTo{x,y}=>x.is_finite()&&y.is_finite(),SvgPathCommand::QuadraticTo{control_x,control_y,x,y}=>[control_x,control_y,x,y].into_iter().all(|v|v.is_finite()),SvgPathCommand::CubicTo{control1_x,control1_y,control2_x,control2_y,x,y}=>[control1_x,control1_y,control2_x,control2_y,x,y].into_iter().all(|v|v.is_finite()),SvgPathCommand::Close=>true} }

fn parse_numbers(s:&str)->Vec<f32>{let mut scanner=Numbers::new(s);let mut out=Vec::new();while scanner.skip_separators(){match scanner.number(){Ok(v)=>out.push(v),Err(_)=>return Vec::new()}}out}
struct Numbers<'a>{s:&'a str,i:usize}
impl<'a> Numbers<'a>{fn new(s:&'a str)->Self{Self{s,i:0}}fn peek_char(&self)->Option<char>{self.s[self.i..].chars().next()}fn bump(&mut self){if let Some(c)=self.peek_char(){self.i+=c.len_utf8();}}fn skip_separators(&mut self)->bool{while let Some(c)=self.peek_char(){if c.is_ascii_whitespace()||c==','{self.bump()}else{break}}self.i<self.s.len()}fn number(&mut self)->Result<f32,SvgParseError>{self.skip_separators();let start=self.i;if self.peek_char().is_some_and(|c|c=='+'||c=='-'){self.bump()}while self.peek_char().is_some_and(|c|c.is_ascii_digit()){self.bump()}if self.peek_char()==Some('.') {self.bump();while self.peek_char().is_some_and(|c|c.is_ascii_digit()){self.bump()}}if self.peek_char().is_some_and(|c|c=='e'||c=='E'){self.bump();if self.peek_char().is_some_and(|c|c=='+'||c=='-'){self.bump()}while self.peek_char().is_some_and(|c|c.is_ascii_digit()){self.bump()}}if self.i==start{return Err(SvgParseError::InvalidNumber)}let n=self.s[start..self.i].parse::<f32>().map_err(|_|SvgParseError::InvalidNumber)?;if n.is_finite(){Ok(n)}else{Err(SvgParseError::InvalidNumber)}}}

fn m(x:f32,y:f32)->SvgPathCommand{SvgPathCommand::MoveTo{x,y}}
fn l(x:f32,y:f32)->SvgPathCommand{SvgPathCommand::LineTo{x,y}}
fn q(control_x:f32,control_y:f32,x:f32,y:f32)->SvgPathCommand{SvgPathCommand::QuadraticTo{control_x,control_y,x,y}}
fn c(control1_x:f32,control1_y:f32,control2_x:f32,control2_y:f32,x:f32,y:f32)->SvgPathCommand{SvgPathCommand::CubicTo{control1_x,control1_y,control2_x,control2_y,x,y}}
